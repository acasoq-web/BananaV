//! Project Media's storyboards, as VEGAS's Help describes them ("Using
//! Storyboards"): a storyboard bin shows as cells — its start in the
//! sequence over each picture, the name under it —, then empty cells to
//! let media go on. Media let go on a cell take its place (on its left
//! edge, go before it), past the last go there with placeholders in
//! between (notes of what is to come, renamed with a double-click); Alt
//! and a drag moves a cell, the others closing up and making room; cells
//! drag onto the timeline (the bin itself: all of them). The Main
//! Timeline storyboard is the timeline's sequence (`bv_timeline`'s
//! `storyboards.rs`): what is done to it is done to the timeline, and
//! while it shows an older state of the timeline its background turns
//! red until Refresh Storyboard, unless Automatically Synchronize Main
//! Timeline Storyboard is on.

use bv_core::{Tick, TimeRange};
use bv_timeline::{
    BinId, ClipId, Generator, MediaId, MediaInfo, PluginParam, PluginValue, StoryCell,
    TimelineCell, TrackId, TrackKind,
};

use crate::app::{BananaApp, MediaDrag};
use crate::egui::{
    self, Align2, Color32, FontId, Modifiers, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2,
};
use crate::i18n::tr;
use crate::icons;
use crate::theme;

use super::media::{BinChoice, DragFrom};
use super::thumb_part::{self, PartDrag};

/// A cell: its picture, the time over it and the name under it.
const PICTURE: egui::Vec2 = vec2(160.0, 90.0);
const TIME_H: f32 = 16.0;
const NAME_H: f32 = 30.0;
const GAP: f32 = 10.0;
/// The background of a Main Timeline storyboard that shows an older state
/// of the timeline, and an empty cell's, as VEGAS's pictures have them.
const OUT_OF_SYNC: Color32 = Color32::from_rgb(164, 90, 82);
const EMPTY: Color32 = Color32::from_rgb(70, 70, 70);

/// What the storyboards keep between frames.
#[derive(Debug, Default)]
pub(crate) struct StoryboardState {
    /// The cells picked, by place.
    pub picked: Vec<usize>,
    /// The Main Timeline storyboard as it was last refreshed (without
    /// Automatically Synchronize).
    pub snapshot: Option<Vec<TimelineCell>>,
    /// The cell being moved with Alt.
    moving: Option<usize>,
    /// A cell's part being dragged across its picture.
    parting: Option<(usize, PartDrag)>,
    /// A placeholder being renamed: its place and the name so far.
    renaming: Option<(usize, String)>,
    /// Where the cells were laid out last: the first's corner and the
    /// columns.
    layout: Option<(Pos2, usize)>,
}

/// The middle of cell `i` laid out from `origin` in `cols` columns.
#[cfg(test)]
fn cell_center(origin: Pos2, cols: usize, i: usize) -> Pos2 {
    let (col, row) = (i % cols.max(1), i / cols.max(1));
    origin
        + vec2(
            col as f32 * (PICTURE.x + GAP) + PICTURE.x / 2.0,
            row as f32 * (TIME_H + PICTURE.y + NAME_H + GAP) + TIME_H + PICTURE.y / 2.0,
        )
}

/// What a cell's menu asks, done once the cells are drawn.
type Later = Box<dyn FnOnce(&mut BananaApp)>;

/// Which storyboard: the Main Timeline's, or one of the project's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Board {
    Main,
    Own(BinId),
}

/// A cell as shown.
#[derive(Clone, Debug)]
struct Shown {
    media: Option<MediaId>,
    info: Option<MediaInfo>,
    /// The media time its picture is of.
    at: Tick,
    /// Where it starts in the sequence.
    start: Tick,
    /// Its media's part (a cell's).
    range: Option<TimeRange>,
    name: String,
    placeholder: bool,
    /// The Main Timeline's event.
    clip: Option<ClipId>,
}

fn board(app: &BananaApp) -> Option<Board> {
    match app.media_window.bin {
        BinChoice::MainTimeline => Some(Board::Main),
        BinChoice::Storyboard(id) => Some(Board::Own(id)),
        _ => None,
    }
}

/// A media's (or subclip's) information.
fn info_of(app: &BananaApp, m: MediaId) -> Option<MediaInfo> {
    let p = app.project();
    p.media(m).cloned().or_else(|| p.subclip_info(m))
}

/// The Main Timeline storyboard's cells as shown (live, or as last
/// refreshed) and whether that is not the timeline's state now.
fn main_cells(app: &mut BananaApp) -> (Vec<TimelineCell>, bool) {
    let live = app.project().timeline_cells();
    if app.settings.storyboard_auto_sync {
        app.media_window.story.snapshot = None;
        return (live, false);
    }
    let shown = app
        .media_window
        .story
        .snapshot
        .get_or_insert_with(|| live.clone())
        .clone();
    let stale = shown != live;
    (shown, stale)
}

