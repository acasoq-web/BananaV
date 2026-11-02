//! Track groups in the timeline, as VEGAS's Help and its pictures show
//! them (the model is `bv_timeline::track_groups`): a header row over each
//! group's tracks — the Collapse/Expand Track Group button, its name (a
//! scribble strip: double-click to rename it), Mute Track Group and Solo
//! Track Group — and the grouping lane down the left of its tracks'
//! headers. A click on the header or the lane selects the group's tracks;
//! their menu's Ungroup Selected Tracks clears the group; the header
//! dragged moves the group. Collapsed, its tracks are out of sight and dark
//! gray bars on the header's lane show where their events are. Track
//! Group › Group Selected Tracks and Ungroup Selected Tracks are in a
//! track header's menu; Shift+click and Ctrl+click on track headers select
//! several tracks.

use bv_timeline::{Project, TrackGroupId, TrackId};

use super::headers::{MUTE_ON, TrackDrag};
use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Key, Rect, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;

/// The header row's height, and the grouping lane's width (VEGAS's 22 and
/// 24 pixels; the lane narrower in BananaV's narrower headers).
pub(crate) const GROUP_H: f32 = 22.0;
pub(crate) const LANE_W: f32 = 16.0;

/// A group's place in the timeline: its header row's top, and the bottom
/// of its last track shown (the header's when collapsed).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GroupRow {
    pub group: TrackGroupId,
    pub top: f32,
    pub bottom: f32,
    pub collapsed: bool,
}

impl GroupRow {
    /// Its header row, `x0` to `x1`.
    pub fn header(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(pos2(x0, self.top), pos2(x1, self.top + GROUP_H))
    }

    /// Its tracks' rows (empty when collapsed), `x0` to `x1`.
    pub fn tracks(&self, x0: f32, x1: f32) -> Rect {
        Rect::from_min_max(pos2(x0, self.top + GROUP_H), pos2(x1, self.bottom))
    }
}

impl BananaApp {
    /// The tracks selected: those picked with Shift and Ctrl while they
    /// hold the selected track, else the selected track; top first.
    pub(crate) fn track_selection(&self) -> Vec<TrackId> {
        let p = self.project();
        let Some(sel) = self.selected_track.filter(|t| p.track(*t).is_some()) else {
            return Vec::new();
        };
        let mut v: Vec<TrackId> = if self.track_picks.contains(&sel) {
            self.track_picks
                .iter()
                .copied()
                .filter(|t| p.track(*t).is_some())
                .collect()
        } else {
            vec![sel]
        };
        v.sort_by_key(|t| p.track_index(*t));
        v.dedup();
        v
    }

    /// The tracks a command on track `id` acts on (its menu's Delete
    /// Track, Delete in the track list): those selected when it is one of
    /// them, as VEGAS's, else it alone.
    pub(crate) fn tracks_with(&self, id: TrackId) -> Vec<TrackId> {
        let selection = self.track_selection();
        if selection.contains(&id) {
            selection
        } else {
            vec![id]
        }
    }

    /// A click on track `id`'s header: with Shift the tracks from the
    /// selected one to it, with Ctrl it added or taken away, else it alone.
    pub(crate) fn pick_track(&mut self, id: TrackId, shift: bool, ctrl: bool) {
        let p = self.project();
        if shift
            && let Some(from) = self.selected_track.and_then(|t| p.track_index(t))
            && let Some(to) = p.track_index(id)
        {
            let (a, b) = (from.min(to), from.max(to));
            self.track_picks = p.tracks[a..=b].iter().map(|t| t.id).collect();
            // The anchor stays: the range is from it.
            return;
        }
        if ctrl {
            let mut picks = self.track_selection();
            match picks.iter().position(|t| *t == id) {
                Some(i) if picks.len() > 1 => {
                    picks.remove(i);
                    self.selected_track = picks.first().copied();
                }
                Some(_) => {}
                None => {
                    picks.push(id);
                    self.selected_track = Some(id);
                }
            }
            self.track_picks = picks;
            return;
        }
        self.selected_track = Some(id);
        self.track_picks = vec![id];
    }

