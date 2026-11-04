//! Faixas das trilhas: clipes, edição com o mouse, soltar mídia e desenho.

use crate::i18n::tr;
use bv_core::Tick;
use bv_timeline::{
    ClipId, FadeCurve, MarkerId, MediaInfo, MediaSource, Project, TrackId, TrackKind,
};

use super::{Layout, Row, TimelineView};
use crate::app::{BananaApp, MediaDrag, Tool};
use crate::dock::Tab;
use crate::egui::{
    self, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2,
    vec2,
};
use crate::icons::{self, Icon};
use crate::settings::{FrameNumbers, SnapKind, Thumbnails};
use crate::tabs;
use crate::theme;
use crate::timefmt::{RulerFormat, RulerFormatExt};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContextTarget {
    None,
    Clip(ClipId),
    Track(TrackId),
    /// The space below the tracks.
    Empty,
    Marker(Option<MarkerId>, Tick),
    /// The CD layout bar (a tag, or a time), the command bar.
    CdBar(Option<super::marker_bars::CdTag>, Tick),
    CommandBar(Option<MarkerId>, Tick),
    Envelope(super::envelopes::EnvHit),
    /// An OFX effect's envelope (`TimelineView::ofx_menu`), a video
    /// track's keyframes area.
    OfxEnvelope,
    OfxArea(TrackId),
    /// An event's fade (`out`: its fade-out): VEGAS's popup of curves.
    Fade {
        clip: ClipId,
        out: bool,
    },
    /// A crossfade: the event going out and the one coming in.
    Crossfade {
        out: ClipId,
        inn: ClipId,
    },
}

/// Auto-scroll, VEGAS's "click wheel button and move mouse": where the
/// wheel was pressed; `held`: still down (letting go far from there ends
/// it, a click leaves it on until the next one).
#[derive(Clone, Copy, Debug)]
pub(crate) struct AutoScroll {
    origin: Pos2,
    held: bool,
}

/// Events dragged with the right mouse button and let go: VEGAS's menu
/// asking to move or copy them there, the move pending meanwhile.
pub(crate) struct DropMenu {
    at: Pos2,
    clip: ClipId,
    gesture: bv_timeline::Gesture<Project>,
}

/// Media dragged in with the right mouse button and let go: where, and
/// the event under the pointer (for Add as Takes).
#[derive(Clone, Debug)]
pub(crate) struct MediaDropMenu {
    at: Pos2,
    items: Vec<DropItem>,
    time: Tick,
    track: Option<bv_timeline::TrackId>,
    clip: Option<ClipId>,
}

impl MediaDropMenu {
    /// The menu open at `at` for media `items` let go at `time` (screen
    /// tests).
    pub(crate) fn for_review(
        at: Pos2,
        items: Vec<bv_timeline::MediaId>,
        time: Tick,
    ) -> MediaDropMenu {
        MediaDropMenu {
            at,
            items: items.into_iter().map(DropItem::Media).collect(),
            time,
            track: None,
            clip: None,
        }
    }
}

/// What a right-drag lets go: media of the project or files.
#[derive(Clone, Debug)]
pub(crate) enum DropItem {
    Media(bv_timeline::MediaId),
    File(std::path::PathBuf),
}

/// How the right-drag menu lays several media out (VEGAS's Help, "Add
/// multiple files and create events").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrange {
    /// End to end on the track they were let go on.
    AcrossTime,
    /// Each on its own track, from that one down.
    AcrossTracks,
    /// Takes of one event.
    AsTakes,
}

/// The buttons at the bottom right of each event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipButton {
    /// Generated media: its Video Media Generators window.
    Generated,
    PanCrop,
    Fx,
    More,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Zone {
    Button(ClipButton),
    Body,
    Start,
    End,
    Level,
    FadeIn,
    FadeOut,
    /// The snap offset flag: the white triangle at its bottom.
    SnapOffset,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Nothing,
    Clip { clip: ClipId, zone: Zone },
    Track { track: TrackId },
}

pub(crate) enum Drag {
    Move {
        clip: ClipId,
        grab: Tick,
    },
    /// An event double-clicked and dragged (VEGAS's): the time selection
    /// from its edges out to the next fade or event edge on its track.
    Extend {
        clip: ClipId,
    },
    /// An event's snap offset flag dragged along it.
    SnapOffset {
        clip: ClipId,
    },
    /// Shift+drag over an envelope: a trail of points (the envelope
    /// brush), each pointer place a sample.
    EnvBrush {
        track: TrackId,
        kind: bv_timeline::EnvelopeKind,
        samples: Vec<(Tick, f32)>,
    },
    /// Trimming an edge; `ungrouped`: Shift+drag, the event alone (L and
    /// J cuts).
    TrimStart {
        clip: ClipId,
        ungrouped: bool,
    },
    TrimEnd {
        clip: ClipId,
        ungrouped: bool,
    },
    Level {
        clip: ClipId,
        level0: f32,
        y0: f32,
        span: f32,
    },
    FadeIn {
        clip: ClipId,
    },
    FadeOut {
        clip: ClipId,
    },
    TimeSelect {
        anchor: Tick,
    },
    /// The selection rectangle, from an empty spot.
    Marquee {
        start: Pos2,
        add: bool,
        shape: BoxShape,
    },
    ZoomBox {
        start: Pos2,
    },
    Slip {
        clip: ClipId,
        t0: Tick,
    },
    Slide {
        clip: ClipId,
        t0: Tick,
    },
    /// Alt+drag of an edge (Alt+Shift+drag of the event: its end): the
    /// media moves with the edge, from `edge0` as the pointer goes on from
    /// `t0`.
    SlipTrim {
        clip: ClipId,
        end: bool,
        t0: Tick,
        edge0: Tick,
    },
    /// Ctrl+Alt+drag of an edge, or of the event over a crossfade: the
    /// edge of the event touching it moves along (`ungrouped`: Shift too).
    TrimAdjacent {
        clip: ClipId,
        end: bool,
        t0: Tick,
        edge0: Tick,
        ungrouped: bool,
    },
    /// Ctrl+Alt+Shift+drag of an event: split at `at`, then the side
    /// dragged toward is trimmed back.
    SplitTrim {
        clip: ClipId,
        at: Tick,
    },
    /// Stretching an event (its speed): by its end, or by its start with
    /// the end staying put.
    Stretch {
        clip: ClipId,
        rate0: f64,
        dur0: Tick,
        from_start: bool,
    },
    /// Dragging an envelope's line within the time selection: the part
    /// inside moves up and down, with fades to the edges (Options ›
    /// Preferences › Editing: "Time selection envelope fades").
    EnvRange {
        track: TrackId,
        kind: bv_timeline::EnvelopeKind,
        y0: f32,
        outer: bv_core::TimeRange,
        inner: bv_core::TimeRange,
    },
    /// Dragging an envelope point; `index: None` adds it at `origin` first.
    EnvPoint {
        track: TrackId,
        /// An event's envelope (`None`: the track's).
        clip: Option<ClipId>,
        kind: bv_timeline::EnvelopeKind,
        index: Option<usize>,
        origin: (Tick, f32),
    },
    /// Dragging an envelope's line, as VEGAS's: its segment's points
    /// (`points` as they were: index, time, value) up or down, their times
    /// kept (Ctrl: fine) — once the pointer has gone a few pixels
    /// (`moved`), so a click changes nothing; a double-click adds a point.
    EnvSegment {
        track: TrackId,
        clip: Option<ClipId>,
        kind: bv_timeline::EnvelopeKind,
        points: Vec<(usize, Tick, f32)>,
        y0: f32,
        moved: bool,
    },
    /// Dragging a needle by its line (Options › Preferences › General:
    /// "Allow edit cursor to be dragged"), or scrubbing with Ctrl.
    Needle(super::Needle),
}

/// The selection rectangle's shape: a right-click while dragging goes from
/// one to the next, as VEGAS's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoxShape {
    Free,
    /// The time it spans, on every track.
    Time,
    /// The tracks it spans, all along them.
    Tracks,
}

impl BoxShape {
    fn next(self) -> BoxShape {
        match self {
            BoxShape::Free => BoxShape::Time,
            BoxShape::Time => BoxShape::Tracks,
            BoxShape::Tracks => BoxShape::Free,
        }
    }

    /// The rectangle from `start` to `p`, in this shape.
    fn rect(self, l: &Layout, start: Pos2, p: Pos2) -> Rect {
        let r = Rect::from_two_pos(start, p);
        match self {
            BoxShape::Free => r,
            BoxShape::Time => Rect::from_x_y_ranges(r.x_range(), l.lanes.y_range()),
            BoxShape::Tracks => Rect::from_x_y_ranges(l.lanes.x_range(), r.y_range()),
        }
    }

    /// Whether an event drawn at `r` is in the rectangle `area` (all along
    /// the tracks, or every track, out of sight too).
    fn takes(self, area: Rect, r: Rect) -> bool {
        let across = r.right() >= area.left() && r.left() <= area.right();
        let down = r.bottom() >= area.top() && r.top() <= area.bottom();
        match self {
            BoxShape::Free => across && down,
            BoxShape::Time => across,
            BoxShape::Tracks => down,
        }
    }
}

pub(crate) const TITLE_H: f32 = 14.0;

/// The playback rates a Ctrl+drag of an edge reaches, as VEGAS's: a
/// quarter of the speed to four times it.
const MIN_RATE: f64 = 0.25;
const MAX_RATE: f64 = 4.0;
const EDGE_PX: f32 = 6.0;

pub(crate) fn clip_rect(
    view: &TimelineView,
    l: &Layout,
    row: &Row,
    start: Tick,
    end: Tick,
) -> Rect {
    // Expanded Edit Mode: the A roll above, the B roll below; Expand
    // Track Layers: the A roll at the top, the B roll at the bottom (over
    // the Surround Pan Keyframes row).
    let body = row.height - row.keys;
    let (from, share) = view.band(row.track, start);
    let (top, height) = (row.top + body * from, body * share);
    Rect::from_min_max(
        pos2(view.x_of(l, start), top + 1.0),
        pos2(view.x_of(l, end), top + height - 1.0),
    )
}

/// "Opacity is 52 %" / "Gain is −4.1 dB": an event's level as VEGAS
/// writes it while its line is dragged.
pub(crate) fn level_text(kind: TrackKind, level: f32) -> String {
    match kind {
        TrackKind::Video => trf!(
            "Opacity is {} %",
            crate::fmt::num((level * 100.0).round() as f64, 0)
        ),
        TrackKind::Audio if level <= 1e-4 => tr("Gain is −∞ dB").to_string(),
        TrackKind::Audio => trf!(
            "Gain is {} dB",
            crate::fmt::num((20.0 * level.log10()) as f64, 1)
        ),
    }
}

fn paint_level_value(painter: &egui::Painter, at: Pos2, kind: TrackKind, level: f32) {
    paint_tip(painter, at, level_text(kind, level));
}

/// A value in a small box by the pointer (a dragged handle's).
fn paint_tip(painter: &egui::Painter, at: Pos2, text: String) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.5), theme::text());
    let r = Rect::from_min_size(at + vec2(12.0, 14.0), galley.size()).expand(3.0);
    painter.rect_filled(r, 3.0, theme::panel_dark());
    painter.rect_stroke(
        r,
        3.0,
        Stroke::new(1.0, theme::separator()),
        egui::StrokeKind::Inside,
    );
    painter.galley(r.min + vec2(3.0, 3.0), galley, theme::text());
}

/// The blue handle in the middle of an event's level line, shown while the
/// event is picked or under the pointer, like VEGAS's: wide across its top
/// on the line, narrower at its foot, a dark edge round its sides and foot
/// so that it shows over any picture or waveform.
fn paint_level_handle(painter: &egui::Painter, center: Pos2) {
    let top = center.y - 3.0;
    let corners = vec![
        pos2(center.x - 12.0, top),
        pos2(center.x + 12.0, top),
        pos2(center.x + 7.0, top + 7.0),
        pos2(center.x - 7.0, top + 7.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        corners.clone(),
        HANDLE_BLUE,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::line(
        corners[1..].iter().chain(&corners[..1]).copied().collect(),
        Stroke::new(1.0, HANDLE_EDGE),
    ));
}

/// The blue of VEGAS's handles on an event's top, and their dark edge.
const HANDLE_BLUE: Color32 = Color32::from_rgb(0x50, 0x8C, 0xF0);
const HANDLE_EDGE: Color32 = Color32::from_rgb(0x10, 0x22, 0x44);

/// A fade's handle as VEGAS draws it: a blue triangle, its right angle at
/// `corner` (the level line's top where the fade ends), toward the event's
/// inside (`way` 1 rightward, −1 leftward), its slanted side edged dark.
fn paint_fade_tab(painter: &egui::Painter, corner: Pos2, way: f32) {
    let (across, down) = (corner + vec2(10.0 * way, 0.0), corner + vec2(0.0, 10.0));
    painter.add(egui::Shape::convex_polygon(
        vec![corner, across, down],
        HANDLE_BLUE,
        Stroke::NONE,
    ));
    painter.line_segment([across, down], Stroke::new(1.0, HANDLE_EDGE));
}

/// The pointer over a fade's handle, so one sees it can be dragged: an
/// arrow with a fade's curve beside it (rising for a fade-in), drawn — the
/// system has none.
fn fade_cursor(ctx: &egui::Context, p: Pos2, rising: bool) {
    ctx.set_cursor_icon(CursorIcon::None);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("fade cursor"),
    ));
    let (black, white) = (Color32::BLACK, Color32::WHITE);
    let at = |x: f32, y: f32| p + vec2(x, y);
    // The arrow, its tip at the pointer: its head and its tail.
    let head = vec![at(0.0, 0.0), at(0.0, 15.0), at(11.0, 10.6)];
    let tail = vec![at(3.4, 11.4), at(6.1, 17.2), at(8.6, 16.1), at(5.9, 10.4)];
    let outline = vec![
        at(0.0, 0.0),
        at(0.0, 15.0),
        at(3.4, 11.4),
        at(6.1, 17.2),
        at(8.6, 16.1),
        at(5.9, 10.4),
        at(11.0, 10.6),
    ];
    painter.add(Shape::convex_polygon(head, white, Stroke::NONE));
    painter.add(Shape::convex_polygon(tail, white, Stroke::NONE));
    painter.add(Shape::closed_line(outline, Stroke::new(1.0, black)));
    // The fade: a curve from a bottom corner up, beside the arrow.
    let curve: Vec<Pos2> = (0..=12)
        .map(|i| {
            let f = i as f32 / 12.0;
            let s = f * f * (3.0 - 2.0 * f);
            let g = if rising { s } else { 1.0 - s };
            at(12.0 + 12.0 * f, 22.0 - 10.0 * g)
        })
        .collect();
    painter.add(Shape::line(curve.clone(), Stroke::new(3.4, black)));
    painter.add(Shape::line(curve, Stroke::new(1.6, HANDLE_BLUE)));
}

/// Text and icons on events: light, whatever the interface type (events
/// take their tracks' colors, not the panels').
const ON_EVENT: Color32 = Color32::from_rgb(0xE6, 0xE6, 0xE6);
const ON_EVENT_DIM: Color32 = Color32::from_rgb(0x9A, 0x9C, 0xA3);

/// An event's fill: its track's color at half the brightness.
fn event_fill(track: Color32) -> Color32 {
    let [r, g, b, _] = track.to_array();
    Color32::from_rgb(r / 2, g / 2, b / 2)
}

pub(super) fn level_y(r: Rect, level: f32) -> f32 {
    r.top() + 3.0 + (1.0 - level.clamp(0.0, 1.0)) * (r.height() - 6.0)
}

/// Where an event's fade handles are: on its level line, where the fades'
/// curves end, kept inside the event.
fn fade_handle_y(r: Rect, level: f32) -> f32 {
    level_y(r, level).clamp(r.top() + 4.0, r.bottom() - 4.0)
}

fn px(view: &TimelineView, t: Tick) -> f32 {
    (t.0 as f64 / view.ticks_per_px) as f32
}

const BUTTON: f32 = 17.0;

/// Retângulos dos botões do clipe (da esquerda para a direita), alinhados ao
/// canto visível do clipe. Vazio se o clipe for estreito ou baixo demais.
/// The part of the event `idx` of `track` that is not covered by later
/// events of the same track (those are drawn on top of it). The event
/// buttons and labels go in this part, like in VEGAS.
pub(crate) fn uncovered(
    view: &TimelineView,
    l: &Layout,
    track: &bv_timeline::Track,
    idx: usize,
    r: Rect,
) -> Rect {
    let c = &track.clips[idx];
    let cover = track.clips[idx + 1..]
        .iter()
        .filter(|o| o.start < c.end())
        .map(|o| o.start)
        .min();
    match cover {
        Some(t) => Rect::from_min_max(r.min, pos2(view.x_of(l, t).max(r.left()), r.bottom())),
        None => r,
    }
}

/// Crossfade colour (the blue fade curves of VEGAS).
const XFADE: Color32 = Color32::from_rgb(0x3F, 0x66, 0xFF);

/// "3:20" style length (seconds:frames, or minutes:seconds:frames).
fn short_duration(d: Tick, rate: bv_core::FrameRate) -> String {
    let frames = rate.frame_at(rate.snap_nearest(d)).max(0);
    let base = rate.timecode_base().max(1) as i64;
    let (secs, ff) = (frames / base, frames % base);
    if secs >= 60 {
        format!("{}:{:02}:{ff:02}", secs / 60, secs % 60)
    } else {
        format!("{secs}:{ff:02}")
    }
}

/// Where an event's thumbnails go (their left edges) and the timeline time
/// each one shows, as Options › Preferences › Video › "Thumbnails to show in
/// video events" says: the first frame, the last, the middle one, or as many
/// as fit.
fn thumbnail_spots(
    view: &TimelineView,
    l: &Layout,
    clip: &bv_timeline::Clip,
    r: Rect,
    tw: f32,
    mode: Thumbnails,
) -> Vec<(f32, Tick)> {
    let last = clip.end() - Tick(1);
    if mode == Thumbnails::All {
        let first = ((l.lanes.left() - r.left()) / tw).floor().max(0.0) as i32;
        let mut x = r.left() + first as f32 * tw;
        let mut spots = Vec::new();
        while x < r.right().min(l.lanes.right()) && spots.len() < 64 {
            spots.push((x, view.time_at(l, x + tw / 2.0).clamp(clip.start, last)));
            x += tw;
        }
        return spots;
    }
    // As many of the head, tail and middle as fit side by side.
    let fit = (r.width() / tw).floor() as usize;
    let mut spots = vec![(r.left(), clip.start)];
    if matches!(mode, Thumbnails::HeadTail | Thumbnails::HeadCenterTail) && fit >= 2 {
        spots.push((r.right() - tw, last));
    }
    if mode == Thumbnails::HeadCenterTail && fit >= 3 {
        let middle = Tick(clip.start.0 + (clip.end().0 - clip.start.0) / 2);
        spots.push((r.center().x - tw / 2.0, middle));
    }
    spots
}

/// The box with a thumbnail's source frame number (or time) at its
/// lower-left corner, as VEGAS's.
fn frame_number_box(painter: &egui::Painter, thumb: Rect, text: String) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(10.0), Color32::WHITE);
    let size = galley.size() + vec2(6.0, 2.0);
    if size.x > thumb.width() - 2.0 || size.y > thumb.height() - 2.0 {
        return;
    }
    let r = Rect::from_min_size(
        pos2(thumb.left() + 1.0, thumb.bottom() - 1.0 - size.y),
        size,
    );
    painter.rect_filled(r, 0.0, Color32::from_black_alpha(170));
    painter.galley(r.min + vec2(3.0, 1.0), galley, Color32::WHITE);
}

/// Small boxed label with a fade/transition length: `align` says which of
/// its points is at `at`; it stays between `within`'s top and bottom.
fn duration_badge(painter: &egui::Painter, at: Pos2, align: Align2, within: Rect, text: String) {
    // VEGAS's: dark text in a small white box with a thin dark border.
    let galley = painter.layout_no_wrap(text, FontId::proportional(10.5), Color32::BLACK);
    let size = (galley.size() + vec2(6.0, 2.0)).round();
    let r = align.anchor_size(at, size);
    let up = (within.bottom() - r.bottom()).min(0.0);
    let down = (within.top() - (r.top() + up)).max(0.0);
    let r = Rect::from_min_size((r.min + vec2(0.0, up + down)).round(), size);
    painter.rect(
        r,
        0.0,
        Color32::from_gray(248),
        Stroke::new(1.0, Color32::from_gray(70)),
        StrokeKind::Inside,
    );
    painter.galley(r.min + vec2(3.0, 1.0), galley, Color32::BLACK);
}

/// The buttons of an event in `r` (`generated`: of generated media).
pub(crate) fn clip_buttons(
    view: &TimelineView,
    l: &Layout,
    kind: TrackKind,
    generated: bool,
    r: Rect,
) -> Vec<(ClipButton, Rect)> {
    let b = view.buttons;
    let mut list = Vec::new();
    if !view.show_buttons {
        return Vec::new();
    }
    if generated {
        list.push(ClipButton::Generated);
    }
    if kind == TrackKind::Video && b.pan_crop {
        list.push(ClipButton::PanCrop);
    }
    if b.fx {
        list.push(ClipButton::Fx);
    }
    if b.more {
        list.push(ClipButton::More);
    }
    let visible = r.intersect(l.lanes);
    if r.height() < 30.0 {
        return Vec::new();
    }
    // As the event narrows, they go one by one from the left, as VEGAS's.
    let fit = ((visible.width() - 36.0) / (BUTTON + 2.0)).floor().max(0.0) as usize;
    let list: Vec<ClipButton> = list.split_off(list.len().saturating_sub(fit));
    if list.is_empty() {
        return Vec::new();
    }
    let mut x = visible.right() - 3.0 - list.len() as f32 * (BUTTON + 2.0) + 2.0;
    let y = r.bottom() - 3.0 - BUTTON;
    list.into_iter()
        .map(|kind| {
            let rect = Rect::from_min_size(pos2(x, y), vec2(BUTTON, BUTTON));
            x += BUTTON + 2.0;
            (kind, rect)
        })
        .collect()
}

fn hit(view: &TimelineView, l: &Layout, project: &Project, pos: Pos2) -> Hit {
    if !l.lanes.contains(pos) {
        return Hit::Nothing;
    }
    let Some(row) = l.row_at(pos.y) else {
        return Hit::Nothing;
    };
    let Some(track) = project.track(row.track) else {
        return Hit::Nothing;
    };
    let found = track.clips.iter().enumerate().rev().find_map(|(i, c)| {
        let r = clip_rect(view, l, row, c.start, c.end());
        let inside = pos.x >= r.left() - 1.0
            && pos.x <= r.right() + 1.0
            && pos.y >= r.top() - 1.0
            && pos.y <= r.bottom() + 1.0;
        inside.then_some((i, c, r))
    });
    let Some((idx, clip, r)) = found else {
        return Hit::Track { track: row.track };
    };
    let vis = uncovered(view, l, track, idx, r);
    let generated = project
        .media(clip.media)
        .is_some_and(|m| matches!(m.source, bv_timeline::MediaSource::Generated(_)));
    if let Some((b, _)) = clip_buttons(view, l, row.kind, generated, vis)
        .into_iter()
        .find(|(_, br)| br.contains(pos))
    {
        return Hit::Clip {
            clip: clip.id,
            zone: Zone::Button(b),
        };
    }
    let fade_in_x = r.left() + px(view, clip.fade_in);
    let fade_out_x = r.right() - px(view, clip.fade_out);
    let (level_from, level_to) =
        level_span(view, track, clip, r, project.auto_crossfade).unwrap_or((r.left(), r.right()));
    // The fades' handles are on the level line, where their curves end.
    let near_handle = (pos.y - fade_handle_y(r, clip.level)).abs() <= 6.0;
    // An edge an event overlaps has no fade handle: the crossfade is its
    // fade.
    let (cross_in, cross_out) = if project.auto_crossfade {
        track.crossfades(clip)
    } else {
        (None, None)
    };
    // The snap offset flag, at the bottom where it was put.
    let flag_x = r.left() + px(view, clip.snap_point());
    let on_flag = r.width() > 16.0
        && pos.y >= r.bottom() - 8.0
        && pos.x >= flag_x - 2.0
        && pos.x <= flag_x + 8.0;
    let zone = if near_handle
        && cross_in.is_none()
        && (pos.x - fade_in_x).abs() <= 6.0
        && pos.x - r.left() > 2.0
    {
        Zone::FadeIn
    } else if near_handle
        && cross_out.is_none()
        && (pos.x - fade_out_x).abs() <= 6.0
        && r.right() - pos.x > 2.0
    {
        Zone::FadeOut
    } else if on_flag {
        Zone::SnapOffset
    } else if let Some((clip, zone)) = edge_at(view, l, row, track, project.auto_crossfade, pos) {
        return Hit::Clip { clip, zone };
    } else if (pos.y - level_y(r, clip.level)).abs() <= 4.0
        && (level_from..=level_to).contains(&pos.x)
    {
        Zone::Level
    } else {
        Zone::Body
    };
    Hit::Clip {
        clip: clip.id,
        zone,
    }
}

/// The event edge the pointer at `pos` trims, the nearest one in reach: an
/// event's own a few points into it (a point out of it), and, as VEGAS's,
/// either dark line of a crossfade from both its sides — the start of the
/// event coming in, the end of the one going out under it (an edge a later
/// event covers shows only as a crossfade's line).
fn edge_at(
    view: &TimelineView,
    l: &Layout,
    row: &Row,
    track: &bv_timeline::Track,
    auto_crossfade: bool,
    pos: Pos2,
) -> Option<(bv_timeline::ClipId, Zone)> {
    // The events at the pointer's height around it.
    let t = view.time_at(l, pos.x);
    let slack = Tick(((EDGE_PX + 4.0) as f64 * view.ticks_per_px).ceil() as i64);
    let near: Vec<(usize, Rect)> = track
        .clips
        .iter()
        .enumerate()
        .filter(|(_, c)| c.start <= t + slack && c.end() >= t - slack)
        .map(|(i, c)| (i, clip_rect(view, l, row, c.start, c.end())))
        .filter(|(_, r)| pos.y >= r.top() - 1.0 && pos.y <= r.bottom() + 1.0)
        .collect();
    let mut best: Option<(f32, bv_timeline::ClipId, Zone)> = None;
    for &(i, r) in &near {
        let c = &track.clips[i];
        let reach = EDGE_PX.min(r.width() / 4.0);
        for (x, way, zone) in [(r.left(), 1.0, Zone::Start), (r.right(), -1.0, Zone::End)] {
            let hidden = near.iter().any(|&(j, o)| {
                j > i
                    && o.left() < x
                    && o.right() > x
                    && !(auto_crossfade && !c.muted && !track.clips[j].muted)
            });
            if hidden {
                continue;
            }
            // Past it, another event: its line is reached from there too.
            let out = if near
                .iter()
                .any(|&(j, o)| j != i && o.contains(pos2(x - way * 2.0, pos.y)))
            {
                3.0
            } else {
                1.0
            };
            // How far into the event the pointer is.
            let d = (pos.x - x) * way;
            if d > reach || d < -out {
                continue;
            }
            // As near, the event under the pointer's.
            let score = d.abs() + if d < 0.0 { 0.5 } else { 0.0 };
            if best.is_none_or(|(s, ..)| score < s) {
                best = Some((score, c.id, zone));
            }
        }
    }
    best.map(|(_, id, zone)| (id, zone))
}

/// The mouse wheel over the tracks, as VEGAS's: it zooms; Ctrl scrolls up
/// and down, Shift across (over the headers or with Alt: up and down);
/// Ctrl+Shift moves the cursor a pixel a notch, Ctrl+Alt+Shift a frame
/// (the edge being trimmed, in edge-trimming mode; the scrub rate, while
/// playing).
fn wheel(
    view: &mut TimelineView,
    ui: &egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    pointer: Option<Pos2>,
) {
    let Some(pos) = pointer else { return };
    // Over the bus tracks the wheel scrolls them (their headers, or with
    // Alt), else it works as over the tracks.
    let over_bus_headers = l.bus_headers.contains(pos);
    let over_bus = over_bus_headers || l.bus_lanes.contains(pos);
    let over_lanes =
        l.lanes.contains(pos) || l.ruler.contains(pos) || l.marker_bar.contains(pos) || over_bus;
    let over_headers = l.headers.contains(pos) || over_bus_headers;
    // Over an output meter the wheel changes the meters' range.
    if (!over_lanes && !over_headers)
        || (!app.settings.hide_track_meters
            && super::headers::over_meter(l, pos, app.settings.track_meters_across))
    {
        return;
    }
    let (scroll, zoom, m) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers));
    let (shift, alt) = (m.shift, m.alt);
    let max_scroll = (l.content_height - l.lanes.height() + 20.0).max(0.0);
    if m.command && shift {
        let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        view.wheel_rest += notches;
        let n = view.wheel_rest.trunc();
        view.wheel_rest -= n;
        if n != 0.0 {
            nudge(view, app, n as i64, alt);
        }
        return;
    }
    view.wheel_rest = 0.0;
    if m.command {
        // Ctrl+wheel reaches egui as a zoom: back to points, up and down.
        if zoom != 1.0 {
            let speed = ui.ctx().options(|o| o.input_options.scroll_zoom_speed);
            view.scroll_y = (view.scroll_y - zoom.ln() / speed).clamp(0.0, max_scroll);
        }
        return;
    }
    if over_headers || alt {
        if over_bus {
            let max = (l.bus_content - l.bus_lanes.height()).max(0.0);
            view.bus_scroll = (view.bus_scroll - scroll.y - scroll.x).clamp(0.0, max);
        } else {
            view.scroll_y = (view.scroll_y - scroll.y - scroll.x).clamp(0.0, max_scroll);
        }
        return;
    }
    if shift || scroll.x != 0.0 {
        let dx = if shift { scroll.y + scroll.x } else { scroll.x };
        view.view_start =
            (view.view_start - Tick((dx as f64 * view.ticks_per_px) as i64)).max(Tick::ZERO);
    } else if scroll.y != 0.0 {
        // The wheel zooms toward the needle (playing, the playback
        // position's), wherever the pointer is, as VEGAS's.
        view.zoom_around((-scroll.y as f64 * 0.004).exp(), app.display_time());
    }
    if zoom != 1.0 {
        view.zoom_around(1.0 / zoom as f64, app.display_time());
    }
    view.scroll_y = view.scroll_y.clamp(0.0, max_scroll);
}