/// The cells of `b` as shown; whether they are out of date.
fn shown_cells(app: &mut BananaApp, b: Board) -> (Vec<Shown>, bool) {
    match b {
        Board::Main => {
            let (cells, stale) = main_cells(app);
            let shown = cells
                .iter()
                .map(|c| {
                    let info = info_of(app, c.media);
                    Shown {
                        media: Some(c.media),
                        name: info
                            .as_ref()
                            .map_or_else(String::new, crate::i18n::media_name),
                        info,
                        at: c.source_in,
                        start: c.start,
                        range: None,
                        placeholder: false,
                        clip: Some(c.clip),
                    }
                })
                .collect();
            (shown, stale)
        }
        Board::Own(id) => {
            let Some(s) = app.project().storyboard(id).cloned() else {
                return (Vec::new(), false);
            };
            let new = app.settings.prefs.new_events();
            let mut start = Tick::ZERO;
            let shown = s
                .cells
                .iter()
                .map(|c| {
                    let info = c.media.and_then(|m| info_of(app, m));
                    let cell = Shown {
                        media: c.media,
                        name: match (&info, c.media) {
                            (Some(i), Some(_)) => crate::i18n::media_name(i),
                            _ => placeholder_name(&c.name),
                        },
                        info,
                        at: c.range.map_or(Tick::ZERO, |r| r.start),
                        start,
                        range: c.range,
                        placeholder: c.media.is_none(),
                        clip: None,
                    };
                    start += app.project().cell_length(c, &new);
                    cell
                })
                .collect();
            (shown, false)
        }
    }
}

/// A placeholder's name, the model's default shown in the interface's
/// language.
fn placeholder_name(name: &str) -> String {
    if name == bv_timeline::storyboards::PLACEHOLDER {
        tr("Placeholder").to_string()
    } else {
        name.to_string()
    }
}