    /// Group `g`'s tracks selected (a click on its header or lane).
    pub(crate) fn select_track_group(&mut self, g: TrackGroupId) {
        let members = self.project().track_group_members(g);
        self.selected_track = members.first().copied();
        self.track_picks = members;
    }

    /// Track Group › Group Selected Tracks: two or more adjacent tracks.
    pub(crate) fn group_selected_tracks(&mut self) {
        let ids = self.track_selection();
        let p = self.project();
        let rows: Vec<usize> = ids.iter().filter_map(|t| p.track_index(*t)).collect();
        if rows.len() < 2 || rows.windows(2).any(|w| w[1] != w[0] + 1) {
            self.report_error(tr(
                "Select two or more adjacent tracks to group them (Shift+click their headers).",
            ));
            return;
        }
        if let Some(g) = self.edit(|p| p.group_tracks(&ids)) {
            self.select_track_group(g);
        }
    }

    /// Track Group › Ungroup Selected Tracks.
    pub(crate) fn ungroup_selected_tracks(&mut self) {
        let ids = self.track_selection();
        self.modify(|p| p.ungroup_tracks(&ids));
    }

    /// Whether any selected track is in a group.
    pub(crate) fn selection_grouped(&self) -> bool {
        let p = self.project();
        self.track_selection()
            .iter()
            .any(|t| p.track(*t).is_some_and(|t| t.group.is_some()))
    }

    /// Whether the selection can be grouped: two or more adjacent tracks.
    pub(crate) fn selection_groupable(&self) -> bool {
        let p = self.project();
        let rows: Vec<usize> = self
            .track_selection()
            .iter()
            .filter_map(|t| p.track_index(*t))
            .collect();
        rows.len() >= 2 && rows.windows(2).all(|w| w[1] == w[0] + 1)
    }
}

/// The Track Group submenu of a track header's menu.
pub(super) fn track_group_items(ui: &mut egui::Ui, app: &mut BananaApp) {
    if ui
        .add_enabled(
            app.selection_groupable(),
            egui::Button::new(tr("Group Selected Tracks")),
        )
        .clicked()
    {
        app.group_selected_tracks();
        ui.close();
    }
    if ui
        .add_enabled(
            app.selection_grouped(),
            egui::Button::new(tr("Ungroup Selected Tracks")),
        )
        .clicked()
    {
        app.ungroup_selected_tracks();
        ui.close();
    }
}

/// The groups' header rows (in the track list and over the lanes) and
/// their grouping lanes.
pub(super) fn ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    for g in &l.groups {
        let Some(group) = project.track_group(g.group) else {
            continue;
        };
        let head = g.header(l.headers.left(), l.headers.right());
        if head.top() > l.headers.bottom() || g.bottom < l.headers.top() {
            continue;
        }
        lane_bars(view, ui, l, project, g);
        // The grouping lane, down the left of its tracks' headers.
        let strip = g.tracks(l.headers.left(), l.headers.left() + LANE_W);
        let hp = ui.painter().with_clip_rect(l.headers);
        if !g.collapsed && strip.height() > 0.0 {
            hp.rect_filled(strip, 0.0, theme::panel());
            hp.line_segment(
                [strip.right_top(), strip.right_bottom()],
                Stroke::new(1.0, theme::panel_dark()),
            );
            let resp = ui.interact(
                strip.intersect(l.headers),
                egui::Id::new(("track-group-lane", g.group.0)),
                Sense::click(),
            );
            if resp.clicked() {
                app.select_track_group(g.group);
            }
            let gid = g.group;
            resp.context_menu(|ui| group_menu(ui, app, gid));
        }
        header_ui(view, ui, app, l, group, g, head);
    }
}