/// Ctrl+Shift+wheel, `notches` of it (`frames`: Alt too, a frame a notch;
/// a pixel otherwise): the edge being trimmed in edge-trimming mode; the
/// scrub rate while it shuttles or plays; the cursor otherwise.
fn nudge(view: &mut TimelineView, app: &mut BananaApp, notches: i64, frames: bool) {
    let step = if frames {
        app.frame_rate().frame_duration()
    } else {
        Tick(view.ticks_per_px.round().max(1.0) as i64)
    };
    let d = Tick(step.0 * notches);
    if let Some(e) = view.edge_trim {
        let Some(c) = app.project().clip(e.clip).cloned() else {
            return;
        };
        if e.end {
            app.edit(|p| p.trim_end(c.id, c.end() + d));
        } else {
            app.edit(|p| p.trim_start(c.id, c.start + d));
        }
        if let Some(c) = app.project().clip(e.clip) {
            app.seek(if e.end { c.end() } else { c.start });
        }
        return;
    }
    let rate = 0.1 * notches as f64;
    let max = super::toolbar::MAX_RATE;
    if app.scrub_rate != 0.0 {
        app.scrub_rate = (app.scrub_rate + rate).clamp(-max, max);
    } else if app.is_playing() {
        app.play_rate = (app.play_rate + rate).clamp(0.1, max);
        app.restart_if_playing();
    } else if frames {
        app.step_frames(notches);
    } else {
        app.seek((app.cursor + d).max(Tick::ZERO));
    }
}

/// Auto-scroll, VEGAS's "click wheel button and move mouse": the view runs
/// toward the pointer, faster the further it is from where the wheel was
/// pressed. Held, it ends when the wheel is let go; clicked, at the next
/// click (which does nothing else).
fn autoscroll(
    view: &mut TimelineView,
    ui: &egui::Ui,
    l: &Layout,
    hovered: bool,
    pointer: Option<Pos2>,
) {
    let middle = egui::PointerButton::Middle;
    let (pressed, released, any_pressed, dt) = ui.input(|i| {
        (
            i.pointer.button_pressed(middle),
            i.pointer.button_released(middle),
            i.pointer.any_pressed(),
            i.stable_dt.min(0.1),
        )
    });
    match view.autoscroll {
        None => {
            if pressed
                && hovered
                && let Some(p) = pointer
            {
                view.autoscroll = Some(AutoScroll {
                    origin: p,
                    held: true,
                });
            }
        }
        Some(a) if a.held && released => {
            let moved = pointer.is_some_and(|p| (p - a.origin).length() > 6.0);
            view.autoscroll = (!moved).then_some(AutoScroll { held: false, ..a });
        }
        Some(a) if !a.held && any_pressed => {
            view.autoscroll = None;
            view.ignore_press = true;
        }
        Some(_) => {}
    }
    let Some(a) = view.autoscroll else { return };
    if let Some(p) = pointer {
        // Still within a few pixels; then a little faster each pixel.
        let speed = |v: f32| {
            let past = v.abs() - 8.0;
            if past <= 0.0 {
                0.0
            } else {
                past * 8.0 * v.signum()
            }
        };
        let d = p - a.origin;
        let dx = (speed(d.x) * dt) as f64 * view.ticks_per_px;
        view.view_start = (view.view_start + Tick(dx as i64)).max(Tick::ZERO);
        let max_scroll = (l.content_height - l.lanes.height() + 20.0).max(0.0);
        view.scroll_y = (view.scroll_y + speed(d.y) * dt).clamp(0.0, max_scroll);
    }
    // Where it was pressed: a ring with arrows.
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("auto-rolagem"),
    ));
    let c = a.origin;
    painter.circle(
        c,
        11.0,
        Color32::from_black_alpha(150),
        Stroke::new(1.0, Color32::from_white_alpha(200)),
    );
    for dir in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        let tip = c + dir * 8.0;
        let side = vec2(-dir.y, dir.x) * 3.0;
        painter.add(Shape::convex_polygon(
            vec![tip, tip - dir * 4.0 + side, tip - dir * 4.0 - side],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }
    ui.ctx().set_cursor_icon(CursorIcon::AllScroll);
    ui.ctx().request_repaint();
}

/// A drag's pointer near or past the tracks' edges, as VEGAS's: the view
/// runs that way (`scroll`) and the drag goes on at the edge — across for
/// what moves in time, up and down too for events, the selection
/// rectangle and envelope points. The rectangle's corner stays where it
/// was on the timeline.
fn edge_held(view: &mut TimelineView, ui: &egui::Ui, l: &Layout, p: Pos2, scroll: bool) -> Pos2 {
    let (across, down) = match &view.drag {
        Some(Drag::Move { .. } | Drag::Marquee { .. } | Drag::EnvPoint { .. }) => (true, true),
        Some(
            Drag::Level { .. } | Drag::EnvRange { .. } | Drag::ZoomBox { .. } | Drag::Slip { .. },
        )
        | None => (false, false),
        Some(_) => (true, false),
    };
    let (start0, scroll0) = (view.view_start, view.scroll_y);
    let mut q = p;
    if across {
        q.x = if scroll {
            view.edge_scroll_x(ui, l, p.x)
        } else {
            p.x.clamp(l.lanes.left(), l.lanes.right() - 1.0)
        };
    }
    if down {
        q.y = if scroll {
            view.edge_scroll_y(ui, l, p.y)
        } else {
            p.y.clamp(l.lanes.top(), l.lanes.bottom() - 1.0)
        };
    }
    let dx = ((view.view_start - start0).0 as f64 / view.ticks_per_px) as f32;
    let dy = view.scroll_y - scroll0;
    if let Some(Drag::Marquee { start, .. }) = &mut view.drag {
        *start -= vec2(dx, dy);
    }
    q
}

/// Whether `hit` is on one of an event's handles: an edge, a fade, its
/// level line or a button.
fn on_handle(hit: Hit) -> bool {
    matches!(
        hit,
        Hit::Clip {
            zone: Zone::Start
                | Zone::End
                | Zone::FadeIn
                | Zone::FadeOut
                | Zone::Level
                | Zone::SnapOffset
                | Zone::Button(_),
            ..
        }
    )
}

/// The crossfade under `t` on the track of event `clip`: the event fading
/// out there (the earlier of the two overlapping).
fn crossfade_at(project: &Project, clip: ClipId, t: Tick) -> Option<ClipId> {
    let track = project.clip_track(clip)?;
    let here: Vec<&bv_timeline::Clip> = track
        .clips
        .iter()
        .filter(|c| c.start <= t && t < c.end())
        .collect();
    if here.len() < 2 {
        return None;
    }
    here.iter().min_by_key(|c| c.start).map(|c| c.id)
}

/// Shift+click on an event, like VEGAS's: the events from the last one
/// clicked to it — on the tracks between them, starting between their
/// starts — become the selection.
fn select_range(app: &mut BananaApp, project: &Project, from: ClipId, to: ClipId) {
    let index = |id: ClipId| {
        project
            .tracks
            .iter()
            .position(|t| t.clips.iter().any(|c| c.id == id))
    };
    let (Some(a), Some(b), Some(ca), Some(cb)) =
        (index(from), index(to), project.clip(from), project.clip(to))
    else {
        app.select_clip(to, false);
        return;
    };
    let (s0, s1) = (ca.start.min(cb.start), ca.start.max(cb.start));
    app.selection.clear();
    let mut picked = std::collections::BTreeSet::new();
    for t in &project.tracks[a.min(b)..=a.max(b)] {
        for c in t.clips.iter().filter(|c| c.start >= s0 && c.start <= s1) {
            picked.insert(c.id);
            app.selection.extend(project.group_members(c.id));
        }
    }
    app.set_picked(picked);
}

/// `edit` with event grouping ignored when `ungrouped` (L and J cuts).
fn alone(p: &mut Project, ungrouped: bool, edit: impl FnOnce(&mut Project)) {
    let was = p.ignore_groups;
    p.ignore_groups |= ungrouped;
    edit(p);
    p.ignore_groups = was;
}

/// What the menu after a right-drag of events does.
#[derive(Clone, Copy, PartialEq)]
enum DropChoice {
    Move,
    Copy,
    Shuffle,
    Cancel,
}

/// Media dragged with the right mouse button and let go: VEGAS's menu, as
/// its Help lists it — Add Across Time (end to end, as with the left
/// button), Add Across Tracks (each on its own track), Add As Takes (takes
/// of the event under the pointer, or of the first one added), Video Only
/// and Audio Only (the same with only that stream) or Cancel.
fn media_drop_menu(view: &mut TimelineView, ui: &egui::Ui, app: &mut BananaApp) {
    let Some(menu) = view.media_drop_menu.clone() else {
        return;
    };
    let mut open = true;
    let mut choice: Option<(Arrange, (bool, bool))> = None;
    let mut cancel = false;
    egui::Popup::new(
        ui.id().with("menu-midia-arrasto-direito"),
        ui.ctx().clone(),
        egui::PopupAnchor::Position(menu.at),
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .style(egui::containers::menu::menu_style)
    .open_bool(&mut open)
    .show(|ui| {
        let arranges = [
            (Arrange::AcrossTime, tr("Add Across Time")),
            (Arrange::AcrossTracks, tr("Add Across Tracks")),
            (Arrange::AsTakes, tr("Add as Takes")),
        ];
        for (arrange, label) in arranges {
            if crate::menus::item(ui, label) {
                choice = Some((arrange, (true, true)));
            }
        }
        for (label, streams) in [
            (tr("Video Only"), (true, false)),
            (tr("Audio Only"), (false, true)),
        ] {
            crate::menus::sub(ui, label, true, |ui| {
                for (arrange, label) in arranges {
                    if crate::menus::item(ui, label) {
                        choice = Some((arrange, streams));
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        if crate::menus::item(ui, tr("Cancel")) {
            cancel = true;
        }
    });
    if open && choice.is_none() && !cancel {
        return;
    }
    view.media_drop_menu = None;
    if let Some((arrange, streams)) = choice {
        // Files join Project Media first.
        let mut ids = Vec::new();
        for item in menu.items {
            match item {
                DropItem::Media(id) => ids.push(id),
                DropItem::File(path) => ids.extend(app.import_paths(&[path])),
            }
        }
        app.add_dropped(ids, menu.time, menu.track, menu.clip, arrange, streams);
    }
}

/// Events dragged with the right mouse button and let go: Move Here, Copy
/// Here (the events go back, copies stay where they were let go) or
/// Cancel, as VEGAS asks.
fn drop_menu(view: &mut TimelineView, ui: &egui::Ui, app: &mut BananaApp) {
    let Some(menu) = &view.drop_menu else { return };
    let (at, clip) = (menu.at, menu.clip);
    let mut open = true;
    let mut choice = None;
    egui::Popup::new(
        ui.id().with("menu-arrasto-direito"),
        ui.ctx().clone(),
        egui::PopupAnchor::Position(at),
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .style(egui::containers::menu::menu_style)
    .open_bool(&mut open)
    .show(|ui| {
        if crate::menus::item(ui, tr("Move Here")) {
            choice = Some(DropChoice::Move);
        }
        if crate::menus::item(ui, tr("Copy Here")) {
            choice = Some(DropChoice::Copy);
        }
        if crate::menus::item(ui, tr("Shuffle Events")) {
            choice = Some(DropChoice::Shuffle);
        }
        ui.separator();
        if crate::menus::item(ui, tr("Cancel")) {
            choice = Some(DropChoice::Cancel);
        }
    });
    if open && choice.is_none() {
        return;
    }
    let Some(menu) = view.drop_menu.take() else {
        return;
    };
    match choice {
        Some(DropChoice::Move) => app.history.commit_gesture(menu.gesture),
        // Where it was let go, in its order among the others.
        Some(DropChoice::Shuffle) => match app.project().clip(clip).map(|c| c.start) {
            Some(to) => {
                app.history.preview(&menu.gesture, |p| {
                    let _ = p.shuffle_events(clip, to);
                });
                app.history.commit_gesture(menu.gesture);
            }
            None => app.history.cancel_gesture(menu.gesture),
        },
        Some(DropChoice::Copy) => {
            let before = menu.gesture.before();
            let now = app.project();
            let moved = before
                .clip(clip)
                .zip(now.clip(clip))
                .map(|(b, n)| (n.start - b.start, now.clip_track(clip).map(|t| t.id)));
            let Some((delta, to)) = moved else {
                app.history.cancel_gesture(menu.gesture);
                return;
            };
            let mut ids: Vec<ClipId> = app.selection.iter().copied().collect();
            ids.push(clip);
            let mark = menu.gesture.before().id_counter();
            app.history.preview(&menu.gesture, |p| {
                p.copy_here(&ids, clip, delta, to);
            });
            app.history.commit_gesture(menu.gesture);
            app.ask_generated_copies(mark);
        }
        _ => app.history.cancel_gesture(menu.gesture),
    }
}

pub(crate) fn interact(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
) {
    let project = app.project().clone();
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let resp = ui.interact(l.lanes, ui.id().with("faixas"), Sense::click_and_drag());
    if ui.rect_contains_pointer(l.lanes) || ui.rect_contains_pointer(l.headers) {
        wheel(view, ui, app, l, pointer);
    }
    // No selection rectangle: nothing it began from.
    if !matches!(view.drag, Some(Drag::Marquee { .. })) {
        view.box_base = None;
    }
    // A right-drag's menu waits for its answer.
    if view.drop_menu.is_some() {
        drop_menu(view, ui, app);
        return;
    }
    if view.media_drop_menu.is_some() {
        media_drop_menu(view, ui, app);
        return;
    }
    autoscroll(view, ui, l, resp.hovered(), pointer);
    let modifiers = ui.input(|i| i.modifiers);
    // A fade's handle held: its pointer, as over it.
    if let (Some(Drag::FadeIn { .. } | Drag::FadeOut { .. }), Some(p)) = (&view.drag, pointer) {
        fade_cursor(ui.ctx(), p, matches!(view.drag, Some(Drag::FadeIn { .. })));
    }

    // Envelope under the pointer: cursor and tooltip, its point a white
    // diamond, as VEGAS's.
    let env_tool = matches!(app.tool, Tool::Edit | Tool::Envelope);
    let env_hover = pointer
        .filter(|_| env_tool && resp.hovered() && app.media_drag.is_none())
        .and_then(|p| super::envelopes::hit(view, l, &project, p));
    if let (Some(h), Some(p)) = (env_hover, pointer)
        && view.drag.is_none()
    {
        ui.ctx().set_cursor_icon(if h.point.is_some() {
            CursorIcon::Move
        } else {
            CursorIcon::ResizeVertical
        });
        // Not over a menu (a point's own, opened there).
        if !egui::Popup::is_any_open(ui.ctx()) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("valor-envelope"),
            ));
            if h.point.is_some()
                && let Some(r) = super::envelopes::lane_for(view, l, &project, h.track, h.clip)
            {
                let at = pos2(
                    view.x_of(l, h.at),
                    super::envelopes::y_of(r, h.kind, h.value),
                );
                super::envelopes::paint_hot_point(&painter.with_clip_rect(l.lanes), at);
            }
            let time = app.ruler_text(h.at);
            super::envelopes::paint_value(&painter, &project, p, h.owner(), h.kind, &time, h.value);
        }
    }
    // The event's level line: its value next to the pointer while it is
    // hovered or dragged ("Opacity is 52 %", "Gain is −4.1 dB").
    let level_clip = match (&view.drag, pointer) {
        (Some(Drag::Level { clip, .. }), _) => Some(*clip),
        (None, Some(p)) if resp.hovered() && app.media_drag.is_none() => {
            match hit(view, l, &project, p) {
                Hit::Clip {
                    clip,
                    zone: Zone::Level,
                } => Some(clip),
                _ => None,
            }
        }
        _ => None,
    };
    if let (Some(id), Some(p)) = (level_clip, pointer)
        && let Some(c) = app.project().clip(id)
        && let Some(kind) = app.project().clip_track(id).map(|t| t.kind)
    {
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("nivel-do-evento"),
        ));
        paint_level_value(&painter, p, kind, c.level);
    }
    // The snap offset flag: how far it is from the event's start.
    let flag_clip = match (&view.drag, pointer) {
        (Some(Drag::SnapOffset { clip }), _) => Some(*clip),
        (None, Some(p)) if resp.hovered() && app.media_drag.is_none() => {
            match hit(view, l, &project, p) {
                Hit::Clip {
                    clip,
                    zone: Zone::SnapOffset,
                } => Some(clip),
                _ => None,
            }
        }
        _ => None,
    };
    if let (Some(id), Some(p)) = (flag_clip, pointer)
        && let Some(c) = app.project().clip(id)
    {
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("deslocamento-de-encaixe"),
        ));
        let rate = app.frame_rate();
        let text = trf!(
            "Snap offset: {}",
            bv_core::timecode::format(c.snap_point(), rate)
        );
        paint_tip(&painter, p, text);
    }
    // A point dragged: the diamond and the tooltip go with it.
    if let (
        Some(Drag::EnvPoint {
            track,
            clip,
            kind,
            index,
            ..
        }),
        Some(p),
    ) = (&view.drag, pointer)
        && let Some(r) = super::envelopes::lane_for(view, l, &project, *track, *clip)
    {
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("valor-envelope"),
        ));
        let owner = super::envelopes::owner(*track, *clip);
        let point = index.and_then(|i| {
            super::envelopes::envelope_of(&project, owner, *kind)?
                .points
                .get(i)
                .copied()
        });
        let (at, v) = match point {
            Some(pt) => (
                super::envelopes::timeline_time(&project, owner, pt.at),
                pt.value,
            ),
            None => (
                view.time_at(l, p.x).max(Tick::ZERO),
                super::envelopes::value_at_y(r, *kind, p.y),
            ),
        };
        let spot = pos2(view.x_of(l, at), super::envelopes::y_of(r, *kind, v));
        super::envelopes::paint_hot_point(&painter.with_clip_rect(l.lanes), spot);
        let time = app.ruler_text(at);
        super::envelopes::paint_value(&painter, &project, p, owner, *kind, &time, v);
    }
    // Its line dragged: the value on it where the pointer is.
    if let (
        Some(Drag::EnvSegment {
            track, clip, kind, ..
        }),
        Some(p),
    ) = (&view.drag, pointer)
    {
        let owner = super::envelopes::owner(*track, *clip);
        let at = view.time_at(l, p.x).max(Tick::ZERO);
        if let Some(v) = super::envelopes::envelope_of(&project, owner, *kind)
            .map(|e| e.value_at(super::envelopes::local(&project, owner, at)))
        {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("valor-envelope"),
            ));
            let time = app.ruler_text(at);
            super::envelopes::paint_value(&painter, &project, p, owner, *kind, &time, v);
        }
    }
    // Options › Preferences › General: "Double Click event edge to toggle
    // Expanded Edit Mode".
    let edge = resp
        .double_clicked()
        .then(|| resp.interact_pointer_pos())
        .flatten()
        .map(|p| hit(view, l, &project, p));
    // A point a double-click put on an envelope: where the cursor goes.
    let mut new_point = None;
    if let Some(Hit::Clip {
        clip,
        zone: zone @ (Zone::Start | Zone::End),
    }) = edge
        && app.settings.prefs.edge_expanded_edit
        && !matches!(app.tool, Tool::Envelope | Tool::Zoom)
    {
        let end = zone == Zone::End;
        match view.edge_trim {
            Some(e) if e.expanded && e.clip == clip && e.end == end => view.edge_trim = None,
            _ => view.trim_edge(&project, clip, end, true),
        }
    } else if resp.double_clicked()
        && env_tool
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(h) = super::envelopes::hit(view, l, &project, p)
        && h.point.is_none()
    {
        let at = view.snap(app, &project, h.at, &[Tick::ZERO], &[]);
        let owner = h.owner();
        let added = app.edit(|pr| {
            let at = super::envelopes::local(pr, owner, at);
            pr.set_envelope_point(owner, h.kind, at, h.value)
        });
        new_point = added
            .and_then(|i| {
                super::envelopes::envelope_of(app.project(), owner, h.kind)?
                    .points
                    .get(i)
            })
            .map(|pt| super::envelopes::timeline_time(app.project(), owner, pt.at));
    } else if resp.double_clicked()
        && app.tool == Tool::Edit
        && let Some(p) = resp.interact_pointer_pos()
        && let Hit::Clip {
            clip,
            zone: Zone::Body | Zone::Level,
        } = hit(view, l, &project, p)
        && let Some(c) = project.clip(clip)
    {
        // A double-click on an event selects its time, like VEGAS's.
        app.set_loop_region(Some(bv_core::TimeRange::new(c.start, c.duration)));
    }

    // Cursor do mouse conforme a zona.
    if view.drag.is_none()
        && env_hover.is_none()
        && resp.hovered()
        && app.media_drag.is_none()
        && let Some(p) = pointer
    {
        let prefs = &app.settings.prefs;
        let needle = prefs.cursor_drag
            && !modifiers.shift
            && !modifiers.alt
            && matches!(app.tool, Tool::Edit | Tool::Envelope)
            && !on_handle(hit(view, l, &project, p))
            && view.needle_line(app, l, p.x).is_some();
        // The modifiers pick another tool for the moment, like VEGAS's:
        // Alt slips, Ctrl+Alt slides, Ctrl+Alt+Shift splits.
        let m = modifiers;
        let tool = match app.tool {
            Tool::Edit if m.command && m.alt && m.shift => Tool::SplitTrim,
            Tool::Edit if m.command && m.alt => Tool::Slide,
            Tool::Edit if m.alt => Tool::Slip,
            t => t,
        };
        let icon = match (tool, hit(view, l, &project, p)) {
            _ if needle => Some(CursorIcon::ResizeHorizontal),
            (
                _,
                Hit::Clip {
                    zone: Zone::Button(_),
                    ..
                },
            ) => Some(CursorIcon::PointingHand),
            (Tool::Zoom, _) => Some(CursorIcon::ZoomIn),
            (Tool::Select, _) => Some(CursorIcon::Crosshair),
            (
                _,
                Hit::Clip {
                    zone: Zone::SnapOffset,
                    ..
                },
            ) => Some(CursorIcon::ResizeHorizontal),
            (
                _,
                Hit::Clip {
                    zone: Zone::Start | Zone::End,
                    ..
                },
            ) if !matches!(app.tool, Tool::Envelope) => Some(CursorIcon::ResizeHorizontal),
            (
                _,
                Hit::Clip {
                    zone: zone @ (Zone::FadeIn | Zone::FadeOut),
                    ..
                },
            ) => {
                fade_cursor(ui.ctx(), p, zone == Zone::FadeIn);
                None
            }
            (
                _,
                Hit::Clip {
                    zone: Zone::Level, ..
                },
            ) => Some(CursorIcon::ResizeVertical),
            (
                Tool::Edit | Tool::Shuffle,
                Hit::Clip {
                    zone: Zone::Body, ..
                },
            ) => Some(CursorIcon::Grab),
            (
                Tool::Slip,
                Hit::Clip {
                    zone: Zone::Body, ..
                },
            ) => Some(CursorIcon::AllScroll),
            (
                Tool::Slide,
                Hit::Clip {
                    zone: Zone::Body, ..
                },
            ) => Some(CursorIcon::ResizeColumn),
            (
                Tool::Stretch,
                Hit::Clip {
                    zone: Zone::Body, ..
                },
            ) => Some(CursorIcon::ResizeHorizontal),
            (
                Tool::SplitTrim,
                Hit::Clip {
                    zone: Zone::Body, ..
                },
            ) => Some(CursorIcon::VerticalText),
            (Tool::Envelope, Hit::Clip { .. }) => Some(CursorIcon::ResizeVertical),
            _ => None,
        };
        if let Some(i) = icon {
            ui.ctx().set_cursor_icon(i);
        }
    }

    // The wheel held: auto-scroll, nothing else.
    let middle = ui.input(|i| i.pointer.middle_down());
    // An edge under the pointer: what dragging it does, as VEGAS tells.
    if view.drag.is_none()
        && app.media_drag.is_none()
        && let Some(p) = pointer
        && resp.hovered()
        && matches!(app.tool, Tool::Edit)
        && let Hit::Clip {
            zone: zone @ (Zone::Start | Zone::End),
            ..
        } = hit(view, l, &project, p)
    {
        let (title, what) = if zone == Zone::End {
            (
                tr("Trim Event End"),
                tr("Drag to adjust the ending point of the event."),
            )
        } else {
            (
                tr("Trim Event Start"),
                tr("Drag to adjust the starting point of the event."),
            )
        };
        resp.clone().on_hover_ui(|ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.label(what);
        });
    }
    // A needle grabbed by its line follows the mouse from the press, as
    // VEGAS's (egui's drag starts only some pixels away).
    if view.drag.is_none()
        && app.media_drag.is_none()
        && !middle
        && !view.ignore_press
        && !modifiers.shift
        && !modifiers.alt
        && app.settings.prefs.cursor_drag
        && matches!(app.tool, Tool::Edit | Tool::Envelope)
        && resp.is_pointer_button_down_on()
        && ui.input(|i| i.pointer.primary_pressed())
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && !on_handle(hit(view, l, &project, origin))
        // An envelope's point or line under it comes first.
        && super::envelopes::hit(view, l, &project, origin).is_none()
        && let Some(n) = view.needle_line(app, l, origin.x)
    {
        if n == super::Needle::Playhead {
            app.hold_playback();
        }
        view.drag = Some(Drag::Needle(n));
    }
    let needle_held = matches!(view.drag, Some(Drag::Needle(_)));
    if resp.drag_started()
        && !needle_held
        && app.media_drag.is_none()
        && !middle
        && !view.ignore_press
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        // Options › Preferences › Editing: "Use right mouse button to
        // switch to the Selection Edit Tool": a right-drag selects the
        // events in its rectangle (a right-click still opens the menu).
        let right = ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
        view.right_drag = right;
        // The event clicked just before, pressed again and dragged: the
        // selection grows from it (its time selected first, as a
        // double-click selects it).
        let again = ui
            .input(|i| i.pointer.press_start_time())
            .zip(view.event_click);
        let extend = match (hit(view, l, &project, origin), again) {
            (
                Hit::Clip {
                    clip,
                    zone: Zone::Body | Zone::Level,
                },
                Some((pressed, (last, at))),
            ) if !right
                && app.tool == Tool::Edit
                && !modifiers.any()
                && last == clip
                && pressed - at < DOUBLE_PRESS =>
            {
                Some(clip)
            }
            _ => None,
        };
        if let Some(clip) = extend
            && let Some(c) = project.clip(clip)
        {
            let r = bv_core::TimeRange::new(c.start, c.duration);
            app.modify_merged(egui::Id::new(("extend-selection", clip.0)), |p| {
                p.loop_region = Some(r);
            });
            view.drag = Some(Drag::Extend { clip });
        } else if right && app.settings.prefs.right_button_selection {
            view.drag = Some(Drag::Marquee {
                start: origin,
                add: modifiers.command,
                shape: BoxShape::Free,
            });
        } else {
            start_drag(view, app, l, &project, origin, modifiers, right);
        }
    }
    if let Some(Drag::Marquee { shape, .. }) = &mut view.drag
        && ui.input(|i| {
            i.pointer.button_pressed(egui::PointerButton::Secondary) && i.pointer.primary_down()
        })
    {
        *shape = shape.next();
        view.box_click = true;
    }
    let needle_held = matches!(view.drag, Some(Drag::Needle(_)));
    if (resp.dragged() || (needle_held && resp.is_pointer_button_down_on()))
        && let Some(p) = pointer
    {
        let p = edge_held(view, ui, l, p, true);
        update_drag(view, app, l, p, modifiers.command);
    }
    if resp.drag_stopped() || (needle_held && !ui.input(|i| i.pointer.any_down())) {
        let held = pointer.map(|p| edge_held(view, ui, l, p, false));
        finish_drag(view, app, l, held);
    }
    // A transition's label: its icon (or a double-click on it) opens the
    // transition's settings, as VEGAS's Transition Properties.
    let mut on_label = false;
    if let Some(p) = pointer.filter(|_| resp.hovered() && view.drag.is_none()) {
        for t in transition_labels(view, l, &project) {
            let on_icon = t.icon.is_some_and(|i| i.contains(p));
            if on_icon {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if t.icon.is_some()
                && ((on_icon && resp.clicked()) || (t.band.contains(p) && resp.double_clicked()))
            {
                app.open_transition_window(t.clip);
                on_label = true;
            }
        }
    }
    let cursor_before = app.cursor;
    if resp.clicked()
        && !view.ignore_press
        && !on_label
        && let Some(p) = resp.interact_pointer_pos()
    {
        // Noted for a double-click and drag.
        view.event_click = match hit(view, l, &project, p) {
            Hit::Clip {
                clip,
                zone: Zone::Body | Zone::Level,
            } => Some((clip, ui.input(|i| i.time))),
            _ => None,
        };
        click(view, app, l, &project, p, modifiers);
    }
    // Empty spots, as VEGAS's: a double-click selects the gap between two
    // events, or puts the cursor at the project's end past the last one; a
    // third click selects the whole project.
    if (resp.double_clicked() || resp.triple_clicked())
        && app.tool == Tool::Edit
        && !view.ignore_press
        && modifiers.is_none()
        && new_point.is_none()
        && let Some(p) = resp.interact_pointer_pos()
    {
        empty_clicks(
            view,
            app,
            l,
            &project,
            p,
            resp.triple_clicked(),
            cursor_before,
        );
    }
    // The cursor goes to the point the double-click put on an envelope.
    if let Some(at) = new_point {
        app.seek(at);
    }
    if !ui.input(|i| i.pointer.any_down()) {
        view.ignore_press = false;
    }
    if resp.secondary_clicked()
        && !view.box_click
        && let Some(p) = resp.interact_pointer_pos()
    {
        if app.tool == Tool::Zoom {
            let t = view.time_at(l, p.x);
            view.zoom_at(t, p.x - l.lanes.left(), 2.0);
        } else if let Some(h) = super::ofx_envelopes::hit(view, l, &project, &app.settings.prefs, p)
            .filter(|_| view.show_ofx_envelopes)
        {
            view.ofx_menu = Some(h);
            view.context = ContextTarget::OfxEnvelope;
        } else if let Some(track) = super::ofx_envelopes::area_at(view, l, &project, p) {
            view.context = ContextTarget::OfxArea(track);
        } else if let Some(h) = super::envelopes::hit(view, l, &project, p)
            .filter(|_| matches!(app.tool, Tool::Edit | Tool::Envelope))
        {
            view.context = ContextTarget::Envelope(h);
        } else {
            view.context = match hit(view, l, &project, p) {
                Hit::Clip { clip, zone } => {
                    if !app.selection.contains(&clip) {
                        app.select_clip(clip, false);
                    }
                    // On a fade or a crossfade, VEGAS's popup of curves.
                    let button = matches!(zone, Zone::Button(_));
                    match (
                        fade_at(view, l, &project, clip, p),
                        crossfade_under(view, l, &project, p),
                    ) {
                        (Some(out), _) if !button => ContextTarget::Fade { clip, out },
                        (_, Some((out, inn))) if !button => ContextTarget::Crossfade { out, inn },
                        _ => ContextTarget::Clip(clip),
                    }
                }
                Hit::Track { track } => ContextTarget::Track(track),
                Hit::Nothing => ContextTarget::Empty,
            };
        }
    }
    if app.tool != Tool::Zoom && !view.box_click {
        resp.context_menu(|ui| context_menu(view, ui, app));
    }
    if !ui.input(|i| i.pointer.any_down()) {
        view.box_click = false;
    }
    super::envelopes::set_to_ui(view, ui, app, l, &project);
    drop_media(view, ui, app, l, &project, pointer);
}