/// What a drag from elsewhere brings: media of the project (files of the
/// Explorer are imported first) and their parts, another storyboard's
/// cells.
fn incoming(app: &mut BananaApp, drag: &MediaDrag) -> Vec<StoryCell> {
    match drag {
        MediaDrag::Cells(cells) => cells.clone(),
        MediaDrag::Media(m) => vec![StoryCell::of(*m, None)],
        MediaDrag::MediaPart(m, part) => vec![StoryCell::of(*m, Some(*part))],
        MediaDrag::Many(ids) => ids.iter().map(|&m| StoryCell::of(m, None)).collect(),
        MediaDrag::File(p) => app
            .import_paths(std::slice::from_ref(p))
            .into_iter()
            .map(|m| StoryCell::of(m, None))
            .collect(),
        MediaDrag::Files(ps) => app
            .import_paths(ps)
            .into_iter()
            .map(|m| StoryCell::of(m, None))
            .collect(),
        MediaDrag::FilePart(p, part) => app
            .import_paths(std::slice::from_ref(p))
            .into_iter()
            .map(|m| StoryCell::of(m, Some(*part)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether `drag` brings media a storyboard takes (cells only from
/// another storyboard: `from`, where they were dragged from, is not `b`).
fn takes(drag: &MediaDrag, b: Board, from: Option<&DragFrom>) -> bool {
    match drag {
        MediaDrag::Cells(_) => {
            matches!((from, b), (Some(DragFrom::Cells(f, _)), Board::Own(id)) if *f != id)
        }
        _ => matches!(
            drag,
            MediaDrag::Media(_)
                | MediaDrag::MediaPart(..)
                | MediaDrag::Many(_)
                | MediaDrag::File(_)
                | MediaDrag::Files(_)
                | MediaDrag::FilePart(..)
        ),
    }
}

/// Where a pointer at `p` falls among `n` cells laid out from `origin` in
/// `cols` columns: the cell's place, and whether on its left edge.
fn place_at(origin: Pos2, cols: usize, p: Pos2) -> Option<(usize, bool)> {
    let (w, h) = (PICTURE.x + GAP, TIME_H + PICTURE.y + NAME_H + GAP);
    let (dx, dy) = (p.x - origin.x, p.y - origin.y);
    if dx < 0.0 || dy < 0.0 {
        return None;
    }
    let (col, row) = ((dx / w) as usize, (dy / h) as usize);
    if col >= cols {
        return None;
    }
    let left = dx - col as f32 * w < PICTURE.x * 0.25;
    Some((row * cols + col, left))
}

/// The storyboard chosen, in `rect`.
pub(crate) fn ui(ui: &mut Ui, app: &mut BananaApp, rect: Rect) {
    let Some(b) = board(app) else {
        return;
    };
    let (cells, stale) = shown_cells(app, b);
    if !stale {
        // Picks of cells no longer there go.
        app.media_window.story.picked.retain(|&i| i < cells.len());
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, if stale { OUT_OF_SYNC } else { theme::panel() });
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let cols = (((inner.width() + GAP) / (PICTURE.x + GAP)).floor() as usize).max(1);
    // The cells, then empty ones to the end of the row and one row more.
    let total = (cells.len() / cols + 2) * cols;
    let rows = total / cols;
    let cell_h = TIME_H + PICTURE.y + NAME_H;
    let mut moved: Option<(usize, usize)> = None;
    let mut dropped: Option<(usize, bool)> = None;
    let mut action: Option<Later> = None;
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let released = ui.input(|i| i.pointer.any_released());
    let alt = ui.input(|i| i.modifiers.alt);
    let taken = app
        .media_drag
        .as_ref()
        .is_some_and(|d| takes(d, b, app.media_window.drag_from.as_ref()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("storyboard-cells")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (area, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), rows as f32 * (cell_h + GAP)),
                    Sense::hover(),
                );
                let origin = area.min;
                app.media_window.story.layout = Some((origin, cols));
                let under = pointer
                    .filter(|p| area.contains(*p))
                    .and_then(|p| place_at(origin, cols, p));
                for i in 0..total {
                    let (col, row) = (i % cols, i / cols);
                    let min =
                        origin + vec2(col as f32 * (PICTURE.x + GAP), row as f32 * (cell_h + GAP));
                    let r = Rect::from_min_size(min, vec2(PICTURE.x, cell_h));
                    if !ui.is_rect_visible(r) {
                        continue;
                    }
                    let pic = Rect::from_min_size(min + vec2(0.0, TIME_H), PICTURE);
                    let Some(cell) = cells.get(i) else {
                        ui.painter().rect_filled(pic, 0.0, EMPTY);
                        continue;
                    };
                    let resp =
                        ui.interact(r, ui.id().with(("story-cell", i)), Sense::click_and_drag());
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &cell.name)
                    });
                    let parted = part_drag(app, b, i, cell, pic, &resp, alt);
                    let picked = app.media_window.story.picked.contains(&i);
                    paint_cell(ui, app, cell, r, pic, picked, &resp);
                    // Where a cell being moved, or media, would go.
                    let showing_drop = app.media_window.story.moving.is_some() || taken;
                    if showing_drop && under.is_some_and(|(k, left)| k == i && left) {
                        ui.painter().vline(
                            r.left() - GAP / 2.0,
                            r.y_range(),
                            Stroke::new(2.0, theme::banana()),
                        );
                    }
                    if resp.clicked() {
                        let mods = ui.input(|i| i.modifiers);
                        pick(app, i, mods);
                        if let Some(m) = cell.media {
                            app.selected_media = Some(m);
                        }
                    }
                    if resp.double_clicked() {
                        if cell.placeholder {
                            app.media_window.story.renaming = Some((i, cell.name.clone()));
                        } else if let Some(c) = cell.clip {
                            let start = app.project().clip(c).map(|c| c.start);
                            if let Some(t) = start {
                                app.seek(t);
                            }
                        } else if let Some(m) = cell.media {
                            let at = app.cursor;
                            app.place_media_range(m, at, cell.range, None);
                        }
                    }
                    if resp.drag_started() && !parted {
                        if alt {
                            app.media_window.story.moving = Some(i);
                        } else {
                            // To the timeline: the picked cells (this one
                            // among them), else this one.
                            let picked = &app.media_window.story.picked;
                            let which: Vec<usize> = if picked.contains(&i) {
                                picked.clone()
                            } else {
                                vec![i]
                            };
                            let mut which = which;
                            which.sort_unstable();
                            let out: Vec<StoryCell> = which
                                .iter()
                                .filter_map(|&k| cells.get(k))
                                .map(|c| match c.media {
                                    Some(m) => StoryCell::of(m, c.range),
                                    None => StoryCell::placeholder(&c.name),
                                })
                                .collect();
                            app.media_drag = Some(MediaDrag::Cells(out));
                            app.media_window.drag_from = match b {
                                Board::Own(id) => Some(DragFrom::Cells(id, which)),
                                Board::Main => None,
                            };
                        }
                    }
                    let name = cell.name.clone();
                    let (media, placeholder) = (cell.media, cell.placeholder);
                    resp.context_menu(|ui| {
                        if let Some(m) = media
                            && ui.button(tr("Open in Trimmer")).clicked()
                        {
                            action = Some(Box::new(move |app: &mut BananaApp| {
                                app.open_in_trimmer(m);
                                app.show_tab(crate::dock::Tab::Trimmer, false);
                            }));
                            ui.close();
                        }
                        if placeholder && ui.button(tr("Rename Placeholder")).clicked() {
                            let n = name.clone();
                            action = Some(Box::new(move |app: &mut BananaApp| {
                                app.media_window.story.renaming = Some((i, n));
                            }));
                            ui.close();
                        }
                        if let Board::Own(id) = b
                            && ui.button(tr("Remove from Storyboard")).clicked()
                        {
                            action = Some(Box::new(move |app: &mut BananaApp| {
                                app.modify(|p| p.storyboard_remove(id, i));
                                app.media_window.story.picked.clear();
                            }));
                            ui.close();
                        }
                    });
                }
                // Let go: a cell moved, or media brought.
                if released && let Some((k, left)) = under {
                    if let Some(from) = app.media_window.story.moving {
                        let to = if k >= cells.len() {
                            cells.len()
                        } else if left {
                            k
                        } else {
                            k + 1
                        };
                        moved = Some((from, to));
                    } else if taken {
                        dropped = Some((k, left));
                    }
                }
            });
    });
    if released {
        app.media_window.story.moving = None;
    }
    if cells.is_empty() {
        let note = match b {
            Board::Main => tr(
                "The timeline has no events yet: what is added to it shows here, and media let go here go to the timeline.",
            ),
            Board::Own(_) => {
                tr("Let media go on the cells to lay out a sequence; Alt and a drag moves a cell.")
            }
        };
        let r = Rect::from_min_size(
            inner.min + vec2(0.0, 2.0 * (cell_h + GAP)),
            vec2(inner.width(), 40.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            crate::tabs::hint(ui, note)
        });
    }
    if let Some(f) = action {
        f(app);
    }
    rename_dialog(ui, app, b, &cells);
    if let Some((from, to)) = moved {
        move_cell(app, b, &cells, from, to, stale);
    }
    if let Some((k, left)) = dropped {
        let Some(drag) = app.media_drag.take() else {
            return;
        };
        // Imports, cells and the move out of where they came from: one
        // undo step.
        let copy = ui.input(|i| i.modifiers.command);
        app.as_one_undo(|app| {
            let brought = incoming(app, &drag);
            let media: Vec<MediaId> = brought.iter().filter_map(|c| c.media).collect();
            if drop_cells(app, b, &cells, brought, k, left, stale)
                && let Board::Own(id) = b
            {
                app.moved_into_storyboard(id, copy, &media);
            }
        });
    }
}

