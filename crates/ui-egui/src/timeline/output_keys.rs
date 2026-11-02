//! The video bus track's keyframes rows (VEGAS's Help, "Video bus track":
//! keyframes on it animate the video output effects "just like working
//! with any other video track"): under the track, a row for each Video
//! Output FX plug-in that is animated, its keyframes at their times. A
//! keyframe clicked brings the cursor to it; dragged, the effect's
//! keyframes there move (a frame from the ones before and after, their
//! values kept); double-clicked, the Video Output FX window opens at it;
//! its menu deletes them. View › Video Envelopes › Track Keyframes hides
//! the rows.

use bv_core::Tick;
use bv_timeline::{KeyCurve, Project};

use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Sense, Stroke, pos2, vec2};
use crate::fx_window::FxTarget;
use crate::i18n::tr;
use crate::theme;

/// A row's height, as a keyframes row's.
pub(crate) const ROW_H: f32 = super::surround_keys::KEYS_H;

/// The output effects with keyframes: their places in the chain and the
/// times of their keyframes (with each time's curve).
fn animated(project: &Project) -> Vec<(usize, Vec<(Tick, KeyCurve)>)> {
    project
        .video_output_fx
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let fx = e.plugin.as_deref()?;
            let mut times: Vec<(Tick, KeyCurve)> = fx
                .params
                .values()
                .flat_map(|p| p.keys.iter().map(|k| (k.at, k.curve)))
                .collect();
            times.sort_by_key(|t| t.0);
            times.dedup_by_key(|t| t.0);
            (!times.is_empty()).then_some((i, times))
        })
        .collect()
}

/// How tall the video bus track's keyframes rows are.
pub(crate) fn height(view: &TimelineView, project: &Project) -> f32 {
    if !view.show_track_keys {
        return 0.0;
    }
    animated(project).len() as f32 * ROW_H
}

/// The keyframes of output effect `i` at `from` to `to`, kept a frame
/// from its keyframes before and after; where they are now.
pub(crate) fn move_keys(p: &mut Project, i: usize, from: Tick, to: Tick) -> Tick {
    let frame = p.settings.frame_rate.frame_duration();
    let times: Vec<Tick> = animated(p)
        .into_iter()
        .find(|(e, _)| *e == i)
        .map_or_else(Vec::new, |(_, t)| t.into_iter().map(|(t, _)| t).collect());
    let Some(k) = times.iter().position(|&x| x == from) else {
        return from;
    };
    let lo = k.checked_sub(1).map_or(Tick::ZERO, |j| times[j] + frame);
    let to = match times.get(k + 1).map(|&n| n - frame) {
        Some(hi) if hi < lo => return from,
        Some(hi) => to.clamp(lo, hi),
        None => to.max(lo),
    };
    if let Some(fx) = p
        .video_output_fx
        .get_mut(i)
        .and_then(|e| e.plugin.as_deref_mut())
    {
        for param in fx.params.values_mut() {
            for key in param.keys.iter_mut().filter(|key| key.at == from) {
                key.at = to;
            }
        }
    }
    to
}

/// Output effect `i`'s keyframes at `at` deleted.
pub(crate) fn delete_keys(p: &mut Project, i: usize, at: Tick) {
    if let Some(fx) = p
        .video_output_fx
        .get_mut(i)
        .and_then(|e| e.plugin.as_deref_mut())
    {
        for param in fx.params.values_mut() {
            param.keys.retain(|k| k.at != at);
        }
    }
}