/// A double-click (`triple`: a third click) on an empty spot: the gap
/// between the events around it becomes the time selection, past the
/// last event the cursor goes to the project's end; a third click selects
/// the whole project, the cursor staying where it was (`cursor`).
fn empty_clicks(
    view: &TimelineView,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    p: Pos2,
    triple: bool,
    cursor: Tick,
) {
    let track = match hit(view, l, project, p) {
        Hit::Clip { .. } => return,
        Hit::Track { track } => Some(track),
        Hit::Nothing => None,
    };
    let end = project.duration();
    if triple {
        if end > Tick::ZERO {
            app.set_loop_region(Some(bv_core::TimeRange::new(Tick::ZERO, end)));
        }
        app.seek(cursor);
        return;
    }
    let t = view.time_at(l, p.x).max(Tick::ZERO);
    match track.and_then(|id| gap_at(project, id, t)) {
        Some(gap) => app.set_loop_region(Some(gap)),
        None => app.seek_playback(end),
    }
}

/// The gap around `t` on the track: from the end of the event before it
/// (or the start) to the start of the event after it (`None` past the last
/// one).
fn gap_at(project: &Project, track: bv_timeline::TrackId, t: Tick) -> Option<bv_core::TimeRange> {
    let track = project.track(track)?;
    let before = track
        .clips
        .iter()
        .filter(|c| c.end() <= t)
        .map(|c| c.end())
        .max()
        .unwrap_or(Tick::ZERO);
    let after = track
        .clips
        .iter()
        .filter(|c| c.start >= t)
        .map(|c| c.start)
        .min()?;
    (after > before).then(|| bv_core::TimeRange::from_bounds(before, after))
}

fn start_drag(
    view: &mut TimelineView,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    origin: Pos2,
    m: egui::Modifiers,
    right: bool,
) {
    let (add, shift, alt) = (m.command, m.shift, m.alt);
    let t = view.time_at(l, origin.x).max(Tick::ZERO);
    if let Hit::Clip {
        zone: Zone::Button(_),
        ..
    } = hit(view, l, project, origin)
    {
        return;
    }
    // The needles: grabbed by their lines, over events too (VEGAS says to
    // press an event away from the cursor to move it); Ctrl+drag elsewhere
    // scrubs, like VEGAS's cursor-style scrub (over events when the
    // preference allows it). Shift makes a time selection instead; Alt
    // slips.
    let prefs = &app.settings.prefs;
    let over_event = matches!(hit(view, l, project, origin), Hit::Clip { .. });
    // An event's handles (edges, fades, level) come first: after a split
    // at the cursor its new edges are trimmed there; and an envelope's
    // points and line, as the pointer over them shows.
    let handle = on_handle(hit(view, l, project, origin))
        || (matches!(app.tool, Tool::Edit | Tool::Envelope)
            && super::envelopes::hit(view, l, project, origin).is_some());
    if !right && !shift && !alt && matches!(app.tool, Tool::Edit | Tool::Envelope) {
        let grabbed = if prefs.cursor_drag && !handle {
            view.needle_line(app, l, origin.x)
        } else {
            None
        };
        let scrub = add && (!over_event || prefs.ctrl_scrub_over_events);
        let needle = grabbed.or(scrub.then_some(super::Needle::Cursor));
        if let Some(n) = needle {
            if n == super::Needle::Playhead {
                app.hold_playback();
            } else if grabbed.is_none() {
                // Cursor-style scrub: the cursor comes to the mouse.
                app.seek(view.snap_cursor(app, project, t));
            }
            view.drag = Some(Drag::Needle(n));
            return;
        }
    }
    if !right
        && matches!(app.tool, Tool::Edit | Tool::Envelope)
        && let Some(h) = super::envelopes::hit(view, l, project, origin)
    {
        view.gesture = Some(app.history.begin_gesture());
        // Shift+drag over the line: the envelope brush, as VEGAS's.
        if shift && h.point.is_none() && h.clip.is_none() {
            view.drag = Some(Drag::EnvBrush {
                track: h.track,
                kind: h.kind,
                samples: vec![(h.at, h.value)],
            });
            return;
        }
        // The line within the time selection: the selection's part moves,
        // with fades at its edges.
        let prefs = &app.settings.prefs;
        if h.point.is_none()
            && h.clip.is_none()
            && prefs.envelope_fades
            && let Some(sel) = project
                .loop_region
                .filter(|r| r.duration > Tick::ZERO && r.contains(h.at))
        {
            let fade = Tick::from_seconds_f64(prefs.envelope_fade_ms as f64 / 1000.0);
            let (outer, inner) = prefs.envelope_fade_align.ranges(sel, fade);
            view.drag = Some(Drag::EnvRange {
                track: h.track,
                kind: h.kind,
                y0: origin.y,
                outer,
                inner,
            });
            return;
        }
        let owner = h.owner();
        view.drag = Some(match h.point {
            Some(i) => Drag::EnvPoint {
                track: h.track,
                clip: h.clip,
                kind: h.kind,
                index: Some(i),
                origin: (h.at, h.value),
            },
            None => Drag::EnvSegment {
                track: h.track,
                clip: h.clip,
                kind: h.kind,
                points: super::envelopes::segment(
                    project,
                    owner,
                    h.kind,
                    super::envelopes::local(project, owner, h.at),
                ),
                y0: origin.y,
                moved: false,
            },
        });
        return;
    }
    let kind = match app.tool {
        Tool::Select => Some(Drag::Marquee {
            start: origin,
            add,
            shape: BoxShape::Free,
        }),
        Tool::Zoom => Some(Drag::ZoomBox { start: origin }),
        Tool::Edit
        | Tool::Shuffle
        | Tool::Envelope
        | Tool::Slip
        | Tool::Slide
        | Tool::Stretch
        | Tool::SplitTrim => match hit(view, l, project, origin) {
            // Ctrl+Shift+drag on an event: a time selection, the event
            // staying, like VEGAS's.
            Hit::Clip { .. } if add && shift && !alt && !right => {
                let anchor = view.snap(app, project, t, &[Tick::ZERO], &[]);
                Some(Drag::TimeSelect { anchor })
            }
            Hit::Clip { clip, zone } => {
                if !app.selection.contains(&clip) {
                    app.select_clip(clip, add);
                }
                view.range_anchor = Some(clip);
                let c = project.clip(clip).expect("clipe atingido existe");
                let edit = app.tool == Tool::Edit;
                let edge = matches!(zone, Zone::Start | Zone::End);
                let end = zone == Zone::End;
                let edge0 = if end { c.end() } else { c.start };
                let rate = project.settings.frame_rate;
                let envelope = app.tool == Tool::Envelope;
                let level_drag = || {
                    let row = l
                        .rows
                        .iter()
                        .find(|r| Some(r.track) == project.clip_track(clip).map(|t| t.id))
                        .expect("trilha do clipe");
                    Drag::Level {
                        clip,
                        level0: c.level,
                        y0: origin.y,
                        span: row.height - 8.0,
                    }
                };
                Some(match zone {
                    Zone::Button(_) => return,
                    // The right button moves events (a menu asks how when
                    // they are let go).
                    _ if right => Drag::Move {
                        clip,
                        grab: t - c.start,
                    },
                    Zone::FadeIn => Drag::FadeIn { clip },
                    Zone::FadeOut => Drag::FadeOut { clip },
                    Zone::Level => level_drag(),
                    Zone::SnapOffset => Drag::SnapOffset { clip },
                    _ if envelope => level_drag(),
                    // VEGAS's modifiers on events: Ctrl+Alt+Shift splits
                    // there and trims (on an edge, trims the adjacent
                    // event, groups ignored); Ctrl+Alt trims the adjacent
                    // event, slides a crossfade or the event; Alt+Shift
                    // slip-trims the end; Alt slip-trims an edge or slips
                    // the media.
                    _ if edit && add && alt && shift && edge => Drag::TrimAdjacent {
                        clip,
                        end,
                        t0: t,
                        edge0,
                        ungrouped: true,
                    },
                    _ if edit && add && alt && shift => Drag::SplitTrim {
                        clip,
                        at: rate.snap_floor(t),
                    },
                    _ if edit && add && alt && edge => Drag::TrimAdjacent {
                        clip,
                        end,
                        t0: t,
                        edge0,
                        ungrouped: false,
                    },
                    _ if edit && add && alt => match crossfade_at(project, clip, t) {
                        Some(a) => Drag::TrimAdjacent {
                            clip: a,
                            end: true,
                            t0: t,
                            edge0: project.clip(a).map_or(t, |c| c.end()),
                            ungrouped: false,
                        },
                        None => Drag::Slide { clip, t0: t },
                    },
                    _ if edit && alt && shift => Drag::SlipTrim {
                        clip,
                        end: true,
                        t0: t,
                        edge0: c.end(),
                    },
                    _ if edit && alt && edge => Drag::SlipTrim {
                        clip,
                        end,
                        t0: t,
                        edge0,
                    },
                    _ if edit && alt => Drag::Slip { clip, t0: t },
                    Zone::End | Zone::Body if app.tool == Tool::Stretch => Drag::Stretch {
                        clip,
                        rate0: c.rate,
                        dur0: c.duration,
                        from_start: false,
                    },
                    // Ctrl+drag of an edge stretches the event, like VEGAS's.
                    Zone::Start | Zone::End if add && app.tool == Tool::Edit => Drag::Stretch {
                        clip,
                        rate0: c.rate,
                        dur0: c.duration,
                        from_start: zone == Zone::Start,
                    },
                    Zone::Body if app.tool == Tool::Slip => Drag::Slip { clip, t0: t },
                    Zone::Body if app.tool == Tool::Slide => Drag::Slide { clip, t0: t },
                    Zone::Body => Drag::Move {
                        clip,
                        grab: t - c.start,
                    },
                    // Shift: the event alone, its group left (L and J cuts).
                    Zone::Start => Drag::TrimStart {
                        clip,
                        ungrouped: shift && edit,
                    },
                    Zone::End => Drag::TrimEnd {
                        clip,
                        ungrouped: shift && edit,
                    },
                })
            }
            // An empty spot, as VEGAS's: dragging draws the selection
            // rectangle; with the right button, a time selection.
            Hit::Track { .. } | Hit::Nothing if right => {
                let anchor = view.snap(app, project, t, &[Tick::ZERO], &[]);
                Some(Drag::TimeSelect { anchor })
            }
            Hit::Track { .. } | Hit::Nothing => Some(Drag::Marquee {
                start: origin,
                add,
                shape: BoxShape::Free,
            }),
        },
    };
    if let Some(kind) = kind {
        let needs_gesture = matches!(
            kind,
            Drag::Move { .. }
                | Drag::SnapOffset { .. }
                | Drag::TrimStart { .. }
                | Drag::TrimEnd { .. }
                | Drag::Level { .. }
                | Drag::FadeIn { .. }
                | Drag::FadeOut { .. }
                | Drag::Slip { .. }
                | Drag::Slide { .. }
                | Drag::Stretch { .. }
                | Drag::EnvPoint { .. }
                | Drag::SlipTrim { .. }
                | Drag::TrimAdjacent { .. }
                | Drag::SplitTrim { .. }
        );
        view.gesture = needs_gesture.then(|| app.history.begin_gesture());
        view.drag = Some(kind);
    }
}

/// Where an event's media starts and ends on the timeline near `t`, like
/// VEGAS's: a dragged edge sticks there before running past the media (Loop
/// repeats it).
/// Where the media of event `id` starts and ends around `t`, for an edge
/// dragged there to snap to; with `markers`, its media markers too.
fn media_edges(
    project: &Project,
    id: bv_timeline::ClipId,
    t: Tick,
    markers: bool,
) -> Vec<(Tick, SnapKind)> {
    let Some(c) = project.clip(id) else {
        return Vec::new();
    };
    let Some(info) = project.media(c.media) else {
        return Vec::new();
    };
    let len = info.duration.0;
    if info.still || c.freeze.is_some() || len <= 0 {
        return Vec::new();
    }
    // The laps of the media around the pointer.
    let k = c.source_time(t).0.div_euclid(len);
    let mut offsets = vec![(Tick::ZERO, SnapKind::Event)];
    if markers {
        for m in &info.markers {
            let kind = if m.is_region() {
                SnapKind::Region
            } else {
                SnapKind::Marker
            };
            offsets.push((m.time, kind));
            if m.is_region() {
                offsets.push((m.end(), kind));
            }
        }
    }
    (k - 1..=k + 2)
        .flat_map(|k| {
            offsets.iter().map(move |&(o, kind)| {
                let at = c.start + c.to_timeline(Tick(k * len) + o - c.source_in);
                (at, kind)
            })
        })
        .filter(|&(t, _)| t >= Tick::ZERO)
        .collect()
}

/// How far an event's media goes on past its end (`after`) or before its
/// start: the frames there are to trim to, on the timeline. Media that
/// repeats or lasts forever has as many as are shown (`cap`).
fn spare_media(c: &bv_timeline::Clip, info: Option<&MediaInfo>, after: bool, cap: Tick) -> Tick {
    let Some(info) = info else {
        return Tick::ZERO;
    };
    let endless = info.still
        || matches!(info.source, MediaSource::Generated(_))
        || c.looped
        || c.freeze.is_some();
    if endless {
        return cap;
    }
    let spare = if after {
        info.duration - c.source_time(c.end())
    } else {
        c.source_in
    };
    c.to_timeline(spare.max(Tick::ZERO)).min(cap)
}

/// Edge-trimming and Expanded Edit Mode: the red bracket on the edge being
/// trimmed and, expanded, the two rolls with the frames there are beyond
/// the edit point.
fn paint_edge_trim(
    painter: &egui::Painter,
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    prefs: &crate::settings::Prefs,
) {
    let Some(e) = view.edge_trim else {
        return;
    };
    let (Some(row), Some(track), Some(c)) = (
        l.rows.iter().find(|r| r.track == e.track),
        project.track(e.track),
        project.clip(e.clip),
    ) else {
        return;
    };
    let red = Color32::from_rgb(0xE8, 0x2A, 0x2A);
    if e.expanded {
        // The rolls: a line between them and their letters.
        let mid = row.top + row.height / 2.0;
        painter.hline(
            l.lanes.x_range(),
            mid,
            Stroke::new(1.0, Color32::from_black_alpha(160)),
        );
        for (letter, y) in [("A", row.top + 3.0), ("B", mid + 3.0)] {
            painter.text(
                pos2(l.lanes.left() + 4.0, y),
                Align2::LEFT_TOP,
                letter,
                FontId::proportional(11.0),
                Color32::from_white_alpha(120),
            );
        }
        // The frames there are past the edit point: the event before can
        // go on after it, the one after can start before it.
        let cap = Tick((view.ticks_per_px * 160.0) as i64);
        let ghost = |a: Tick, b: Tick, r: Rect| {
            let g = Rect::from_min_max(
                pos2(view.x_of(l, a), r.top()),
                pos2(view.x_of(l, b), r.bottom()),
            );
            if g.width() >= 1.0 {
                let color = event_fill(prefs.color_of(track));
                painter.rect_filled(g, 2.0, color.gamma_multiply(0.35));
                painter.rect_stroke(
                    g,
                    2.0,
                    Stroke::new(1.0, color.gamma_multiply(0.8)),
                    StrokeKind::Inside,
                );
            }
        };
        let (before, after) = edit_point_events(project, e);
        if let Some(a) = before.and_then(|id| project.clip(id)) {
            let r = clip_rect(view, l, row, a.start, a.end());
            let spare = spare_media(a, project.media(a.media), true, cap);
            ghost(a.end(), a.end() + spare, r);
        }
        if let Some(b) = after.and_then(|id| project.clip(id)) {
            let r = clip_rect(view, l, row, b.start, b.end());
            let spare = spare_media(b, project.media(b.media), false, cap);
            ghost(b.start - spare, b.start, r);
        }
    }
    // The red bracket: "]" at an end, "[" at a start, just inside the
    // event (the cursor line stands on the edge itself).
    let r = clip_rect(view, l, row, c.start, c.end());
    let (x, arm) = if e.end {
        (r.right() - 2.0, -8.0)
    } else {
        (r.left() + 2.0, 8.0)
    };
    let stroke = Stroke::new(4.0, red);
    painter.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], stroke);
    painter.line_segment(
        [pos2(x, r.top() + 1.0), pos2(x + arm, r.top() + 1.0)],
        stroke,
    );
    painter.line_segment(
        [pos2(x, r.bottom() - 1.0), pos2(x + arm, r.bottom() - 1.0)],
        stroke,
    );
}

/// The events on either side of an edit point: the one before it (the A
/// roll's, ending there) and the one after it (the B roll's, starting
/// there).
pub(crate) fn edit_point_events(
    project: &Project,
    e: super::EdgeTrim,
) -> (Option<ClipId>, Option<ClipId>) {
    let Some(track) = project.track(e.track) else {
        return (None, None);
    };
    let Some(c) = project.clip(e.clip) else {
        return (None, None);
    };
    if e.end {
        let next = track
            .clips
            .iter()
            .filter(|o| o.id != c.id && o.start > c.start)
            .min_by_key(|o| o.start)
            .map(|o| o.id);
        (Some(c.id), next)
    } else {
        let prev = track
            .clips
            .iter()
            .filter(|o| o.id != c.id && o.start < c.start)
            .max_by_key(|o| o.start)
            .map(|o| o.id);
        (prev, Some(c.id))
    }
}

/// The media markers and regions of `clip` in its rectangle `r`: a line at
/// each marker, a band under each region (with names, View › Event Media
/// Markers › Show Markers and Labels).
fn paint_media_markers(
    cp: &egui::Painter,
    view: &TimelineView,
    l: &Layout,
    r: Rect,
    clip: &bv_timeline::Clip,
    info: &bv_timeline::MediaInfo,
) {
    let marker = theme::marker().gamma_multiply(0.9);
    for m in &info.markers {
        for t in clip.media_points(m.time, info.duration) {
            let x = view.x_of(l, t);
            let color = if m.is_region() {
                let end = (t + clip.to_timeline(m.length)).min(clip.end());
                let x1 = view.x_of(l, end);
                cp.rect_filled(
                    Rect::from_min_max(pos2(x, r.bottom() - 4.0), pos2(x1, r.bottom() - 1.0)),
                    0.0,
                    theme::region().gamma_multiply(0.8),
                );
                cp.line_segment(
                    [pos2(x1, r.top() + 2.0), pos2(x1, r.bottom())],
                    Stroke::new(1.0, theme::region().gamma_multiply(0.6)),
                );
                theme::region()
            } else {
                marker
            };
            cp.line_segment(
                [pos2(x, r.top() + 2.0), pos2(x, r.bottom())],
                Stroke::new(1.0, color),
            );
            if view.media_marker_labels && !m.name.is_empty() {
                cp.text(
                    pos2(x + 3.0, r.bottom() - 5.0),
                    Align2::LEFT_BOTTOM,
                    &m.name,
                    FontId::proportional(10.5),
                    color,
                );
            }
        }
    }
}

/// The one-second blocks of audio samples a zoomed-in waveform is drawn
/// from, fetched once per frame.
#[derive(Default)]
struct WaveChunks {
    got: std::collections::HashMap<i64, Option<std::sync::Arc<bv_engine::WaveChunk>>>,
}

impl WaveChunks {
    /// Sample `i` of channel `ch` (`None` while its block is read).
    fn sample(
        &mut self,
        app: &BananaApp,
        info: &bv_timeline::MediaInfo,
        stream: u16,
        ch: usize,
        rate: i64,
        i: i64,
    ) -> Option<f32> {
        let index = i.div_euclid(rate);
        let chunk = self
            .got
            .entry(index)
            .or_insert_with(|| app.engine.wave_chunk(info, stream, index))
            .clone()?;
        let samples = chunk
            .samples
            .get(ch.min(chunk.samples.len().saturating_sub(1)))?;
        Some(
            samples
                .get((i - chunk.start) as usize)
                .copied()
                .unwrap_or(0.0),
        )
    }

    /// Lowest and highest value of the media between `s0` and `s1`, the
    /// ends interpolated between samples so that a pixel narrower than a
    /// sample still joins its neighbours.
    fn range(
        &mut self,
        app: &BananaApp,
        info: &bv_timeline::MediaInfo,
        stream: u16,
        ch: usize,
        s0: Tick,
        s1: Tick,
    ) -> Option<(f32, f32)> {
        let rate = info.audio_stream(stream)?.sample_rate.0 as i64;
        let to_sample = |t: Tick| t.0 as f64 * rate as f64 / bv_core::TICKS_PER_SECOND as f64;
        let (a, b) = (to_sample(s0), to_sample(s1));
        let mut at = |x: f64| -> Option<f32> {
            let i = x.floor() as i64;
            let f = (x - i as f64) as f32;
            let v0 = self.sample(app, info, stream, ch, rate, i)?;
            let v1 = self.sample(app, info, stream, ch, rate, i + 1)?;
            Some(v0 + (v1 - v0) * f)
        };
        let (va, vb) = (at(a)?, at(b)?);
        let (mut lo, mut hi) = (va.min(vb), va.max(vb));
        for i in (a.floor() as i64 + 1)..=(b.floor() as i64) {
            let v = self.sample(app, info, stream, ch, rate, i)?;
            lo = lo.min(v);
            hi = hi.max(v);
        }
        Some((lo, hi))
    }
}

/// `fine`: Ctrl held, for the drags that take fine steps (the level line).
/// How soon (seconds) a press after a click on the same event makes a
/// double-click.
const DOUBLE_PRESS: f64 = 0.4;

/// The time selection an event double-clicked and dragged to `t` makes:
/// its own span, out to the first fade or event edge on its track at or
/// past `t` (`t` itself where there is none), on the side `t` is on.
pub(crate) fn extended_selection(
    project: &Project,
    clip: ClipId,
    t: Tick,
) -> Option<bv_core::TimeRange> {
    let c = project.clip(clip)?;
    let track = project.clip_track(clip)?;
    let mut edges: Vec<Tick> = Vec::new();
    for o in &track.clips {
        let (fade_in, fade_out) = track.effective_fades(o, project.auto_crossfade);
        edges.extend([o.start, o.end(), o.start + fade_in, o.end() - fade_out]);
    }
    let (mut start, mut end) = (c.start, c.end());
    if t > end {
        end = edges.iter().copied().filter(|&e| e >= t).min().unwrap_or(t);
    } else if t < start {
        start = edges
            .iter()
            .copied()
            .filter(|&e| e <= t)
            .max()
            .unwrap_or(t.max(Tick::ZERO));
    }
    (end > start).then(|| bv_core::TimeRange::new(start, end - start))
}