/// A cell's part (a storyboard's of its own, its media's video): a drag
/// across its picture without Alt picks it or moves an end (VEGAS's Help:
/// "simply dragging a thumbnail sets a selection on the media"). Whether
/// the drag was taken.
fn part_drag(
    app: &mut BananaApp,
    b: Board,
    i: usize,
    cell: &Shown,
    pic: Rect,
    resp: &egui::Response,
    alt: bool,
) -> bool {
    let (Board::Own(id), Some(info)) = (b, &cell.info) else {
        return false;
    };
    if info.video.is_none() || info.still || info.duration <= Tick::ZERO {
        return false;
    }
    let rate = info.video.map(|v| v.frame_rate);
    let st = &mut app.media_window.story;
    if resp.drag_started()
        && !alt
        && let Some(o) = thumb_part::across(resp).filter(|o| pic.contains(*o))
    {
        st.parting = Some((
            i,
            thumb_part::drag_at(pic, o.x, info.duration, rate, cell.range),
        ));
    }
    let Some((_, drag)) = st.parting.filter(|(k, _)| *k == i) else {
        return false;
    };
    if let Some(p) = resp.interact_pointer_pos() {
        let t = thumb_part::time_across(pic, p.x, info.duration, rate);
        let range = thumb_part::dragged(drag, cell.range, t, info.duration);
        // One undo step a drag.
        let pressed = resp
            .ctx
            .input(|i| i.pointer.press_start_time().map(f64::to_bits));
        let key = egui::Id::new(("story-part", id, i, pressed));
        app.modify_merged(key, |p| {
            if let Some(s) = p.storyboard_mut(id)
                && let Some(c) = s.cells.get_mut(i)
            {
                c.range = range;
            }
        });
    }
    if resp.drag_stopped() || !resp.dragged() {
        app.media_window.story.parting = None;
    }
    true
}

/// Ctrl one cell more or less, Shift the run from the last, else this one.
fn pick(app: &mut BananaApp, i: usize, mods: Modifiers) {
    let picked = &mut app.media_window.story.picked;
    if mods.ctrl || mods.command {
        match picked.iter().position(|&k| k == i) {
            Some(k) => {
                picked.remove(k);
            }
            None => picked.push(i),
        }
    } else if mods.shift
        && let Some(&last) = picked.last()
    {
        *picked = (last.min(i)..=last.max(i)).collect();
    } else {
        *picked = vec![i];
    }
}

