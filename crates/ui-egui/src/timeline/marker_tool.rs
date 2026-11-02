//! The Marker Tool (the flag button at the timeline's top right), as
//! VEGAS's Help describes it: it picks markers on the marker bars — the
//! marker and region bar, the CD layout bar or the command bar, whichever
//! was clicked last (the active bar). ← and → go to the previous or next
//! marker of the active bar, Shift with them extends the pick; Shift+click
//! picks the tags between two, Ctrl+click one more or one less; dragging
//! one of the picked tags moves them all, Delete removes them. A press
//! anywhere else in the window turns it off, as VEGAS's does when the
//! focus goes elsewhere.

use std::collections::BTreeSet;

use crate::i18n::tr;
use bv_core::Tick;
use bv_timeline::{MarkerId, Project};

use super::bars::BarDrag;
use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Color32, Rect, Sense, Stroke, StrokeKind, pos2};
use crate::theme;

/// The marker bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MarkerBar {
    #[default]
    Markers,
    Cd,
    Commands,
}

/// The tool's state (on the timeline's view).
#[derive(Clone, Debug, Default)]
pub(crate) struct MarkerTool {
    pub active: bool,
    /// The active bar: the one clicked last.
    pub bar: MarkerBar,
    pub picks: BTreeSet<MarkerId>,
    /// The tag picked last: where Shift ranges and the arrows go from.
    pub last: Option<MarkerId>,
    /// While the picked tags are dragged: where the drag began and each
    /// one's time then.
    pub dragging: Option<(Tick, Vec<(MarkerId, Tick)>)>,
}

/// A bar's tags in time order: their ids and times (a region's and a CD
/// track's by their start: the tool moves them whole).
pub(crate) fn items(p: &Project, bar: MarkerBar) -> Vec<(MarkerId, Tick)> {
    let mut v: Vec<(MarkerId, Tick)> = match bar {
        MarkerBar::Markers => p
            .markers
            .iter()
            .map(|m| (m.id, m.time))
            .chain(p.regions.iter().map(|r| (r.id, r.range.start)))
            .collect(),
        MarkerBar::Cd => p
            .cd_tracks
            .iter()
            .map(|t| (t.id, t.range.start))
            .chain(p.cd_indices.iter().map(|i| (i.id, i.time)))
            .collect(),
        MarkerBar::Commands => p.commands.iter().map(|c| (c.id, c.time)).collect(),
    };
    v.sort_by_key(|&(id, t)| (t, id));
    v
}

impl MarkerTool {
    /// A click on tag `id` of `bar`: picked alone, with Shift the tags
    /// from the last one picked to it, with Ctrl added or taken away.
    pub(crate) fn click(
        &mut self,
        p: &Project,
        bar: MarkerBar,
        id: MarkerId,
        shift: bool,
        ctrl: bool,
    ) {
        if bar != self.bar {
            self.picks.clear();
            self.last = None;
            self.bar = bar;
        }
        let list = items(p, bar);
        let at = |x: MarkerId| list.iter().position(|&(i, _)| i == x);
        match (shift, self.last.and_then(at), at(id)) {
            (true, Some(a), Some(b)) => {
                self.picks = list[a.min(b)..=a.max(b)].iter().map(|&(i, _)| i).collect();
            }
            _ if ctrl => {
                if !self.picks.remove(&id) {
                    self.picks.insert(id);
                }
            }
            _ => self.picks = BTreeSet::from([id]),
        }
        self.last = Some(id);
    }

    /// ← (−1) or → (1): the previous or next tag of the active bar from
    /// the last one picked (or from the cursor), picked alone or, with
    /// `extend`, added to the pick. Where it is.
    pub(crate) fn step(
        &mut self,
        p: &Project,
        cursor: Tick,
        dir: i8,
        extend: bool,
    ) -> Option<Tick> {
        let list = items(p, self.bar);
        let from = self
            .last
            .and_then(|l| list.iter().position(|&(i, _)| i == l));
        let next = match from {
            Some(k) if dir > 0 => list.get(k + 1),
            Some(k) => k.checked_sub(1).and_then(|k| list.get(k)),
            None if dir > 0 => list.iter().find(|&&(_, t)| t > cursor),
            None => list.iter().rev().find(|&&(_, t)| t < cursor),
        }?;
        let (id, t) = *next;
        if !extend {
            self.picks.clear();
        }
        self.picks.insert(id);
        self.last = Some(id);
        Some(t)
    }

    /// The picked tags that still exist, with their times then.
    pub(crate) fn picked_times(&self, p: &Project) -> Vec<(MarkerId, Tick)> {
        items(p, self.bar)
            .into_iter()
            .filter(|(id, _)| self.picks.contains(id))
            .collect()
    }
}