fn update_drag(view: &mut TimelineView, app: &mut BananaApp, l: &Layout, pos: Pos2, fine: bool) {
    let Some(mut drag) = view.drag.take() else {
        return;
    };
    let gesture = view.gesture.take();
    let t = view.time_at(l, pos.x);
    // An event double-clicked and dragged: the selection out to the next
    // edge past the pointer.
    if let Drag::Extend { clip } = drag {
        let project = app.project();
        if let Some(r) = extended_selection(project, clip, t) {
            app.modify_merged(egui::Id::new(("extend-selection", clip.0)), |p| {
                p.loop_region = Some(r);
            });
        }
        view.drag = Some(drag);
        view.gesture = gesture;
        return;
    }
    // The envelope brush: a point wherever the pointer went.
    if let (
        Drag::EnvBrush {
            track,
            kind,
            samples,
        },
        Some(g),
    ) = (&mut drag, &gesture)
        && let Some(r) = super::envelopes::lane_of(l, *track)
    {
        let at = t.max(Tick::ZERO);
        let value = super::envelopes::value_at_y(r, *kind, pos.y);
        if samples.last().is_none_or(|s| s.0 != at) {
            samples.push((at, value));
        }
        let (track, kind, curve) = (*track, *kind, app.settings.prefs.new_envelope_curve(*kind));
        let samples = samples.clone();
        app.history.preview(g, |p| {
            let _ = p.write_automation(track, kind, &samples, false, curve);
        });
    }
    // An envelope's segment: its points up or down by how far the pointer
    // went from where the line was taken, as a share of the lane.
    if let (
        Drag::EnvSegment {
            track,
            clip,
            kind,
            points,
            y0,
            moved,
        },
        Some(g),
    ) = (&mut drag, &gesture)
        && let Some(r) = super::envelopes::lane_for(view, l, g.before(), *track, *clip)
    {
        *moved |= (pos.y - *y0).abs() >= 3.0;
        let mut d = if *moved {
            (*y0 - pos.y) / r.height().max(1.0)
        } else {
            0.0
        };
        if fine {
            d *= 0.1;
        }
        let (owner, kind) = (super::envelopes::owner(*track, *clip), *kind);
        let points = points.clone();
        app.history.preview(g, |p| {
            for &(i, at, v) in &points {
                let _ =
                    p.move_envelope_point(owner, kind, i, at, kind.from_unit(kind.to_unit(v) + d));
            }
        });
    }
    match (&drag, &gesture) {
        (Drag::Needle(n), _) => {
            let project = app.project().clone();
            let t = view.snap_cursor(app, &project, t.max(Tick::ZERO));
            match n {
                super::Needle::Playhead => app.move_playhead(t),
                _ => app.seek(t),
            }
        }
        (Drag::Move { clip, grab }, Some(g)) => {
            let before = g.before();
            let group = before.group_members(*clip);
            // The other selected events go along (each with its group),
            // like VEGAS's.
            let mut done: std::collections::HashSet<ClipId> = group.iter().copied().collect();
            let mut others = Vec::new();
            for &s in &app.selection {
                if before.clip(s).is_some() && !done.contains(&s) {
                    done.extend(before.group_members(s));
                    others.push(s);
                }
            }
            let movers: Vec<ClipId> = done.into_iter().collect();
            let duration = before.clip(*clip).map_or(Tick::ZERO, |c| c.duration);
            // Where the event was: it can snap back there.
            let origin = before.clip(*clip).map_or(Vec::new(), |c| {
                vec![
                    (c.start, SnapKind::DragOrigin),
                    (c.end(), SnapKind::DragOrigin),
                ]
            });
            // Its snap offset flag snaps where its start would (VEGAS's
            // left snap point).
            let flag = before.clip(*clip).map_or(Tick::ZERO, |c| c.snap_point());
            let offsets = [flag, duration];
            let start = view.snap_with(app, before, t - *grab, &offsets, &movers, &origin);
            let kind = before.clip_track(*clip).map(|tr| tr.kind);
            // Below the tracks: a new track of its kind, made when it is
            // let go (its ghost meanwhile), as VEGAS's.
            let below = l
                .rows
                .iter()
                .map(|r| r.top + r.height)
                .reduce(f32::max)
                .is_some_and(|bottom| pos.y >= bottom);
            view.new_track = kind
                .filter(|_| below)
                .map(|kind| NewTrack { kind, clip: *clip });
            let target = l
                .row_at(pos.y)
                .filter(|r| Some(r.kind) == kind)
                .map(|r| r.track);
            let id = *clip;
            app.history.preview(g, |p| {
                let Some(from) = p.clip(id).map(|c| c.start) else {
                    return;
                };
                // None of them before the start of the timeline.
                let earliest = movers
                    .iter()
                    .filter_map(|&m| p.clip(m))
                    .map(|c| c.start)
                    .min()
                    .unwrap_or(from);
                let delta = (start - from).max(-earliest);
                // As many tracks up or down, onto tracks of their kind.
                let index = |p: &Project, id: ClipId| {
                    p.tracks
                        .iter()
                        .position(|t| t.clips.iter().any(|c| c.id == id))
                };
                let rows = match (target.and_then(|t| p.track_index(t)), index(&*p, id)) {
                    (Some(to), Some(at)) => to as i64 - at as i64,
                    _ => 0,
                };
                let _ = p.move_clip(id, from + delta, target);
                for &o in &others {
                    let (Some(s), Some(at)) = (p.clip(o).map(|c| c.start), index(&*p, o)) else {
                        continue;
                    };
                    let kind = p.tracks[at].kind;
                    let to = usize::try_from(at as i64 + rows)
                        .ok()
                        .and_then(|i| p.tracks.get(i))
                        .filter(|t| t.kind == kind && rows != 0)
                        .map(|t| t.id);
                    let _ = p.move_clip(o, s + delta, to);
                }
                // Options › Lock Envelopes to Events.
                if p.lock_envelopes {
                    p.carry_envelopes(before, &movers);
                }
            });
        }
        (Drag::SnapOffset { clip }, Some(g)) => {
            if let Some(c) = g.before().clip(*clip) {
                let rate = app.frame_rate();
                let offset = rate.snap_nearest(t - c.start).clamp(Tick::ZERO, c.duration);
                let id = *clip;
                app.history.preview(g, |p| {
                    let _ = p.set_snap_offset(id, offset);
                });
            }
        }
        (Drag::TrimStart { clip, ungrouped }, Some(g)) => {
            let group = g.before().group_members(*clip);
            let markers = app.snap_markers && view.show_media_markers;
            let mut edges = media_edges(g.before(), *clip, t, markers);
            if let Some(c) = g.before().clip(*clip) {
                edges.push((c.start, SnapKind::DragOrigin));
            }
            let new_start = view.snap_with(app, g.before(), t, &[Tick::ZERO], &group, &edges);
            let (id, ungrouped) = (*clip, *ungrouped);
            app.history.preview(g, |p| {
                alone(p, ungrouped, |p| {
                    let _ = p.trim_start(id, new_start);
                });
            });
        }
        (Drag::TrimEnd { clip, ungrouped }, Some(g)) => {
            let group = g.before().group_members(*clip);
            let markers = app.snap_markers && view.show_media_markers;
            let mut edges = media_edges(g.before(), *clip, t, markers);
            if let Some(c) = g.before().clip(*clip) {
                edges.push((c.end(), SnapKind::DragOrigin));
            }
            let new_end = view.snap_with(app, g.before(), t, &[Tick::ZERO], &group, &edges);
            let (id, ungrouped) = (*clip, *ungrouped);
            app.history.preview(g, |p| {
                alone(p, ungrouped, |p| {
                    let _ = p.trim_end(id, new_end);
                });
            });
        }
        (
            Drag::SlipTrim {
                clip,
                end,
                t0,
                edge0,
            },
            Some(g),
        ) => {
            let group = g.before().group_members(*clip);
            let edge = view.snap(app, g.before(), *edge0 + (t - *t0), &[Tick::ZERO], &group);
            let (id, end) = (*clip, *end);
            app.history.preview(g, |p| {
                let _ = p.slip_trim(id, end, edge);
            });
        }
        (
            Drag::TrimAdjacent {
                clip,
                end,
                t0,
                edge0,
                ungrouped,
            },
            Some(g),
        ) => {
            let edge = view.snap(app, g.before(), *edge0 + (t - *t0), &[Tick::ZERO], &[*clip]);
            let (id, end, ungrouped) = (*clip, *end, *ungrouped);
            app.history.preview(g, |p| {
                alone(p, ungrouped, |p| {
                    let _ = p.trim_adjacent(id, end, edge);
                });
            });
        }
        (Drag::SplitTrim { clip, at }, Some(g)) => {
            let t = view.snap(app, g.before(), t, &[Tick::ZERO], &[*clip]);
            let (id, at) = (*clip, *at);
            app.history.preview(g, |p| {
                let Some(track) = p.clip_track(id).map(|t| t.id) else {
                    return;
                };
                let Ok(right) = p.split(id, at) else {
                    return;
                };
                // The event's own right part (its group's split too).
                let Some(r) = right
                    .into_iter()
                    .find(|&r| p.clip_track(r).is_some_and(|t| t.id == track))
                else {
                    return;
                };
                if t > at {
                    let _ = p.trim_start(r, t);
                } else if t < at {
                    let _ = p.trim_end(id, t);
                }
            });
        }
        (
            Drag::Level {
                clip,
                level0,
                y0,
                span,
            },
            Some(g),
        ) => {
            // Ctrl: fine steps, like VEGAS's.
            let step = if fine { 0.1 } else { 1.0 };
            let level = (level0 - (pos.y - y0) / span * step).clamp(0.0, 1.0);
            let id = *clip;
            app.history.preview(g, |p| {
                let _ = p.set_clip_level(id, level);
            });
        }
        (Drag::FadeIn { clip }, Some(g)) => {
            if let Some(c) = g.before().clip(*clip) {
                let f = view.snap(app, g.before(), t, &[Tick::ZERO], &[*clip]) - c.start;
                let id = *clip;
                app.history.preview(g, |p| {
                    let _ = p.set_fades(id, Some(f.max(Tick::ZERO)), None);
                });
            }
        }
        (Drag::FadeOut { clip }, Some(g)) => {
            if let Some(c) = g.before().clip(*clip) {
                let f = c.end() - view.snap(app, g.before(), t, &[Tick::ZERO], &[*clip]);
                let id = *clip;
                app.history.preview(g, |p| {
                    let _ = p.set_fades(id, None, Some(f.max(Tick::ZERO)));
                });
            }
        }
        (
            Drag::EnvPoint {
                track,
                clip,
                kind,
                index,
                origin,
            },
            Some(g),
        ) => {
            if let Some(r) = super::envelopes::lane_for(view, l, g.before(), *track, *clip) {
                let at = view.snap(app, g.before(), t.max(Tick::ZERO), &[Tick::ZERO], &[]);
                let value = super::envelopes::value_at_y(r, *kind, pos.y);
                let owner = super::envelopes::owner(*track, *clip);
                let (kind, index, origin) = (*kind, *index, *origin);
                app.history.preview(g, |p| {
                    let at = super::envelopes::local(p, owner, at);
                    let i = match index {
                        Some(i) => Some(i),
                        None => {
                            let o = super::envelopes::local(p, owner, origin.0);
                            p.set_envelope_point(owner, kind, o, origin.1).ok()
                        }
                    };
                    if let Some(i) = i {
                        let _ = p.move_envelope_point(owner, kind, i, at, value);
                    }
                });
            }
        }
        (Drag::Slip { clip, t0 }, Some(g)) => {
            let d = app.frame_rate().snap_nearest(t - *t0);
            let id = *clip;
            app.history.preview(g, |p| {
                let _ = p.slip_takes(id, d);
            });
        }
        (Drag::Slide { clip, t0 }, Some(g)) => {
            let d = app.frame_rate().snap_nearest(t - *t0);
            let id = *clip;
            app.history.preview(g, |p| {
                let _ = p.slide_clip(id, d);
            });
        }
        (
            Drag::Stretch {
                clip,
                rate0,
                dur0,
                from_start,
            },
            Some(g),
        ) => {
            if let Some(c) = g.before().clip(*clip) {
                let group = g.before().group_members(*clip);
                let edge = view.snap(app, g.before(), t, &[Tick::ZERO], &group);
                let frame = app.frame_rate().frame_duration();
                let new_dur = if *from_start {
                    (c.end() - edge).max(frame)
                } else {
                    (edge - c.start).max(frame)
                };
                // The length at its normal speed: the edge sticks there.
                let natural = rate0 * dur0.0 as f64;
                let near = (new_dur.0 as f64 - natural).abs() / view.ticks_per_px <= 6.0;
                let rate = if near {
                    1.0
                } else {
                    (rate0 * dur0.0 as f64 / new_dur.0 as f64).clamp(MIN_RATE, MAX_RATE)
                };
                let (id, end, from_start) = (*clip, c.end(), *from_start);
                app.history.preview(g, |p| {
                    let _ = p.set_clip_rate(id, rate);
                    // Stretched by its start, the event keeps its end.
                    if from_start && let Some(d) = p.clip(id).map(|c| c.duration) {
                        let _ = p.move_clip(id, end - d, None);
                    }
                });
            }
        }
        (
            Drag::EnvRange {
                track,
                kind,
                y0,
                outer,
                inner,
            },
            Some(g),
        ) => {
            if let Some(r) = super::envelopes::lane_of(l, *track) {
                let delta = (y0 - pos.y) / r.height().max(1.0);
                let (track, kind, outer, inner) = (*track, *kind, *outer, *inner);
                app.history.preview(g, |p| {
                    let _ = p.lift_envelope(track, kind, outer, inner, delta);
                });
            }
        }
        (Drag::Marquee { start, add, shape }, _) => {
            // As VEGAS's, the events it takes are picked as it goes, after
            // those picked when it began (with Ctrl).
            let base = view
                .box_base
                .get_or_insert_with(|| (app.picked(), app.selection.clone()))
                .clone();
            let r = shape.rect(l, *start, pos);
            box_pick(view, app, l, r, *shape, *add, &base);
        }
        (Drag::TimeSelect { anchor }, _) => {
            let project = app.project().clone();
            let t = view.snap(app, &project, t.max(Tick::ZERO), &[Tick::ZERO], &[]);
            let r = bv_core::TimeRange::from_bounds((*anchor).min(t), (*anchor).max(t));
            let key = egui::Id::new("selecao-de-tempo");
            if r.duration > Tick::ZERO {
                app.modify_merged(key, |p| p.loop_region = Some(r));
            }
        }
        _ => {}
    }
    view.gesture = gesture;
    view.drag = Some(drag);
}

fn finish_drag(view: &mut TimelineView, app: &mut BananaApp, l: &Layout, pointer: Option<Pos2>) {
    let drag = view.drag.take();
    // What the selection rectangle began from, whatever ends it.
    let box_base = view.box_base.take();
    if matches!(drag, Some(Drag::Needle(super::Needle::Playhead))) {
        app.release_playback();
    }
    let new_track = view.new_track.take();
    // Events dragged with the right button: a menu asks what to do.
    if std::mem::take(&mut view.right_drag)
        && let Some(Drag::Move { clip, .. }) = &drag
        && let Some(at) = pointer
        && let Some(g) = view.gesture.take()
    {
        view.drop_menu = Some(DropMenu {
            at,
            clip: *clip,
            gesture: g,
        });
        view.snap_line = None;
        return;
    }
    // The Shuffle tool: the event goes between the others where it was let
    // go, and they close up its place.
    if app.tool == Tool::Shuffle
        && let Some(Drag::Move { clip, .. }) = &drag
        && let Some(g) = view.gesture.as_ref()
        && let Some(to) = app.project().clip(*clip).map(|c| c.start)
    {
        let id = *clip;
        app.history.preview(g, |p| {
            let _ = p.shuffle_events(id, to);
        });
        view.new_track = None;
        if let Some(g) = view.gesture.take() {
            app.history.commit_gesture(g);
        }
        view.snap_line = None;
        return;
    }
    // The envelope brush let go: its points thinned when Preferences ›
    // External Control & Automation says so.
    if let Some(Drag::EnvBrush {
        track,
        kind,
        samples,
    }) = &drag
        && let Some(g) = view.gesture.as_ref()
        && app.settings.prefs.thin_automation
    {
        let (track, kind, curve) = (*track, *kind, app.settings.prefs.new_envelope_curve(*kind));
        app.history.preview(g, |p| {
            let _ = p.write_automation(track, kind, samples, true, curve);
        });
    }
    // Let go below the tracks: onto a new track of its kind, at the
    // bottom (one undo with the move).
    if let Some(nt) = new_track
        && matches!(drag, Some(Drag::Move { .. }))
        && let Some(g) = view.gesture.as_ref()
    {
        let dragged = app.history.current().clone();
        app.history.preview(g, |p| {
            *p = dragged;
            let t = p.add_track(nt.kind);
            let _ = p.move_track_to(t, p.tracks.len());
            if let Some(start) = p.clip(nt.clip).map(|c| c.start) {
                let _ = p.move_clip(nt.clip, start, Some(t));
            }
        });
    }
    if let Some(g) = view.gesture.take() {
        app.history.commit_gesture(g);
    }
    view.snap_line = None;
    let Some(p) = pointer else { return };
    match drag {
        Some(Drag::Marquee { start, add, shape }) => {
            let r = shape.rect(l, start, p);
            let base = box_base.unwrap_or_else(|| (app.picked(), app.selection.clone()));
            box_pick(view, app, l, r, shape, add, &base);
        }
        Some(Drag::ZoomBox { start }) => {
            let (x0, x1) = (start.x.min(p.x), start.x.max(p.x));
            if x1 - x0 > 8.0 {
                let t0 = view.time_at(l, x0).max(Tick::ZERO);
                let t1 = view.time_at(l, x1);
                view.ticks_per_px =
                    ((t1 - t0).0 as f64 / l.lanes.width() as f64).max(super::MIN_TICKS_PER_PX);
                view.view_start = t0;
            }
        }
        Some(Drag::TimeSelect { .. }) => app.restart_if_playing(),
        _ => {}
    }
}

/// The selection rectangle `r` picks the events it takes, each with its
/// group, after those picked when it began (`base`, with `add`; else
/// none).
fn box_pick(
    view: &TimelineView,
    app: &mut BananaApp,
    l: &Layout,
    r: Rect,
    shape: BoxShape,
    add: bool,
    base: &super::BoxBase,
) {
    let (mut picked, mut selection) = if add {
        base.clone()
    } else {
        Default::default()
    };
    let project = app.project();
    for row in &l.rows {
        let Some(track) = project.track(row.track) else {
            continue;
        };
        for c in &track.clips {
            if shape.takes(r, clip_rect(view, l, row, c.start, c.end())) {
                picked.insert(c.id);
                selection.extend(project.group_members(c.id));
            }
        }
    }
    app.selection = selection;
    app.set_picked(picked);
}

fn click(
    view: &mut TimelineView,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    pos: Pos2,
    m: egui::Modifiers,
) {
    let add = m.command;
    let rate = project.settings.frame_rate;
    let t = rate.snap_floor(view.time_at(l, pos.x).max(Tick::ZERO));
    if let Hit::Clip {
        clip,
        zone: Zone::Button(b),
    } = hit(view, l, project, pos)
    {
        if !app.selection.contains(&clip) {
            app.select_clip(clip, false);
        }
        match b {
            ClipButton::Generated => app.open_generated_media(clip),
            ClipButton::PanCrop => app.clip_windows.pan_crop = Some(clip),
            ClipButton::Fx => app.clip_windows.clip_fx = Some(clip),
            ClipButton::More => {
                view.more_menu = Some(super::MoreMenu {
                    clip,
                    at: pos2(pos.x - 8.0, pos.y + 12.0),
                    fresh: true,
                });
            }
        }
        return;
    }
    // Ctrl+Alt+Shift+click splits the event there, like VEGAS's.
    let split = app.tool == Tool::Edit && m.command && m.alt && m.shift;
    if (app.tool == Tool::SplitTrim || split)
        && let Hit::Clip { clip, .. } = hit(view, l, project, pos)
    {
        app.select_clip(clip, false);
        if project
            .clip(clip)
            .is_some_and(|c| t > c.start && t < c.end())
        {
            app.edit(|p| p.split(clip, t));
        }
        return;
    }
    if app.tool == Tool::Zoom {
        let factor = if add { 2.0 } else { 0.5 };
        view.zoom_at(view.time_at(l, pos.x), pos.x - l.lanes.left(), factor);
        return;
    }
    // Trimming edges: a click on another edge makes it the edit point.
    if let Some(e) = view.edge_trim
        && let Hit::Clip {
            clip,
            zone: zone @ (Zone::Start | Zone::End),
        } = hit(view, l, project, pos)
    {
        view.trim_edge(project, clip, zone == Zone::End, e.expanded);
        app.select_clip(clip, false);
        return;
    }
    // Shift+click on an event: the events from the last one clicked to it.
    if m.shift
        && !add
        && let Hit::Clip { clip, .. } = hit(view, l, project, pos)
    {
        match view.range_anchor.filter(|a| project.clip(*a).is_some()) {
            Some(a) => select_range(app, project, a, clip),
            None => {
                app.select_clip(clip, false);
                view.range_anchor = Some(clip);
            }
        }
        return;
    }
    // The cursor lands on event edges and markers, like VEGAS's.
    let t = view.snap_cursor(app, project, view.time_at(l, pos.x).max(Tick::ZERO));
    view.snap_line = None;
    // Shift+click elsewhere: a time selection from the cursor to there.
    if m.shift && !add {
        let from = app.cursor;
        if t != from {
            app.set_loop_region(Some(bv_core::TimeRange::from_bounds(
                from.min(t),
                from.max(t),
            )));
        }
        return;
    }
    // A click leaves no time selection: with Options › Preferences ›
    // Editing's "Collapse loop region when no time selection is present"
    // the loop region goes too (otherwise it stays).
    if app.settings.prefs.collapse_loop_region && project.loop_region.is_some() {
        app.set_loop_region(None);
    }
    match hit(view, l, project, pos) {
        Hit::Clip { clip, .. } => {
            app.select_clip(clip, add);
            view.range_anchor = Some(clip);
            // Options › Preferences › Editing: "Move timeline cursor when
            // clicking on an event".
            if app.settings.prefs.click_moves_cursor {
                app.seek(t);
            }
        }
        Hit::Track { .. } | Hit::Nothing => {
            if !add {
                app.selection.clear();
            }
            app.seek(t);
        }
    }
}

/// How much darker the part of an audio event another overlaps shows, as
/// VEGAS's (seven tenths of its color).
const OVERLAP_SHADE: Color32 = Color32::from_black_alpha(74);

/// A crossfade's transition label on a video track, as VEGAS's: the
/// transition's name in a blue band over the overlap; a plug-in's has its
/// Transition Properties icon at the band's right.
pub(crate) struct TransitionLabel {
    /// The event coming in (whose transition it is).
    pub clip: ClipId,
    pub name: String,
    pub band: Rect,
    pub icon: Option<Rect>,
    /// Switched off in the FX window: drawn dimmed.
    pub off: bool,
}

/// The name a crossfade's label shows: its plug-in transition's, or a
/// built-in one's other than the plain dissolve (none: a plain crossfade,
/// its X).
fn transition_label(b: &bv_timeline::Clip) -> Option<String> {
    match &b.transition_fx {
        Some(fx) => Some(tr(&fx.name).to_string()),
        None if b.transition != bv_timeline::Transition::Dissolve => {
            Some(tr(b.transition.name()).to_string())
        }
        None => None,
    }
}

/// Where the label of the transition of `b` (the event coming in) goes,
/// across its overlap from `x0` to `x1`: under the event's title, as VEGAS
/// places it.
fn label_band(
    view: &TimelineView,
    l: &Layout,
    row: &Row,
    b: &bv_timeline::Clip,
    x0: f32,
    x1: f32,
) -> Rect {
    let title = if view.show_clip_headers { TITLE_H } else { 0.0 };
    let r = clip_rect(view, l, row, b.start, b.end());
    let top = (r.top() + title + 6.0).min(r.bottom() - 19.0).max(r.top());
    Rect::from_min_max(pos2(x0, top), pos2(x1, top + 18.0))
}

/// The transitions' labels in sight.
pub(crate) fn transition_labels(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
) -> Vec<TransitionLabel> {
    let mut out = Vec::new();
    if !project.auto_crossfade {
        return out;
    }
    for row in l.rows.iter().filter(|r| r.kind == TrackKind::Video) {
        let Some(track) = project.track(row.track) else {
            continue;
        };
        for (i, a) in track.clips.iter().enumerate() {
            for b in &track.clips[i + 1..] {
                if b.start >= a.end() || a.muted || b.muted {
                    continue;
                }
                let Some(name) = transition_label(b) else {
                    continue;
                };
                let (x0, x1) = (view.x_of(l, b.start), view.x_of(l, a.end().min(b.end())));
                if x1 - x0 < 12.0 || x1 < l.lanes.left() || x0 > l.lanes.right() {
                    continue;
                }
                let band = label_band(view, l, row, b, x0, x1);
                let icon = (b.transition_fx.is_some() && band.width() >= 22.0)
                    .then(|| Rect::from_min_max(pos2(band.right() - 19.0, band.top()), band.max));
                out.push(TransitionLabel {
                    clip: b.id,
                    name,
                    band,
                    icon,
                    off: b.transition_bypass,
                });
            }
        }
    }
    out
}

/// A transition's label: the blue band, the name in its middle, the icon
/// (lit under the pointer: `hot`).
fn paint_transition_label(p: &egui::Painter, t: &TransitionLabel, hot: bool) {
    let fill = if t.off {
        theme::selection().gamma_multiply(0.45)
    } else {
        theme::selection()
    };
    p.rect_filled(t.band, 0.0, fill);
    let right = t.icon.map_or(t.band.right(), |i| i.left());
    let room = right - t.band.left() - 6.0;
    if room > 8.0 {
        crate::widgets::fit_text(
            p,
            pos2((t.band.left() + right) / 2.0, t.band.center().y),
            Align2::CENTER_CENTER,
            &t.name,
            FontId::proportional(11.0),
            Color32::WHITE,
            room,
        );
    }
    if let Some(i) = t.icon {
        if hot {
            p.rect_filled(i.shrink(1.0), 2.0, Color32::from_white_alpha(60));
        }
        icons::paint(p, i.shrink(2.0), Icon::Transition, Color32::WHITE);
    }
}

/// Expand Track Layers' transition roll: the middle third of `track`'s
/// row, a block where each event overlaps the one before, named after the
/// later one's transition.
fn paint_transition_roll(
    view: &TimelineView,
    painter: &egui::Painter,
    l: &Layout,
    row: &Row,
    track: &bv_timeline::Track,
) {
    let body = row.height - row.keys;
    let roll = Rect::from_min_max(
        pos2(l.lanes.left(), row.top + body / 3.0),
        pos2(l.lanes.right(), row.top + body * 2.0 / 3.0),
    );
    painter.rect_filled(roll, 0.0, theme::panel_dark());
    let mut clips: Vec<&bv_timeline::Clip> = track.clips.iter().collect();
    clips.sort_by_key(|c| c.start);
    for w in clips.windows(2) {
        let (a, b) = (w[0], w[1]);
        let end = a.end().min(b.end());
        if b.start >= end {
            continue;
        }
        let r = Rect::from_min_max(
            pos2(view.x_of(l, b.start), roll.top() + 1.0),
            pos2(view.x_of(l, end), roll.bottom() - 1.0),
        );
        if r.right() < l.lanes.left() || r.left() > l.lanes.right() {
            continue;
        }
        painter.rect_filled(r, 2.0, theme::banana_dim());
        painter.rect_stroke(
            r,
            2.0,
            Stroke::new(1.0, theme::banana()),
            egui::StrokeKind::Inside,
        );
        let name = match &b.transition_fx {
            Some(fx) => fx.name.clone(),
            None => tr(b.transition.name()).to_string(),
        };
        let cp = painter.with_clip_rect(r.shrink(2.0).intersect(l.lanes));
        cp.text(
            pos2(r.left() + 4.0, r.center().y),
            Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(10.0),
            ON_EVENT,
        );
    }
}

/// A point's curves as VEGAS's envelope menus name them.
const POINT_CURVES: [(bv_timeline::KeyCurve, &str); 6] = [
    (bv_timeline::KeyCurve::Linear, "Linear Fade"),
    (bv_timeline::KeyCurve::Fast, "Fast Fade"),
    (bv_timeline::KeyCurve::Slow, "Slow Fade"),
    (bv_timeline::KeyCurve::Smooth, "Smooth Fade"),
    (bv_timeline::KeyCurve::Sharp, "Sharp Fade"),
    (bv_timeline::KeyCurve::Hold, "Hold"),
];

/// A track envelope point's menu, as VEGAS's (the user's prints): its
/// kind's values — a composite level's 100 % and 0 % Composite Strength,
/// the first in bold, the default —, Set To… (typed in a box at the
/// point), Delete, the curve from it, then Flip All Points, Thin All
/// Points (their Selected ones while there is a time selection) and Reset
/// All.
fn point_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    h: super::envelopes::EnvHit,
    i: usize,
) {
    use crate::menus::{item, item_enabled, mark_item, strong_item};
    use bv_timeline::EnvelopeKind;
    let (owner, track, kind) = (h.owner(), h.track, h.kind);
    let Some(point) = super::envelopes::envelope_of(app.project(), owner, kind)
        .and_then(|e| e.points.get(i))
        .copied()
    else {
        ui.close();
        return;
    };
    let presets: &[(f32, &str)] = match kind {
        EnvelopeKind::CompositeLevel => &[
            (1.0, "Set to 100% Composite Strength"),
            (0.0, "Set to 0% Composite Strength"),
        ],
        _ => &[],
    };
    for (n, &(v, label)) in presets.iter().enumerate() {
        let clicked = if n == 0 {
            strong_item(ui, tr(label))
        } else {
            item(ui, tr(label))
        };
        if clicked {
            app.edit(|p| p.move_envelope_point(owner, kind, i, point.at, v));
            ui.close();
        }
    }
    if !presets.is_empty() {
        ui.separator();
    }
    if item_enabled(ui, tr("Set To…"), !kind.is_held()).clicked() {
        view.env_set_to = Some(super::envelopes::SetTo {
            track,
            clip: None,
            kind,
            index: i,
            text: super::envelopes::set_to_text(kind, point.value),
            fresh: true,
        });
        ui.close();
    }
    ui.separator();
    if item_enabled(ui, tr("Delete"), i > 0).clicked() {
        app.edit(|p| p.remove_envelope_point(owner, kind, i));
        ui.close();
    }
    ui.separator();
    if !kind.is_held() {
        for (c, label) in POINT_CURVES {
            if mark_item(ui, tr(label), point.curve == c) {
                app.edit(|p| p.set_envelope_curve(owner, kind, i, c));
                ui.close();
            }
        }
        ui.separator();
    }
    let sel = app
        .project()
        .loop_region
        .filter(|r| r.duration > Tick::ZERO);
    if kind.flips() && item(ui, tr("Flip All Points")) {
        app.edit(|p| p.flip_envelope(track, kind, None));
        ui.close();
    }
    if item(ui, tr("Thin All Points")) {
        app.edit(|p| p.thin_envelope(track, kind, None));
        ui.close();
    }
    if let Some(sel) = sel {
        if kind.flips() && item(ui, tr("Flip Selected Points")) {
            app.edit(|p| p.flip_envelope(track, kind, Some(sel)));
            ui.close();
        }
        if item(ui, tr("Thin Selected Points")) {
            app.edit(|p| p.thin_envelope(track, kind, Some(sel)));
            ui.close();
        }
    }
    if item(ui, tr("Reset All")) {
        let curve = app.settings.prefs.new_envelope_curve(kind);
        app.edit(|p| -> Result<(), bv_timeline::EditError> {
            p.toggle_envelope(track, kind)?;
            p.toggle_envelope_as(track, kind, curve)?;
            Ok(())
        });
        ui.close();
    }
}

fn envelope_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    h: super::envelopes::EnvHit,
) {
    use bv_timeline::EnvelopeKind;
    // A track envelope's point: VEGAS's menu.
    if let (Some(i), None) = (h.point, h.clip) {
        point_menu(ui, view, app, h, i);
        return;
    }
    let (owner, track, kind) = (h.owner(), h.track, h.kind);
    ui.label(
        egui::RichText::new(trf!(
            "Envelope: {}",
            super::envelopes::label(app.project(), owner, kind)
        ))
        .color(super::envelopes::color(&app.settings.prefs, kind)),
    );
    ui.separator();
    match h.point {
        Some(i) => {
            let point = super::envelopes::envelope_of(app.project(), owner, kind)
                .and_then(|e| e.points.get(i))
                .copied();
            let curve = point.map(|p| p.curve);
            if !kind.is_held() {
                for c in bv_timeline::KeyCurve::ALL {
                    if crate::widgets::radio(ui, curve == Some(c), tr(c.name())).clicked() {
                        app.edit(|p| p.set_envelope_curve(owner, kind, i, c));
                        ui.close();
                    }
                }
                ui.separator();
            }
            // A velocity point's presets: from reverse to triple speed.
            if kind == EnvelopeKind::Velocity
                && let Some(pt) = point
            {
                for v in [-1.0f32, -0.5, 0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0] {
                    let text = trf!("Set to {}", super::envelopes::value_text(kind, v));
                    if crate::widgets::radio(ui, (pt.value - v).abs() < 1e-4, &text).clicked() {
                        app.edit(|p| p.move_envelope_point(owner, kind, i, pt.at, v));
                        ui.close();
                    }
                }
                ui.separator();
            }
            if ui
                .add_enabled(i > 0, egui::Button::new(tr("Delete Point")))
                .clicked()
            {
                app.edit(|p| p.remove_envelope_point(owner, kind, i));
                ui.close();
            }
        }
        None => {
            if ui.button(tr("Add Point")).clicked() {
                app.edit(|p| {
                    let at = super::envelopes::local(p, owner, h.at);
                    p.set_envelope_point(owner, kind, at, h.value)
                });
                ui.close();
            }
        }
    }
    ui.separator();
    if let Some(clip) = h.clip {
        // An event's envelope.
        if ui.button(tr("Reset All Points")).clicked() {
            app.edit(|p| -> Result<(), bv_timeline::EditError> {
                p.toggle_event_envelope(clip, kind)?;
                p.toggle_event_envelope(clip, kind)?;
                Ok(())
            });
            ui.close();
        }
        if ui.button(tr("Remove Envelope")).clicked() {
            app.edit(|p| p.toggle_event_envelope(clip, kind));
            ui.close();
        }
        return;
    }
    // Thin / Flip Selected Points act within the time selection.
    let sel = app
        .project()
        .loop_region
        .filter(|r| r.duration > Tick::ZERO);
    if ui.button(tr("Thin All Points")).clicked() {
        app.edit(|p| p.thin_envelope(track, kind, None));
        ui.close();
    }
    if ui
        .add_enabled(sel.is_some(), egui::Button::new(tr("Thin Selected Points")))
        .clicked()
    {
        app.edit(|p| p.thin_envelope(track, kind, sel));
        ui.close();
    }
    if kind.flips() {
        if ui.button(tr("Flip All Points")).clicked() {
            app.edit(|p| p.flip_envelope(track, kind, None));
            ui.close();
        }
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Flip Selected Points")))
            .clicked()
        {
            app.edit(|p| p.flip_envelope(track, kind, sel));
            ui.close();
        }
    }
    ui.separator();
    if ui.button(tr("Reset All Points")).clicked() {
        let curve = app.settings.prefs.new_envelope_curve(kind);
        app.edit(|p| -> Result<(), bv_timeline::EditError> {
            p.toggle_envelope(track, kind)?;
            p.toggle_envelope_as(track, kind, curve)?;
            Ok(())
        });
        ui.close();
    }
    if ui.button(tr("Remove Envelope")).clicked() {
        app.edit(|p| p.toggle_envelope(track, kind));
        ui.close();
    }
}