/// Draws a cell: the time over its picture, the picture (a placeholder's
/// name in red over the squares), its name under it.
fn paint_cell(
    ui: &Ui,
    app: &mut BananaApp,
    cell: &Shown,
    r: Rect,
    pic: Rect,
    picked: bool,
    resp: &egui::Response,
) {
    let hovered = resp.hovered();
    let painter = ui.painter_at(r.expand(4.0));
    if picked {
        painter.rect_filled(r.expand(3.0), 2.0, theme::selection().gamma_multiply(0.5));
    } else if hovered {
        painter.rect_filled(r.expand(3.0), 2.0, theme::hover());
    }
    let time = app.ruler_text(cell.start);
    painter.text(
        pos2(r.left() + 1.0, r.top() + TIME_H / 2.0),
        Align2::LEFT_CENTER,
        time,
        FontId::monospace(11.0),
        theme::text(),
    );
    painter.rect_filled(pic, 0.0, Color32::BLACK);
    if cell.placeholder {
        // The squares, the placeholder's note in red over them.
        let [light, dark] = [Color32::from_gray(0x62), Color32::from_gray(0x50)];
        painter.rect_filled(pic, 0.0, light);
        let side = 10.0;
        for row in 0..(pic.height() / side).ceil() as usize {
            for col in (row % 2..(pic.width() / side).ceil() as usize).step_by(2) {
                let min = pic.min + vec2(col as f32 * side, row as f32 * side);
                let sq = Rect::from_min_size(min, vec2(side, side)).intersect(pic);
                painter.rect_filled(sq, 0.0, dark);
            }
        }
        crate::widgets::fit_text(
            &painter,
            pic.center(),
            Align2::CENTER_CENTER,
            &cell.name,
            FontId::proportional(18.0),
            Color32::from_rgb(0xe0, 0x20, 0x20),
            pic.width() - 8.0,
        );
    } else if let Some(info) = &cell.info {
        let h = (pic.height() * ui.ctx().pixels_per_point()).round() as u32;
        // Hover scrub (unless Bypass Hover Scrub): the frame of the
        // pointer's place across, the picture spanning the media.
        let scrubs = info.video.is_some() && !info.still && info.duration > Tick::ZERO;
        let pointer = resp
            .hover_pos()
            .or_else(|| {
                resp.dragged()
                    .then(|| resp.interact_pointer_pos())
                    .flatten()
            })
            .filter(|p| pic.contains(*p));
        let at = match pointer {
            Some(p) if scrubs && (!app.media_window.bypass_scrub || resp.dragged()) => {
                let k = ((p.x - pic.left()) / pic.width()).clamp(0.0, 1.0);
                let step = (k * 40.0).round() / 40.0;
                Tick((info.duration.0 as f64 * step as f64) as i64)
            }
            _ => cell.at,
        };
        if let Some(t) = app.engine.thumbnail(info, at, h) {
            let id = app.textures.get(ui.ctx(), &t);
            let aspect = t.width as f32 / t.height.max(1) as f32;
            let w = (pic.height() * aspect).min(pic.width());
            let fit = Rect::from_center_size(pic.center(), vec2(w, w / aspect.max(0.01)));
            painter.image(
                id,
                fit,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            let icon = if info.video.is_none() {
                icons::Icon::FileAudio
            } else {
                icons::Icon::FileVideo
            };
            icons::paint(&painter, pic.shrink(28.0), icon, theme::text_dim());
        }
        if scrubs && let Some(part) = cell.range.filter(|_| cell.clip.is_none()) {
            thumb_part::paint(&painter, pic, info.duration, part);
        }
    }
    let galley = painter.layout(
        cell.name.clone(),
        FontId::proportional(12.0),
        theme::text(),
        PICTURE.x,
    );
    painter.galley(
        pos2(r.center().x - galley.size().x / 2.0, pic.bottom() + 3.0),
        galley,
        theme::text(),
    );
}

/// The placeholder being renamed: its name in a small window.
fn rename_dialog(ui: &Ui, app: &mut BananaApp, b: Board, cells: &[Shown]) {
    let Some((i, mut text)) = app.media_window.story.renaming.take() else {
        return;
    };
    let Board::Own(id) = b else {
        return;
    };
    if cells.get(i).is_none_or(|c| !c.placeholder) {
        return;
    }
    let (mut open, mut ok) = (true, false);
    crate::floating::Window::new(tr("Rename Placeholder"))
        .id(egui::Id::new("rename-placeholder"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(220.0));
            edit.request_focus();
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            }
            if ui.button("OK").clicked() {
                ok = true;
            }
        });
    if ok {
        let name = text.trim().to_string();
        if !name.is_empty() {
            app.modify(|p| {
                if let Some(s) = p.storyboard_mut(id)
                    && let Some(c) = s.cells.get_mut(i)
                {
                    c.name = name;
                }
            });
        }
    } else if open {
        app.media_window.story.renaming = Some((i, text));
    }
}

/// The Main Timeline storyboard may be changed: in step with the
/// timeline (else a note says to refresh it first).
fn may_change(app: &mut BananaApp, b: Board, stale: bool) -> bool {
    if b == Board::Main && stale {
        app.notify(tr(
            "The Main Timeline storyboard shows an older state of the timeline: refresh it first.",
        ));
        return false;
    }
    true
}

/// Cell `from` moved before place `to`.
fn move_cell(app: &mut BananaApp, b: Board, cells: &[Shown], from: usize, to: usize, stale: bool) {
    if from == to || to == from + 1 || !may_change(app, b, stale) {
        return;
    }
    match b {
        Board::Own(id) => app.modify(|p| p.storyboard_move(id, from, to)),
        Board::Main => {
            let (Some(clip), before) = (
                cells.get(from).and_then(|c| c.clip),
                cells.get(to).and_then(|c| c.clip),
            ) else {
                return;
            };
            app.modify(|p| {
                let _ = p.move_in_sequence(clip, before);
            });
            app.media_window.story.snapshot = None;
        }
    }
    app.media_window.story.picked.clear();
}

