//! The Video Preview's Transform mode — BananaV's own, as other editors
//! have it. A button by Preview Quality turns it on; the event shown at the
//! cursor (the selected one there, else the top one) gets a box over its
//! picture: a drag inside moves it, a corner or a side resizes it (about
//! the opposite one; Alt: about its middle), just outside a corner turns
//! it. It writes the event's Pan/Crop — the keyframe at the cursor when it
//! has keyframes, else what it holds all along — through the framing the
//! Properties panel shows (`pancrop::transform_from_key`), so the Pan/Crop
//! window shows the same. Turning it on opens the Transform window beside
//! the Explorer and the other browsers (`tabs::transform`): every control
//! of the event, as complete as the Pan/Crop window's.

use bv_core::Tick;
use bv_timeline::pancrop::{self, PlaceIn};
use bv_timeline::{ClipId, TrackKind, Transform};

use crate::app::BananaApp;
use crate::egui::{self, Color32, CursorIcon, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::theme;

/// What a drag on the picture does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Grip {
    Move,
    /// A corner (0 top left, clockwise), or the middle of a side (0 top,
    /// clockwise): resizes about the opposite one.
    Corner(usize),
    Side(usize),
    Turn,
}

/// A drag under way.
#[derive(Clone, Debug)]
pub(crate) struct Drag {
    grip: Grip,
    clip: ClipId,
    /// Where it began on the frame (pixels) and the framing then.
    from: [f64; 2],
    start: Transform,
    /// The picture's middle and the point that stays (frame pixels).
    center: [f64; 2],
    anchor: [f64; 2],
}

/// The event the mode works on: the selected video event at the cursor
/// (the top one), else the top video event there.
pub(crate) fn target(app: &BananaApp) -> Option<ClipId> {
    let p = app.project();
    let t = app.display_time();
    let mut top = None;
    for track in p
        .tracks
        .iter()
        .filter(|tr| tr.kind == TrackKind::Video && !tr.muted)
    {
        for c in track
            .clips
            .iter()
            .filter(|c| c.start <= t && t < c.start + c.duration)
        {
            if app.selection.contains(&c.id) {
                return Some(c.id);
            }
            top.get_or_insert(c.id);
        }
    }
    top
}

/// `clip`'s framing at the cursor, its placement and the time a change
/// goes to: the cursor when it has keyframes, its start (what it holds all
/// along) when it has none.
pub(crate) fn framing(app: &BananaApp, clip: ClipId) -> Option<(Transform, PlaceIn, Tick)> {
    let p = app.project();
    let c = p.clip(clip)?;
    let place = p.clip_place_in(clip)?;
    let t = app
        .display_time()
        .clamp(c.start, c.start + c.duration - Tick(1).min(c.duration));
    let key = p.clip_box_at(clip, t)?;
    let at = if c.pan_crop.keys.len() > 1 {
        t
    } else {
        c.start
    };
    Some((pancrop::transform_from_key(&key, &place), place, at))
}

/// Writes `tf` as `clip`'s Pan/Crop at `at` (one undo step per `id`).
pub(crate) fn write(
    app: &mut BananaApp,
    id: egui::Id,
    clip: ClipId,
    at: Tick,
    tf: &Transform,
    place: &PlaceIn,
) {
    let key = pancrop::key_from_transform(tf, place);
    let new = app.settings.prefs.new_motion_keys();
    app.modify_merged(id, |p| {
        let _ = p.set_clip_box_at(clip, at, key, new);
    });
}

/// The picture's corners on the frame (pixels), clockwise from its top
/// left: the part of the media its crop leaves.
fn corners(tf: &Transform, place: &PlaceIn) -> Option<[[f64; 2]; 4]> {
    let key = pancrop::key_from_transform(tf, place);
    let pl = pancrop::place(&key, place);
    let [ux, uy, u0, vx, vy, v0] = pl.to_media;
    let det = ux * vy - uy * vx;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let to_frame = |m: [f64; 2]| {
        let (dx, dy) = (m[0] - u0, m[1] - v0);
        [(vy * dx - uy * dy) / det, (-vx * dx + ux * dy) / det]
    };
    let [mw, mh] = pl.media;
    let c = key.crop;
    let (l, r) = (f64::from(c[0]) * mw, (1.0 - f64::from(c[1])) * mw);
    let (t, b) = (f64::from(c[2]) * mh, (1.0 - f64::from(c[3])) * mh);
    Some([
        to_frame([l, t]),
        to_frame([r, t]),
        to_frame([r, b]),
        to_frame([l, b]),
    ])
}

