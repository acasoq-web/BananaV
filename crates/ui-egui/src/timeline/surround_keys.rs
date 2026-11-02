//! The Surround Pan Keyframes rows (Insert › Audio Envelopes › Surround
//! Pan Keyframes; a bus track's menu), as VEGAS's Help describes them:
//! under an audio track of a 5.1 project — or a bus track — a keyframe
//! for each place its Surround Panner was set at, colored by its curve. A
//! double-click on the row adds one (the panning there, the curve of the
//! one before); a keyframe clicked brings the cursor to it, dragged moves
//! it (Ctrl: a copy), double-clicked opens the Surround Panner there; its
//! menu sets its curve (Linear, Fast, Slow, Smooth, Sharp, Hold) or deletes
//! it. View › Show Envelopes › Audio Envelopes › Surround Pan Keyframes
//! hides the rows.

use bv_core::Tick;
use bv_timeline::{KeyCurve, Project, SurroundOwner, TrackKind};

use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;

/// The row's height (VEGAS's 13 pixels and its line).
pub(crate) const KEYS_H: f32 = 14.0;

/// A keyframe being dragged: whose, which, and whether it is still to be
/// copied (Ctrl).
#[derive(Clone, Copy, Debug)]
pub(crate) struct KeyDrag {
    owner: SurroundOwner,
    index: usize,
    copy: bool,
}

/// How tall `owner`'s keyframes row is: shown while it pans in the room,
/// its row is inserted and View › Show Envelopes shows them.
pub(crate) fn height(view: &TimelineView, project: &Project, owner: SurroundOwner) -> f32 {
    let shown = view.show_surround_keys
        && project.surround_pans(owner)
        && project.surround(owner).is_some_and(|s| s.keyframes);
    if shown { KEYS_H } else { 0.0 }
}

/// The tracks' and bus tracks' keyframes rows: drawn over their bottoms,
/// the label in their headers.
pub(super) fn ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    for row in &l.rows {
        if row.keys <= 0.0 || project.track(row.track).map(|t| t.kind) != Some(TrackKind::Audio) {
            continue;
        }
        let owner = SurroundOwner::Track(row.track);
        let lane = row.keys_rect(l.lanes.left(), l.lanes.right());
        let head = row.keys_rect(l.headers.left(), l.headers.right());
        row_ui(view, ui, app, l, owner, lane, head, l.lanes, l.headers);
    }
    for row in &l.bus_rows {
        let super::bus_tracks::BusTrack::Strip(s) = row.track else {
            continue;
        };
        if row.keys <= 0.0 {
            continue;
        }
        let owner = SurroundOwner::Strip(s);
        let lane = row.keys_rect(l.bus_lanes.left(), l.bus_lanes.right());
        let head = row.keys_rect(l.bus_headers.left(), l.bus_headers.right());
        row_ui(
            view,
            ui,
            app,
            l,
            owner,
            lane,
            head,
            l.bus_lanes,
            l.bus_headers,
        );
    }
}