/// Media brought to place `k` (`left`: before that cell). Whether they
/// went in.
fn drop_cells(
    app: &mut BananaApp,
    b: Board,
    cells: &[Shown],
    brought: Vec<StoryCell>,
    k: usize,
    left: bool,
    stale: bool,
) -> bool {
    if brought.is_empty() || !may_change(app, b, stale) {
        return false;
    }
    match b {
        Board::Own(id) => {
            let len = cells.len();
            app.modify(|p| {
                for (n, cell) in brought.into_iter().enumerate() {
                    // The first goes on the empty cell, takes the cell's
                    // place or goes before it; the others follow it.
                    let end = p.storyboard(id).map_or(0, |s| s.cells.len());
                    match (k >= len, n) {
                        (true, 0) => p.storyboard_drop(id, k, cell, false),
                        (true, _) => p.storyboard_drop(id, end, cell, false),
                        (false, 0) => p.storyboard_drop(id, k, cell, left),
                        (false, _) => p.storyboard_drop(id, k + n, cell, true),
                    }
                }
            });
        }
        Board::Main => {
            // To the timeline's sequence: at its end, then before the cell
            // let go on.
            let before = cells.get(k).and_then(|c| c.clip);
            add_to_sequence(app, &brought, before);
            app.media_window.story.snapshot = None;
        }
    }
    true
}

/// Puts `cells` at the end of the timeline's sequence, then before event
/// `before` when there is one.
fn add_to_sequence(app: &mut BananaApp, cells: &[StoryCell], before: Option<ClipId>) {
    let p = app.project();
    let track = p.storyboard_track().or_else(|| {
        p.tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && !t.adjustment)
            .map(|t| t.id)
    });
    let end = p
        .timeline_cells()
        .iter()
        .map(|c| c.start + c.length)
        .max()
        .unwrap_or(Tick::ZERO);
    app.as_one_undo(|app| {
        let added = app.add_story_cells(cells, end, track);
        if let Some(b) = before {
            app.modify(|p| {
                for c in added {
                    let _ = p.move_in_sequence(c, Some(b));
                }
            });
        }
    });
}

impl BananaApp {
    /// Storyboard cells on the timeline from `at` on `track`, end to end:
    /// each media (or its part), a placeholder as a text media saying its
    /// name. The events added on `track` (or the first of each placed).
    pub(crate) fn add_story_cells(
        &mut self,
        cells: &[StoryCell],
        at: Tick,
        track: Option<TrackId>,
    ) -> Vec<ClipId> {
        let new = self.settings.prefs.new_events();
        let mut t = at;
        let mut added = Vec::new();
        // Every cell placed: one undo step.
        let mark = self.history.undo_mark();
        for c in cells {
            let before: Vec<ClipId> = self.project().clips().map(|(_, c)| c.id).collect();
            let length = match (c.media, c.range) {
                (Some(m), range) => {
                    let Some(info) = info_of(self, m) else {
                        continue;
                    };
                    self.place_media_range(m, t, range, track);
                    range.map_or_else(|| new.length(&info), |r| r.duration)
                }
                (None, _) => {
                    let Some(Generator::Plugin(mut fx)) =
                        crate::tabs::generators::titles_generator()
                    else {
                        continue;
                    };
                    fx.params.insert(
                        "text".into(),
                        PluginParam {
                            value: PluginValue::Text(placeholder_name(&c.name)),
                            keys: Vec::new(),
                        },
                    );
                    let g = Generator::Plugin(fx);
                    let length = Tick::from_seconds(5);
                    let video = track.filter(|&id| {
                        self.project()
                            .track(id)
                            .is_some_and(|tr| tr.kind == TrackKind::Video)
                    });
                    let mut new_tracks = self.drop_tracks;
                    self.modify(|p| {
                        // The track let go on, else the first video track
                        // (made when there is none); below the tracks, a
                        // new one.
                        let video = video
                            .or_else(|| new_tracks.as_mut().map(|f| f.track(p, TrackKind::Video)))
                            .or_else(|| {
                                p.tracks
                                    .iter()
                                    .find(|tr| tr.kind == TrackKind::Video && !tr.adjustment)
                                    .map(|tr| tr.id)
                            })
                            .unwrap_or_else(|| p.add_track(TrackKind::Video));
                        let info = MediaInfo::generated(g, &p.settings);
                        let m = p.add_media(info);
                        if let Ok(ids) = p.insert_media(m, t, Some(video), None)
                            && let Some(&id) = ids.first()
                        {
                            let _ = p.set_clip_duration(id, length);
                        }
                    });
                    if self.drop_tracks.is_some() {
                        self.drop_tracks = new_tracks;
                    }
                    length
                }
            };
            // The new event on the sequence's track (or any new one).
            let p = self.project();
            let fresh: Vec<(TrackId, ClipId)> = p
                .clips()
                .filter(|(_, c)| !before.contains(&c.id))
                .map(|(tr, c)| (tr.id, c.id))
                .collect();
            if let Some(&(_, id)) = fresh
                .iter()
                .find(|(tr, _)| Some(*tr) == track)
                .or_else(|| fresh.first())
            {
                added.push(id);
            }
            t += length;
        }
        self.history.join_since(mark);
        added
    }