fn context_menu(view: &mut TimelineView, ui: &mut egui::Ui, app: &mut BananaApp) {
    match view.context {
        ContextTarget::Envelope(h) => envelope_menu(ui, view, app, h),
        ContextTarget::OfxEnvelope => match view.ofx_menu.clone() {
            Some(h) => super::ofx_envelopes::menu(ui, view, app, &h),
            None => ui.close(),
        },
        ContextTarget::OfxArea(track) => super::ofx_envelopes::area_menu(ui, view, app, track),
        ContextTarget::Clip(clip) => super::shortcut_menus::event_menu(ui, app, clip),
        ContextTarget::Track(track) => super::shortcut_menus::track_menu(ui, app, track),
        ContextTarget::Empty => super::shortcut_menus::empty_menu(ui, app),
        ContextTarget::Fade { clip, out } => fade_menu(ui, app, clip, out),
        ContextTarget::Crossfade { out, inn } => crossfade_menu(ui, app, out, inn),
        _ => ui.close(),
    }
}

/// The fade of the event `id` under `pos` (`true`: its fade-out): from
/// the event's edge to where the fade ends, where its curve is drawn. An
/// edge another event overlaps has the crossfade instead.
fn fade_at(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    id: ClipId,
    pos: Pos2,
) -> Option<bool> {
    let row = l.row_at(pos.y)?;
    let track = project.track(row.track)?;
    let clip = track.clips.iter().find(|c| c.id == id)?;
    let r = clip_rect(view, l, row, clip.start, clip.end());
    let (cross_in, cross_out) = if project.auto_crossfade {
        track.crossfades(clip)
    } else {
        (None, None)
    };
    let (fi, fo) = (px(view, clip.fade_in), px(view, clip.fade_out));
    let fade_in = fi > 0.5 && cross_in.is_none() && pos.x <= r.left() + fi;
    let fade_out = fo > 0.5 && cross_out.is_none() && pos.x >= r.right() - fo;
    match (fade_in, fade_out) {
        (true, true) => Some(pos.x > r.center().x),
        (true, false) => Some(false),
        (false, true) => Some(true),
        (false, false) => None,
    }
}

/// The crossfade under `pos`: the last two events there (the one going
/// out, the one coming in), as their X is drawn.
fn crossfade_under(
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    pos: Pos2,
) -> Option<(ClipId, ClipId)> {
    if !project.auto_crossfade {
        return None;
    }
    let track = project.track(l.row_at(pos.y)?.track)?;
    let t = view.time_at(l, pos.x);
    let here: Vec<_> = track
        .clips
        .iter()
        .filter(|c| !c.muted && c.start <= t && t < c.end())
        .collect();
    match here.as_slice() {
        [.., a, b] => Some((a.id, b.id)),
        _ => None,
    }
}

/// VEGAS's lists' order: its crossfade popup's rows and columns.
const CROSSFADE_POPUP: [FadeCurve; 5] = [
    FadeCurve::Linear,
    FadeCurve::Fast,
    FadeCurve::Slow,
    FadeCurve::Smooth,
    FadeCurve::Sharp,
];

/// VEGAS's popup of a crossfade's curves: every pair as a picture — a
/// column for each curve of the event coming in, a row for each of the
/// one going out, lines between the columns —, a dot by the crossfade's;
/// a click sets both (one undo).
fn crossfade_menu(ui: &mut egui::Ui, app: &mut BananaApp, out: ClipId, inn: ClipId) {
    let p = app.project();
    let (Some(a), Some(b)) = (p.clip(out), p.clip(inn)) else {
        ui.close();
        return;
    };
    let now = (a.fade_out_curve, b.fade_in_curve);
    // A dot's room, the picture and a gap; a line between the columns.
    let (cell, line_gap, row) = (vec2(74.0, 25.0), 13.0, 30.0);
    let size = vec2(5.0 * cell.x + 4.0 * line_gap, 4.0 * row + cell.y);
    let (grid, _) = ui.allocate_exact_size(size, Sense::hover());
    let mut picked = None;
    for (c, &inc) in CROSSFADE_POPUP.iter().enumerate() {
        let x = grid.left() + c as f32 * (cell.x + line_gap);
        if c > 0 {
            let lx = ui.painter().round_to_pixel_center(x - line_gap / 2.0);
            ui.painter().line_segment(
                [pos2(lx, grid.top()), pos2(lx, grid.bottom())],
                Stroke::new(2.0, theme::text()),
            );
        }
        for (r, &outc) in CROSSFADE_POPUP.iter().enumerate() {
            let rect = Rect::from_min_size(pos2(x, grid.top() + r as f32 * row), cell);
            let resp = ui.interact(rect, ui.id().with(("crossfade", c, r)), Sense::click());
            let name = trf!(
                "Out: {}, in: {}",
                crate::preferences::fade_name(outc),
                crate::preferences::fade_name(inc)
            );
            let chosen = (outc, inc) == now;
            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, &name)
            });
            let picture =
                Rect::from_min_size(pos2(rect.left() + 16.0, rect.top()), vec2(48.0, cell.y));
            let painter = ui.painter();
            if resp.hovered() {
                painter.rect_filled(picture.expand(2.0), 2.0, theme::hover());
            }
            if chosen {
                painter.circle_filled(
                    pos2(picture.left() - 8.0, picture.center().y),
                    3.0,
                    theme::text_strong(),
                );
            }
            paint_crossfade_picture(painter, picture, outc, inc);
            if resp.on_hover_text(&name).clicked() {
                picked = Some((outc, inc));
            }
        }
    }
    if let Some((outc, inc)) = picked {
        if (outc, inc) != now {
            app.edit(|p| -> Result<(), bv_timeline::EditError> {
                p.set_fade_curve(out, true, outc)?;
                p.set_fade_curve(inn, false, inc)
            });
        }
        ui.close();
    }
}

/// A crossfade's picture, as VEGAS's popup draws them: the curve going
/// out and the one coming in, crossed in a frame.
fn paint_crossfade_picture(p: &egui::Painter, r: Rect, out: FadeCurve, inn: FadeCurve) {
    p.rect_filled(r, 0.0, theme::panel());
    p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::text()), StrokeKind::Inside);
    let inner = r.shrink(3.0);
    let line = |g: &dyn Fn(f32) -> f32| -> Vec<Pos2> {
        (0..=24)
            .map(|i| {
                let f = i as f32 / 24.0;
                pos2(
                    inner.left() + inner.width() * f,
                    inner.bottom() - inner.height() * g(f),
                )
            })
            .collect()
    };
    let stroke = Stroke::new(1.5, theme::text_dim());
    let falling = line(&|f| bv_timeline::scene::video_curve(out.out_shape(), 1.0 - f));
    let rising = line(&|f| bv_timeline::scene::video_curve(inn, f));
    p.add(Shape::line(falling, stroke));
    p.add(Shape::line(rising, stroke));
}

/// VEGAS's popup's curves, in its order.
const FADE_POPUP: [FadeCurve; 5] = [
    FadeCurve::Fast,
    FadeCurve::Linear,
    FadeCurve::Slow,
    FadeCurve::Smooth,
    FadeCurve::Sharp,
];

/// VEGAS's popup of a fade's curves: their pictures one under the other,
/// a dot by the event's; a click sets it.
fn fade_menu(ui: &mut egui::Ui, app: &mut BananaApp, clip: ClipId, out: bool) {
    let Some(c) = app.project().clip(clip) else {
        ui.close();
        return;
    };
    let now = if out {
        c.fade_out_curve
    } else {
        c.fade_in_curve
    };
    ui.spacing_mut().item_spacing.y = 5.0;
    for curve in FADE_POPUP {
        let name = crate::preferences::fade_name(curve);
        let (rect, resp) = ui.allocate_exact_size(vec2(66.0, 25.0), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, curve == now, name)
        });
        let picture = Rect::from_min_size(pos2(rect.right() - 48.0, rect.top()), vec2(48.0, 25.0));
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(picture.expand(2.0), 2.0, theme::hover());
        }
        if curve == now {
            p.circle_filled(
                pos2(picture.left() - 8.0, picture.center().y),
                3.0,
                theme::text_strong(),
            );
        }
        paint_fade_picture(p, picture, curve, out);
        if resp.on_hover_text(name).clicked() {
            if curve != now {
                app.edit(|p| p.set_fade_curve(clip, out, curve));
            }
            ui.close();
        }
    }
}

/// A fade curve's picture, as VEGAS's popup draws them: the curve in a
/// frame, rising for a fade-in and falling for a fade-out (Smooth as the
/// S its name stands for).
fn paint_fade_picture(p: &egui::Painter, r: Rect, curve: FadeCurve, out: bool) {
    p.rect_filled(r, 0.0, theme::panel());
    p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::text()), StrokeKind::Inside);
    let inner = r.shrink(3.0);
    let pts = (0..=24)
        .map(|i| {
            let f = i as f32 / 24.0;
            let g = if out {
                bv_timeline::scene::video_curve(curve.out_shape(), 1.0 - f)
            } else {
                bv_timeline::scene::video_curve(curve, f)
            };
            pos2(
                inner.left() + inner.width() * f,
                inner.bottom() - inner.height() * g,
            )
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.5, theme::text_dim())));
}

/// Arrasto vindo de fora (mídia, arquivo, gerador, transição, efeito).
fn drop_media(
    view: &mut TimelineView,
    ui: &egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
    pointer: Option<Pos2>,
) {
    view.drop = None;
    let Some(drag) = app.media_drag.clone() else {
        return;
    };
    // Files dragged in from the system: the OS tells where the mouse is.
    let pointer = match &app.os_drag {
        Some(d) => d.pointer,
        None => pointer,
    };
    let Some(pos) = pointer.filter(|p| l.lanes.contains(*p)) else {
        if app.media_drag.is_some() {
            view.snap_line = None;
        }
        return;
    };
    let released =
        ui.input(|i| i.pointer.any_released()) || app.os_drag.as_ref().is_some_and(|d| d.dropped);
    // Held near the tracks' sides, the view runs on, as VEGAS's.
    if !released {
        view.edge_scroll_x(ui, l, pos.x);
    }
    if drag.is_media() {
        let file_name = |p: &std::path::Path| {
            p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().to_string(),
            )
        };
        // What is being dropped, in order: name and media info (probed for
        // files that are not imported yet).
        let items: Vec<(String, Option<MediaInfo>)> = match &drag {
            MediaDrag::Media(id) => {
                let info = project
                    .media(*id)
                    .cloned()
                    .or_else(|| project.subclip_info(*id));
                vec![(
                    info.as_ref()
                        .map_or(tr("Media").into(), crate::i18n::media_name),
                    info,
                )]
            }
            MediaDrag::MediaPart(id, part) => {
                let info = project
                    .media(*id)
                    .cloned()
                    .or_else(|| project.subclip_info(*id))
                    .map(|mut info| {
                        info.duration = part.duration;
                        info
                    });
                vec![(
                    info.as_ref()
                        .map_or(tr("Media").into(), crate::i18n::media_name),
                    info,
                )]
            }
            MediaDrag::File(p) => vec![(file_name(p), app.probe_cached(p))],
            MediaDrag::Many(ids) => ids
                .iter()
                .map(|id| {
                    let info = project
                        .media(*id)
                        .cloned()
                        .or_else(|| project.subclip_info(*id));
                    (
                        info.as_ref()
                            .map_or(tr("Media").into(), crate::i18n::media_name),
                        info,
                    )
                })
                .collect(),
            MediaDrag::FilePart(p, part) => vec![(
                file_name(p),
                app.probe_cached(p).map(|mut info| {
                    info.duration = part.duration;
                    info
                }),
            )],
            MediaDrag::Files(paths) => paths
                .iter()
                .map(|p| (file_name(p), app.probe_cached(p)))
                .collect(),
            MediaDrag::Generator(g) => vec![(
                crate::i18n::generator_name(g),
                Some(MediaInfo::generated(g.clone(), &project.settings)),
            )],
            // A storyboard's cells: each media's part, a placeholder five
            // seconds of text.
            MediaDrag::Cells(cells) => cells
                .iter()
                .map(|c| match c.media {
                    Some(m) => {
                        let info = project
                            .media(m)
                            .cloned()
                            .or_else(|| project.subclip_info(m))
                            .map(|mut info| {
                                if let Some(r) = c.range {
                                    info.duration = r.duration;
                                }
                                info
                            });
                        (
                            info.as_ref()
                                .map_or(tr("Media").into(), crate::i18n::media_name),
                            info,
                        )
                    }
                    None => (
                        c.name.clone(),
                        Some(MediaInfo::generated(
                            bv_timeline::Generator::SolidColor { rgba: [0.0; 4] },
                            &project.settings,
                        )),
                    ),
                })
                .collect(),
            _ => Vec::new(),
        };
        // As long as they will be (Options › Preferences › Editing: "New
        // still image length"), one after another or overlapping.
        let new_events = app.settings.prefs.new_events();
        let lengths: Vec<Tick> = items
            .iter()
            .map(|(_, info)| {
                info.as_ref()
                    .map_or(bv_timeline::Generator::DEFAULT_CLIP, |m| {
                        new_events.length(m)
                    })
            })
            .collect();
        let overlap = added_overlap(app, project);
        let offsets = added_starts(Tick::ZERO, &lengths, overlap);
        let total = offsets
            .last()
            .zip(lengths.last())
            .map_or(Tick::ZERO, |(&s, &l)| s + l);
        let t = view
            .snap(
                app,
                project,
                view.time_at(l, pos.x),
                &[Tick::ZERO, total],
                &[],
            )
            .max(Tick::ZERO);
        let row = l.row_at(pos.y);
        let track = row.map(|r| r.track);
        // Same choice of tracks as `place_media`: the row under the mouse
        // for its kind (an adjustment track's too), otherwise the first
        // track of the other kind.
        let track_for = |kind: TrackKind| {
            row.filter(|r| r.kind == kind).map(|r| r.track).or_else(|| {
                project
                    .tracks
                    .iter()
                    .find(|t| t.kind == kind && !t.adjustment)
                    .map(|t| t.id)
            })
        };
        let mut ghosts = Vec::new();
        for (((name, info), len), offset) in items.into_iter().zip(lengths).zip(offsets) {
            let at = t + offset;
            let (video, audio) = info.as_ref().map_or_else(
                || {
                    (
                        row.is_none_or(|r| r.kind == TrackKind::Video),
                        row.is_some_and(|r| r.kind == TrackKind::Audio),
                    )
                },
                |m| (m.video.is_some(), m.audio.is_some()),
            );
            for (has, kind) in [(video, TrackKind::Video), (audio, TrackKind::Audio)] {
                if has {
                    ghosts.push(Ghost {
                        at,
                        duration: len,
                        track: track_for(kind),
                        kind,
                        name: name.clone(),
                        info: info.clone(),
                    });
                }
            }
        }
        view.drop = Some(Drop::Ghosts(ghosts));
        if released {
            app.media_drag = None;
            view.snap_line = None;
            let secondary = std::mem::take(&mut app.media_drag_secondary);
            match drag {
                // Dragged with the right button: a menu asks how to add it.
                MediaDrag::Media(_)
                | MediaDrag::Many(_)
                | MediaDrag::File(_)
                | MediaDrag::Files(_)
                    if secondary =>
                {
                    let at = view.time_at(l, pos.x);
                    let clip = track.and_then(|tr| {
                        project.track(tr).and_then(|t| {
                            t.clips
                                .iter()
                                .find(|c| c.start <= at && at < c.start + c.duration)
                                .map(|c| c.id)
                        })
                    });
                    let items = match drag {
                        MediaDrag::Media(id) => vec![DropItem::Media(id)],
                        MediaDrag::Many(ids) => ids.into_iter().map(DropItem::Media).collect(),
                        MediaDrag::File(p) => vec![DropItem::File(p)],
                        MediaDrag::Files(ps) => ps.into_iter().map(DropItem::File).collect(),
                        _ => Vec::new(),
                    };
                    view.media_drop_menu = Some(MediaDropMenu {
                        at: pos,
                        items,
                        time: t,
                        track,
                        clip,
                    });
                }
                // Generated media another event shows: a copy or a
                // reference, as asked.
                MediaDrag::Many(ids) => {
                    let mark = app.project().id_counter();
                    app.add_dropped(ids, t, track, None, Arrange::AcrossTime, (true, true));
                    app.ask_generated_copies(mark);
                }
                MediaDrag::Cells(cells) => {
                    app.add_story_cells(&cells, t, track);
                }
                MediaDrag::Media(id) => {
                    let mark = app.project().id_counter();
                    app.place_media(id, t, track);
                    app.ask_generated_copies(mark);
                }
                MediaDrag::MediaPart(id, part) => {
                    let mark = app.project().id_counter();
                    app.place_media_range(id, t, Some(part), track);
                    app.ask_generated_copies(mark);
                }
                // Files imported and placed: one undo step.
                MediaDrag::FilePart(path, part) => app.as_one_undo(|app| {
                    if let Some(&id) = app.import_paths(&[path]).first() {
                        app.place_media_range(id, t, Some(part), track);
                    }
                }),
                MediaDrag::File(path) => app.as_one_undo(|app| {
                    if let Some(&id) = app.import_paths(&[path]).first() {
                        app.place_media(id, t, track);
                    }
                }),
                MediaDrag::Files(paths) => app.as_one_undo(|app| {
                    app.os_drag = None;
                    let ids = app.import_paths(&paths);
                    let lengths: Vec<Tick> = ids
                        .iter()
                        .map(|&id| {
                            app.project()
                                .media(id)
                                .map_or(Tick::ZERO, |m| new_events.length(m))
                        })
                        .collect();
                    for (id, at) in ids.into_iter().zip(added_starts(t, &lengths, overlap)) {
                        app.place_media(id, at, track);
                    }
                }),
                MediaDrag::Generator(g) => app.add_generator(g, t, track),
                _ => {}
            }
        }
    } else if let Hit::Clip { clip, .. } = hit(view, l, project, pos) {
        view.drop = Some(Drop::OnClip(clip));
        if released {
            app.media_drag = None;
            match drag {
                MediaDrag::Transition(t) => tabs::transitions::apply(app, &[clip], t),
                MediaDrag::PluginTransition(fx) => {
                    tabs::transitions::apply_plugin(app, &[clip], &fx);
                }
                MediaDrag::Effect(effect) => drop_effects(app, clip, &[effect]),
                MediaDrag::Package(_, chain) => drop_effects(app, clip, &chain),
                _ => {}
            }
        }
    } else if matches!(drag, MediaDrag::Effect(_) | MediaDrag::Package(..))
        && let Some(track) = l.row_at(pos.y).map(|r| r.track)
        && project.track(track).is_some_and(|t| t.adjustment)
    {
        // Off its events, an adjustment track takes the effects as its own.
        view.drop = Some(Drop::OnTrack(track));
        if released {
            app.media_drag = None;
            super::headers::drop_track_effects(app, track, drag);
        }
    }
}

/// Video effects (a preset's, a filter package's chain) let go on event
/// `clip`: added to it, which is selected, its settings in Properties.
fn drop_effects(app: &mut BananaApp, clip: ClipId, chain: &[bv_timeline::Effect]) {
    if tabs::effects::apply_to_clips(app, &[clip], chain) == 0 {
        app.notify(tr("Video effects only apply to video clips."));
    } else {
        app.selection.clear();
        app.select_clip(clip, false);
        app.show_tab(Tab::Properties, false);
    }
}

/// How far media added together overlap (Options › Preferences › Editing:
/// "Automatically overlap multiple selected media when added", by the
/// "Cut-to-overlap conversion" amount; Automatic Crossfades must be on).
fn added_overlap(app: &BananaApp, project: &Project) -> Tick {
    let p = &app.settings.prefs;
    if p.overlap_added_media && project.auto_crossfade {
        Tick::from_seconds_f64(p.cut_overlap.max(0.0))
    } else {
        Tick::ZERO
    }
}

/// How much media added one after another overlap (Options › Preferences
/// › Editing), for the app's own adds.
pub(crate) fn added_overlap_for(app: &BananaApp) -> Tick {
    let project = app.project().clone();
    added_overlap(app, &project)
}

/// Where media `lengths` long added one after another start, from `t`:
/// end to end, or overlapping by `overlap` (at most half of either).
pub(crate) fn added_starts(t: Tick, lengths: &[Tick], overlap: Tick) -> Vec<Tick> {
    let mut at = t;
    let mut out = Vec::with_capacity(lengths.len());
    for (i, &len) in lengths.iter().enumerate() {
        out.push(at);
        let next = lengths.get(i + 1).copied().unwrap_or(Tick::ZERO);
        at += len - overlap.min(Tick(len.0 / 2)).min(Tick(next.0 / 2));
    }
    out
}

/// One event of what is being dragged onto the timeline, drawn where it
/// will land and as long as it will be (like VEGAS).
#[derive(Clone, Debug)]
pub(crate) struct Ghost {
    pub at: Tick,
    pub duration: Tick,
    /// `None`: a track of this kind will be created.
    pub track: Option<TrackId>,
    pub kind: TrackKind,
    pub name: String,
    pub info: Option<MediaInfo>,
}

#[derive(Clone, Debug)]
pub(crate) enum Drop {
    Ghosts(Vec<Ghost>),
    OnClip(ClipId),
    /// Video effects over an adjustment track: its own when let go.
    OnTrack(TrackId),
}

/// An event being dragged below the tracks: the new track of `kind` it
/// will land on when let go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NewTrack {
    pub kind: TrackKind,
    pub clip: ClipId,
}

/// Draws the drop preview: event-shaped boxes on the target tracks, with
/// the first frame, the name and the length.
fn paint_ghosts(
    painter: &egui::Painter,
    ctx: &egui::Context,
    app: &mut BananaApp,
    view: &TimelineView,
    l: &Layout,
    project: &Project,
    ghosts: &[Ghost],
) {
    let rate = project.settings.frame_rate;
    let below = l.rows.last().map_or(l.lanes.top(), |r| r.top + r.height);
    for g in ghosts {
        let (top, bottom) = match g.track.and_then(|t| l.row_of(t)) {
            Some(r) => (r.top + 1.0, r.top + r.height - 1.0),
            None => {
                // New track: shown under the last one.
                let h = super::track_height(g.kind, view.compact_tracks);
                let y = if g.kind == TrackKind::Audio
                    && ghosts
                        .iter()
                        .any(|o| o.kind == TrackKind::Video && o.track.is_none())
                {
                    below + super::track_height(TrackKind::Video, view.compact_tracks)
                } else {
                    below
                };
                (y + 1.0, y + h - 1.0)
            }
        };
        let r = Rect::from_min_max(
            pos2(view.x_of(l, g.at), top),
            pos2(
                view.x_of(l, g.at + g.duration)
                    .max(view.x_of(l, g.at) + 4.0),
                bottom,
            ),
        );
        // The color of the track it lands on (a new track's: the one it
        // will get).
        let prefs = &app.settings.prefs;
        let accent = event_fill(match g.track.and_then(|t| project.track(t)) {
            Some(track) => prefs.color_of(track),
            None => prefs.track_color(project.new_track_color(g.kind)),
        });
        let cp = painter.with_clip_rect(r.intersect(l.lanes));
        cp.rect_filled(r, 3.0, accent.gamma_multiply(0.6));
        // First frame at the start of a video event.
        if g.kind == TrackKind::Video
            && let Some(info) = &g.info
            && let Some(v) = info.video
        {
            let h = (r.height() - TITLE_H).max(8.0);
            if let Some(thumb) = app.engine.thumbnail(info, Tick::ZERO, h.round() as u32) {
                let w = h * v.width as f32 / v.height.max(1) as f32;
                let tr = Rect::from_min_size(
                    pos2(r.left(), r.top() + TITLE_H),
                    vec2(w.min(r.width()), h),
                );
                let tex = app.textures.get(ctx, &thumb);
                cp.image(
                    tex,
                    tr,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2((tr.width() / w).min(1.0), 1.0)),
                    Color32::from_white_alpha(170),
                );
            }
        }
        cp.rect_filled(
            Rect::from_min_max(r.min, pos2(r.right(), r.top() + TITLE_H)),
            egui::CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            },
            accent.gamma_multiply(0.9),
        );
        cp.text(
            pos2(r.left() + 5.0, r.top() + 1.0),
            Align2::LEFT_TOP,
            &g.name,
            FontId::proportional(11.5),
            ON_EVENT,
        );
        cp.text(
            pos2(r.right() - 5.0, r.bottom() - 3.0),
            Align2::RIGHT_BOTTOM,
            bv_core::timecode::format(g.duration, rate),
            FontId::monospace(10.5),
            ON_EVENT,
        );
        cp.rect_stroke(
            r,
            3.0,
            Stroke::new(1.5, theme::banana()),
            StrokeKind::Inside,
        );
    }
}

/// A fade's curve from `x0` to `x1` over the event `r` of a `kind` track,
/// as VEGAS draws it: the fade's gain times the event's `level`, on the
/// level line's scale — from the bottom corner up to the level line (its
/// opacity, its gain) —, the curve as the event plays it.
fn fade_points(
    r: Rect,
    x0: f32,
    x1: f32,
    kind: TrackKind,
    curve: FadeCurve,
    rising: bool,
    level: f32,
) -> Vec<Pos2> {
    (0..=24)
        .map(|i| {
            let f = i as f32 / 24.0;
            let x = x0 + (x1 - x0) * f;
            let (curve, p) = if rising {
                (curve, f)
            } else {
                (curve.out_shape(), 1.0 - f)
            };
            let g = match kind {
                TrackKind::Video => bv_timeline::scene::video_curve(curve, p),
                TrackKind::Audio => curve.eval(p),
            };
            pos2(x, level_y(r, g * level.clamp(0.0, 1.0)))
        })
        .collect()
}

/// An audio event's fades' gain at `t` on the timeline, `fade_in` and
/// `fade_out` long (its crossfades' where it has them): each fade's curve as
/// the event plays it and the timeline draws it.
fn fade_gain(clip: &bv_timeline::Clip, fade_in: Tick, fade_out: Tick, t: Tick) -> f32 {
    let share = |from: Tick, len: Tick| ((t - from).0 as f64 / len.0.max(1) as f64).clamp(0.0, 1.0);
    let mut g = 1.0;
    if fade_in > Tick::ZERO && t < clip.start + fade_in {
        g *= clip.fade_in_curve.eval(share(clip.start, fade_in) as f32);
    }
    let from = clip.end() - fade_out;
    if fade_out > Tick::ZERO && t > from {
        g *= clip
            .fade_out_curve
            .out_shape()
            .eval(1.0 - share(from, fade_out) as f32);
    }
    g
}

/// An audio event's waveform over the columns `x0..x1` of `body`, each as
/// loud as the event plays it there — its level and its fades (its
/// crossfades too), as VEGAS's shrinks along a fade, live while one is
/// dragged; `zero_line`: its channels' zero lines too (below the summary's
/// 10 ms of media a pixel, the samples themselves).
#[allow(clippy::too_many_arguments)]
fn paint_waveform(
    cp: &egui::Painter,
    app: &BananaApp,
    view: &TimelineView,
    l: &Layout,
    track: &bv_timeline::Track,
    clip: &bv_timeline::Clip,
    info: &MediaInfo,
    body: Rect,
    (x0, x1): (i32, i32),
    color: Color32,
    auto_crossfade: bool,
    zero_line: bool,
) {
    let stream = clip.audio_stream;
    let wf = app.engine.waveform(info, stream);
    let px = view.ticks_per_px * clip.rate.abs();
    let detail = px < Tick::SECOND.0 as f64 / 100.0;
    if wf.is_none() && !detail {
        return;
    }
    let channels = info.audio_stream(stream).map_or(1, |a| a.channels);
    let chans = (channels as usize).clamp(1, 2);
    let band = body.height() / chans as f32;
    let len = info.duration;
    let (fade_in, fade_out) = track.effective_fades(clip, auto_crossfade);
    let mut chunks = WaveChunks::default();
    for c in 0..chans {
        let mid = (body.top() + band * (c as f32 + 0.5)).round();
        // The channel's zero line, as in VEGAS: pauses show as a line, not
        // as nothing.
        if zero_line {
            cp.rect_filled(
                Rect::from_min_max(pos2(x0 as f32, mid - 0.5), pos2(x1 as f32, mid + 0.5)),
                0.0,
                Color32::from_black_alpha(90),
            );
        }
        // Each column reaches the previous one: a continuous line when
        // zoomed far in.
        let mut prev: Option<(f32, f32)> = None;
        for x in x0..x1 {
            let mut s0 = clip.media_time(view.time_at(l, x as f32), len);
            let mut s1 = clip.media_time(view.time_at(l, x as f32 + 1.0), len);
            if s1 < s0 && clip.reverse.is_some() {
                // Reversed: the column spans its media backward.
                std::mem::swap(&mut s0, &mut s1);
            } else if s1 < s0 {
                // Where a looped event starts over.
                s1 = s0 + Tick(px as i64);
            }
            let range = if detail {
                chunks.range(app, info, stream, c, s0, s1)
            } else {
                None
            }
            .or_else(|| wf.as_ref().map(|w| w.range(c, s0, s1)));
            let Some((lo, hi)) = range else {
                prev = None;
                continue;
            };
            let (lo, hi) = match prev.replace((lo, hi)) {
                Some((plo, phi)) => (lo.min(phi), hi.max(plo)),
                None => (lo, hi),
            };
            let gain = fade_gain(clip, fade_in, fade_out, view.time_at(l, x as f32 + 0.5));
            let k = band * 0.48 * clip.level.min(2.0) * gain;
            // A column at least a pixel tall, so that a flat stretch stays
            // a solid line. Whole pixels: no faint, blurred edges.
            let (mut ya, mut yb) = ((mid - hi * k).floor(), (mid - lo * k).ceil());
            if yb - ya < 1.0 {
                (ya, yb) = (ya, ya + 1.0);
            }
            cp.rect_filled(
                Rect::from_min_max(pos2(x as f32, ya), pos2(x as f32 + 1.0, yb)),
                0.0,
                color,
            );
        }
    }
}

/// Where an event's level line runs: between its fades (its crossfades'
/// ends), as VEGAS draws it — their curves are the line there. `None` when
/// the fades leave no room.
fn level_span(
    view: &TimelineView,
    track: &bv_timeline::Track,
    clip: &bv_timeline::Clip,
    r: Rect,
    auto_crossfade: bool,
) -> Option<(f32, f32)> {
    let (fade_in, fade_out) = track.effective_fades(clip, auto_crossfade);
    let a = r.left() + px(view, fade_in);
    let b = r.right() - px(view, fade_out);
    (b > a).then_some((a, b))
}

