//! The CD layout bar and the command bar, under the marker bar while the
//! project has a CD layout or command markers, as VEGAS's Help shows them:
//! a track region's tags in red at both its ends with "Track n" after the
//! first, an index marker's in purple with "Index n", a command's a blue
//! flag (their colors measured on the Help's pictures). A click puts the
//! cursor at a tag, a drag moves it (Alt: a track keeps its length), a
//! double-click selects a track or edits a command, the right button
//! opens their menus.

use crate::i18n::{tr, trc};
use bv_core::{Tick, TimeRange};
use bv_timeline::{MarkerId, Project};

use super::bars::{self, BarDrag, BarSpot};
use super::lanes::ContextTarget;
use super::marker_tool::{self, MarkerBar};
use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Shape, Stroke, pos2};
use crate::theme;

/// The bars' height.
pub(crate) const BAR_H: f32 = 15.0;
/// A tag's width.
const TAG_W: f32 = 8.0;

/// A tag's colors: its face, its lit left edge, its shaded right edge
/// (the same in every interface type, as markers').
struct TagColors {
    face: Color32,
    light: Color32,
    dark: Color32,
}

const CD_TRACK: TagColors = TagColors {
    face: Color32::from_rgb(0xDB, 0x57, 0x57),
    light: Color32::from_rgb(0xE7, 0x8F, 0x8F),
    dark: Color32::from_rgb(0x92, 0x3A, 0x3A),
};
const CD_INDEX: TagColors = TagColors {
    face: Color32::from_rgb(0xBD, 0x57, 0xDB),
    light: Color32::from_rgb(0xD3, 0x8F, 0xE7),
    dark: Color32::from_rgb(0x7E, 0x3A, 0x92),
};
const COMMAND: TagColors = TagColors {
    face: Color32::from_rgb(0x21, 0x61, 0xFF),
    light: Color32::from_rgb(0x6B, 0x96, 0xFF),
    dark: Color32::from_rgb(0x10, 0x41, 0xAD),
};

/// A tag of the CD layout bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CdTag {
    Start(MarkerId),
    End(MarkerId),
    Index(MarkerId),
}

/// The CD layout bar's tag under `x`: an index's, a track's start or end.
fn cd_tag_at(view: &TimelineView, l: &Layout, p: &Project, x: f32) -> Option<CdTag> {
    let over = |left: f32| x >= left - 2.0 && x <= left + TAG_W + 2.0;
    p.cd_indices
        .iter()
        .find(|i| over(view.x_of(l, i.time)))
        .map(|i| CdTag::Index(i.id))
        .or_else(|| {
            p.cd_tracks.iter().find_map(|t| {
                if over(view.x_of(l, t.range.start)) {
                    Some(CdTag::Start(t.id))
                } else if over(view.x_of(l, t.range.end()) - TAG_W) {
                    Some(CdTag::End(t.id))
                } else {
                    None
                }
            })
        })
}

/// Where a CD tag stands.
fn cd_tag_time(p: &Project, tag: CdTag) -> Option<Tick> {
    match tag {
        CdTag::Start(id) => p.cd_track(id).map(|t| t.range.start),
        CdTag::End(id) => p.cd_track(id).map(|t| t.range.end()),
        CdTag::Index(id) => p.cd_indices.iter().find(|i| i.id == id).map(|i| i.time),
    }
}

/// The command bar's flag under `x`.
fn command_at(view: &TimelineView, l: &Layout, p: &Project, x: f32) -> Option<MarkerId> {
    p.commands
        .iter()
        .rev()
        .find(|c| {
            let left = view.x_of(l, c.time);
            x >= left - 2.0 && x <= left + TAG_W + 2.0
        })
        .map(|c| c.id)
}

/// A CD track's label: its number, and its name when it has one.
pub(crate) fn cd_track_label(p: &Project, id: MarkerId) -> String {
    let n = p.cd_track_number(id).unwrap_or(1);
    match p.cd_track(id).map(|t| t.name.trim()) {
        Some(name) if !name.is_empty() => trf!("Track {}: {}", n, name),
        _ => trf!("Track {}", n),
    }
}