    /// Start Preview in a storyboard: the picked cells (all of them when
    /// none or all are picked), one after another.
    pub(crate) fn preview_storyboard(&mut self) {
        let Some(b) = board(self) else {
            return;
        };
        let (cells, _) = shown_cells(self, b);
        let mut picked = self.media_window.story.picked.clone();
        picked.sort_unstable();
        let which: Vec<&Shown> = if picked.is_empty() || picked.len() == cells.len() {
            cells.iter().collect()
        } else {
            picked.iter().filter_map(|&i| cells.get(i)).collect()
        };
        let items: Vec<(MediaInfo, Option<TimeRange>)> = which
            .iter()
            .filter_map(|c| {
                let info = c.info.clone()?;
                let range = c.range.or_else(|| {
                    // A Main Timeline cell: its event's part.
                    let clip = self.project().clip(c.clip?)?;
                    Some(TimeRange::new(clip.source_in, clip.duration))
                });
                Some((info, range))
            })
            .collect();
        if items.is_empty() {
            return;
        }
        self.start_sequence_preview(items);
    }

    /// Refresh Storyboard: the Main Timeline storyboard shows the timeline
    /// as it is now.
    pub(crate) fn refresh_storyboard(&mut self) {
        self.media_window.story.snapshot = Some(self.project().timeline_cells());
    }

    /// Whether the Main Timeline storyboard shows an older state of the
    /// timeline.
    pub(crate) fn storyboard_stale(&self) -> bool {
        !self.settings.storyboard_auto_sync
            && self
                .media_window
                .story
                .snapshot
                .as_ref()
                .is_some_and(|s| *s != self.project().timeline_cells())
    }
}