/// One keyframes row: `lane` within `lanes`, its header part `head`
/// within `headers`.
#[allow(clippy::too_many_arguments)]
fn row_ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    owner: SurroundOwner,
    lane: Rect,
    head: Rect,
    lanes: Rect,
    headers: Rect,
) {
    let visible = lane.intersect(lanes);
    if visible.height() <= 0.0 {
        return;
    }
    let Some(sp) = app.project().surround(owner).cloned() else {
        return;
    };
    // The header's part: its name.
    let hp = ui.painter().with_clip_rect(headers);
    hp.rect_filled(head, 0.0, theme::panel_dark());
    hp.line_segment(
        [head.left_top(), head.right_top()],
        Stroke::new(1.0, theme::separator()),
    );
    crate::widgets::fit_text(
        &hp,
        pos2(head.left() + 26.0, head.center().y),
        Align2::LEFT_CENTER,
        tr("Surround Pan Keyframes"),
        FontId::proportional(10.0),
        theme::text_dim(),
        head.width() - 30.0,
    );
    // The lane's part: the keyframes.
    let p = ui.painter().with_clip_rect(lanes);
    p.rect_filled(lane, 0.0, theme::panel_dark());
    p.line_segment(
        [lane.left_top(), lane.right_top()],
        Stroke::new(1.0, theme::separator()),
    );
    let y = lane.center().y;
    let picked = view
        .surround_pick
        .filter(|(o, _)| *o == owner)
        .map(|(_, i)| i);
    let id = egui::Id::new(("surround-keys", owner));
    let resp = ui.interact(visible, id, Sense::click_and_drag());
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let key_at = |x: f32| {
        sp.keys
            .iter()
            .enumerate()
            .filter(|(_, k)| (view.x_of(l, k.at) - x).abs() <= 5.0)
            .min_by(|a, b| {
                let da = (view.x_of(l, a.1.at) - x).abs();
                let db = (view.x_of(l, b.1.at) - x).abs();
                da.total_cmp(&db)
            })
            .map(|(i, _)| i)
    };
    let hot = pointer.and_then(|p| key_at(p.x));
    for (i, k) in sp.keys.iter().enumerate() {
        let x = view.x_of(l, k.at);
        if x < lanes.left() - 8.0 || x > lanes.right() + 8.0 {
            continue;
        }
        paint_key(&p, pos2(x, y), k.curve, picked == Some(i), hot == Some(i));
    }
    let rate = app.project().settings.frame_rate;
    let time = |v: &TimelineView, x: f32| rate.snap_nearest(v.time_at(l, x).max(Tick::ZERO));
    // A keyframe clicked: picked, the cursor to it.
    if resp.clicked()
        && let Some(i) = hot
    {
        view.surround_pick = Some((owner, i));
        app.seek(sp.keys[i].at);
    }
    // A double-click: on a keyframe, the Surround Panner at its time;
    // elsewhere, a keyframe added there.
    if resp.double_clicked()
        && let Some(pos) = pointer
    {
        match hot {
            Some(i) => {
                app.seek(sp.keys[i].at);
                app.open_surround_panner(owner);
            }
            None => {
                let at = time(view, pos.x);
                let i = app.modify(|p| p.surround_mut(owner).map(|s| s.add_key(at)));
                view.surround_pick = i.map(|i| (owner, i));
            }
        }
    }
    // Dragged: moved (Ctrl: a copy moved).
    if resp.drag_started()
        && let Some(i) = hot
    {
        let copy = ui.input(|i| i.modifiers.ctrl);
        view.surround_drag = Some(KeyDrag {
            owner,
            index: i,
            copy,
        });
        view.surround_pick = Some((owner, i));
    }
    if resp.dragged()
        && let (Some(mut d), Some(pos)) = (view.surround_drag, pointer)
        && d.owner == owner
    {
        let at = time(view, pos.x);
        let key = id.with("drag");
        if d.copy {
            if let Some(j) = app.modify_merged(key, |p| {
                p.surround_mut(owner).and_then(|s| s.copy_key(d.index, at))
            }) {
                d.index = j;
                d.copy = false;
            }
        } else {
            d.index = app.modify_merged(key, |p| {
                p.surround_mut(owner)
                    .map_or(d.index, |s| s.move_key(d.index, at))
            });
        }
        view.surround_drag = Some(d);
        view.surround_pick = Some((owner, d.index));
    }
    if resp.drag_stopped() {
        view.surround_drag = None;
    }
    // Its menu: a keyframe's curve, or Delete.
    if resp.secondary_clicked() {
        view.surround_menu = hot.map(|i| (owner, i));
    }
    let menu = view.surround_menu.filter(|(o, _)| *o == owner);
    resp.context_menu(|ui| {
        let Some((_, i)) = menu else {
            ui.label(egui::RichText::new(tr("Surround Pan Keyframes")).color(theme::text_dim()));
            return;
        };
        let Some(k) = sp.keys.get(i).copied() else {
            ui.close();
            return;
        };
        for c in KeyCurve::ALL {
            if crate::widgets::radio(ui, k.curve == c, tr(c.name())).clicked() {
                app.modify(|p| {
                    if let Some(s) = p.surround_mut(owner)
                        && let Some(k) = s.keys.get_mut(i)
                    {
                        k.curve = c;
                    }
                });
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Delete")).clicked() {
            app.modify(|p| {
                if let Some(s) = p.surround_mut(owner)
                    && i < s.keys.len()
                {
                    s.keys.remove(i);
                }
            });
            view.surround_pick = None;
            ui.close();
        }
    });
}

/// A keyframe: a diamond in its curve's color, as VEGAS's keyframes rows
/// draw them (outlined when picked).
fn paint_key(p: &egui::Painter, at: egui::Pos2, curve: KeyCurve, picked: bool, hot: bool) {
    let base = crate::ofx_keys::key_color(curve);
    let fill = if hot {
        crate::ofx_keys::mix(base, Color32::WHITE, 0.5)
    } else {
        base
    };
    let outline = if picked {
        Stroke::new(2.0, theme::selection())
    } else {
        Stroke::new(1.0, Color32::from_gray(66))
    };
    p.add(egui::Shape::convex_polygon(
        vec![
            at + vec2(0.0, -5.5),
            at + vec2(5.5, 0.0),
            at + vec2(0.0, 5.5),
            at + vec2(-5.5, 0.0),
        ],
        fill,
        outline,
    ));
}