/// A command marker's label: its comment (its name, as a marker's), or the
/// command and what it is given; a caption's, the text it shows (its
/// comment holds its markup).
pub(crate) fn command_label(c: &bv_timeline::CommandMarker) -> String {
    if bv_timeline::captions::channel_of(&c.command).is_some() {
        let text = bv_timeline::captions::shown_text(&bv_timeline::captions::marker_markup(c));
        let text = text.replace("[BR]", " ");
        return if text.trim().is_empty() {
            c.command.clone()
        } else {
            format!("{}: {}", c.command, text)
        };
    }
    if !c.comment.trim().is_empty() {
        c.comment.lines().next().unwrap_or_default().to_string()
    } else if c.parameter.is_empty() {
        c.command.clone()
    } else {
        format!("{}: {}", c.command, c.parameter)
    }
}

pub(crate) fn interact(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
) {
    if l.cd_bar.height() > 0.0 {
        cd_interact(view, ui, app, l);
    }
    if l.command_bar.height() > 0.0 {
        command_interact(view, ui, app, l);
    }
}

fn cd_interact(view: &mut TimelineView, ui: &mut egui::Ui, app: &mut BananaApp, l: &Layout) {
    let project = app.project().clone();
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let bar = ui.interact(l.cd_bar, ui.id().with("barra-cd"), Sense::click_and_drag());
    let hit = pointer.and_then(|p| cd_tag_at(view, l, &project, p.x));
    // A tag moves when dragged: the arrow with ↔, as VEGAS's flags (while
    // it is dragged too: see `bars::interact`).
    if bar.hovered() && hit.is_some() {
        bars::show(ui.ctx(), BarSpot::Movable);
    }
    if bar.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && let Some(tag) = cd_tag_at(view, l, &project, o.x)
        && let Some(at) = cd_tag_time(&project, tag)
    {
        if view.marker_tool.active {
            // The Marker Tool moves its picked tags together.
            let at = view.time_at(l, o.x);
            marker_tool::start_drag(view, &project, MarkerBar::Cd, tag_id(tag), at);
        } else {
            // Alt moves a whole track, its length kept (as regions).
            let whole = ui.input(|i| i.modifiers.alt);
            let grab = view.time_at(l, o.x) - at;
            view.bar_drag = Some(BarDrag::Cd { tag, whole, grab });
        }
    }
    if bar.dragged()
        && let Some(p) = pointer
        && matches!(view.bar_drag, Some(BarDrag::Picked))
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let now = view.time_at(l, x);
        marker_tool::drag(view, app, &project, now, bar.id);
    }
    if bar.dragged()
        && let Some(p) = pointer
        && let Some(BarDrag::Cd { tag, whole, grab }) = view.bar_drag
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let raw = (view.time_at(l, x) - grab).max(Tick::ZERO);
        let t = view.snap(app, &project, raw, &[Tick::ZERO], &[]);
        app.modify_merged(bar.id, |p| match tag {
            CdTag::Index(id) => {
                let _ = p.move_cd_index(id, t);
            }
            CdTag::Start(id) | CdTag::End(id) if whole => {
                // The whole track, its indices with it.
                let Some(r) = p.cd_track(id).map(|c| c.range) else {
                    return;
                };
                let start = match tag {
                    CdTag::Start(_) => t,
                    _ => (t - r.duration).max(Tick::ZERO),
                };
                let _ = p.move_cd_track(id, start);
            }
            CdTag::Start(id) | CdTag::End(id) => {
                let _ = p.set_cd_track(id, |c| {
                    let r = c.range;
                    c.range = match tag {
                        CdTag::Start(_) => {
                            TimeRange::from_bounds(t.min(r.end() - Tick(1)), r.end())
                        }
                        _ => TimeRange::from_bounds(r.start, t.max(r.start + Tick(1))),
                    };
                });
            }
        });
    }
    if bar.drag_stopped() && matches!(view.bar_drag, Some(BarDrag::Cd { .. })) {
        view.bar_drag = None;
        view.snap_line = None;
    }
    if bar.drag_stopped() {
        marker_tool::end_drag(view);
    }
    if bar.clicked() {
        pick(view, ui, &project, MarkerBar::Cd, hit.map(tag_id));
    }
    if bar.clicked()
        && let Some(t) = hit.and_then(|h| cd_tag_time(&project, h))
    {
        app.seek(t);
    }
    // A double-click on a track's tag selects the track.
    if bar.double_clicked()
        && let Some(CdTag::Start(id) | CdTag::End(id)) = hit
        && let Some(t) = project.cd_track(id)
    {
        app.set_loop_region(Some(t.range));
    }
    if bar.secondary_clicked()
        && let Some(p) = pointer
    {
        view.context = ContextTarget::CdBar(hit, view.time_at(l, p.x));
    }
    if let Some(tag) = hit {
        let text = match tag {
            CdTag::Index(id) => {
                let n = project
                    .cd_rows()
                    .iter()
                    .find(|r| r.id == id)
                    .map_or(2, |r| r.index);
                trf!("Index {}", n)
            }
            CdTag::Start(id) | CdTag::End(id) => cd_track_label(&project, id),
        };
        bar.clone().on_hover_text(text);
    }
    bar.context_menu(|ui| {
        let ContextTarget::CdBar(hit, t) = view.context else {
            ui.close();
            return;
        };
        match hit {
            Some(CdTag::Start(id) | CdTag::End(id)) => {
                let Some(track) = project.cd_track(id) else {
                    ui.close();
                    return;
                };
                if ui.button(tr("Rename…")).clicked() {
                    app.dialogs.rename = Some((id, track.name.clone()));
                    ui.close();
                }
                if ui.button(tr("Select Track")).clicked() {
                    app.set_loop_region(Some(track.range));
                    ui.close();
                }
                if ui.button(trc("tag", "Go to Start")).clicked() {
                    app.seek(track.range.start);
                    ui.close();
                }
                if ui.button(trc("tag", "Go to End")).clicked() {
                    app.seek(track.range.end());
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Delete")).clicked() {
                    app.remove_marker(id);
                    ui.close();
                }
            }
            Some(CdTag::Index(id)) => {
                if ui.button(tr("Delete")).clicked() {
                    app.remove_marker(id);
                    ui.close();
                }
            }
            None => {
                let t = project.settings.frame_rate.snap_floor(t.max(Tick::ZERO));
                let inside = project.cd_track_at(t).is_some_and(|c| t > c.range.start);
                if ui
                    .add_enabled(
                        inside,
                        egui::Button::new(tr("Insert Audio CD Track Index Here")),
                    )
                    .clicked()
                {
                    app.modify(|p| p.add_cd_index(t));
                    ui.close();
                }
            }
        }
    });
}