/// Moves tag `id` to `t`: a marker or command, a region or CD track whole
/// (its length kept; a track's indices with it), an index within its
/// track.
pub(crate) fn move_item(p: &mut Project, id: MarkerId, t: Tick) {
    let t = t.max(Tick::ZERO);
    if p.markers.iter().any(|m| m.id == id) {
        p.move_marker(id, t);
    } else if let Some(r) = p.regions.iter_mut().find(|r| r.id == id) {
        r.range.start = t;
        p.regions.sort_by_key(|r| (r.range.start, r.id));
    } else if p.command(id).is_some() {
        p.set_command(id, |c| c.time = t);
    } else if p.cd_track(id).is_some() {
        let _ = p.move_cd_track(id, t);
    } else {
        let _ = p.move_cd_index(id, t);
    }
}

/// The picked tags moved together: each from its time at the press by
/// `delta` (the last ones first going right, so tracks never meet).
pub(crate) fn move_picked(p: &mut Project, originals: &[(MarkerId, Tick)], delta: Tick) {
    let mut order: Vec<&(MarkerId, Tick)> = originals.iter().collect();
    order.sort_by_key(|&&(_, t)| t);
    if delta > Tick::ZERO {
        order.reverse();
    }
    for &&(id, t) in &order {
        move_item(p, id, t + delta);
    }
}

/// A drag of the picked tags of `bar` from tag `id` (picked alone first
/// when it was not), the pointer at `at`.
pub(crate) fn start_drag(
    view: &mut TimelineView,
    p: &Project,
    bar: MarkerBar,
    id: MarkerId,
    at: Tick,
) {
    let tool = &mut view.marker_tool;
    if tool.bar != bar || !tool.picks.contains(&id) {
        tool.click(p, bar, id, false, false);
    }
    tool.dragging = Some((at, tool.picked_times(p)));
    view.bar_drag = Some(BarDrag::Picked);
}

/// The picked tags follow the pointer (at `now`), the first one snapping.
pub(crate) fn drag(
    view: &mut TimelineView,
    app: &mut BananaApp,
    project: &Project,
    now: Tick,
    merge: egui::Id,
) {
    let Some((from, originals)) = view.marker_tool.dragging.clone() else {
        return;
    };
    let Some(&(_, first)) = originals.first() else {
        return;
    };
    let raw = (first + now - from).max(Tick::ZERO);
    let delta = view.snap(app, project, raw, &[Tick::ZERO], &[]) - first;
    app.modify_merged(merge, |p| move_picked(p, &originals, delta));
}

pub(crate) fn end_drag(view: &mut TimelineView) {
    if matches!(view.bar_drag, Some(BarDrag::Picked)) {
        view.bar_drag = None;
        view.snap_line = None;
        view.marker_tool.dragging = None;
    }
}

/// Whether tag `id` of `bar` is picked (drawn outlined).
pub(crate) fn picked(view: &TimelineView, bar: MarkerBar, id: MarkerId) -> bool {
    let t = &view.marker_tool;
    t.active && t.bar == bar && t.picks.contains(&id)
}

/// A picked tag's outline.
pub(crate) fn outline(painter: &egui::Painter, r: Rect) {
    painter.rect_stroke(
        r.expand(1.0),
        1.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Outside,
    );
}

/// The tool's button, at the timeline's top right, as VEGAS's: a click
/// turns it on or off.
pub(crate) fn button(view: &mut TimelineView, ui: &mut egui::Ui, l: &Layout) {
    let r = l.marker_tool;
    let resp = ui
        .interact(r, ui.id().with("marker-tool"), Sense::click())
        .on_hover_text(tr("Marker Tool"));
    if resp.clicked() {
        let tool = &mut view.marker_tool;
        tool.active = !tool.active;
        if !tool.active {
            tool.picks.clear();
            tool.last = None;
        }
    }
    let painter = ui.painter();
    let back = if view.marker_tool.active {
        theme::selection()
    } else if resp.hovered() {
        theme::hover()
    } else {
        theme::panel()
    };
    painter.rect_filled(r, 0.0, back);
    // A flag on its pole.
    let c = r.center();
    let ink = if view.marker_tool.active {
        Color32::WHITE
    } else {
        theme::text()
    };
    painter.line_segment(
        [pos2(c.x - 3.0, c.y - 5.0), pos2(c.x - 3.0, c.y + 5.0)],
        Stroke::new(1.0, ink),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(c.x - 3.0, c.y - 5.0),
            pos2(c.x + 4.0, c.y - 2.5),
            pos2(c.x - 3.0, c.y),
        ],
        ink,
        Stroke::NONE,
    ));
}