/// The rows, under the video bus track: each effect's name in the
/// header, its keyframes in the lane.
pub(super) fn ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    let Some(row) = l
        .bus_rows
        .iter()
        .find(|r| r.track == super::bus_tracks::BusTrack::Video && r.keys > 0.0)
    else {
        return;
    };
    let lanes_all = row.keys_rect(l.bus_lanes.left(), l.bus_lanes.right());
    let heads_all = row.keys_rect(l.bus_headers.left(), l.bus_headers.right());
    let rate = project.settings.frame_rate;
    for (n, (i, times)) in animated(project).into_iter().enumerate() {
        let y0 = lanes_all.top() + n as f32 * ROW_H;
        let lane = egui::Rect::from_min_max(
            pos2(lanes_all.left(), y0),
            pos2(lanes_all.right(), y0 + ROW_H),
        );
        let head = egui::Rect::from_min_max(
            pos2(heads_all.left(), y0),
            pos2(heads_all.right(), y0 + ROW_H),
        );
        let visible = lane.intersect(l.bus_lanes);
        if visible.height() <= 0.0 {
            continue;
        }
        // The header's part: the effect's name.
        let hp = ui.painter().with_clip_rect(l.bus_headers);
        hp.rect_filled(head, 0.0, theme::panel_dark());
        hp.line_segment(
            [head.left_top(), head.right_top()],
            Stroke::new(1.0, theme::separator()),
        );
        let name = project
            .video_output_fx
            .get(i)
            .map(crate::ofx::fx_name)
            .unwrap_or_default();
        crate::widgets::fit_text(
            &hp,
            pos2(head.left() + 26.0, head.center().y),
            Align2::LEFT_CENTER,
            &name,
            FontId::proportional(10.0),
            theme::text_dim(),
            head.width() - 30.0,
        );
        // The lane's part: the keyframes.
        let p = ui.painter().with_clip_rect(l.bus_lanes);
        p.rect_filled(lane, 0.0, theme::panel_dark());
        p.line_segment(
            [lane.left_top(), lane.right_top()],
            Stroke::new(1.0, theme::separator()),
        );
        let id = egui::Id::new(("output-keys", i));
        let resp = ui.interact(visible, id, Sense::click_and_drag());
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let key_at = |x: f32| {
            times
                .iter()
                .map(|t| t.0)
                .filter(|&t| (view.x_of(l, t) - x).abs() <= 5.0)
                .min_by(|a, b| {
                    (view.x_of(l, *a) - x)
                        .abs()
                        .total_cmp(&(view.x_of(l, *b) - x).abs())
                })
        };
        let hot = pointer.and_then(|q| key_at(q.x));
        let dragged = view.output_key_drag.filter(|(e, _)| *e == i).map(|d| d.1);
        let y = lane.center().y;
        for &(t, curve) in &times {
            let x = view.x_of(l, t);
            if x < l.bus_lanes.left() - 8.0 || x > l.bus_lanes.right() + 8.0 {
                continue;
            }
            paint_key(&p, pos2(x, y), curve, dragged == Some(t), hot == Some(t));
        }
        // A keyframe clicked: the cursor to it.
        if resp.clicked()
            && let Some(t) = hot
        {
            app.seek(t);
        }
        // Double-clicked: the Video Output FX window there.
        if resp.double_clicked()
            && let Some(t) = hot
        {
            app.seek(t);
            app.clip_windows.fx_target = Some(FxTarget::VideoOutput);
        }
        // Dragged: the effect's keyframes there move.
        if resp.drag_started()
            && let Some(t) = hot
        {
            view.output_key_drag = Some((i, t));
        }
        if resp.dragged()
            && let (Some((e, from)), Some(q)) = (view.output_key_drag, pointer)
            && e == i
        {
            let to = rate.snap_nearest(view.time_at(l, q.x).max(Tick::ZERO));
            if to != from {
                let now = app.modify_merged(id.with("drag"), |p| move_keys(p, i, from, to));
                view.output_key_drag = Some((i, now));
            }
        }
        if resp.drag_stopped() {
            view.output_key_drag = None;
        }
        // Its menu: Delete.
        if resp.secondary_clicked() {
            view.output_key_menu = hot.map(|t| (i, t));
        }
        let menu = view.output_key_menu.filter(|(e, _)| *e == i);
        resp.context_menu(|ui| {
            let Some((_, t)) = menu else {
                ui.label(egui::RichText::new(name.as_str()).color(theme::text_dim()));
                return;
            };
            if ui.button(tr("Delete")).clicked() {
                app.modify(|p| delete_keys(p, i, t));
                ui.close();
            }
        });
    }
}

/// A keyframe: a diamond in its curve's color (outlined while dragged).
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

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Effect, PluginFx, PluginKey, PluginParam, PluginValue};

    /// An output effect animated at 1 s and 2 s: its keyframes move
    /// together, never past a neighbour, and go together.
    #[test]
    fn output_effects_keyframes_move_and_go_together() {
        let mut p = Project::default();
        let s = Tick::from_seconds;
        let key =
            |at: Tick, v: f64| PluginKey::new(at, PluginValue::Num(vec![v]), KeyCurve::Linear);
        let mut fx = PluginFx {
            id: "org.example.Glow".into(),
            version: [1, 0],
            name: "Glow".into(),
            params: Default::default(),
        };
        for name in ["amount", "radius"] {
            fx.params.insert(
                name.into(),
                PluginParam {
                    value: PluginValue::Num(vec![0.0]),
                    keys: vec![key(s(1), 0.0), key(s(2), 1.0)],
                },
            );
        }
        p.video_output_fx.push(Effect::plugin(fx));
        assert_eq!(animated(&p).len(), 1);
        let to = move_keys(&mut p, 0, s(1), Tick::from_seconds_f64(1.5));
        assert_eq!(to, Tick::from_seconds_f64(1.5));
        let at = |p: &Project| -> Vec<Tick> {
            p.video_output_fx[0].plugin.as_deref().unwrap().params["radius"]
                .keys
                .iter()
                .map(|k| k.at)
                .collect()
        };
        assert_eq!(at(&p), vec![to, s(2)]);
        // Never onto the next one.
        let frame = p.settings.frame_rate.frame_duration();
        assert_eq!(move_keys(&mut p, 0, to, s(5)), s(2) - frame);
        delete_keys(&mut p, 0, s(2));
        assert_eq!(at(&p).len(), 1);
    }
}