fn command_interact(view: &mut TimelineView, ui: &mut egui::Ui, app: &mut BananaApp, l: &Layout) {
    let project = app.project().clone();
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let bar = ui.interact(
        l.command_bar,
        ui.id().with("barra-comandos"),
        Sense::click_and_drag(),
    );
    let hit = pointer.and_then(|p| command_at(view, l, &project, p.x));
    if bar.hovered() && hit.is_some() {
        bars::show(ui.ctx(), BarSpot::Movable);
    }
    if bar.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && let Some(id) = command_at(view, l, &project, o.x)
        && let Some(c) = project.command(id)
    {
        if view.marker_tool.active {
            let at = view.time_at(l, o.x);
            marker_tool::start_drag(view, &project, MarkerBar::Commands, id, at);
        } else {
            let grab = view.time_at(l, o.x) - c.time;
            view.bar_drag = Some(BarDrag::Command { id, grab });
        }
    }
    if bar.dragged()
        && let Some(p) = pointer
        && matches!(view.bar_drag, Some(BarDrag::Picked))
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let now = view.time_at(l, x);
        marker_tool::drag(view, app, &project, now, bar.id);
    }
    if bar.dragged()
        && let Some(p) = pointer
        && let Some(BarDrag::Command { id, grab }) = view.bar_drag
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let raw = (view.time_at(l, x) - grab).max(Tick::ZERO);
        let t = view.snap(app, &project, raw, &[Tick::ZERO], &[]);
        app.modify_merged(bar.id, |p| p.set_command(id, |c| c.time = t));
    }
    if bar.drag_stopped() && matches!(view.bar_drag, Some(BarDrag::Command { .. })) {
        view.bar_drag = None;
        view.snap_line = None;
    }
    if bar.drag_stopped() {
        marker_tool::end_drag(view);
    }
    if bar.clicked() {
        pick(view, ui, &project, MarkerBar::Commands, hit);
    }
    if bar.clicked()
        && let Some(c) = hit.and_then(|id| project.command(id))
    {
        app.seek(c.time);
    }
    if bar.double_clicked()
        && let Some(id) = hit
    {
        app.edit_command(id);
    }
    if bar.secondary_clicked()
        && let Some(p) = pointer
    {
        view.context = ContextTarget::CommandBar(hit, view.time_at(l, p.x));
    }
    if let Some(c) = hit.and_then(|id| project.command(id)) {
        let mut text = c.command.clone();
        if !c.parameter.is_empty() {
            text = format!("{text}: {}", c.parameter);
        }
        if !c.comment.is_empty() {
            text = format!("{text}\n{}", c.comment);
        }
        bar.clone().on_hover_text(text);
    }
    bar.context_menu(|ui| {
        let ContextTarget::CommandBar(hit, t) = view.context else {
            ui.close();
            return;
        };
        match hit {
            Some(id) => {
                if ui.button(tr("Edit…")).clicked() {
                    app.edit_command(id);
                    ui.close();
                }
                if ui.button(tr("Delete")).clicked() {
                    app.remove_marker(id);
                    ui.close();
                }
            }
            None => {
                if ui.button(tr("Insert Command Here…")).clicked() {
                    let t = project.settings.frame_rate.snap_floor(t.max(Tick::ZERO));
                    app.open_command_dialog(t);
                    ui.close();
                }
            }
        }
    });
}