fn mid(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

/// The framing of the whole media, as a new event's.
pub(crate) fn whole_framing(place: &PlaceIn) -> Transform {
    pancrop::transform_from_key(&bv_timeline::pancrop::PanCropKey::whole(place.media), place)
}

/// The picture's middle on the frame (pixels).
pub(crate) fn center_of(tf: &Transform, place: &PlaceIn) -> [f64; 2] {
    let [ow, oh] = place.frame;
    [
        ow / 2.0 + f64::from(tf.x) * ow,
        oh / 2.0 - f64::from(tf.y) * oh,
    ]
}

/// `tf` with its middle at `c` (frame pixels).
pub(crate) fn centered_at(mut tf: Transform, c: [f64; 2], place: &PlaceIn) -> Transform {
    let [ow, oh] = place.frame;
    tf.x = ((c[0] - ow / 2.0) / ow) as f32;
    tf.y = (-(c[1] - oh / 2.0) / oh) as f32;
    tf
}

/// `tf` scaled `k` times about `anchor` (frame pixels).
pub(crate) fn scaled_about(tf: Transform, k: f64, anchor: [f64; 2], place: &PlaceIn) -> Transform {
    let k = k.clamp(0.02, 50.0);
    let c = center_of(&tf, place);
    let moved = [
        anchor[0] + (c[0] - anchor[0]) * k,
        anchor[1] + (c[1] - anchor[1]) * k,
    ];
    let mut out = centered_at(tf, moved, place);
    out.scale = (f64::from(tf.scale) * k).clamp(0.001, 1000.0) as f32;
    out
}

/// `tf` turned `degrees` more, clockwise on the screen.
pub(crate) fn turned(mut tf: Transform, degrees: f64) -> Transform {
    let r = f64::from(tf.rotation_deg) + degrees;
    tf.rotation_deg = (r.rem_euclid(360.0)) as f32;
    if tf.rotation_deg > 180.0 {
        tf.rotation_deg -= 360.0;
    }
    tf
}

/// What a press at `p` (frame pixels) takes of the picture whose corners
/// are `q`, `reach` pixels around the handles.
fn grip_at(q: &[[f64; 2]; 4], p: [f64; 2], reach: f64) -> Option<Grip> {
    let near = |a: [f64; 2]| (a[0] - p[0]).hypot(a[1] - p[1]) <= reach;
    if let Some(i) = (0..4).find(|&i| near(q[i])) {
        return Some(Grip::Corner(i));
    }
    if let Some(i) = (0..4).find(|&i| near(mid(q[i], q[(i + 1) % 4]))) {
        return Some(Grip::Side(i));
    }
    if inside(q, p) {
        return Some(Grip::Move);
    }
    // Just outside a corner: turning.
    (0..4)
        .any(|i| (q[i][0] - p[0]).hypot(q[i][1] - p[1]) <= reach * 4.0)
        .then_some(Grip::Turn)
}

/// Whether `p` is within the quadrilateral `q` (either winding).
fn inside(q: &[[f64; 2]; 4], p: [f64; 2]) -> bool {
    let side =
        |a: [f64; 2], b: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let s: Vec<f64> = (0..4).map(|i| side(q[i], q[(i + 1) % 4])).collect();
    s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0)
}

/// The framing a drag gives with the pointer at `p` (frame pixels).
fn dragged(d: &Drag, p: [f64; 2], about_middle: bool, place: &PlaceIn) -> Transform {
    match d.grip {
        Grip::Move => {
            let delta = sub(p, d.from);
            let c = center_of(&d.start, place);
            centered_at(d.start, [c[0] + delta[0], c[1] + delta[1]], place)
        }
        Grip::Corner(_) | Grip::Side(_) => {
            let anchor = if about_middle { d.center } else { d.anchor };
            let was = sub(d.from, anchor);
            let now = sub(p, anchor);
            let len = was[0].hypot(was[1]).max(1e-6);
            // Along the way from the anchor to where the drag began.
            let k = (now[0] * was[0] + now[1] * was[1]) / (len * len);
            scaled_about(d.start, k.max(0.02), anchor, place)
        }
        Grip::Turn => {
            let a0 = (d.from[1] - d.center[1]).atan2(d.from[0] - d.center[0]);
            let a1 = (p[1] - d.center[1]).atan2(p[0] - d.center[0]);
            turned(d.start, (a1 - a0).to_degrees())
        }
    }
}