/// The line VEGAS draws across an event where a fade ends: the event's
/// height, the color of its border.
fn paint_fade_edge(p: &egui::Painter, x: f32, r: Rect) {
    let x = p.round_to_pixel_center(x);
    p.line_segment(
        [pos2(x, r.top() + 1.0), pos2(x, r.bottom() - 1.0)],
        Stroke::new(1.0, Color32::from_gray(20)),
    );
}

/// The grid's lines (Options › Grid Spacing) over the heights `ys`, as
/// VEGAS draws them on the timeline's background: a pixel wide, dotted
/// three on and three off, in step with the screen (the same in every
/// row), where they are far enough apart.
pub(super) fn paint_grid(
    painter: &egui::Painter,
    view: &TimelineView,
    l: &Layout,
    app: &BananaApp,
    ys: egui::Rangef,
) {
    let step = app.grid_ticks(view).0.max(1);
    if (step as f64 / view.ticks_per_px) < 8.0 {
        return;
    }
    let ppp = painter.pixels_per_point();
    let color = theme::grid();
    let mut mesh = egui::Mesh::default();
    let origin = app.grid_origin();
    let mut k = (view.view_start - origin).0.div_euclid(step);
    loop {
        let x = view.x_of(l, origin + Tick(k * step));
        if x > l.lanes.right() {
            break;
        }
        k += 1;
        if x < l.lanes.left() {
            continue;
        }
        // One pixel of the screen.
        let left = (x * ppp).round() / ppp;
        let right = left + 1.0 / ppp;
        let mut y = (ys.min / 6.0).floor() * 6.0;
        while y < ys.max {
            let (top, bottom) = (y.max(ys.min), (y + 3.0).min(ys.max));
            if bottom > top {
                mesh.add_colored_rect(
                    Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
                    color,
                );
            }
            y += 6.0;
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// How far each event is out of sync with the other part of its media, as
/// VEGAS tells it: a video event and an audio event of the same file that
/// overlap on the timeline show the media apart by that much (moved after
/// U, or split and moved). Each event is matched with the one overlapping
/// it longest; events of other speeds are left alone.
pub(crate) fn sync_offsets(project: &Project) -> std::collections::HashMap<ClipId, Tick> {
    use std::collections::HashMap;
    type Events<'a> = Vec<&'a bv_timeline::Clip>;
    // Each media's video events and audio events.
    let mut by_media: HashMap<bv_timeline::MediaId, (Events<'_>, Events<'_>)> = HashMap::new();
    for (t, c) in project.clips() {
        if c.freeze.is_none() {
            let (video, audio) = by_media.entry(c.media).or_default();
            match t.kind {
                TrackKind::Video => video.push(c),
                TrackKind::Audio => audio.push(c),
            }
        }
    }
    // Where the media's start falls on the timeline.
    let origin = |c: &bv_timeline::Clip| c.start - c.to_timeline(c.source_in);
    let half_frame = Tick(project.settings.frame_rate.frame_duration().0 / 2);
    let mut out = HashMap::new();
    // Drawn every frame, and one long recording may be cut into thousands
    // of events: only the partners that overlap are looked at. By their
    // starts, with how far the ones so far reach, those that can overlap
    // an event are found going back from where it ends.
    let mut match_with = |events: &[&bv_timeline::Clip], others: &mut Events<'_>| {
        others.sort_by_key(|o| o.start);
        let mut reach = Tick::ZERO;
        let reaches: Vec<Tick> = others
            .iter()
            .map(|o| {
                reach = reach.max(o.end());
                reach
            })
            .collect();
        for c in events {
            let mut partner: Option<(&bv_timeline::Clip, Tick)> = None;
            let until = others.partition_point(|o| o.start < c.end());
            for i in (0..until).rev() {
                if reaches[i] <= c.start {
                    break;
                }
                let o = others[i];
                let overlap = c.end().min(o.end()) - c.start.max(o.start);
                if o.rate == c.rate
                    && overlap > Tick::ZERO
                    && partner.is_none_or(|(_, longest)| overlap > longest)
                {
                    partner = Some((o, overlap));
                }
            }
            if let Some((o, _)) = partner {
                let d = origin(c) - origin(o);
                if d.abs() >= half_frame {
                    out.insert(c.id, d);
                }
            }
        }
    };
    for (video, audio) in by_media.values_mut() {
        match_with(video, audio);
        match_with(audio, video);
    }
    out
}

impl BananaApp {
    /// The first audio event made the sound of the first video event, 27
    /// seconds and 2 frames later in it: both show how far apart they are
    /// (for screen tests).
    pub fn sync_offset_for_review(&mut self) {
        let p = self.project();
        let video = p
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| c.media);
        let audio = p
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Audio)
            .map(|(_, c)| c.id);
        let rate = p.settings.frame_rate;
        if let (Some(media), Some(a)) = (video, audio) {
            self.modify(|p| {
                if let Some(c) = p.clip_mut(a) {
                    c.media = media;
                    c.source_in = rate.frame_start(27 * 30 + 2);
                }
            });
        }
    }

    /// The demo's events at 4 seconds (the sunset and la) grouped and the
    /// sunset clicked, la selected along with it; the bars Ctrl+clicked
    /// (for screen tests).
    pub fn grouped_selection_for_review(&mut self) {
        let at4 = |p: &Project, kind| {
            p.clips()
                .find(|(t, c)| t.kind == kind && c.start == Tick::from_seconds(4))
                .map(|(_, c)| c.id)
        };
        let p = self.project();
        let bars = p.tracks.first().and_then(|t| t.clips.first()).map(|c| c.id);
        let (Some(sunset), Some(la), Some(bars)) =
            (at4(p, TrackKind::Video), at4(p, TrackKind::Audio), bars)
        else {
            return;
        };
        self.modify(|p| p.group_clips(&[sunset, la]));
        self.select_clip(sunset, false);
        self.select_clip(bars, true);
    }
}

/// "+27:02": an offset in seconds and frames (minutes and hours before
/// them when it is that long), as VEGAS writes it.
pub(crate) fn sync_text(offset: Tick, rate: bv_core::FrameRate) -> String {
    let frames = (offset.as_seconds_f64() * rate.as_f64()).round() as i64;
    let sign = if frames < 0 { '-' } else { '+' };
    let fps = (rate.as_f64().round() as i64).max(1);
    let f = frames.abs();
    let (secs, ff) = (f / fps, f % fps);
    let (mins, ss) = (secs / 60, secs % 60);
    let (hours, mm) = (mins / 60, mins % 60);
    if hours > 0 {
        format!("{sign}{hours}:{mm:02}:{ss:02}:{ff:02}")
    } else if mins > 0 {
        format!("{sign}{mins}:{ss:02}:{ff:02}")
    } else {
        format!("{sign}{ss}:{ff:02}")
    }
}

/// VEGAS's red box with how far an event is out of sync.
fn sync_badge(painter: &egui::Painter, at: Pos2, text: String) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.5), Color32::WHITE);
    let r = Rect::from_min_size(at, galley.size() + vec2(10.0, 4.0));
    painter.rect_filled(r.expand(1.0), 1.0, Color32::from_gray(0x14));
    painter.rect_filled(r, 0.0, Color32::from_rgb(0xF0, 0x64, 0x3C));
    painter.galley(r.min + vec2(5.0, 2.0), galley, Color32::WHITE);
}

/// The wavy line along a video event played at another rate, as VEGAS
/// draws it: the tighter the waves, the faster.
fn rate_wave(painter: &egui::Painter, from: f32, to: f32, y: f32, rate: f64) {
    let period = (12.0 / rate as f32).clamp(3.0, 48.0);
    let amp = 3.0;
    let mut pts = Vec::new();
    let mut x = from;
    let mut up = true;
    while x <= to {
        pts.push(pos2(x, if up { y - amp } else { y + amp }));
        x += period / 2.0;
        up = !up;
    }
    if pts.len() >= 2 {
        painter.add(Shape::line(
            pts.clone(),
            Stroke::new(3.0, Color32::from_white_alpha(70)),
        ));
        painter.add(Shape::line(pts, Stroke::new(1.5, Color32::BLACK)));
    }
}

/// An audio event's rate, at its right above its buttons ("154.3%").
fn rate_badge(painter: &egui::Painter, right_bottom: Pos2, rate: f64) {
    let text = format!("{}%", crate::fmt::num(rate * 100.0, 1));
    let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), ON_EVENT);
    let r = Rect::from_min_max(right_bottom - galley.size() - vec2(8.0, 4.0), right_bottom);
    painter.rect_filled(r, 2.0, Color32::from_black_alpha(150));
    painter.galley(r.min + vec2(4.0, 2.0), galley, ON_EVENT);
}