/// A group's header row in the track list.
fn header_ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    group: &bv_timeline::TrackGroup,
    g: &GroupRow,
    head: Rect,
) {
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(l.headers));
    cui.set_clip_rect(l.headers);
    let ui = &mut cui;
    let gid = g.group;
    let id = |s: &str| egui::Id::new(("track-group", gid.0, s));
    let body = ui.interact(head, id("body"), Sense::click_and_drag());
    let painter = ui.painter().clone();
    let picked = app.track_selection();
    let members = app.project().track_group_members(gid);
    let all_picked = !members.is_empty() && members.iter().all(|m| picked.contains(m));
    painter.rect_filled(
        head,
        0.0,
        if all_picked {
            theme::header_selected()
        } else {
            theme::panel()
        },
    );
    painter.line_segment(
        [head.left_top(), head.right_top()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    painter.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    // Collapse/Expand: a triangle, down while open.
    let tri = Rect::from_center_size(pos2(head.left() + 10.0, head.center().y), vec2(14.0, 14.0));
    let tresp = ui
        .interact(tri, id("toggle"), Sense::click())
        .on_hover_text(if g.collapsed {
            tr("Expand Track Group")
        } else {
            tr("Collapse Track Group")
        });
    let c = tri.center();
    let color = if tresp.hovered() {
        theme::text_strong()
    } else {
        theme::text()
    };
    let pts = if g.collapsed {
        vec![
            c + vec2(-2.5, -4.0),
            c + vec2(3.0, 0.0),
            c + vec2(-2.5, 4.0),
        ]
    } else {
        vec![
            c + vec2(-4.0, -2.5),
            c + vec2(4.0, -2.5),
            c + vec2(0.0, 3.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
    if tresp.clicked() {
        let collapsed = !g.collapsed;
        // A view of the project: no undo of its own.
        app.modify_untracked(|p| {
            if let Some(x) = p.track_group_mut(gid) {
                x.collapsed = collapsed;
            }
        });
    }
    // Mute and Solo Track Group, at the right.
    let solo_rect = Rect::from_min_size(
        pos2(head.right() - 22.0, head.top() + 3.0),
        vec2(17.0, 16.0),
    );
    let mute_rect = solo_rect.translate(vec2(-20.0, 0.0));
    if group_button(
        ui,
        mute_rect,
        id("mute"),
        group.muted,
        false,
        tr("Mute Track Group: overrides its soloed tracks"),
    ) {
        let v = !group.muted;
        app.modify(|p| {
            if let Some(x) = p.track_group_mut(gid) {
                x.muted = v;
            }
        });
    }
    if group_button(
        ui,
        solo_rect,
        id("solo"),
        group.solo,
        true,
        tr("Solo Track Group: overrides its muted tracks"),
    ) {
        let v = !group.solo;
        app.modify(|p| {
            if let Some(x) = p.track_group_mut(gid) {
                x.solo = v;
            }
        });
    }
    // Its name: a scribble strip.
    let name_rect = Rect::from_min_max(
        pos2(tri.right() + 4.0, head.top() + 2.0),
        pos2(mute_rect.left() - 4.0, head.bottom() - 2.0),
    );
    if view.renaming_group.as_ref().is_some_and(|(g, _)| *g == gid) {
        rename_field(view, ui, app, name_rect, gid, id("name"));
    } else {
        let (text, color) = if group.name.is_empty() {
            (tr("Track Group").to_string(), theme::text_dim())
        } else {
            (group.name.clone(), theme::text())
        };
        crate::widgets::fit_text(
            &painter,
            pos2(name_rect.left(), name_rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(12.0),
            color,
            name_rect.width(),
        );
    }
    if body.double_clicked()
        && body
            .interact_pointer_pos()
            .is_some_and(|p| name_rect.contains(p))
    {
        view.renaming_group = Some((gid, group.name.clone()));
    } else if body.clicked() {
        app.select_track_group(gid);
    }
    if body.drag_started() {
        view.track_drag = Some(TrackDrag::Group(gid));
    }
    body.context_menu(|ui| group_menu(ui, app, gid));
}

/// A group's header or lane menu: Ungroup Selected Tracks (the whole
/// group, as VEGAS's).
fn group_menu(ui: &mut egui::Ui, app: &mut BananaApp, g: TrackGroupId) {
    if ui.button(tr("Ungroup Selected Tracks")).clicked() {
        app.modify(|p| p.clear_track_group(g));
        ui.close();
    }
}

/// The group's header row over the lanes: collapsed, dark gray bars where
/// its tracks' events are.
fn lane_bars(view: &TimelineView, ui: &egui::Ui, l: &Layout, project: &Project, g: &GroupRow) {
    let lane = g.header(l.lanes.left(), l.lanes.right());
    if !lane.intersects(l.lanes) {
        return;
    }
    let p = ui.painter().with_clip_rect(l.lanes);
    p.rect_filled(lane, 0.0, theme::panel());
    p.line_segment(
        [lane.left_bottom(), lane.right_bottom()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    if !g.collapsed {
        return;
    }
    let bar_color = Color32::from_gray(78);
    for t in project.tracks.iter().filter(|t| t.group == Some(g.group)) {
        for c in &t.clips {
            let (x0, x1) = (view.x_of(l, c.start), view.x_of(l, c.end()));
            if x1 < l.lanes.left() || x0 > l.lanes.right() {
                continue;
            }
            let bar = Rect::from_min_max(
                pos2(x0, lane.top() + 5.0),
                pos2(x1.max(x0 + 1.0), lane.bottom() - 5.0),
            );
            p.rect_filled(bar, 1.0, bar_color);
        }
    }
}

/// A group's name typed in its scribble strip.
fn rename_field(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    rect: Rect,
    g: TrackGroupId,
    id: egui::Id,
) {
    let Some((_, text)) = view.renaming_group.as_mut() else {
        return;
    };
    let resp = ui.put(
        rect,
        egui::TextEdit::singleline(text)
            .id(id)
            .font(FontId::proportional(12.0))
            .margin(vec2(3.0, 1.0)),
    );
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    let cancel = ui.input(|i| i.key_pressed(Key::Escape));
    if resp.lost_focus() || cancel {
        let name = view
            .renaming_group
            .take()
            .map(|(_, n)| n.trim().to_string())
            .unwrap_or_default();
        if cancel || app.project().track_group(g).is_some_and(|x| x.name == name) {
            return;
        }
        app.modify(|p| {
            if let Some(x) = p.track_group_mut(g) {
                x.name = name;
            }
        });
    }
}

impl BananaApp {
    /// The demo with two track groups: its video tracks collapsed ("Picture",
    /// the bars of their events on its header's lane), its audio tracks
    /// open and selected ("Dialogue", soloed), a third audio track outside
    /// them (screen tests).
    pub fn track_groups_for_review(&mut self) -> bool {
        use bv_timeline::TrackKind;
        let made = self.modify(|p| {
            let v1 = p.tracks.iter().find(|t| t.kind == TrackKind::Video)?.id;
            let a1 = p.tracks.iter().find(|t| t.kind == TrackKind::Audio)?.id;
            let v2 = p.insert_track_at(TrackKind::Video, 1);
            let a2 = p.add_track(TrackKind::Audio);
            p.add_track(TrackKind::Audio);
            let pictures = p.group_tracks(&[v1, v2]).ok()?;
            let dialogue = p.group_tracks(&[a1, a2]).ok()?;
            let g = p.track_group_mut(pictures)?;
            g.name = "Picture".into();
            g.collapsed = true;
            let g = p.track_group_mut(dialogue)?;
            g.name = "Dialogue".into();
            g.solo = true;
            Some(dialogue)
        });
        let Some(dialogue) = made else {
            return false;
        };
        self.select_track_group(dialogue);
        self.show_tab(crate::dock::Tab::Timeline, false);
        true
    }
}

/// Mute or Solo Track Group's button, as VEGAS's Help draws them: the
/// group's tracks as bars, with a "no" sign (mute) or an exclamation mark
/// (solo) over their corner; lit while on.
fn group_button(ui: &egui::Ui, rect: Rect, id: egui::Id, on: bool, solo: bool, tip: &str) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    let mark = if solo { theme::banana() } else { MUTE_ON };
    if on {
        painter.rect_filled(rect, 2.0, mark.gamma_multiply(0.35));
    } else if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let ink = if on || resp.hovered() {
        theme::text_strong()
    } else {
        theme::text()
    };
    // Four bars, the lower ones shorter where the mark sits.
    let x0 = rect.left() + 3.0;
    for i in 0..4 {
        let y = rect.top() + 3.5 + i as f32 * 3.0;
        let x1 = if i < 2 {
            rect.right() - 3.0
        } else {
            rect.right() - 7.5
        };
        painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.6, ink));
    }
    let c = pos2(rect.right() - 4.0, rect.bottom() - 4.5);
    if solo {
        painter.line_segment(
            [pos2(c.x, c.y - 5.0), pos2(c.x, c.y + 1.0)],
            Stroke::new(2.0, mark),
        );
        painter.circle_filled(pos2(c.x, c.y + 3.2), 1.1, mark);
    } else {
        painter.circle_stroke(c, 3.3, Stroke::new(1.4, mark));
        painter.line_segment(
            [c + egui::vec2(-2.3, -2.3), c + egui::vec2(2.3, 2.3)],
            Stroke::new(1.4, mark),
        );
    }
    resp.on_hover_text(tip).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::Rect;
    use bv_timeline::TrackKind;

    /// A group's header row over its tracks, the tracks after it; collapsed,
    /// its tracks out of the rows; a track dropped among a group's rows
    /// joins it, below them not.
    #[test]
    fn groups_lay_out_their_header_and_hide_when_collapsed() {
        let mut p = Project::new(bv_timeline::ProjectSettings::default());
        let a1 = p.tracks[1].id;
        let a2 = p.add_track(TrackKind::Audio);
        let a3 = p.add_track(TrackKind::Audio);
        let g = p.group_tracks(&[a1, a2]).unwrap();
        let view = TimelineView::default();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 900.0));
        let l = view.layout(rect, &p);
        assert_eq!(l.rows.len(), 4);
        assert_eq!(l.groups.len(), 1);
        let head = l.groups[0];
        let first = l.row_of(a1).unwrap();
        assert!((first.top - (head.top + GROUP_H)).abs() < 1e-3);
        assert_eq!(first.group, Some(g));
        assert!((head.bottom - l.row_of(a2).map(|r| r.top + r.height).unwrap()).abs() < 1e-3);
        assert_eq!(l.row_of(a3).unwrap().group, None);
        p.track_group_mut(g).unwrap().collapsed = true;
        let l = view.layout(rect, &p);
        assert_eq!(l.rows.len(), 2);
        assert!(l.row_of(a1).is_none());
        assert!((l.groups[0].bottom - (l.groups[0].top + GROUP_H)).abs() < 1e-3);
        assert!((l.row_of(a3).unwrap().top - l.groups[0].bottom).abs() < 1e-3);
    }

    /// Shift+click picks a run of tracks, Ctrl+click adds or takes one;
    /// Group Selected Tracks groups the run (not tracks apart), Ungroup
    /// Selected Tracks takes them out.
    #[test]
    fn picked_tracks_group_and_ungroup() {
        let mut app = BananaApp::new(egui::Context::default());
        let (a1, a2, a3) = app.modify(|p| {
            (
                p.add_track(TrackKind::Audio),
                p.add_track(TrackKind::Audio),
                p.add_track(TrackKind::Audio),
            )
        });
        app.pick_track(a1, false, false);
        app.pick_track(a3, true, false);
        assert_eq!(app.track_selection(), vec![a1, a2, a3]);
        app.pick_track(a2, false, true);
        assert_eq!(app.track_selection(), vec![a1, a3]);
        assert!(!app.selection_groupable());
        app.group_selected_tracks();
        assert!(app.project().track_groups.is_empty());
        app.pick_track(a1, false, false);
        app.pick_track(a2, true, false);
        assert!(app.selection_groupable());
        app.group_selected_tracks();
        let g = app.project().track_groups[0].id;
        assert_eq!(app.project().track_group_members(g), vec![a1, a2]);
        // A click on the group's header picks its tracks.
        app.pick_track(a3, false, false);
        app.select_track_group(g);
        assert_eq!(app.track_selection(), vec![a1, a2]);
        assert!(app.selection_grouped());
        app.ungroup_selected_tracks();
        assert!(app.project().track_groups.is_empty());
    }
}