/// A tag at `x`, `bar` tall: the CD bar's rectangles, the command bar's
/// flag cut away to its lower left (VEGAS's 8 × 13 picture).
fn paint_tag(painter: &egui::Painter, bar: Rect, x: f32, colors: &TagColors, flag: bool) {
    let (top, bottom) = (bar.top() + 1.0, bar.bottom());
    if flag {
        let bottom = (top + 13.0).min(bottom);
        let cut = top + 6.0;
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x, top),
                pos2(x + TAG_W, top),
                pos2(x + TAG_W, cut),
                pos2(x, bottom),
            ],
            colors.face,
            Stroke::NONE,
        ));
        painter.line_segment(
            [pos2(x + 0.5, top), pos2(x + 0.5, bottom)],
            Stroke::new(1.0, colors.light),
        );
        painter.line_segment(
            [pos2(x, top + 0.5), pos2(x + TAG_W, top + 0.5)],
            Stroke::new(1.0, colors.light),
        );
        painter.line_segment(
            [pos2(x + TAG_W - 0.5, top), pos2(x + TAG_W - 0.5, cut)],
            Stroke::new(1.0, colors.dark),
        );
        painter.line_segment(
            [pos2(x + TAG_W, cut), pos2(x, bottom)],
            Stroke::new(1.0, colors.dark),
        );
    } else {
        let r = Rect::from_min_max(pos2(x, top), pos2(x + TAG_W, bottom));
        painter.rect_filled(r, 0.0, colors.face);
        painter.line_segment(
            [pos2(x + 0.5, top), pos2(x + 0.5, bottom)],
            Stroke::new(1.0, colors.light),
        );
        painter.line_segment(
            [pos2(x + TAG_W - 0.5, top), pos2(x + TAG_W - 0.5, bottom)],
            Stroke::new(1.0, colors.dark),
        );
    }
}

/// A tag's label after it, cut where the next tag begins.
fn paint_label(painter: &egui::Painter, bar: Rect, x: f32, until: f32, text: &str) {
    let room = until - x - 4.0;
    if room < 8.0 {
        return;
    }
    crate::widgets::fit_text(
        painter,
        pos2(x, bar.center().y),
        Align2::LEFT_CENTER,
        text.to_string(),
        FontId::proportional(10.5),
        theme::text(),
        room,
    );
}

/// A CD tag's item: the track's (either end) or the index's.
fn tag_id(tag: CdTag) -> MarkerId {
    match tag {
        CdTag::Start(id) | CdTag::End(id) | CdTag::Index(id) => id,
    }
}

/// A click with the Marker Tool on: the tag picked (Shift: the tags
/// between, Ctrl: one more or less), or nothing picked on an empty spot.
pub(crate) fn pick(
    view: &mut TimelineView,
    ui: &egui::Ui,
    project: &Project,
    bar: MarkerBar,
    hit: Option<MarkerId>,
) {
    if !view.marker_tool.active {
        return;
    }
    let m = ui.input(|i| i.modifiers);
    match hit {
        Some(id) => view
            .marker_tool
            .click(project, bar, id, m.shift, m.ctrl || m.command),
        None if !m.shift && !(m.ctrl || m.command) => {
            view.marker_tool.picks.clear();
            view.marker_tool.last = None;
            view.marker_tool.bar = bar;
        }
        None => {}
    }
}