pub(crate) fn paint(
    view: &TimelineView,
    ui: &egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    let painter = ui.painter().with_clip_rect(l.lanes);
    painter.rect_filled(l.lanes, 0.0, theme::bg());
    // Below the tracks too (each track's row draws its own over it).
    paint_grid(&painter, view, l, app, l.lanes.y_range());
    let offsets = sync_offsets(project);
    let ctx = ui.ctx().clone();
    // The crossfades' lengths, drawn last.
    let mut badges = Vec::new();
    for row in &l.rows {
        let Some(track) = project.track(row.track) else {
            continue;
        };
        let lane = row.rect(l.lanes.left(), l.lanes.right());
        if !lane.intersects(l.lanes) {
            continue;
        }
        let lane_color = match row.kind {
            TrackKind::Video => theme::track_video(),
            TrackKind::Audio => theme::track_audio(),
        };
        // Events take their track's color at half its brightness, and the
        // waveform the color itself, as VEGAS draws them (the other way
        // round with "Reverse Event Background color").
        let track_color = app.settings.prefs.color_of(track);
        let (accent, wave_color) = if app.settings.prefs.reverse_event_colors {
            (track_color, event_fill(track_color))
        } else {
            (event_fill(track_color), track_color)
        };
        painter.rect_filled(
            lane,
            0.0,
            if track.muted {
                theme::panel_dark()
            } else {
                lane_color
            },
        );
        // The grid under the line between the tracks, as VEGAS's.
        paint_grid(&painter, view, l, app, lane.y_range());
        painter.line_segment(
            [lane.left_bottom(), lane.right_bottom()],
            Stroke::new(1.0, theme::panel_dark()),
        );
        // Expand Track Layers: the transition roll between the A and the B
        // roll, where events overlap their transition.
        if view.layer_starts.contains_key(&row.track) {
            paint_transition_roll(view, &painter, l, row, track);
        }
        let mut outlines = Vec::new();
        // The handles' tabs: drawn last, over the outlines, as VEGAS's.
        let mut fade_tabs: Vec<(Pos2, f32)> = Vec::new();
        let mut level_tabs: Vec<Pos2> = Vec::new();
        for (ci, clip) in track.clips.iter().enumerate() {
            let r = clip_rect(view, l, row, clip.start, clip.end());
            if r.right() < l.lanes.left() || r.left() > l.lanes.right() {
                continue;
            }
            // A linked event (sync links) shows shorter, as VEGAS's.
            let r = if clip.sync_link.is_some() {
                r.shrink2(vec2(0.0, (r.height() * 0.08).min(6.0)))
            } else {
                r
            };
            let info = project.media(clip.media);
            let selected = app.selection.contains(&clip.id);
            let generated = info.is_some_and(|m| matches!(m.source, MediaSource::Generated(_)));
            let base = if generated {
                accent.gamma_multiply(0.85)
            } else {
                accent
            };
            // Options › Preferences › Editing: "Change cursor and selected
            // event color when in Auto Ripple mode".
            let ripple = app.auto_ripple && app.settings.prefs.ripple_colors;
            let fill = match (selected, ripple) {
                (true, true) => base.lerp_to_gamma(theme::ripple(), 0.35),
                (true, false) => base.gamma_multiply(1.35),
                _ => base,
            };
            painter.rect_filled(r, 3.0, fill);
            // Audio: where another event overlaps it (a crossfade), darker,
            // as VEGAS's; its sound and name drawn over.
            if row.kind == TrackKind::Audio && project.auto_crossfade && !clip.muted {
                for other in track.clips.iter().filter(|o| o.id != clip.id && !o.muted) {
                    let (from, to) = (clip.start.max(other.start), clip.end().min(other.end()));
                    if to > from {
                        let over = Rect::from_min_max(
                            pos2(view.x_of(l, from), r.top()),
                            pos2(view.x_of(l, to), r.bottom()),
                        );
                        painter.rect_filled(over.intersect(r), 0.0, OVERLAP_SHADE);
                    }
                }
            }
            let title_h = if view.show_clip_headers { TITLE_H } else { 0.0 };
            let body = Rect::from_min_max(pos2(r.left(), r.top() + title_h), r.max);
            let cp = painter.with_clip_rect(r.intersect(l.lanes).shrink(1.0));

            // An adjustment event: diagonal shading, as VEGAS marks one.
            if let Some(info) = info.filter(|m| m.is_adjustment()) {
                let _ = info;
                let shade = Color32::from_white_alpha(28);
                let mut x = body.left() - body.height();
                while x < body.right() {
                    cp.line_segment(
                        [pos2(x, body.bottom()), pos2(x + body.height(), body.top())],
                        Stroke::new(3.0, shade),
                    );
                    x += 10.0;
                }
            }
            // Miniaturas ou forma de onda.
            if let Some(info) = info.filter(|m| !m.is_adjustment()) {
                match row.kind {
                    TrackKind::Video
                        if view.show_thumbnails
                            && app.settings.prefs.thumbnails != Thumbnails::None
                            && body.height() > 12.0 =>
                    {
                        if let Some(v) = info.video {
                            let th = body.height().round() as u32;
                            let tw = th as f32 * v.width as f32 * v.pixel_aspect.ratio()
                                / v.height as f32;
                            let prefs = &app.settings.prefs;
                            for (x, t) in thumbnail_spots(view, l, clip, r, tw, prefs.thumbnails) {
                                let at = clip.media_time(t, info.duration);
                                let rr = Rect::from_min_size(
                                    pos2(x, body.top()),
                                    vec2(tw, body.height()),
                                );
                                if let Some(thumb) = app.engine.thumbnail(info, at, th) {
                                    let tex = app.textures.get(&ctx, &thumb);
                                    cp.image(
                                        tex,
                                        rr,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        Color32::from_white_alpha(225),
                                    );
                                }
                                let number = match prefs.frame_numbers {
                                    FrameNumbers::None => None,
                                    FrameNumbers::Frames => {
                                        Some(v.frame_rate.frame_at(at).to_string())
                                    }
                                    FrameNumbers::Time => {
                                        // When the frame shown starts.
                                        let rate = v.frame_rate;
                                        let start = rate.frame_start(rate.frame_at(at));
                                        Some(
                                            RulerFormat::Time
                                                .format_ruler_time(start, &project.settings),
                                        )
                                    }
                                    FrameNumbers::Timecode => {
                                        Some(bv_core::timecode::format(at, v.frame_rate))
                                    }
                                };
                                if let Some(text) = number {
                                    frame_number_box(&cp, rr, text);
                                }
                            }
                        }
                    }
                    TrackKind::Audio if view.show_waveforms => {
                        let x0 = r.left().max(l.lanes.left()) as i32;
                        let x1 = r.right().min(l.lanes.right()) as i32;
                        let auto = project.auto_crossfade;
                        paint_waveform(
                            &cp,
                            app,
                            view,
                            l,
                            track,
                            clip,
                            info,
                            body,
                            (x0, x1),
                            wave_color,
                            auto,
                            true,
                        );
                        // A crossfade: the sound going out over its part too,
                        // the two merged, as VEGAS's.
                        for a in track.clips[..ci]
                            .iter()
                            .filter(|a| auto && a.end() > clip.start && !a.muted && !clip.muted)
                        {
                            let Some(a_info) = project.media(a.media).filter(|m| m.audio.is_some())
                            else {
                                continue;
                            };
                            let ax0 = view.x_of(l, clip.start).max(x0 as f32) as i32;
                            let ax1 = view.x_of(l, a.end().min(clip.end())).min(x1 as f32) as i32;
                            if ax1 > ax0 {
                                paint_waveform(
                                    &cp,
                                    app,
                                    view,
                                    l,
                                    track,
                                    a,
                                    a_info,
                                    body,
                                    (ax0, ax1),
                                    wave_color,
                                    auto,
                                    false,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }

            // A changed playback rate (Ctrl+drag of an edge), as VEGAS shows
            // it: a wavy line along a video event, the rate on an audio one.
            if (clip.rate - 1.0).abs() > 1e-6 {
                match row.kind {
                    TrackKind::Video => rate_wave(
                        &cp,
                        r.left().max(l.lanes.left()),
                        r.right().min(l.lanes.right()),
                        body.center().y,
                        clip.rate,
                    ),
                    TrackKind::Audio if body.height() >= 30.0 => {
                        let right = r.right().min(l.lanes.right()) - 3.0;
                        rate_badge(&cp, pos2(right, r.bottom() - 6.0 - BUTTON), clip.rate);
                    }
                    TrackKind::Audio => {}
                }
            }

            // Barra de título com nome e indicadores.
            let header = view.show_clip_headers;
            if header {
                cp.rect_filled(
                    Rect::from_min_max(r.min, pos2(r.right(), r.top() + TITLE_H)),
                    egui::CornerRadius {
                        nw: 3,
                        ne: 3,
                        sw: 0,
                        se: 0,
                    },
                    fill.gamma_multiply(0.8),
                );
                let mut tx = r.left().max(l.lanes.left()) + 5.0;
                if clip.locked {
                    icons::paint(
                        &cp,
                        Rect::from_min_size(pos2(tx, r.top() + 2.0), vec2(10.0, 10.0)),
                        Icon::Lock,
                        ON_EVENT,
                    );
                    tx += 13.0;
                }
                // The active take's name: its own, or its media's.
                let name = clip
                    .takes
                    .get(clip.active_take())
                    .and_then(|t| t.name.clone())
                    .unwrap_or_else(|| info.map_or("?".to_string(), crate::i18n::media_name));
                let label = if view.show_take_info {
                    let rate = project.settings.frame_rate;
                    let take = if clip.take_count() > 1 {
                        format!("{}/{} ", clip.active_take() + 1, clip.take_count())
                    } else {
                        String::new()
                    };
                    let mut s = format!(
                        "{take}{name} · {}",
                        bv_core::timecode::format(clip.source_in, rate)
                    );
                    if clip.rate != 1.0 {
                        s.push_str(&format!(" · {}×", crate::fmt::num(clip.rate, 2)));
                    }
                    if clip.freeze.is_some() {
                        s.push_str(tr(" · frozen"));
                    }
                    s
                } else {
                    name.to_string()
                };
                let label = if view.show_event_duration {
                    let rate = project.settings.frame_rate;
                    format!(
                        "{label} · {}",
                        bv_core::timecode::format(clip.duration, rate)
                    )
                } else {
                    label
                };
                cp.text(
                    pos2(tx, r.top() + 1.0),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(11.5),
                    ON_EVENT,
                );
                // Its pitch change in the bottom-left corner, as VEGAS's
                // Active Take Information shows it.
                if view.show_take_info && clip.pitch != 0.0 && r.height() >= 30.0 {
                    cp.text(
                        pos2(r.left().max(l.lanes.left()) + 5.0, r.bottom() - 2.0),
                        Align2::LEFT_BOTTOM,
                        format!("{:+.2}", clip.pitch),
                        FontId::proportional(11.0),
                        ON_EVENT,
                    );
                }
            }

            // Out of sync with the other part of its media, as VEGAS shows it:
            // by how much, in red, under the name.
            if let Some(&d) = offsets.get(&clip.id) {
                let text = sync_text(d, project.settings.frame_rate);
                let y = r.top() + title_h + 3.0;
                if r.bottom() - y >= 16.0 {
                    sync_badge(&cp, pos2(r.left().max(l.lanes.left()) + 3.0, y), text);
                }
            }

            // Reversed: an arrow pointing back across the event, as VEGAS
            // marks one.
            if clip.reverse.is_some() && body.height() > 10.0 {
                let y = body.center().y;
                let x0 = r.left().max(l.lanes.left()) + 6.0;
                let x1 = (x0 + 22.0).min(r.right() - 4.0);
                if x1 > x0 + 8.0 {
                    let stroke = Stroke::new(2.0, ON_EVENT);
                    cp.line_segment([pos2(x0, y), pos2(x1, y)], stroke);
                    cp.add(egui::Shape::convex_polygon(
                        vec![
                            pos2(x0 - 1.0, y),
                            pos2(x0 + 6.0, y - 5.0),
                            pos2(x0 + 6.0, y + 5.0),
                        ],
                        ON_EVENT,
                        Stroke::NONE,
                    ));
                }
            }

            // Where the media starts over (Loop): VEGAS's notches on the top
            // edge.
            if let Some(info) = info
                && !info.still
                && matches!(info.source, bv_timeline::MediaSource::File { .. })
            {
                for t in clip.loop_points(info.duration) {
                    let x = view.x_of(l, t);
                    cp.add(egui::Shape::convex_polygon(
                        vec![
                            pos2(x - 4.5, r.top()),
                            pos2(x + 4.5, r.top()),
                            pos2(x, r.top() + 6.0),
                        ],
                        Color32::from_white_alpha(230),
                        Stroke::NONE,
                    ));
                    cp.line_segment(
                        [pos2(x, r.top() + 6.0), pos2(x, r.bottom())],
                        Stroke::new(1.0, Color32::from_black_alpha(70)),
                    );
                }
            }

            // View › Event Media Markers: the markers and regions of the
            // media file (the Trimmer's), where the event shows them.
            if view.show_media_markers
                && let Some(info) = info
            {
                paint_media_markers(&cp, view, l, r, clip, info);
            }

            // Fades (curves and handles). Where an event overlaps, the
            // crossfade is the fade (its X below), without a handle: VEGAS
            // does not adjust fades between overlapping events.
            let (cross_in, cross_out) = if project.auto_crossfade {
                track.crossfades(clip)
            } else {
                (None, None)
            };
            let (fi, fo) = (px(view, clip.fade_in), px(view, clip.fade_out));
            let curve_stroke = Stroke::new(1.5, theme::banana());
            // As VEGAS's: from the bottom corner up to the level line, and
            // a dark line across the event where the fade ends.
            if fi > 0.5 && cross_in.is_none() {
                let x1 = r.left() + fi;
                paint_fade_edge(&cp, x1, r);
                let pts = fade_points(
                    r,
                    r.left(),
                    x1,
                    row.kind,
                    clip.fade_in_curve,
                    true,
                    clip.level,
                );
                cp.add(Shape::line(pts, curve_stroke));
            }
            if fo > 0.5 && cross_out.is_none() {
                let x0 = r.right() - fo;
                paint_fade_edge(&cp, x0, r);
                let pts = fade_points(
                    r,
                    x0,
                    r.right(),
                    row.kind,
                    clip.fade_out_curve,
                    false,
                    clip.level,
                );
                cp.add(Shape::line(pts, curve_stroke));
            }
            if view.show_fade_durations {
                let rate = project.settings.frame_rate;
                let joined_in = track.clips[..ci].iter().any(|o| o.end() > clip.start);
                let joined_out = track.clips[ci + 1..].iter().any(|o| o.start < clip.end());
                for (len, w, x0, joined) in [
                    (clip.fade_in, fi, r.left(), joined_in),
                    (clip.fade_out, fo, r.right() - fo, joined_out),
                ] {
                    if len > Tick::ZERO && w > 30.0 && !joined {
                        duration_badge(
                            &cp,
                            pos2(x0 + w / 2.0, r.center().y + 6.0),
                            Align2::CENTER_CENTER,
                            r,
                            short_duration(len, rate),
                        );
                    }
                }
            }
            if view.show_envelopes && (selected || ui.rect_contains_pointer(r)) {
                for (x, way, crossed) in [
                    (r.left() + fi, 1.0, cross_in.is_some()),
                    (r.right() - fo, -1.0, cross_out.is_some()),
                ] {
                    // VEGAS's blue corner, on the level line where the
                    // curve ends: it comes down with the line (the event's
                    // top corner without a fade).
                    if !crossed {
                        fade_tabs.push((pos2(x, level_y(r, clip.level) - 3.0), way));
                    }
                }
            }

            // Level line: between the fades, as VEGAS draws it (their
            // curves are the line there).
            let ly = level_y(r, clip.level);
            let span = level_span(view, track, clip, r, project.auto_crossfade);
            let level_dragged =
                matches!(view.drag, Some(Drag::Level { clip: c, .. }) if c == clip.id);
            let on_screen = uncovered(view, l, track, ci, r).intersect(l.lanes);
            // Shown while the event is picked or under the pointer, as
            // VEGAS's.
            if level_dragged
                || ((selected || ui.rect_contains_pointer(on_screen)) && view.drag.is_none())
            {
                // In the middle of the line between the fades, of its part
                // on screen, as VEGAS's (an event without fades: of the part
                // the next one does not cover).
                let x = match span {
                    Some((a, b)) => {
                        let (lo, hi) = (a.max(on_screen.left()), b.min(on_screen.right()));
                        if hi > lo {
                            (lo + hi) / 2.0
                        } else {
                            on_screen.center().x.clamp(a, b)
                        }
                    }
                    None => on_screen.center().x,
                };
                level_tabs.push(pos2(x, ly));
            }
            let lc = match (view.show_envelopes, clip.level < 1.0) {
                (false, _) => Color32::TRANSPARENT,
                (true, true) => theme::banana(),
                (true, false) => Color32::from_white_alpha(60),
            };
            if let Some((a, b)) = span {
                cp.line_segment([pos2(a, ly), pos2(b, ly)], Stroke::new(1.2, lc));
            }
            if clip.level < 1.0 {
                let label = match row.kind {
                    TrackKind::Video => format!("{:.0}%", clip.level * 100.0),
                    TrackKind::Audio if clip.level <= 0.0 => "−∞ dB".to_string(),
                    TrackKind::Audio => format!("{:.1} dB", 20.0 * clip.level.log10()),
                };
                let vis = uncovered(view, l, track, ci, r);
                let right = clip_buttons(view, l, row.kind, generated, vis)
                    .first()
                    .map_or(vis.right() - 5.0, |(_, br)| br.left() - 4.0);
                cp.text(
                    pos2(right, r.bottom() - 3.0),
                    Align2::RIGHT_BOTTOM,
                    label,
                    FontId::proportional(11.0),
                    theme::banana(),
                );
            }
            // Event buttons (Pan/Crop, FX, extras) at the right end of the
            // part of the event that is not covered by the next one.
            let pointer = ui.input(|i| i.pointer.latest_pos());
            let has_fx = clip.effects.iter().any(|e| e.enabled)
                || info.is_some_and(|m| m.effects.iter().any(|e| e.enabled));
            let vis = uncovered(view, l, track, ci, r);
            for (b, br) in clip_buttons(view, l, row.kind, generated, vis) {
                let hovered = pointer.is_some_and(|p| br.contains(p));
                // Each in a dark square over the picture, as VEGAS's.
                cp.rect_filled(br, 1.0, Color32::from_black_alpha(165));
                if hovered {
                    cp.rect_filled(br, 1.0, Color32::from_white_alpha(45));
                }
                let (icon, color) = match b {
                    ClipButton::Generated => (Icon::Generator, ON_EVENT),
                    ClipButton::PanCrop => (
                        Icon::PanCrop,
                        if clip.pan_crop.is_default() {
                            ON_EVENT
                        } else {
                            theme::banana()
                        },
                    ),
                    ClipButton::Fx => (Icon::Fx, if has_fx { theme::banana() } else { ON_EVENT }),
                    ClipButton::More => (Icon::More, ON_EVENT),
                };
                // A dark shadow keeps the white icon readable on any picture.
                let ir = br.shrink(2.5);
                icons::paint(
                    &cp,
                    ir.translate(vec2(1.0, 1.0)),
                    icon,
                    Color32::from_black_alpha(200),
                );
                icons::paint(&cp, ir, icon, color);
            }
            // The snap offset flag: a white triangle at the bottom, at the
            // event's start or where it was dragged, as VEGAS's.
            let fx = r.left() + px(view, clip.snap_point());
            if r.width() > 16.0 && r.height() > 14.0 && fx < vis.right() - 2.0 {
                cp.add(Shape::convex_polygon(
                    vec![
                        pos2(fx, r.bottom() - 6.0),
                        pos2(fx + 6.0, r.bottom() - 1.0),
                        pos2(fx, r.bottom() - 1.0),
                    ],
                    ON_EVENT,
                    Stroke::new(1.0, Color32::from_black_alpha(120)),
                ));
            }
            if clip.muted {
                cp.rect_filled(r, 3.0, Color32::from_black_alpha(140));
                cp.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    tr("muted"),
                    FontId::proportional(11.0),
                    ON_EVENT_DIM,
                );
            }
            if view.show_unquantized {
                let rate = project.settings.frame_rate;
                for (t, x) in [(clip.start, r.left()), (clip.end(), r.right())] {
                    if rate.snap_nearest(t) != t {
                        cp.add(Shape::convex_polygon(
                            vec![
                                pos2(x - 4.0, r.top()),
                                pos2(x + 4.0, r.top()),
                                pos2(x, r.top() + 6.0),
                            ],
                            theme::error(),
                            Stroke::NONE,
                        ));
                    }
                }
            }
            // As VEGAS's: the events picked outlined in yellow, the members
            // of their groups selected along with them in blue.
            let follows_selected = clip
                .sync_link
                .is_some_and(|k| app.selection.contains(&k.to));
            let outline = match (selected, ripple) {
                (true, true) => theme::ripple(),
                (true, false) if app.selected_by_group(clip.id) => theme::event_grouped(),
                (true, false) => theme::event_picked(),
                // Linked to a selected event: purple.
                _ if follows_selected => theme::event_sync_linked(),
                _ => Color32::from_black_alpha(120),
            };
            // A selected event's outline over what overlaps it (drawn after
            // the track's events), as VEGAS's.
            if selected || follows_selected {
                outlines.push((r, outline));
            } else {
                painter.rect_stroke(r, 3.0, Stroke::new(1.0, outline), StrokeKind::Inside);
            }
            if let Some(Drop::OnClip(id)) = view.drop
                && id == clip.id
            {
                painter.rect_stroke(r, 3.0, Stroke::new(2.5, theme::snap()), StrokeKind::Inside);
            }
        }

        for (r, color) in outlines {
            painter.rect_stroke(r, 3.0, Stroke::new(1.0, color), StrokeKind::Inside);
        }
        for (corner, way) in fade_tabs {
            paint_fade_tab(&painter, corner, way);
        }
        for at in level_tabs {
            paint_level_handle(&painter, at);
        }
        if let Some(Drop::OnTrack(t)) = view.drop
            && t == row.track
        {
            painter.rect_stroke(
                lane.shrink(1.0),
                3.0,
                Stroke::new(2.5, theme::snap()),
                StrokeKind::Inside,
            );
        }

        // Crossfades: X na sobreposição de clipes consecutivos.
        if project.auto_crossfade {
            for (i, a) in track.clips.iter().enumerate() {
                for b in &track.clips[i + 1..] {
                    if b.start >= a.end() || a.muted || b.muted {
                        continue;
                    }
                    let x0 = view.x_of(l, b.start);
                    let x1 = view.x_of(l, a.end().min(b.end()));
                    let area = lane.shrink2(vec2(0.0, 2.0));
                    // A video crossfade with a transition: its label
                    // instead (below), as VEGAS's.
                    let labeled = row.kind == TrackKind::Video && transition_label(b).is_some();
                    // Its length, as VEGAS's: while one of its events is
                    // selected (always with View › Event Fade Lengths),
                    // however far out the zoom is; just under the
                    // transition's label when it has one.
                    let picked = app.selection.contains(&a.id) || app.selection.contains(&b.id);
                    if view.show_fade_durations || picked {
                        let len = a.end().min(b.end()) - b.start;
                        let text = short_duration(len, project.settings.frame_rate);
                        let mid = (x0 + x1) / 2.0;
                        badges.push(if labeled {
                            let band = label_band(view, l, row, b, x0, x1);
                            (
                                pos2(mid, band.bottom() + 1.0),
                                Align2::CENTER_TOP,
                                area,
                                text,
                            )
                        } else {
                            (
                                pos2(mid, area.center().y),
                                Align2::CENTER_CENTER,
                                area,
                                text,
                            )
                        });
                    }
                    if x1 - x0 < 2.0 {
                        continue;
                    }
                    // Each side to its own event's level line.
                    let ra = clip_rect(view, l, row, a.start, a.end());
                    let rb = clip_rect(view, l, row, b.start, b.end());
                    let out = fade_points(ra, x0, x1, row.kind, a.fade_out_curve, false, a.level);
                    let inn = fade_points(rb, x0, x1, row.kind, b.fade_in_curve, true, b.level);
                    let xp = painter.with_clip_rect(l.lanes);
                    // Where it begins and ends, a dark line across it, as
                    // VEGAS's.
                    for x in [x0, x1] {
                        paint_fade_edge(&xp, x, rb);
                    }
                    // As VEGAS's: the curve going out blue, the one coming
                    // in white.
                    let curves = [(out, XFADE), (inn, Color32::from_gray(235))];
                    for (pts, color) in curves.into_iter().filter(|_| !labeled) {
                        xp.add(Shape::line(
                            pts.clone(),
                            Stroke::new(3.5, Color32::from_black_alpha(110)),
                        ));
                        xp.add(Shape::line(pts, Stroke::new(2.0, color)));
                    }
                }
            }
        }
    }

    // The transitions' labels, over the events; the crossfades' lengths
    // over them.
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let xp = painter.with_clip_rect(l.lanes);
    for t in transition_labels(view, l, project) {
        let hot = t.icon.zip(pointer).is_some_and(|(i, p)| i.contains(p));
        paint_transition_label(&xp, &t, hot);
    }
    for (at, align, within, text) in badges {
        duration_badge(&xp, at, align, within, text);
    }
    super::envelopes::paint(view, &painter, l, project, &app.settings.prefs);
    paint_edge_trim(&painter, view, l, project, &app.settings.prefs);
    // Recording: the part recorded so far, on each recording track.
    if let (Some(rec), Some(elapsed)) = (&app.recording, app.engine.recording_time()) {
        let end = rec.start + Tick::from_seconds_f64(elapsed.as_secs_f64());
        for row in l.rows.iter().filter(|r| rec.tracks.contains(&r.track)) {
            let r = Rect::from_min_max(
                pos2(view.x_of(l, rec.start), row.top + 2.0),
                pos2(view.x_of(l, end), row.top + row.height - 2.0),
            );
            let red = Color32::from_rgb(0xE5, 0x39, 0x35);
            painter.rect_filled(r, 2.0, red.gamma_multiply(0.25));
            // Options › Preferences › Audio: "Waveform display while
            // recording".
            if let Some(wave) = app.recording_waves.get(&row.track)
                && wave.rate > 0
                && wave.frames > 0
            {
                let per_peak = wave.frames as f64 / wave.rate as f64;
                let mid = r.center().y;
                let half = r.height() / 2.0 - 3.0;
                let color = Color32::from_rgb(0xFF, 0xD0, 0xD0);
                let x0 = r.left().max(l.lanes.left()) as i32;
                let x1 = r.right().min(l.lanes.right()) as i32;
                for x in x0..x1 {
                    let t0 = (view.time_at(l, x as f32) - rec.start).as_seconds_f64();
                    let t1 = (view.time_at(l, x as f32 + 1.0) - rec.start).as_seconds_f64();
                    let a = (t0 / per_peak).max(0.0) as usize;
                    let b = ((t1 / per_peak).ceil() as usize)
                        .max(a + 1)
                        .min(wave.peaks.len());
                    if a >= b {
                        continue;
                    }
                    let (lo, hi) = wave.peaks[a..b]
                        .iter()
                        .fold((0.0f32, 0.0f32), |(l, h), &(pl, ph)| (l.min(pl), h.max(ph)));
                    painter.line_segment(
                        [
                            pos2(x as f32 + 0.5, mid - hi.clamp(-1.0, 1.0) * half),
                            pos2(x as f32 + 0.5, mid - lo.clamp(-1.0, 1.0) * half + 1.0),
                        ],
                        Stroke::new(1.0, color),
                    );
                }
            }
            painter.rect_stroke(r, 2.0, Stroke::new(1.5, red), StrokeKind::Inside);
            painter.text(
                r.left_top() + vec2(4.0, 2.0),
                Align2::LEFT_TOP,
                tr("Recording"),
                FontId::proportional(11.0),
                Color32::WHITE,
            );
        }
    }

    // Drop preview.
    if let Some(Drop::Ghosts(ghosts)) = &view.drop {
        paint_ghosts(&painter, &ctx, app, view, l, project, ghosts);
    }
    // An event dragged below the tracks: where its new track will be, as
    // VEGAS's ghost (a white outline under the last track).
    if let (Some(nt), Some(Drag::Move { .. })) = (view.new_track, &view.drag)
        && let Some(c) = project.clip(nt.clip)
    {
        let below = l.rows.last().map_or(l.lanes.top(), |r| r.top + r.height);
        let h = super::track_height(nt.kind, view.compact_tracks);
        let r = Rect::from_min_max(
            pos2(view.x_of(l, c.start), below + 1.0),
            pos2(view.x_of(l, c.end()), below + h - 1.0),
        );
        painter.with_clip_rect(l.lanes).rect_stroke(
            r,
            0.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Inside,
        );
    }

    // Retângulos de seleção e de zoom.
    if let (Some(drag @ (Drag::Marquee { start, .. } | Drag::ZoomBox { start })), Some(p)) =
        (&view.drag, ui.input(|i| i.pointer.latest_pos()))
    {
        let r = match drag {
            Drag::Marquee { shape, .. } => shape.rect(l, *start, p),
            _ => Rect::from_two_pos(*start, p),
        };
        painter.rect_filled(r, 0.0, theme::snap().gamma_multiply(0.12));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme::snap()), StrokeKind::Inside);
    }

    // Where an edit snapped, in the color of what it snapped to.
    if let Some((t, kind)) = view.snap_line {
        let x = view.x_of(l, t);
        painter.line_segment(
            [pos2(x, l.lanes.top()), pos2(x, l.lanes.bottom())],
            Stroke::new(1.0, app.settings.prefs.snap_color(kind)),
        );
    }
}

#[cfg(test)]
mod drop_tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bv_core::Tick;
    use bv_timeline::TrackKind;

    use super::Drop;
    use crate::app::{BananaApp, TEST_CURSOR};
    use crate::egui;

    #[derive(Debug)]
    struct File(PathBuf);

    impl egui::DroppedFile for File {
        fn path(&self) -> &Path {
            &self.0
        }

        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }

    /// One second of mono 48 kHz 16-bit silence.
    fn wav(path: &Path) {
        let data = vec![0u8; 96_000];
        let mut b = b"RIFF".to_vec();
        b.extend((36 + data.len() as u32).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(48_000u32.to_le_bytes());
        b.extend(96_000u32.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        std::fs::write(path, b).expect("write wav");
    }

    fn frame(ctx: &egui::Context, app: &mut BananaApp, hovered: &[PathBuf], dropped: &[PathBuf]) {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 960.0),
            )),
            hovered_files: hovered
                .iter()
                .map(|p| egui::HoveredFile {
                    path: Some(p.clone()),
                    ..Default::default()
                })
                .collect(),
            dropped_files: dropped
                .iter()
                .map(|p| Arc::new(File(p.clone())) as egui::DroppedFileHandle)
                .collect(),
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .inner_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 960.0),
        ));
        let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn files_dragged_from_the_system_are_previewed_and_placed() {
        let dir = std::env::temp_dir().join(format!("bananav-arrasto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("um segundo.wav");
        wav(&file);
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        frame(&ctx, &mut app, &[], &[]);
        // Somewhere over the audio track of the timeline.
        let (x, y) = (900, 690);
        TEST_CURSOR.with(|c| c.set(Some((x, y))));
        let files = vec![file.clone()];
        frame(&ctx, &mut app, &files, &[]);
        frame(&ctx, &mut app, &files, &[]);
        let ghosts = match &app.timeline.drop {
            Some(Drop::Ghosts(g)) => g.clone(),
            other => panic!("no drop preview: {other:?}"),
        };
        assert_eq!(ghosts.len(), 1, "audio only");
        assert_eq!(ghosts[0].kind, TrackKind::Audio);
        assert_eq!(ghosts[0].duration, Tick::from_seconds(1), "the real length");
        let at = ghosts[0].at;
        let media = app.project().media.len();
        frame(&ctx, &mut app, &[], &files);
        frame(&ctx, &mut app, &[], &[]);
        TEST_CURSOR.with(|c| c.set(None));
        assert_eq!(app.project().media.len(), media + 1, "imported");
        let placed = app
            .project()
            .clips()
            .find(|(t, c)| t.kind == TrackKind::Audio && c.start == at)
            .map(|(_, c)| c.duration);
        assert_eq!(
            placed,
            Some(Tick::from_seconds(1)),
            "placed where the preview was"
        );
        assert!(app.media_drag.is_none() && app.os_drag.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_files_dropped_overlap_when_asked() {
        let dir = std::env::temp_dir().join(format!("bananav-sobrepor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let files: Vec<PathBuf> = ["a.wav", "b.wav"].iter().map(|n| dir.join(n)).collect();
        for f in &files {
            wav(f);
        }
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        app.settings.prefs.overlap_added_media = true;
        app.settings.prefs.cut_overlap = 0.25;
        frame(&ctx, &mut app, &[], &[]);
        TEST_CURSOR.with(|c| c.set(Some((900, 690))));
        frame(&ctx, &mut app, &files, &[]);
        frame(&ctx, &mut app, &files, &[]);
        let ghosts = match &app.timeline.drop {
            Some(Drop::Ghosts(g)) => g.clone(),
            other => panic!("no drop preview: {other:?}"),
        };
        assert_eq!(ghosts.len(), 2);
        let step = ghosts[1].at - ghosts[0].at;
        assert_eq!(step, Tick::from_seconds_f64(0.75), "a quarter second over");
        let before = app.project().clone();
        let undos = app.history.undo_len();
        frame(&ctx, &mut app, &[], &files);
        frame(&ctx, &mut app, &[], &[]);
        TEST_CURSOR.with(|c| c.set(None));
        assert_eq!(
            app.history.undo_len(),
            undos + 1,
            "imports and events: one undo"
        );
        assert_eq!(app.history.undo_target(), Some(&before));
        let starts: Vec<Tick> = app
            .project()
            .clips()
            .filter(|(t, c)| t.kind == TrackKind::Audio && c.duration == Tick::from_seconds(1))
            .map(|(_, c)| c.start)
            .collect();
        assert!(starts.contains(&ghosts[0].at) && starts.contains(&ghosts[1].at));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A timeline (OpenTimelineIO's) dropped on the window opens as a new
    /// project, as with File › Open; it is no media.
    #[test]
    fn a_timeline_dropped_opens() {
        let dir = std::env::temp_dir().join(format!("bananav-otio-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let mut other = app.project().clone();
        other.tracks[0].clips.pop();
        let path = dir.join("Dropped.otio");
        std::fs::write(&path, bv_timeline::otio::export(&other, "Dropped")).expect("write");
        frame(&ctx, &mut app, &[], &[]);
        let media = app.project().media.len();
        frame(&ctx, &mut app, &[], std::slice::from_ref(&path));
        frame(&ctx, &mut app, &[], &[]);
        assert_eq!(app.project().clips().count(), other.clips().count());
        assert!(app.project().media.len() <= media, "not imported as media");
        assert!(app.project_path.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    #[test]
    fn level_texts_like_vegas() {
        assert_eq!(level_text(TrackKind::Video, 0.52), "Opacity is 52 %");
        assert_eq!(level_text(TrackKind::Audio, 1.0), "Gain is 0.0 dB");
        assert_eq!(level_text(TrackKind::Audio, 0.5), "Gain is -6.0 dB");
        assert_eq!(level_text(TrackKind::Audio, 0.0), "Gain is −∞ dB");
    }

    #[test]
    fn snaps_say_what_they_snapped_to() {
        let mut app = BananaApp::new(egui::Context::default());
        let mut p = Project::default();
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let m = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::ColorBars,
            &p.settings,
        ));
        // A 5 s event from 2 s with a 1 s fade in: edges at 2 s and 7 s,
        // the fade's end at 3 s.
        let c = p
            .insert_media(m, Tick::from_seconds(2), Some(v), None)
            .unwrap()[0];
        p.set_fades(c, Some(Tick::from_seconds(1)), None).unwrap();
        p.add_marker(Tick::from_seconds(11));
        p.toggle_envelope(a, bv_timeline::EnvelopeKind::Volume)
            .unwrap();
        p.set_envelope_point(
            a,
            bv_timeline::EnvelopeKind::Volume,
            Tick::from_seconds(13),
            0.5,
        )
        .unwrap();
        let mut view = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 100.0,
            ..TimelineView::default()
        };
        app.snapping = true;
        app.snap_grid = false;
        app.cursor = Tick::from_seconds(9);
        let kind_at = |view: &mut TimelineView, seconds: f64, extra: &[(Tick, SnapKind)]| {
            let t = Tick::from_seconds_f64(seconds);
            view.snap_with(&app, &p, t, &[Tick::ZERO], &[], extra);
            view.snap_line.map(|(_, k)| k)
        };
        assert_eq!(kind_at(&mut view, 2.97, &[]), Some(SnapKind::EventFade));
        assert_eq!(kind_at(&mut view, 7.03, &[]), Some(SnapKind::Event));
        assert_eq!(kind_at(&mut view, 9.02, &[]), Some(SnapKind::Cursor));
        assert_eq!(kind_at(&mut view, 10.98, &[]), Some(SnapKind::Marker));
        assert_eq!(
            kind_at(&mut view, 13.03, &[]),
            Some(SnapKind::EnvelopePoint)
        );
        let origin = [(Tick::from_seconds(15), SnapKind::DragOrigin)];
        assert_eq!(
            kind_at(&mut view, 15.01, &origin),
            Some(SnapKind::DragOrigin)
        );
        // Envelopes hidden: their points do not hold edits.
        view.show_envelopes = false;
        assert_eq!(kind_at(&mut view, 13.03, &[]), None);
    }

    #[test]
    fn the_cursor_snaps_to_event_edges() {
        let mut app = BananaApp::new(egui::Context::default());
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::ColorBars,
            &p.settings,
        ));
        // A 5 s event from 2 s: edges at 2 s and 7 s.
        p.insert_media(m, Tick::from_seconds(2), Some(v), None)
            .unwrap();
        // 1 s = 100 px: 7 px of snap are 70 ms.
        let mut view = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 100.0,
            ..TimelineView::default()
        };
        app.snapping = true;
        let near = Tick::from_seconds_f64(6.95);
        assert_eq!(view.snap_cursor(&app, &p, near), Tick::from_seconds(7));
        assert!(view.snap_line.is_some(), "the snap line shows");
        // Too far (8 px): just onto a frame.
        let rate = p.settings.frame_rate;
        for far in [6.92, 6.5].map(Tick::from_seconds_f64) {
            assert_eq!(view.snap_cursor(&app, &p, far), rate.snap_floor(far));
        }
        // Snapping off: no snap.
        app.snapping = false;
        assert_eq!(view.snap_cursor(&app, &p, near), rate.snap_floor(near));
    }

    #[test]
    fn media_edges_near_the_pointer() {
        use bv_timeline::{MediaInfo, MediaSource};
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let mut info = MediaInfo::generated(bv_timeline::Generator::ColorBars, &p.settings);
        info.source = MediaSource::File {
            path: "x.mp4".into(),
        };
        info.duration = Tick::from_seconds(2);
        let m = p.add_media(info);
        let c = p
            .insert_media(m, Tick::from_seconds(1), Some(v), None)
            .unwrap()[0];
        // A 2 s media placed at 1 s ends at 3 s, then loops at 5 s, 7 s…
        let edges: Vec<Tick> = media_edges(&p, c, Tick::from_seconds(4), false)
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        assert!(edges.contains(&Tick::from_seconds(3)), "{edges:?}");
        assert!(edges.contains(&Tick::from_seconds(5)), "{edges:?}");
        assert!(edges.contains(&Tick::from_seconds(1)), "the media start");
    }

    #[test]
    fn a_click_on_an_event_moves_the_cursor_or_not() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let project = app.project().clone();
        let mut view = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 100.0,
            ..TimelineView::default()
        };
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
        let l = view.layout(screen, &project);
        // The middle of the sunset (4–10 s).
        let (track, clip) = project
            .clips()
            .find(|(t, c)| t.kind == TrackKind::Video && c.start == Tick::from_seconds(4))
            .map(|(t, c)| (t.id, c.id))
            .unwrap();
        let row = l.row_of(track).unwrap();
        let pos = egui::pos2(
            view.x_of(&l, Tick::from_seconds(7)),
            row.top + row.height * 0.6,
        );
        app.cursor = Tick::ZERO;
        app.settings.prefs.click_moves_cursor = false;
        click(
            &mut view,
            &mut app,
            &l,
            &project,
            pos,
            egui::Modifiers::NONE,
        );
        assert!(app.selection.contains(&clip));
        assert_eq!(app.cursor, Tick::ZERO, "the cursor stays");
        app.settings.prefs.click_moves_cursor = true;
        click(
            &mut view,
            &mut app,
            &l,
            &project,
            pos,
            egui::Modifiers::NONE,
        );
        assert!(
            app.cursor > Tick::from_seconds(6),
            "the cursor comes to the click"
        );
        // The demo's time selection stays after a click, unless the loop
        // region collapses.
        assert!(app.project().loop_region.is_some());
        app.settings.prefs.collapse_loop_region = true;
        click(
            &mut view,
            &mut app,
            &l,
            &project,
            pos,
            egui::Modifiers::NONE,
        );
        assert_eq!(app.project().loop_region, None);
    }

    #[test]
    fn audio_alone_is_not_held_to_frames() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let p = app.project().clone();
        let mut view = TimelineView::default();
        let audio = p
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Audio)
            .map(|(_, c)| c.id)
            .unwrap();
        let video = p
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| c.id)
            .unwrap();
        (app.quantize, app.snapping) = (true, false);
        let t = Tick::from_seconds_f64(1.234_567);
        let rate = p.settings.frame_rate;
        assert_eq!(
            view.snap(&app, &p, t, &[Tick::ZERO], &[audio]),
            t,
            "audio alone"
        );
        assert_eq!(
            view.snap(&app, &p, t, &[Tick::ZERO], &[audio, video]),
            rate.snap_nearest(t),
            "with video"
        );
        app.settings.prefs.no_quantize_audio = false;
        assert_eq!(
            view.snap(&app, &p, t, &[Tick::ZERO], &[audio]),
            rate.snap_nearest(t)
        );
    }

    #[test]
    fn media_added_together_end_to_end_or_overlapping() {
        let s = Tick::from_seconds;
        assert_eq!(added_starts(s(1), &[s(4), s(6)], Tick::ZERO), [s(1), s(5)]);
        assert_eq!(added_starts(s(1), &[s(4), s(6)], s(1)), [s(1), s(4)]);
        // Never more than half of either.
        assert_eq!(
            added_starts(Tick::ZERO, &[s(4), s(1)], s(3)),
            [Tick::ZERO, Tick(s(7).0 / 2)]
        );
    }

    #[test]
    fn events_out_of_sync_tell_by_how_much() {
        use bv_core::FrameRate;
        use bv_timeline::{AudioStreamInfo, MediaId, VideoStreamInfo};
        let mut p = Project::default();
        p.settings.frame_rate = FrameRate::FPS_29_97;
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let m = p.add_media(MediaInfo {
            id: MediaId(0),
            source: MediaSource::File {
                path: "clip.mp4".into(),
            },
            name: "clip".into(),
            duration: Tick::from_seconds(240),
            video: Some(VideoStreamInfo {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::FPS_29_97,
                pixel_aspect: Default::default(),
                field_order: Default::default(),
                spherical: false,
                full_range: false,
                reading: Default::default(),
            }),
            audio: Some(AudioStreamInfo {
                sample_rate: bv_core::SampleRate::HZ_48000,
                channels: 2,
            }),
            extra_audio: Vec::new(),
            sequence: None,
            still: false,
            effects: Vec::new(),
            markers: Vec::new(),
            color_space: None,
            reel: Default::default(),
            layer: None,
        });
        let ids = p.insert_media(m, Tick::ZERO, Some(v), Some(a)).unwrap();
        let (video, audio) = (ids[0], ids[1]);
        assert!(sync_offsets(&p).is_empty(), "together, in sync");
        // Out of the group (U) and moved 27 seconds and 2 frames later.
        p.ignore_groups = true;
        let later = FrameRate::FPS_29_97.frame_start(27 * 30 + 2);
        p.move_clip(video, later, None).unwrap();
        p.ignore_groups = false;
        let offsets = sync_offsets(&p);
        let rate = p.settings.frame_rate;
        assert_eq!(sync_text(offsets[&video], rate), "+27:02");
        assert_eq!(sync_text(offsets[&audio], rate), "-27:02");
        // Longer offsets show their minutes; short ones only frames.
        let r30 = FrameRate::FPS_30;
        assert_eq!(sync_text(r30.frame_start(62 * 30 + 5), r30), "+1:02:05");
        assert_eq!(sync_text(-r30.frame_start(15), r30), "-0:15");
    }

    #[test]
    fn events_out_of_sync_are_matched_with_their_longest_overlap() {
        use bv_timeline::{Clip, ClipId, Generator, MediaInfo};
        let mut p = Project::default();
        let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let s = |n: i64| Tick::from_seconds(n);
        let clip = |id: u64, start: i64, length: i64, from: i64| {
            let mut c = Clip::new(ClipId(id), m, s(start), s(length));
            c.source_in = s(from);
            c
        };
        // A long video event from 0 s, then shorter ones it reaches over.
        p.tracks[0].clips = vec![
            clip(901, 0, 100, 0),
            clip(902, 10, 5, 10),
            clip(903, 20, 5, 20),
        ];
        // The first audio event overlaps only the long one, 3 s off; the
        // second overlaps the long one by 10 s and a short one by 5 s: the
        // long one is its partner, and it is 5 s off.
        p.tracks[1].clips = vec![clip(904, 50, 10, 53), clip(905, 15, 10, 20)];
        let offsets = sync_offsets(&p);
        assert_eq!(offsets[&ClipId(904)], s(-3));
        assert_eq!(offsets[&ClipId(905)], s(-5));
        // The long video event's longest overlap is the second audio event's
        // (as long as the first's: either is its partner).
        assert!([s(3), s(5)].contains(&offsets[&ClipId(901)]));
        assert_eq!(offsets[&ClipId(903)], s(5));
    }

    /// The demo at 100 pixels a second, and where `secs` is on `track`.
    fn demo_view() -> (BananaApp, TimelineView, Layout) {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let view = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 100.0,
            ..TimelineView::default()
        };
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
        let l = view.layout(screen, app.project());
        (app, view, l)
    }

    fn spot(view: &TimelineView, l: &Layout, track: TrackId, secs: f64) -> Pos2 {
        let row = l.row_of(track).unwrap();
        pos2(
            view.x_of(l, Tick::from_seconds_f64(secs)),
            row.top + row.height * 0.6,
        )
    }

    #[test]
    fn an_events_envelopes_are_hit_over_the_event() {
        use bv_timeline::EnvelopeKind;
        let (mut app, view, l) = demo_view();
        let v = app.project().tracks[0].id;
        let first = app.project().tracks[0].clips[0].id;
        app.edit(|p| p.toggle_event_envelope(first, EnvelopeKind::Velocity));
        let project = app.project().clone();
        let c = project.clip(first).unwrap();
        let row = l.row_of(v).unwrap();
        let r = super::super::envelopes::event_lane(&view, &l, row, c);
        // 100 % sits in the middle of the event.
        let at = c.start + Tick::from_seconds(1);
        let y = super::super::envelopes::y_of(r, EnvelopeKind::Velocity, 1.0);
        assert!((y - r.center().y).abs() < 0.5);
        let h = super::super::envelopes::hit(&view, &l, &project, pos2(view.x_of(&l, at), y))
            .expect("the velocity line");
        assert_eq!(
            (h.clip, h.kind, h.point),
            (Some(first), EnvelopeKind::Velocity, None)
        );
        // Its first point, at the event's start.
        let h = super::super::envelopes::hit(&view, &l, &project, pos2(view.x_of(&l, c.start), y))
            .expect("the first point");
        assert_eq!(h.point, Some(0));
        // A point added there on the event, counted from its start.
        let owner = h.owner();
        app.edit(|p| {
            let t = super::super::envelopes::local(p, owner, at);
            p.set_envelope_point(owner, EnvelopeKind::Velocity, t, 0.5)
        });
        let env = app
            .project()
            .clip(first)
            .unwrap()
            .velocity()
            .unwrap()
            .clone();
        assert_eq!(env.points.len(), 2);
        assert_eq!(env.points[1].at, Tick::from_seconds(1));
    }

    #[test]
    fn shift_clicks_select_ranges_and_time() {
        let (mut app, mut view, l) = demo_view();
        let project = app.project().clone();
        let (video, audio) = (project.tracks[0].id, project.tracks[1].id);
        let none = egui::Modifiers::NONE;
        let shift = egui::Modifiers::SHIFT;
        // The bars, then Shift on the checkerboard: all three on the track.
        let at = spot(&view, &l, video, 1.0);
        click(&mut view, &mut app, &l, &project, at, none);
        let at = spot(&view, &l, video, 12.0);
        click(&mut view, &mut app, &l, &project, at, shift);
        let on_video: Vec<ClipId> = project.tracks[0].clips.iter().map(|c| c.id).collect();
        assert_eq!(app.selection.len(), on_video.len());
        assert!(on_video.iter().all(|c| app.selection.contains(c)));
        // Down to the audio track: those starting between the two, on both.
        let at = spot(&view, &l, audio, 7.0);
        click(&mut view, &mut app, &l, &project, at, shift);
        let starts: Vec<Tick> = app
            .selection
            .iter()
            .filter_map(|&c| project.clip(c))
            .map(|c| c.start)
            .collect();
        assert_eq!(starts.len(), 4, "the checkerboard starts later");
        assert!(starts.iter().all(|&t| t <= Tick::from_seconds(4)));
        // Shift+click where there is no event: a time selection from the
        // cursor to there.
        app.cursor = Tick::from_seconds(2);
        let at = spot(&view, &l, video, 30.0);
        click(&mut view, &mut app, &l, &project, at, shift);
        let r = app.project().loop_region.expect("a time selection");
        assert_eq!(r.start, Tick::from_seconds(2));
        assert!((r.end() - Tick::from_seconds(30)).abs() <= project.min_clip_duration());
        assert_eq!(app.cursor, Tick::from_seconds(2), "the cursor stays");
    }

    #[test]
    fn ctrl_alt_shift_click_splits_the_event_there() {
        let (mut app, mut view, l) = demo_view();
        let project = app.project().clone();
        let video = project.tracks[0].id;
        let all = egui::Modifiers::COMMAND | egui::Modifiers::ALT | egui::Modifiers::SHIFT;
        let at = spot(&view, &l, video, 7.0);
        click(&mut view, &mut app, &l, &project, at, all);
        let frame = project.min_clip_duration();
        let clips = &app.project().tracks[0].clips;
        assert_eq!(clips.len(), project.tracks[0].clips.len() + 1);
        assert!(
            clips
                .iter()
                .any(|c| (c.start - Tick::from_seconds(7)).abs() <= frame)
        );
    }

    /// As VEGAS's: an event's end dragged past the tracks' right edge runs
    /// the view on, the end following at the edge; the selection
    /// rectangle's corner stays where it was on the timeline; away from
    /// the edges nothing runs.
    #[test]
    fn a_drag_at_the_edge_runs_the_view_on() {
        let (mut app, mut view, l) = demo_view();
        // Zoomed out: the track's last event (no crossfade at its end) in
        // sight.
        view.ticks_per_px = Tick::SECOND.0 as f64 / 50.0;
        let project = app.project().clone();
        let video = project.tracks[0].id;
        let first = project.tracks[0].clips.last().unwrap().clone();
        let at_end = spot(&view, &l, video, first.end().as_seconds_f64()) - vec2(2.0, 0.0);
        assert!(l.lanes.contains(at_end), "{at_end:?} in {:?}", l.lanes);
        start_drag(
            &mut view,
            &mut app,
            &l,
            &project,
            at_end,
            egui::Modifiers::NONE,
            false,
        );
        assert!(matches!(view.drag, Some(Drag::TrimEnd { .. })));
        let past = pos2(l.lanes.right() + 40.0, at_end.y);
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            let p = edge_held(&mut view, ui, &l, past, true);
            update_drag(&mut view, &mut app, &l, p, false);
        });
        harness.run_steps(10);
        drop(harness);
        assert!(
            view.view_start > Tick::from_seconds(1),
            "ran on: {:?}",
            view.view_start
        );
        let edge = view.time_at(&l, l.lanes.right() - 1.0);
        let end = app.project().clip(first.id).unwrap().end();
        assert!(
            (end - edge).abs() <= Tick::from_seconds_f64(0.1),
            "the end at the edge: {end:?} {edge:?}"
        );
        finish_drag(&mut view, &mut app, &l, Some(past));
        // In the middle: still.
        let (start, middle) = (view.view_start, l.lanes.center());
        view.drag = Some(Drag::Marquee {
            start: middle,
            add: false,
            shape: BoxShape::Free,
        });
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            edge_held(&mut view, ui, &l, middle, true);
        });
        harness.run_steps(3);
        drop(harness);
        assert_eq!(view.view_start, start);
        // At the left edge the view runs back; the corner goes with the
        // timeline.
        let left = pos2(l.lanes.left() + 2.0, middle.y);
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            edge_held(&mut view, ui, &l, left, true);
        });
        harness.run_steps(3);
        drop(harness);
        assert!(view.view_start < start);
        let moved = ((start - view.view_start).0 as f64 / view.ticks_per_px) as f32;
        let Some(Drag::Marquee { start: corner, .. }) = view.drag else {
            panic!("still drawing the rectangle");
        };
        assert!((corner.x - (middle.x + moved)).abs() < 0.5, "{corner:?}");
    }

    /// An envelope's line, as VEGAS's: pressed and jiggled it changes
    /// nothing and gets no point; dragged a quarter of its lane down, its
    /// segment goes a quarter down, the point's time kept.
    #[test]
    fn an_envelope_line_drags_its_segment() {
        use bv_timeline::{EnvelopeKind, EnvelopeOwner};
        let (mut app, mut view, _) = demo_view();
        app.insert_adjustment_track(None);
        // Within a time selection the line lifts only its part: none.
        app.set_loop_region(None);
        view.show_envelopes = true;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
        let l = view.layout(screen, app.project());
        let t = app.project().tracks[0].id;
        let (owner, kind) = (EnvelopeOwner::Track(t), EnvelopeKind::CompositeLevel);
        let r = super::super::envelopes::lane_of(&l, t).unwrap();
        let from = pos2(
            view.x_of(&l, Tick::from_seconds(3)),
            super::super::envelopes::y_of(r, kind, 1.0),
        );
        let points = |app: &BananaApp| {
            super::super::envelopes::envelope_of(app.project(), owner, kind)
                .unwrap()
                .points
                .clone()
        };
        let drag = |view: &mut TimelineView, app: &mut BananaApp, to: Pos2| {
            let project = app.project().clone();
            start_drag(view, app, &l, &project, from, egui::Modifiers::NONE, false);
            update_drag(view, app, &l, to, false);
            finish_drag(view, app, &l, Some(to));
        };
        drag(&mut view, &mut app, from + egui::vec2(0.0, 2.0));
        assert_eq!(points(&app).len(), 1);
        assert_eq!(points(&app)[0].value, 1.0);
        drag(
            &mut view,
            &mut app,
            from + egui::vec2(0.0, r.height() / 4.0),
        );
        let p = points(&app);
        assert_eq!(p.len(), 1, "no point added");
        assert!((p[0].value - 0.75).abs() < 0.01, "{p:?}");
        assert_eq!(p[0].at, Tick::ZERO);
    }

    /// An audio event's fades as its waveform shows them: nothing where a
    /// fade-in starts and a fade-out ends, the whole sound past them.
    #[test]
    fn a_fade_thins_the_waveform() {
        let (app, _, _) = demo_view();
        let c = app.project().tracks[1].clips[0].clone();
        let s = Tick::from_seconds;
        let (fi, fo) = (s(1), s(1));
        assert!(fade_gain(&c, fi, fo, c.start) < 0.01);
        assert!((fade_gain(&c, fi, fo, c.start + fi) - 1.0).abs() < 1e-3);
        let half = fade_gain(&c, fi, fo, c.start + Tick(fi.0 / 2));
        assert!(half > 0.05 && half < 0.95, "{half}");
        assert!(fade_gain(&c, fi, fo, c.end()) < 0.01);
        let middle = c.start + Tick(c.duration.0 / 2);
        assert!((fade_gain(&c, Tick::ZERO, Tick::ZERO, middle) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn dragging_one_selected_event_moves_them_all() {
        let (mut app, mut view, l) = demo_view();
        let project = app.project().clone();
        let (video, audio) = (project.tracks[0].id, project.tracks[1].id);
        let at4 = |t: TrackId| {
            project
                .tracks
                .iter()
                .find(|x| x.id == t)
                .unwrap()
                .clips
                .iter()
                .find(|c| c.start == Tick::from_seconds(4))
                .unwrap()
                .id
        };
        let (sunset, la) = (at4(video), at4(audio));
        app.select_clip(sunset, false);
        app.select_clip(la, true);
        let from = spot(&view, &l, video, 7.0);
        let to = spot(&view, &l, video, 9.0);
        start_drag(
            &mut view,
            &mut app,
            &l,
            &project,
            from,
            egui::Modifiers::NONE,
            false,
        );
        update_drag(&mut view, &mut app, &l, to, false);
        finish_drag(&mut view, &mut app, &l, Some(to));
        let start = |id| app.project().clip(id).unwrap().start;
        // On a frame (29.97 a second), two seconds later.
        let frame = project.min_clip_duration();
        assert!((start(sunset) - Tick::from_seconds(6)).abs() <= frame);
        assert_eq!(start(la), start(sunset), "the other one went along");
        assert_eq!(app.project().clip_track(la).unwrap().id, audio);
    }

    /// The event on `track` starting at `start`.
    fn event_at(project: &Project, track: TrackId, start: Tick) -> ClipId {
        project
            .track(track)
            .and_then(|t| t.clips.iter().find(|c| c.start == start))
            .map(|c| c.id)
            .unwrap()
    }

    #[test]
    fn the_event_clicked_is_picked_its_group_selected_along() {
        let (mut app, mut view, l) = demo_view();
        let s = Tick::from_seconds;
        let (video, audio) = (app.project().tracks[0].id, app.project().tracks[1].id);
        let bars = event_at(app.project(), video, Tick::ZERO);
        let (sunset, la) = (
            event_at(app.project(), video, s(4)),
            event_at(app.project(), audio, s(4)),
        );
        app.modify(|p| p.group_clips(&[sunset, la]));
        let project = app.project().clone();
        let none = egui::Modifiers::NONE;
        // A click on the sunset: la comes along, in blue.
        let at = spot(&view, &l, video, 7.0);
        click(&mut view, &mut app, &l, &project, at, none);
        assert_eq!(app.selection, [sunset, la].into_iter().collect());
        assert!(!app.selected_by_group(sunset), "picked: yellow");
        assert!(app.selected_by_group(la), "its group mate: blue");
        // Ctrl+click on the bars: picked as well.
        let at = spot(&view, &l, video, 1.0);
        click(
            &mut view,
            &mut app,
            &l,
            &project,
            at,
            egui::Modifiers::COMMAND,
        );
        assert!(app.selection.contains(&bars));
        assert!(!app.selected_by_group(bars) && !app.selected_by_group(sunset));
        assert!(app.selected_by_group(la));
        // A click on la picks it instead.
        let at = spot(&view, &l, audio, 7.0);
        click(&mut view, &mut app, &l, &project, at, none);
        assert_eq!(app.selection, [sunset, la].into_iter().collect());
        assert!(app.selected_by_group(sunset) && !app.selected_by_group(la));
        // Ctrl+click on a selected event: off, with its group.
        click(
            &mut view,
            &mut app,
            &l,
            &project,
            at,
            egui::Modifiers::COMMAND,
        );
        assert!(app.selection.is_empty());
        // Selected another way (Select All): every event picked.
        app.select_all();
        assert!(app.selection.iter().all(|&c| !app.selected_by_group(c)));
    }

    #[test]
    fn dragging_on_an_empty_spot_draws_the_selection_rectangle() {
        let (mut app, mut view, l) = demo_view();
        let project = app.project().clone();
        let s = Tick::from_seconds;
        let (video, audio) = (project.tracks[0].id, project.tracks[1].id);
        let none = egui::Modifiers::NONE;
        // From past la's end on the audio track to the middle of the sunset.
        let from = spot(&view, &l, audio, 12.0);
        let to = spot(&view, &l, video, 6.0);
        assert!(matches!(hit(&view, &l, &project, from), Hit::Track { .. }));
        start_drag(&mut view, &mut app, &l, &project, from, none, false);
        assert!(matches!(
            view.drag,
            Some(Drag::Marquee {
                shape: BoxShape::Free,
                ..
            })
        ));
        update_drag(&mut view, &mut app, &l, to, false);
        finish_drag(&mut view, &mut app, &l, Some(to));
        let boxed = [
            event_at(&project, video, s(4)),
            event_at(&project, video, s(10)),
            event_at(&project, audio, s(4)),
        ];
        assert_eq!(app.selection, boxed.into_iter().collect());
        assert!(boxed.iter().all(|&c| !app.selected_by_group(c)), "picked");
        assert_eq!(
            app.project().loop_region,
            project.loop_region,
            "no time selection"
        );
        // With the right button: a time selection.
        let (from, to) = (spot(&view, &l, audio, 11.0), spot(&view, &l, audio, 13.0));
        start_drag(&mut view, &mut app, &l, &project, from, none, true);
        assert!(matches!(view.drag, Some(Drag::TimeSelect { .. })));
        update_drag(&mut view, &mut app, &l, to, false);
        finish_drag(&mut view, &mut app, &l, Some(to));
        let r = app.project().loop_region.expect("a time selection");
        let frame = project.min_clip_duration();
        assert!((r.start - s(11)).abs() <= frame, "{r:?}");
        assert!((r.end() - s(13)).abs() <= frame, "{r:?}");
    }

    #[test]
    fn a_right_click_makes_the_rectangle_span_time_or_tracks() {
        let (_, _, l) = demo_view();
        assert_eq!(BoxShape::Free.next(), BoxShape::Time);
        assert_eq!(BoxShape::Time.next(), BoxShape::Tracks);
        assert_eq!(BoxShape::Tracks.next(), BoxShape::Free);
        let (a, b) = (pos2(500.0, 160.0), pos2(400.0, 120.0));
        let time = BoxShape::Time.rect(&l, a, b);
        assert_eq!(time.x_range(), egui::Rangef::new(400.0, 500.0));
        assert_eq!(time.y_range(), l.lanes.y_range(), "every track");
        let tracks = BoxShape::Tracks.rect(&l, a, b);
        assert_eq!(tracks.x_range(), l.lanes.x_range(), "all along them");
        assert_eq!(tracks.y_range(), egui::Rangef::new(120.0, 160.0));
        // Events out of sight count too.
        let area = Rect::from_min_max(pos2(100.0, 50.0), pos2(200.0, 80.0));
        let inside = Rect::from_min_max(pos2(120.0, 55.0), pos2(150.0, 70.0));
        let below = Rect::from_min_max(pos2(150.0, 900.0), pos2(300.0, 940.0));
        let beside = Rect::from_min_max(pos2(4000.0, 60.0), pos2(4100.0, 70.0));
        let takes = |shape: BoxShape| {
            [inside, below, beside]
                .iter()
                .map(|&r| shape.takes(area, r))
                .collect::<Vec<_>>()
        };
        assert_eq!(takes(BoxShape::Free), [true, false, false]);
        assert_eq!(takes(BoxShape::Time), [true, true, false]);
        assert_eq!(takes(BoxShape::Tracks), [true, false, true]);
    }

    #[test]
    fn double_and_triple_clicks_on_empty_spots() {
        let (mut app, view, l) = demo_view();
        let s = Tick::from_seconds;
        let audio = app.project().tracks[1].id;
        // A gap on the audio track: the tone ends at 2 s, la starts at 4 s.
        let tone = event_at(app.project(), audio, Tick::ZERO);
        app.edit(|p| p.trim_end(tone, s(2)));
        let project = app.project().clone();
        let end = project.duration();
        let cursor = app.cursor;
        // In the gap: the gap becomes the time selection.
        let at = spot(&view, &l, audio, 3.0);
        empty_clicks(&view, &mut app, &l, &project, at, false, cursor);
        assert_eq!(
            app.project().loop_region,
            Some(bv_core::TimeRange::from_bounds(s(2), s(4)))
        );
        assert_eq!(app.cursor, cursor);
        // Past the last event: the cursor goes to the project's end.
        let at = spot(&view, &l, audio, 12.0);
        empty_clicks(&view, &mut app, &l, &project, at, false, cursor);
        assert_eq!(app.cursor, end);
        // A third click: the whole project, the cursor where it was before
        // that click moved it.
        app.cursor = s(12);
        empty_clicks(&view, &mut app, &l, &project, at, true, end);
        assert_eq!(
            app.project().loop_region,
            Some(bv_core::TimeRange::new(Tick::ZERO, end))
        );
        assert_eq!(app.cursor, end);
        // Over an event none of this (its own double-click selects it).
        app.set_loop_region(None);
        let at = spot(&view, &l, audio, 6.0);
        empty_clicks(&view, &mut app, &l, &project, at, false, cursor);
        assert_eq!(app.project().loop_region, None);
        assert_eq!(app.cursor, end);
    }

    #[test]
    fn a_double_click_and_drag_extends_to_the_next_edge() {
        let (mut app, _view, _l) = demo_view();
        let s = Tick::from_seconds;
        let audio = app.project().tracks[1].id;
        // The tone ends at 2 s, la starts at 4 s.
        let tone = event_at(app.project(), audio, Tick::ZERO);
        app.edit(|p| p.trim_end(tone, s(2)));
        let p = app.project();
        // Inside the event: its own span.
        assert_eq!(
            extended_selection(p, tone, s(1)),
            Some(bv_core::TimeRange::new(Tick::ZERO, s(2)))
        );
        // Dragged past its end into the gap: out to la's start.
        assert_eq!(
            extended_selection(p, tone, s(3)),
            Some(bv_core::TimeRange::from_bounds(Tick::ZERO, s(4)))
        );
    }

    #[test]
    fn an_edge_under_a_crossfade_has_no_fade_handle() {
        let (mut app, view, l) = demo_view();
        let s = Tick::from_seconds;
        let video = app.project().tracks[0].id;
        // The bars overlap the sunset's start; its end is free.
        let sunset = event_at(app.project(), video, s(4));
        app.edit(|p| p.set_fades(sunset, Some(s(1)), Some(s(1))));
        let project = app.project().clone();
        let row = l.row_of(video).unwrap();
        let top = row.top + 6.0;
        let at = |t: Tick| pos2(view.x_of(&l, t), top);
        let zone = |project: &Project, t: Tick| match hit(&view, &l, project, at(t)) {
            Hit::Clip { clip, zone } if clip == sunset => Some(zone),
            _ => None,
        };
        assert_ne!(zone(&project, s(5)), Some(Zone::FadeIn));
        assert_eq!(zone(&project, s(9)), Some(Zone::FadeOut));
        // Without automatic crossfades the fade in has its handle.
        app.modify(|p| p.auto_crossfade = false);
        let project = app.project().clone();
        assert_eq!(zone(&project, s(5)), Some(Zone::FadeIn));
    }

    /// A crossfade's dark lines are its events' edges, from either side of
    /// them: the start of the event coming in, the end of the one going out
    /// under it.
    #[test]
    fn a_crossfades_lines_trim_its_events() {
        let (mut app, view, l) = demo_view();
        let s = Tick::from_seconds;
        let video = app.project().tracks[0].id;
        let bars = event_at(app.project(), video, Tick::ZERO);
        let sunset = event_at(app.project(), video, s(4));
        let bars_end = app.project().clip(bars).unwrap().end();
        assert!(bars_end > s(4), "they overlap");
        let row = l.row_of(video).unwrap();
        let y = row.top + row.height * 0.45;
        let at = |project: &Project, t: Tick, dx: f32| {
            hit(&view, &l, project, pos2(view.x_of(&l, t) + dx, y))
        };
        let project = app.project().clone();
        let edge = |clip, zone| Hit::Clip { clip, zone };
        for dx in [-2.0, 0.0, 2.0] {
            assert_eq!(at(&project, s(4), dx), edge(sunset, Zone::Start));
            assert_eq!(at(&project, bars_end, dx), edge(bars, Zone::End));
        }
        // Between them, the event on top.
        let middle = Tick((s(4).0 + bars_end.0) / 2);
        assert_eq!(at(&project, middle, 0.0), edge(sunset, Zone::Body));
        // Without automatic crossfades the sunset covers the bars' end.
        app.modify(|p| p.auto_crossfade = false);
        let project = app.project().clone();
        assert_eq!(at(&project, bars_end, 2.0), edge(sunset, Zone::Body));
        assert_eq!(at(&project, s(4), -2.0), edge(sunset, Zone::Start));
    }

    /// The fade handles are on the level line, where the curves end: they
    /// come down with it (VEGAS's anchor).
    #[test]
    fn fade_handles_follow_the_level_line() {
        let (mut app, view, l) = demo_view();
        let s = Tick::from_seconds;
        let video = app.project().tracks[0].id;
        let sunset = event_at(app.project(), video, s(4));
        app.edit(|p| -> Result<(), bv_timeline::EditError> {
            p.set_fades(sunset, Some(s(1)), Some(s(1)))?;
            p.set_clip_level(sunset, 0.5)
        });
        let project = app.project().clone();
        let row = l.row_of(video).unwrap();
        let clip = project.clip(sunset).unwrap();
        let r = clip_rect(&view, &l, row, clip.start, clip.end());
        // The fade-out begins at 9 s (the sunset runs 4–10 s).
        let x = view.x_of(&l, s(9));
        let zone = |y: f32| match hit(&view, &l, &project, pos2(x, y)) {
            Hit::Clip { clip, zone } if clip == sunset => Some(zone),
            _ => None,
        };
        let handle = fade_handle_y(r, 0.5);
        assert!((handle - level_y(r, 0.5)).abs() < 1e-3, "on the level line");
        assert_eq!(zone(handle), Some(Zone::FadeOut));
        assert_ne!(zone(r.top() + 6.0), Some(Zone::FadeOut), "not at the top");
    }
}

#[cfg(test)]
mod fade_tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    /// As VEGAS draws them: from the bottom corner up to the level line.
    #[test]
    fn fades_run_up_to_the_level_line() {
        let r = Rect::from_min_size(pos2(0.0, 100.0), vec2(200.0, 60.0));
        for kind in [TrackKind::Video, TrackKind::Audio] {
            let pts = fade_points(r, 0.0, 50.0, kind, FadeCurve::Fast, true, 0.56);
            assert_eq!(pts[0], pos2(0.0, level_y(r, 0.0)), "from the bottom");
            let end = *pts.last().unwrap();
            assert!((end.x - 50.0).abs() < 1e-3);
            assert!((end.y - level_y(r, 0.56)).abs() < 1e-3, "to the level line");
            let out = fade_points(r, 150.0, 200.0, kind, FadeCurve::Fast, false, 0.56);
            assert!(
                (out[0].y - level_y(r, 0.56)).abs() < 1e-3,
                "from the level line"
            );
            assert_eq!(out.last().unwrap().y, level_y(r, 0.0), "down to the bottom");
        }
    }

    /// As VEGAS draws it: the level line between the fades (their curves
    /// are the line there), and from a crossfade's end.
    #[test]
    fn the_level_line_runs_between_the_fades() {
        use bv_timeline::{Generator, MediaInfo};
        let s = Tick::from_seconds;
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.duration = s(5);
        let m = p.add_media(info);
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.set_fades(a, Some(s(1)), Some(s(1))).unwrap();
        let view = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 100.0,
            ..TimelineView::default()
        };
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 60.0));
        let span = |p: &Project| {
            let track = p.track(v).unwrap();
            let clip = track.clips.iter().find(|c| c.id == a).unwrap();
            level_span(&view, track, clip, r, p.auto_crossfade)
        };
        assert_eq!(span(&p), Some((100.0, 400.0)), "between the fades");
        // Fades that leave no room: no line.
        p.set_fades(a, Some(s(3)), Some(s(2))).unwrap();
        assert_eq!(span(&p), None);
        p.set_fades(a, Some(s(1)), Some(s(1))).unwrap();
        // An event over its last half second: the crossfade is its fade.
        p.insert_media(m, s(4) + Tick::from_seconds_f64(0.5), Some(v), None)
            .unwrap();
        let (from, to) = span(&p).unwrap();
        assert_eq!(from, 100.0);
        assert!((to - 450.0).abs() < 1e-3, "to the crossfade: {to}");
    }

    /// The grid's spacing follows the zoom of the timeline being drawn
    /// (`app.timeline` is a stand-in then): Ruler Marks, the ruler's.
    #[test]
    fn the_grid_follows_the_zoom_of_the_timeline_drawn() {
        let app = BananaApp::new(egui::Context::default());
        let near = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 100.0,
            ..TimelineView::default()
        };
        let far = TimelineView {
            ticks_per_px: Tick::SECOND.0 as f64 / 10.0,
            ..TimelineView::default()
        };
        assert!(app.grid_ticks(&far) > app.grid_ticks(&near));
    }

    /// A right-click on a crossfade opens VEGAS's popup of its curves: the
    /// 25 pairs as pictures, a dot by the crossfade's; a picture picked
    /// sets both curves, in one undo.
    #[test]
    fn a_right_click_on_a_crossfade_offers_its_pairs() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let s = Tick::from_seconds;
        let track = &app.project().tracks[0];
        let at = |t: Tick| track.clips.iter().find(|c| c.start == t).unwrap().id;
        // The bars (from 0 s) overlap the sunset's start (4 s).
        let (bars, sunset) = (at(Tick::ZERO), at(s(4)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 960.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let curves = |h: &Harness<'_, BananaApp>| {
            let p = h.state().project();
            (
                p.clip(bars).unwrap().fade_out_curve,
                p.clip(sunset).unwrap().fade_in_curve,
            )
        };
        let before = curves(&h);
        right_click(&mut h, 4.5);
        for o in CROSSFADE_POPUP {
            for i in CROSSFADE_POPUP {
                let name = trf!(
                    "Out: {}, in: {}",
                    crate::preferences::fade_name(o),
                    crate::preferences::fade_name(i)
                );
                let node = h.get_all_by_label(&name).last().expect("a picture");
                let marked =
                    node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True);
                assert_eq!(marked, (o, i) == before, "{name}");
            }
        }
        pick(&mut h, "Out: Fast, in: Slow");
        assert_eq!(curves(&h), (FadeCurve::Fast, FadeCurve::Slow));
        assert!(
            h.query_all_by_label("Out: Fast, in: Slow").next().is_none(),
            "closed"
        );
        h.state_mut().undo();
        assert_eq!(curves(&h), before, "one undo");
    }

    fn right_click(h: &mut Harness<'_, BananaApp>, seconds: f64) {
        let at = h
            .state()
            .timeline_spot_for_review(0, seconds)
            .expect("spot");
        h.event(egui::Event::PointerMoved(at));
        h.step();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        h.step();
    }

    fn pick(h: &mut Harness<'_, BananaApp>, name: &str) {
        let at = h.get_all_by_label(name).last().expect(name).rect().center();
        h.event(egui::Event::PointerMoved(at));
        h.step();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        h.step();
    }

    /// A right-click on an event's fade opens VEGAS's popup of curves
    /// (the event's marked); a picture picked sets that fade's curve.
    /// Elsewhere on the event the right-click opens its shortcut menu.
    #[test]
    fn a_right_click_on_a_fade_offers_its_curves() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        // The checkerboard (from 10 s, after the sunset): 2 s fades.
        let (clip, start, end) = app
            .project()
            .clips()
            .find(|(t, c)| t.kind == TrackKind::Video && c.start == Tick::from_seconds(10))
            .map(|(_, c)| (c.id, c.start.as_seconds_f64(), c.end().as_seconds_f64()))
            .expect("checkerboard");
        let two = Tick::from_seconds(2);
        app.edit(|p| p.set_fades(clip, Some(two), Some(two)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 960.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let curves = |h: &Harness<'_, BananaApp>| {
            let c = h.state().project().clip(clip).expect("clip");
            (c.fade_in_curve, c.fade_out_curve)
        };
        let before = curves(&h);
        right_click(&mut h, start + 0.5);
        for curve in FADE_POPUP {
            let name = crate::preferences::fade_name(curve);
            let node = h.get_all_by_label(name).last().expect(name);
            let marked = node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True);
            assert_eq!(marked, curve == before.0, "{name}");
        }
        pick(&mut h, "Sharp");
        assert_eq!(curves(&h), (FadeCurve::Sharp, before.1), "the fade-in's");
        assert!(h.query_all_by_label("Sharp").next().is_none(), "it closed");
        right_click(&mut h, end - 0.5);
        pick(&mut h, "Fast");
        assert_eq!(
            curves(&h),
            (FadeCurve::Sharp, FadeCurve::Fast),
            "the fade-out's"
        );
        // Between the fades: the event's shortcut menu.
        right_click(&mut h, (start + end) / 2.0);
        assert!(h.query_all_by_label("Sharp").next().is_none());
        assert!(
            h.query_all_by_label_contains("Open in Trimmer")
                .next()
                .is_some()
        );
    }

    /// A video event and an audio event selected, then a right-click on
    /// one of them: the menu's Delete deletes them both.
    #[test]
    fn delete_in_an_events_menu_deletes_the_events_selected() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let s = Tick::from_seconds;
        let at = |app: &BananaApp, track: usize, start: Tick| {
            app.project().tracks[track]
                .clips
                .iter()
                .find(|c| c.start == start)
                .map(|c| c.id)
                .expect("the event")
        };
        // The checkerboard (from 10 s) and la (from 4 s).
        let (checker, la) = (at(&app, 0, s(10)), at(&app, 1, s(4)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 960.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        h.state_mut().select_clip(checker, false);
        h.state_mut().select_clip(la, true);
        right_click(&mut h, 10.5);
        assert_eq!(h.state().selection.len(), 2, "both still selected");
        // The menu's item: its name, then its key.
        pick(&mut h, "Delete Delete");
        let p = h.state().project();
        assert!(p.clip(checker).is_none() && p.clip(la).is_none());
    }

    /// A double-click on an envelope's line puts a point there and the
    /// cursor on it, whatever is around (no time selection made, the
    /// cursor not sent elsewhere).
    #[test]
    fn a_double_click_on_an_envelope_puts_the_cursor_on_its_new_point() {
        use bv_timeline::{EnvelopeKind, EnvelopeOwner};
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        // An adjustment track on top, its Composite envelope half way down.
        app.insert_adjustment_track(None);
        app.set_loop_region(None);
        app.seek_seconds(1.0);
        let track = app.project().tracks[0].id;
        let (owner, kind) = (EnvelopeOwner::Track(track), EnvelopeKind::CompositeLevel);
        app.edit(|p| p.set_envelope_point(owner, kind, Tick::ZERO, 0.5));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let at = {
            let app = h.state();
            let view = &app.timeline;
            let l = view.layout(view.last_rect, app.project());
            let r = super::super::envelopes::lane_of(&l, track).expect("its lane");
            pos2(
                view.x_of(&l, Tick::from_seconds_f64(7.3)),
                super::super::envelopes::y_of(r, kind, 0.5),
            )
        };
        h.event(egui::Event::PointerMoved(at));
        h.step();
        for _ in 0..2 {
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
            }
        }
        h.run_steps(3);
        let app = h.state();
        let points = &super::super::envelopes::envelope_of(app.project(), owner, kind)
            .expect("the envelope")
            .points;
        assert_eq!(points.len(), 2, "a point added: {points:?}");
        assert!(
            (points[1].at.as_seconds_f64() - 7.3).abs() < 0.5,
            "{points:?}"
        );
        assert_eq!(app.cursor, points[1].at, "the cursor on it");
        assert!(app.project().loop_region.is_none(), "no time selection");
    }
}