impl BananaApp {
    /// The Marker Tool on, two markers and a region picked on the marker
    /// bar, command markers beside (screen tests).
    pub fn marker_tool_for_review(&mut self) {
        let s = Tick::from_seconds;
        let (a, r) = self.modify(|p| {
            let a = p.add_marker(s(1));
            p.add_marker(s(3));
            let r = p.add_region(bv_core::TimeRange::new(s(5), s(3)));
            p.rename_marker(r, "Chorus".into());
            p.add_marker(s(10));
            p.add_command(s(2), "Text", "Hello", "");
            p.add_command(s(7), "URL", "https://www.example.com", "");
            (a, r)
        });
        let tool = &mut self.timeline.marker_tool;
        tool.active = true;
        tool.bar = MarkerBar::Markers;
        tool.picks = BTreeSet::from([a, r]);
        tool.last = Some(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: i64) -> Tick {
        Tick::from_seconds(n)
    }

    #[test]
    fn clicks_and_arrows_pick_as_the_help_says() {
        let mut p = Project::default();
        let a = p.add_marker(s(1));
        let b = p.add_marker(s(3));
        let r = p.add_region(bv_core::TimeRange::new(s(5), s(2)));
        let c = p.add_marker(s(9));
        let mut tool = MarkerTool::default();
        tool.click(&p, MarkerBar::Markers, a, false, false);
        tool.click(&p, MarkerBar::Markers, r, true, false);
        assert_eq!(
            tool.picks,
            BTreeSet::from([a, b, r]),
            "Shift: the tags between"
        );
        tool.click(&p, MarkerBar::Markers, b, false, true);
        assert_eq!(tool.picks, BTreeSet::from([a, r]), "Ctrl: one less");
        // The arrows go on from the tag clicked last.
        assert_eq!(tool.step(&p, Tick::ZERO, 1, true), Some(s(5)));
        assert_eq!(tool.step(&p, Tick::ZERO, 1, true), Some(s(9)));
        assert_eq!(tool.picks, BTreeSet::from([a, r, c]), "Shift: extended");
        assert_eq!(tool.step(&p, Tick::ZERO, -1, false), Some(s(5)));
        assert_eq!(tool.picks, BTreeSet::from([r]));
        // Another bar starts its own pick.
        let cmd = p.add_command(s(2), "URL", "", "");
        tool.click(&p, MarkerBar::Commands, cmd, false, false);
        assert_eq!(tool.picks, BTreeSet::from([cmd]));
    }

    #[test]
    fn dragged_tags_move_together() {
        let mut p = Project::default();
        let a = p.add_marker(s(1));
        let r = p.add_region(bv_core::TimeRange::new(s(5), s(2)));
        let t = p.add_cd_track(bv_core::TimeRange::new(s(2), s(4))).unwrap();
        let i = p.add_cd_index(s(3)).unwrap();
        let mut tool = MarkerTool::default();
        tool.click(&p, MarkerBar::Markers, a, false, false);
        tool.click(&p, MarkerBar::Markers, r, false, true);
        let originals = tool.picked_times(&p);
        move_picked(&mut p, &originals, s(2));
        assert_eq!(p.markers[0].time, s(3));
        assert_eq!(
            p.regions[0].range,
            bv_core::TimeRange::new(s(7), s(2)),
            "whole"
        );
        // A CD track carries its indices.
        tool.click(&p, MarkerBar::Cd, t, false, false);
        let originals = tool.picked_times(&p);
        move_picked(&mut p, &originals, s(10));
        assert_eq!(p.cd_track(t).unwrap().range.start, s(12));
        assert_eq!(p.cd_indices.iter().find(|x| x.id == i).unwrap().time, s(13));
    }

    #[test]
    fn the_button_turns_it_on_and_delete_removes_the_picked_marker() {
        use crate::egui::{self, Event, Key, Modifiers, PointerButton, vec2};
        use egui_kittest::Harness;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let events = app.project().clips().count();
        let m = app.modify(|p| p.add_marker(Tick::from_seconds(6)));
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
        let click = |h: &mut Harness<'_, BananaApp>, at: egui::Pos2| {
            h.event(Event::PointerMoved(at));
            for pressed in [true, false] {
                h.event(Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::default(),
                });
                h.step();
            }
            h.run_steps(2);
        };
        let button = h.state().bar_spot_for_review("tool", 0.0);
        click(&mut h, button);
        assert!(
            h.state().timeline.marker_tool.active,
            "the button turned it on"
        );
        let at = h.state().bar_spot_for_review("markers", 6.0);
        click(&mut h, at);
        assert!(h.state().timeline.marker_tool.picks.contains(&m), "picked");
        for pressed in [true, false] {
            h.event(Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::default(),
            });
            h.step();
        }
        h.run_steps(2);
        let p = h.state().project();
        assert!(
            p.markers.iter().all(|x| x.id != m),
            "Delete removed the marker"
        );
        assert_eq!(p.clips().count(), events, "and no event");
    }
}