impl BananaApp {
    /// Project Media on a storyboard (screen tests): `"own"`, one of the
    /// project's media with a placeholder among them; `"main"`, the Main
    /// Timeline's, refreshed; `"stale"`, the Main Timeline's after an edit
    /// it has not followed.
    pub fn storyboards_for_review(&mut self, which: &str) {
        match which {
            "own" => {
                let media: Vec<MediaId> =
                    self.project().media.iter().take(3).map(|m| m.id).collect();
                let id = self.modify(|p| {
                    let id = p.add_storyboard("Opening");
                    for (k, &m) in media.iter().enumerate() {
                        // A placeholder where the second shot goes.
                        let at = if k == 0 { 0 } else { k + 1 };
                        p.storyboard_drop(id, at, StoryCell::of(m, None), false);
                    }
                    id
                });
                self.media_window.bin = BinChoice::Storyboard(id);
                self.media_window.story.picked = vec![0];
            }
            _ => {
                self.media_window.bin = BinChoice::MainTimeline;
                self.refresh_storyboard();
                if which == "stale"
                    && let Some(c) = self.project().timeline_cells().first().copied()
                {
                    self.modify(|p| {
                        let _ = p.move_clip(c.clip, c.start + Tick::from_seconds(1), None);
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::{Event, PointerButton};
    use egui_kittest::Harness;

    fn harness(app: BananaApp) -> Harness<'static, BananaApp> {
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
        h
    }

    fn spot(h: &Harness<'_, BananaApp>, i: usize) -> Pos2 {
        let (origin, cols) = h.state().media_window.story.layout.expect("the cells");
        cell_center(origin, cols, i)
    }

    fn button(h: &mut Harness<'_, BananaApp>, at: Pos2, pressed: bool, modifiers: Modifiers) {
        h.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        });
        h.step();
    }

    /// A drag across a cell picks its part (one undo); media let go on
    /// a storyboard leave the custom bin they came from, unless Ctrl;
    /// another storyboard's cells move.
    #[test]
    fn cells_take_parts_and_media_move_into_storyboards() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        let a = crate::tabs::media::tests::offline_video(&mut app, "a");
        let b = crate::tabs::media::tests::offline_video(&mut app, "b");
        let bin = app.modify(|p| p.add_bin(None, "Shots").unwrap());
        app.modify(|p| {
            let _ = p.add_to_bin(bin, a);
            let _ = p.add_to_bin(bin, b);
        });
        let one = app.modify(|p| {
            let id = p.add_storyboard("One");
            p.storyboard_drop(id, 0, StoryCell::of(a, None), false);
            id
        });
        let two = app.modify(|p| p.add_storyboard("Two"));
        app.media_window.bin = BinChoice::Storyboard(one);
        app.show_tab(crate::dock::Tab::ProjectMedia, false);
        let mut h = harness(app);
        // Across the first cell's picture, from 30 % to 70 %.
        let c = spot(&h, 0);
        let at = |f: f32| pos2(c.x - PICTURE.x / 2.0 + PICTURE.x * f, c.y);
        let undos = h.state().history.undo_len();
        h.event(Event::PointerMoved(at(0.3)));
        h.step();
        button(&mut h, at(0.3), true, Modifiers::NONE);
        for k in 1..=8 {
            h.event(Event::PointerMoved(at(0.3 + 0.05 * k as f32)));
            h.step();
        }
        button(&mut h, at(0.7), false, Modifiers::NONE);
        h.run_steps(2);
        assert!(h.state().media_drag.is_none());
        let range = h.state().project().storyboard(one).unwrap().cells[0].range;
        let secs = |t: Tick| t.0 as f64 / Tick::from_seconds(1).0 as f64;
        assert!(
            range.is_some_and(
                |r| (secs(r.start) - 3.0).abs() < 0.2 && (secs(r.end()) - 7.0).abs() < 0.2
            ),
            "{range:?}"
        );
        assert_eq!(h.state().history.undo_len(), undos + 1, "one undo");
        // Media of the custom bin let go on storyboard Two: out of the bin.
        h.state_mut().media_window.bin = BinChoice::Storyboard(two);
        h.run_steps(2);
        let drop = |h: &mut Harness<'_, BananaApp>, m: MediaId, i: usize, mods: Modifiers| {
            let at = spot(h, i);
            let w = &mut h.state_mut().media_window;
            w.drag_from = Some(DragFrom::Bin(bin));
            h.state_mut().media_drag = Some(MediaDrag::Media(m));
            h.event(Event::ModifiersChanged(mods));
            h.event(Event::PointerMoved(at));
            h.step();
            button(h, at, false, mods);
            h.event(Event::ModifiersChanged(Modifiers::NONE));
            h.run_steps(2);
        };
        drop(&mut h, a, 0, Modifiers::NONE);
        let held = |h: &Harness<'_, BananaApp>| h.state().project().bin(bin).unwrap().media.clone();
        assert_eq!(held(&h), [b], "moved out of the bin");
        drop(&mut h, b, 1, Modifiers::COMMAND);
        assert_eq!(held(&h), [b], "Ctrl copies");
        let two_cells = h.state().project().storyboard(two).unwrap().cells.len();
        assert_eq!(two_cells, 2);
        // Storyboard One's cell dragged to Two: it leaves One.
        let w = &mut h.state_mut().media_window;
        w.drag_from = Some(DragFrom::Cells(one, vec![0]));
        let cell = h.state().project().storyboard(one).unwrap().cells[0].clone();
        h.state_mut().media_drag = Some(MediaDrag::Cells(vec![cell]));
        let at = spot(&h, 2);
        h.event(Event::PointerMoved(at));
        h.step();
        button(&mut h, at, false, Modifiers::NONE);
        h.run_steps(2);
        assert!(
            h.state()
                .project()
                .storyboard(one)
                .unwrap()
                .cells
                .is_empty()
        );
        assert_eq!(h.state().project().storyboard(two).unwrap().cells.len(), 3);
    }

    #[test]
    fn storyboards_take_media_move_cells_and_follow_the_timeline() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let media: Vec<MediaId> = app.project().media.iter().map(|m| m.id).collect();
        let id = app.modify(|p| p.add_storyboard("Try"));
        app.media_window.bin = BinChoice::Storyboard(id);
        app.show_tab(crate::dock::Tab::ProjectMedia, false);
        let mut h = harness(app);
        // Media let go on the third (empty) cell: two placeholders first.
        let at = spot(&h, 2);
        h.state_mut().media_drag = Some(MediaDrag::Media(media[0]));
        h.event(Event::PointerMoved(at));
        h.step();
        button(&mut h, at, false, Modifiers::NONE);
        h.run_steps(2);
        let cells = |h: &Harness<'_, BananaApp>| -> Vec<Option<MediaId>> {
            h.state()
                .project()
                .storyboard(id)
                .unwrap()
                .cells
                .iter()
                .map(|c| c.media)
                .collect()
        };
        assert_eq!(cells(&h), [None, None, Some(media[0])]);
        // Alt and a drag: the media's cell before the first.
        let (from, to) = (spot(&h, 2), spot(&h, 0) - vec2(PICTURE.x * 0.4, 0.0));
        h.event(Event::PointerMoved(from));
        h.event(Event::ModifiersChanged(Modifiers::ALT));
        h.step();
        button(&mut h, from, true, Modifiers::ALT);
        for k in 1..=8 {
            let p = from + (to - from) * (k as f32 / 8.0);
            h.event(Event::PointerMoved(p));
            h.step();
        }
        button(&mut h, to, false, Modifiers::ALT);
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        h.run_steps(2);
        assert_eq!(cells(&h), [Some(media[0]), None, None]);
        // On the timeline: the media, then a placeholder as text.
        let before = h.state().project().clips().count();
        let story = h.state().project().storyboard(id).unwrap().cells.clone();
        let added = h
            .state_mut()
            .add_story_cells(&story[..2], Tick::from_seconds(30), None);
        assert_eq!(added.len(), 2);
        assert!(h.state().project().clips().count() > before);
        // The Main Timeline storyboard: the timeline's sequence; an edit
        // leaves it behind until it is refreshed.
        h.state_mut().media_window.bin = BinChoice::MainTimeline;
        h.state_mut().refresh_storyboard();
        h.run_steps(2);
        assert!(!h.state().storyboard_stale());
        let first = h.state().project().timeline_cells()[0].clip;
        h.state_mut().modify(|p| {
            let _ = p.move_clip(first, Tick::from_seconds(60), None);
        });
        h.run_steps(2);
        assert!(h.state().storyboard_stale());
        h.state_mut().refresh_storyboard();
        assert!(!h.state().storyboard_stale());
    }
}