#[cfg(test)]
mod wheel_tests {
    use super::*;
    use egui_kittest::Harness;

    /// The mouse wheel zooms toward the needle wherever the pointer is: in
    /// view, the needle keeps its place; out of view, it comes to the
    /// middle.
    #[test]
    fn the_wheel_zooms_toward_the_needle() {
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 900.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(3);
        let needle_x = |h: &Harness<'_, BananaApp>| {
            let app = h.state();
            app.ruler_spot_for_review(app.cursor.as_seconds_f64()).x
        };
        let roll = |h: &mut Harness<'_, BananaApp>, at: Pos2| {
            h.event(egui::Event::PointerMoved(at));
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 120.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
            h.run_steps(12);
        };
        let (before, zoom) = (needle_x(&h), h.state().timeline.ticks_per_px);
        // The pointer over the tracks, far from the needle (at 4.5 s).
        let far = h.state().timeline_spot_for_review(0, 1.0).unwrap();
        assert!((far.x - before).abs() > 100.0);
        roll(&mut h, far);
        assert!(h.state().timeline.ticks_per_px < zoom * 0.8, "zoomed in");
        assert!(
            (needle_x(&h) - before).abs() < 1.5,
            "the needle kept its place"
        );
        // Out of view, 3 s past its right side: to the middle.
        let off = {
            let v = &h.state().timeline;
            let width = Tick((v.lanes_width as f64 * v.ticks_per_px) as i64);
            v.view_start + width + Tick::from_seconds(3)
        };
        h.state_mut().cursor = off;
        h.run_steps(2);
        let far = h.state().timeline_spot_for_review(0, 4.5).unwrap();
        roll(&mut h, far);
        let view = &h.state().timeline;
        let left = view.last_rect.left() + super::super::HEADER_W + super::super::SPLIT_W;
        let middle = left + view.lanes_width / 2.0;
        assert!(
            (needle_x(&h) - middle).abs() < 1.5,
            "the needle in the middle"
        );
    }
}