/// The pointer over a grip, as other editors show it.
fn pointer_for(grip: Grip) -> CursorIcon {
    match grip {
        Grip::Move => CursorIcon::Move,
        Grip::Corner(0 | 2) => CursorIcon::ResizeNwSe,
        Grip::Corner(_) => CursorIcon::ResizeNeSw,
        Grip::Side(0 | 2) => CursorIcon::ResizeVertical,
        Grip::Side(_) => CursorIcon::ResizeHorizontal,
        Grip::Turn => CursorIcon::Alias,
    }
}

/// The button by Preview Quality.
pub(crate) fn toolbar_button(ui: &mut egui::Ui, app: &mut BananaApp) {
    let on = app.preview.transform;
    if crate::widgets::icon_toggle(
        ui,
        crate::icons::Icon::PanCrop,
        on,
        tr("Transform: move, resize and turn the event on the preview"),
    )
    .clicked()
    {
        app.preview.transform = !on;
        app.preview.transform_drag = None;
        if !on {
            app.show_tab(crate::dock::Tab::Transform, false);
        }
    }
}

/// The box over the picture and its drags, in `frame_rect` (the frame on
/// the screen), within `area` (the preview's room).
pub(crate) fn overlay(ui: &mut egui::Ui, app: &mut BananaApp, frame_rect: Rect, area: Rect) {
    let Some(clip) = target(app) else {
        app.preview.transform_drag = None;
        return;
    };
    let Some((tf, place, at)) = framing(app, clip) else {
        return;
    };
    let [ow, oh] = place.frame;
    if ow <= 0.0 || oh <= 0.0 || frame_rect.width() < 2.0 {
        return;
    }
    let k = [
        f64::from(frame_rect.width()) / ow,
        f64::from(frame_rect.height()) / oh,
    ];
    let to_screen = |f: [f64; 2]| {
        pos2(
            frame_rect.left() + (f[0] * k[0]) as f32,
            frame_rect.top() + (f[1] * k[1]) as f32,
        )
    };
    let to_frame = |s: Pos2| {
        [
            f64::from(s.x - frame_rect.left()) / k[0],
            f64::from(s.y - frame_rect.top()) / k[1],
        ]
    };
    let id = ui.id().with("bv-transform");
    let resp = ui.interact(area, id, Sense::drag());
    let reach = 7.0 / k[0].min(k[1]).max(1e-6);
    let playing = app.is_playing();
    let about_middle = ui.input(|i| i.modifiers.alt);
    // A drag: the framing follows the pointer.
    if !playing
        && resp.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && let Some(q) = corners(&tf, &place)
        && let Some(grip) = grip_at(&q, to_frame(o), reach)
    {
        let anchor = match grip {
            Grip::Corner(i) => q[(i + 2) % 4],
            Grip::Side(i) => mid(q[(i + 2) % 4], q[(i + 3) % 4]),
            _ => center_of(&tf, &place),
        };
        app.preview.transform_drag = Some(Drag {
            grip,
            clip,
            from: to_frame(o),
            start: tf,
            center: center_of(&tf, &place),
            anchor,
        });
    }
    if let Some(d) = app.preview.transform_drag.clone() {
        if resp.dragged()
            && d.clip == clip
            && let Some(p) = ui.input(|i| i.pointer.latest_pos())
        {
            let now = dragged(&d, to_frame(p), about_middle, &place);
            write(app, id, clip, at, &now, &place);
            crate::cursors::system(ui.ctx(), pointer_for(d.grip));
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            app.preview.transform_drag = None;
        }
    } else if !playing
        && let Some(p) = resp.hover_pos()
        && let Some(q) = corners(&tf, &place)
        && let Some(grip) = grip_at(&q, to_frame(p), reach)
    {
        crate::cursors::system(ui.ctx(), pointer_for(grip));
    }
    // The box: its outline, its eight handles and its middle.
    let (tf, _, _) = framing(app, clip).unwrap_or((tf, place, at));
    let Some(q) = corners(&tf, &place) else {
        return;
    };
    let painter = ui.painter().with_clip_rect(area);
    let pts: Vec<Pos2> = q.iter().map(|&c| to_screen(c)).collect();
    let mut ring = pts.clone();
    ring.push(pts[0]);
    painter.add(Shape::line(
        ring.clone(),
        Stroke::new(3.0, Color32::from_black_alpha(140)),
    ));
    painter.add(Shape::line(ring, Stroke::new(1.0, theme::selection())));
    let handle = |at: Pos2| {
        let r = Rect::from_center_size(at, vec2(7.0, 7.0));
        painter.rect_filled(r, 0.0, Color32::WHITE);
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, theme::selection()),
            egui::StrokeKind::Inside,
        );
    };
    for i in 0..4 {
        handle(pts[i]);
        handle(to_screen(mid(q[i], q[(i + 1) % 4])));
    }
    let c = to_screen(center_of(&tf, &place));
    let cross = Stroke::new(1.0, Color32::from_white_alpha(200));
    painter.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], cross);
    painter.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], cross);
}