pub(crate) fn paint(
    view: &TimelineView,
    ui: &egui::Ui,
    app: &BananaApp,
    l: &Layout,
    project: &Project,
) {
    let painter = ui.painter();
    // Their lines cross the tracks as the markers' do.
    let lines = app.settings.prefs.marker_lines || app.snapping;
    let lanes = painter.with_clip_rect(if lines { l.lanes } else { Rect::NOTHING });
    if l.cd_bar.height() > 0.0 {
        let bar = l.cd_bar;
        painter.rect_filled(bar, 0.0, theme::panel_dark());
        painter.line_segment(
            [bar.left_top(), bar.right_top()],
            Stroke::new(1.0, theme::bg()),
        );
        let bp = painter.with_clip_rect(bar);
        let rows = project.cd_rows();
        for t in &project.cd_tracks {
            let (x0, x1) = (view.x_of(l, t.range.start), view.x_of(l, t.range.end()));
            paint_tag(&bp, bar, x0, &CD_TRACK, false);
            paint_tag(&bp, bar, x1 - TAG_W, &CD_TRACK, false);
            if marker_tool::picked(view, MarkerBar::Cd, t.id) {
                for x in [x0, x1 - TAG_W] {
                    marker_tool::outline(
                        &bp,
                        Rect::from_min_max(pos2(x, bar.top() + 1.0), pos2(x + TAG_W, bar.bottom())),
                    );
                }
            }
            let next = project
                .cd_indices
                .iter()
                .find(|i| i.time > t.range.start && i.time < t.range.end())
                .map_or(x1 - TAG_W, |i| view.x_of(l, i.time));
            paint_label(
                &bp,
                bar,
                x0 + TAG_W + 4.0,
                next,
                &cd_track_label(project, t.id),
            );
            for x in [x0, x1] {
                lanes.line_segment(
                    [pos2(x, l.lanes.top()), pos2(x, l.lanes.bottom())],
                    Stroke::new(1.0, CD_TRACK.face.gamma_multiply(0.45)),
                );
            }
        }
        // Indices left outside every track, dimmed (deleted or covered
        // again by a track).
        for i in project
            .cd_indices
            .iter()
            .filter(|i| !rows.iter().any(|r| r.id == i.id))
        {
            let x = view.x_of(l, i.time);
            let dim = TagColors {
                face: CD_INDEX.face.gamma_multiply(0.4),
                light: CD_INDEX.light.gamma_multiply(0.4),
                dark: CD_INDEX.dark.gamma_multiply(0.4),
            };
            paint_tag(&bp, bar, x, &dim, false);
            if marker_tool::picked(view, MarkerBar::Cd, i.id) {
                marker_tool::outline(
                    &bp,
                    Rect::from_min_max(pos2(x, bar.top() + 1.0), pos2(x + TAG_W, bar.bottom())),
                );
            }
        }
        for row in rows.iter().filter(|r| r.index > 1) {
            let x = view.x_of(l, row.start);
            paint_tag(&bp, bar, x, &CD_INDEX, false);
            if marker_tool::picked(view, MarkerBar::Cd, row.id) {
                marker_tool::outline(
                    &bp,
                    Rect::from_min_max(pos2(x, bar.top() + 1.0), pos2(x + TAG_W, bar.bottom())),
                );
            }
            let until = view.x_of(l, row.end) - TAG_W;
            paint_label(
                &bp,
                bar,
                x + TAG_W + 4.0,
                until,
                &trf!("Index {}", row.index),
            );
            lanes.line_segment(
                [pos2(x, l.lanes.top()), pos2(x, l.lanes.bottom())],
                Stroke::new(1.0, CD_INDEX.face.gamma_multiply(0.45)),
            );
        }
    }
    if l.command_bar.height() > 0.0 {
        let bar = l.command_bar;
        painter.rect_filled(bar, 0.0, theme::panel_dark());
        painter.line_segment(
            [bar.left_top(), bar.right_top()],
            Stroke::new(1.0, theme::bg()),
        );
        let bp = painter.with_clip_rect(bar);
        for (k, c) in project.commands.iter().enumerate() {
            let x = view.x_of(l, c.time);
            paint_tag(&bp, bar, x, &COMMAND, true);
            if marker_tool::picked(view, MarkerBar::Commands, c.id) {
                marker_tool::outline(
                    &bp,
                    Rect::from_min_max(
                        pos2(x, bar.top() + 1.0),
                        pos2(x + TAG_W, (bar.top() + 14.0).min(bar.bottom())),
                    ),
                );
            }
            let until = project
                .commands
                .get(k + 1)
                .map_or(bar.right(), |n| view.x_of(l, n.time));
            paint_label(&bp, bar, x + TAG_W + 4.0, until, &command_label(c));
            lanes.add(Shape::dashed_line(
                &[pos2(x, l.lanes.top()), pos2(x, l.lanes.bottom())],
                Stroke::new(1.0, COMMAND.face.gamma_multiply(0.5)),
                4.0,
                4.0,
            ));
        }
    }
}