impl BananaApp {
    /// Transform mode on, its event at `scale` (×) and moved right a tenth
    /// of the frame (screen tests).
    pub fn transform_for_review(&mut self, scale: f64) {
        self.preview.transform = true;
        if let Some(clip) = target(self)
            && let Some((tf, place, at)) = framing(self, clip)
        {
            let c = center_of(&tf, &place);
            let tf = scaled_about(tf, scale, c, &place);
            let tf = centered_at(tf, [c[0] + place.frame[0] / 10.0, c[1]], &place);
            write(
                self,
                egui::Id::new("transform-review"),
                clip,
                at,
                &tf,
                &place,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place() -> PlaceIn {
        PlaceIn {
            media: [1920.0, 1080.0],
            media_aspect: 1.0,
            frame: [1920.0, 1080.0],
            frame_aspect: 1.0,
            keep_aspect: true,
            fill: true,
            adjust: 1.0,
        }
    }

    fn whole() -> Transform {
        pancrop::transform_from_key(&pancrop::PanCropKey::whole([1920.0, 1080.0]), &place())
    }

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
    }

    #[test]
    fn the_whole_media_fills_the_frame() {
        let q = corners(&whole(), &place()).unwrap();
        assert!(near(q[0], [0.0, 0.0]), "{q:?}");
        assert!(near(q[2], [1920.0, 1080.0]), "{q:?}");
        assert!(near(center_of(&whole(), &place()), [960.0, 540.0]));
    }

    #[test]
    fn drags_move_resize_and_turn_the_picture() {
        let p = place();
        let start = whole();
        let q = corners(&start, &p).unwrap();
        let drag = |grip, from: [f64; 2], anchor| Drag {
            grip,
            clip: ClipId(1),
            from,
            start,
            center: center_of(&start, &p),
            anchor,
        };
        // Moved 100 right, 50 down.
        let moved = dragged(
            &drag(Grip::Move, [500.0, 500.0], [0.0; 2]),
            [600.0, 550.0],
            false,
            &p,
        );
        let m = corners(&moved, &p).unwrap();
        assert!(near(m[0], [100.0, 50.0]), "{m:?}");
        // The bottom right corner to the middle: half the size, the top
        // left corner staying.
        let half = dragged(
            &drag(Grip::Corner(2), q[2], q[0]),
            [960.0, 540.0],
            false,
            &p,
        );
        let h = corners(&half, &p).unwrap();
        assert!(
            near(h[0], [0.0, 0.0]) && near(h[2], [960.0, 540.0]),
            "{h:?}"
        );
        assert!((half.scale - 0.5).abs() < 1e-4);
        // Alt: about the middle.
        let mid = dragged(
            &drag(Grip::Corner(2), q[2], q[0]),
            [1440.0, 810.0],
            true,
            &p,
        );
        assert!(near(center_of(&mid, &p), [960.0, 540.0]));
        // A quarter turn clockwise: the top left corner goes to the top
        // right of the middle (on the screen, y down).
        let turned_q = dragged(
            &drag(Grip::Turn, [1960.0, 540.0], [0.0; 2]),
            [960.0, 1540.0],
            false,
            &p,
        );
        let t = corners(&turned_q, &p).unwrap();
        assert!(t[0][0] > 960.0 && t[0][1] < 540.0, "{t:?}");
    }

    #[test]
    fn presses_take_corners_sides_the_inside_and_turns() {
        let q = corners(&whole(), &place()).unwrap();
        assert_eq!(grip_at(&q, [2.0, 3.0], 7.0), Some(Grip::Corner(0)));
        assert_eq!(grip_at(&q, [960.0, 1078.0], 7.0), Some(Grip::Side(2)));
        assert_eq!(grip_at(&q, [500.0, 500.0], 7.0), Some(Grip::Move));
        assert_eq!(grip_at(&q, [1940.0, -15.0], 7.0), Some(Grip::Turn));
        assert_eq!(grip_at(&q, [3000.0, 3000.0], 7.0), None);
    }
}
