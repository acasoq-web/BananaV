//! The Transform window, docked beside the Explorer and the other browsers:
//! the controls of the event the Video Preview's Transform mode works on
//! (the selected video event at the cursor, else the top one), laid out as
//! other editors' effect controls — sections that fold, each property's
//! name, its numbers to drag or type in the accent's color, a reset button
//! at the end of its row, and the event's keyframes on a strip of its time
//! beside them. As complete as the Pan/Crop window's properties:
//!
//! - Video › Motion: Position, Scale (with Uniform Scale; Scale Width and
//!   Scale Height without), Rotation, Anchor Point (where it turns), Crop,
//!   Flip; the Pan/Crop box (its Width, Height and Center in the media's
//!   pixels); the keyframe's interpolation (its curve and Smoothness); the
//!   source settings and Reduce Interlace Flicker. The stopwatch animates
//!   the box (keyframes) or keeps one framing all along; the arrows go to
//!   the keyframe before or after the cursor, the diamond adds or takes
//!   away the one at it.
//! - Video › Opacity: the event's level and its fades with their curves.
//! - Video › Time: playback and undersample rates, Loop, Hold Last Frame,
//!   Reverse.
//! - Audio, for the event's audio (its group's audio event at the cursor):
//!   Volume (Mute, Level, Invert Phase, Normalize), Channels and Pitch.
//!
//! On the strip a click moves the cursor there, a keyframe drags in time
//! and its menu gives its curve or deletes it.

use std::collections::BTreeSet;

use bv_core::Tick;
use bv_timeline::pancrop::PanCropKey;
use bv_timeline::{ChannelMap, Clip, ClipId, FadeCurve, KeyCurve, TrackKind};

use crate::app::BananaApp;
use crate::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui,
    pos2, vec2,
};
use crate::i18n::tr;
use crate::transform_ui;
use crate::{theme, widgets};

/// A property's name column, how far rows step in, a row's height.
const LABEL: f32 = 116.0;
const INDENT: f32 = 22.0;
const ROW: f32 = 21.0;

/// The window's own state.
#[derive(Debug)]
pub(crate) struct Panel {
    /// The sections folded, by name.
    closed: BTreeSet<&'static str>,
    /// Uniform Scale, for the event it was set on (else on unless its
    /// picture is stretched).
    uniform: Option<(ClipId, bool)>,
    /// The keyframes' strip shows beside the properties.
    pub(crate) strip: bool,
    /// A keyframe dragged on the strip: the event, its time when taken,
    /// and the drag's undo step.
    drag: Option<(ClipId, Tick, u64)>,
    drags: u64,
    /// Where the Motion's keyframes were drawn last (screen tests).
    pub(crate) spots: Vec<(Tick, Pos2)>,
}

impl Default for Panel {
    fn default() -> Panel {
        Panel {
            closed: ["box", "interpolation"].into_iter().collect(),
            uniform: None,
            strip: true,
            drag: None,
            drags: 0,
            spots: Vec::new(),
        }
    }
}

/// What the strip draws beside a row.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lane {
    /// A section's or an effect's header.
    Header,
    /// The Motion's: the Pan/Crop keyframes.
    Motion,
    /// The Opacity's: the fades.
    Fades,
    Plain,
}

/// The rows drawn this frame, top to bottom, for the strip.
type Rows = Vec<(Lane, Rect)>;

/// The window.
pub(crate) fn ui(ui: &mut Ui, app: &mut BananaApp) {
    let Some(clip) = transform_ui::target(app) else {
        ui.add_space(8.0);
        crate::tabs::hint(
            ui,
            tr("Put the cursor over a video event, or select one, to transform it."),
        );
        return;
    };
    header(ui, app, clip);
    ui.add_space(2.0);
    egui::ScrollArea::vertical()
        .id_salt("transform-window")
        .auto_shrink([false, false])
        .show(ui, |ui| body(ui, app, clip));
}

/// The event's name and track, the Transform mode's button, Open Pan/Crop
/// and the strip's switch.
fn header(ui: &mut Ui, app: &mut BananaApp, clip: ClipId) {
    let p = app.project();
    let name = p
        .clip(clip)
        .and_then(|c| p.media(c.media))
        .map(crate::i18n::media_name)
        .unwrap_or_default();
    let track = p
        .tracks
        .iter()
        .position(|t| t.clips.iter().any(|c| c.id == clip))
        .map_or(String::new(), |i| format!("{} ", i + 1));
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{track}{name}")).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let strip = app.transform_panel.strip;
            if widgets::icon_toggle(
                ui,
                crate::icons::Icon::FilmStrip,
                strip,
                tr("Show the keyframes beside the properties"),
            )
            .clicked()
            {
                app.transform_panel.strip = !strip;
            }
            if widgets::icon_button(ui, crate::icons::Icon::PanCrop, tr("Open Pan/Crop…")).clicked()
            {
                app.select_clip(clip, false);
                crate::commands::Cmd::PanCropEvent.run(app);
            }
            transform_ui::toolbar_button(ui, app);
        });
    });
}

fn body(ui: &mut Ui, app: &mut BananaApp, clip: ClipId) {
    let full = ui.available_width();
    let strip_w = if app.transform_panel.strip && full >= 440.0 {
        (full * 0.42).clamp(140.0, 720.0)
    } else {
        0.0
    };
    let left_w = (full - strip_w - 6.0).max(240.0);
    let mut rows: Rows = Vec::new();
    let top = ui.cursor().top();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(left_w);
            ui.spacing_mut().item_spacing.y = 1.0;
            properties(ui, app, clip, &mut rows);
        });
        if strip_w > 0.0 {
            let bottom = rows
                .iter()
                .map(|(_, r)| r.bottom())
                .fold(top + 40.0, f32::max);
            strip(ui, app, clip, strip_w, top, bottom, &rows);
        }
    });
}

fn properties(ui: &mut Ui, app: &mut BananaApp, clip: ClipId, rows: &mut Rows) {
    if section(ui, app, "video", tr("Video"), rows) {
        motion(ui, app, clip, rows);
        opacity(ui, app, clip, rows);
        time(ui, app, clip, rows);
    }
    if let Some(audio) = audio_of(app, clip)
        && section(ui, app, "audio", tr("Audio"), rows)
    {
        volume(ui, app, audio, rows);
        channels(ui, app, audio, rows);
        pitch(ui, app, audio, rows);
    }
}

/// The audio event of `clip`'s group at the cursor.
fn audio_of(app: &BananaApp, clip: ClipId) -> Option<ClipId> {
    let p = app.project();
    let t = app.display_time();
    p.group_members(clip).into_iter().find(|&m| {
        m != clip
            && p.tracks.iter().any(|tr| {
                tr.kind == TrackKind::Audio
                    && tr
                        .clips
                        .iter()
                        .any(|c| c.id == m && c.start <= t && t < c.end())
            })
    })
}

// ---- rows -----------------------------------------------------------------

/// A section's header (Video, Audio): open or folded; whether it is open.
fn section(
    ui: &mut Ui,
    app: &mut BananaApp,
    key: &'static str,
    title: &str,
    rows: &mut Rows,
) -> bool {
    let open = !app.transform_panel.closed.contains(key);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::panel_dark());
    p.text(
        rect.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(12.5),
        theme::text(),
    );
    caret(
        p,
        rect.right_center() - vec2(12.0, 0.0),
        !open,
        theme::text_dim(),
    );
    if resp.clicked() {
        toggle_fold(app, key);
    }
    rows.push((Lane::Header, rect));
    open
}

fn toggle_fold(app: &mut BananaApp, key: &'static str) {
    let closed = &mut app.transform_panel.closed;
    if !closed.remove(key) {
        closed.insert(key);
    }
}

/// A small triangle: down (open) or right (folded) — up for a section.
fn caret(p: &egui::Painter, c: Pos2, folded: bool, color: Color32) {
    let s = 4.0;
    let pts = if folded {
        vec![
            c + vec2(-s * 0.6, -s),
            c + vec2(s * 0.8, 0.0),
            c + vec2(-s * 0.6, s),
        ]
    } else {
        vec![
            c + vec2(-s, -s * 0.6),
            c + vec2(s, -s * 0.6),
            c + vec2(0.0, s * 0.8),
        ]
    };
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// An effect's header (Motion, Opacity, Time, Volume…): its caret, "fx"
/// and name, then what `tools` adds at its right end; whether it is open.
fn effect(
    ui: &mut Ui,
    app: &mut BananaApp,
    key: &'static str,
    title: &str,
    lane: Lane,
    rows: &mut Rows,
    tools: impl FnOnce(&mut Ui, &mut BananaApp),
) -> bool {
    let open = !app.transform_panel.closed.contains(key);
    let r = ui.horizontal(|ui| {
        ui.set_height(ROW);
        let (c, resp) = ui.allocate_exact_size(vec2(16.0, ROW), Sense::click());
        caret(ui.painter(), c.center(), !open, theme::text_dim());
        if resp.clicked() {
            toggle_fold(app, key);
        }
        ui.label(RichText::new("fx").italics().color(theme::text_dim()));
        let name = ui
            .add(egui::Label::new(RichText::new(title).color(theme::text())).sense(Sense::click()));
        if name.clicked() {
            toggle_fold(app, key);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| tools(ui, app));
    });
    rows.push((lane, r.response.rect));
    open
}

/// A property's row: its name, what `values` draws, and a reset button
/// (when `reset` says whether there is anything to reset); whether reset
/// was clicked.
fn prop(
    ui: &mut Ui,
    name: &str,
    lane: Lane,
    rows: &mut Rows,
    reset: Option<bool>,
    values: impl FnOnce(&mut Ui),
) -> bool {
    let mut clicked = false;
    let r = ui.horizontal(|ui| {
        ui.set_height(ROW);
        ui.add_space(INDENT);
        let (rect, resp) = ui.allocate_exact_size(vec2(LABEL, ROW), Sense::hover());
        // One line; a name too long ends in "…", whole under the pointer.
        let mut job = egui::text::LayoutJob::simple_singleline(
            name.to_string(),
            FontId::proportional(12.0),
            theme::text_dim(),
        );
        job.wrap = egui::text::TextWrapping {
            max_width: LABEL - 4.0,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = ui.painter().layout_job(job);
        if galley.elided {
            resp.on_hover_text(name);
        }
        let at = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
        ui.painter()
            .with_clip_rect(rect)
            .galley(at, galley, theme::text_dim());
        values(ui);
        if let Some(enabled) = reset {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                clicked = reset_button(ui, enabled);
            });
        }
    });
    rows.push((lane, r.response.rect));
    clicked
}

/// The reset button: a turning arrow.
fn reset_button(ui: &mut Ui, enabled: bool) -> bool {
    widgets::icon_button_ex(ui, crate::icons::Icon::Undo, tr("Reset"), false, enabled).clicked()
}

/// A number to drag or type, in the accent's color and without a frame;
/// whether it changed.
fn hot<N: egui::emath::Numeric>(
    ui: &mut Ui,
    value: &mut N,
    decimals: usize,
    speed: f64,
    suffix: &str,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    ui.scope(|ui| {
        let accent = theme::selection();
        let v = ui.visuals_mut();
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.bg_fill = Color32::TRANSPARENT;
            w.weak_bg_fill = Color32::TRANSPARENT;
            w.bg_stroke = Stroke::NONE;
            w.fg_stroke.color = accent;
        }
        v.widgets.hovered.fg_stroke.color = accent.linear_multiply(1.25);
        ui.spacing_mut().button_padding.x = 2.0;
        let v = N::to_f64(*value).clamp(*range.start(), *range.end());
        *value = N::from_f64(v);
        ui.add(
            crate::fmt::drag_grouped(value, decimals)
                .speed(speed)
                .suffix(suffix)
                .range(range),
        )
        .changed()
    })
    .inner
}

/// A check box in a row's values.
fn tick(ui: &mut Ui, on: &mut bool) -> bool {
    widgets::checkbox(ui, on, "").changed()
}

/// A choice among `options` in a row's values; whether it changed.
fn choice<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let shown = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or("", |(_, n)| n);
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(110.0)
        .show_ui(ui, |ui| {
            for (v, n) in options {
                if ui.selectable_label(*v == *value, *n).clicked() && *v != *value {
                    *value = *v;
                    changed = true;
                }
            }
        });
    changed
}

fn key_curves() -> [(KeyCurve, &'static str); 6] {
    [
        (KeyCurve::Linear, tr("Linear")),
        (KeyCurve::Fast, tr("Fast")),
        (KeyCurve::Slow, tr("Slow")),
        (KeyCurve::Smooth, tr("Smooth")),
        (KeyCurve::Sharp, tr("Sharp")),
        (KeyCurve::Hold, tr("Hold")),
    ]
}

fn fade_curves() -> [(FadeCurve, &'static str); 5] {
    [
        (FadeCurve::Linear, tr("Linear")),
        (FadeCurve::Fast, tr("Fast")),
        (FadeCurve::Slow, tr("Slow")),
        (FadeCurve::Smooth, tr("Smooth")),
        (FadeCurve::Sharp, tr("Sharp")),
    ]
}

// ---- Motion ---------------------------------------------------------------

/// The keyframes' buttons: the one before, the diamond (adds the one at
/// the cursor, or takes it away), the one after.
fn key_buttons(ui: &mut Ui, app: &mut BananaApp, clip: &Clip, at: Option<Tick>) {
    let offset = (app.display_time() - clip.start).clamp(Tick::ZERO, clip.duration);
    let keys = &clip.pan_crop.keys;
    let animated = keys.len() > 1;
    let next = keys.iter().find(|k| k.at > offset).map(|k| k.at);
    let before = keys.iter().rev().find(|k| k.at < offset).map(|k| k.at);
    let small =
        |ui: &mut Ui, enabled: bool, tip: &str, paint: &dyn Fn(&egui::Painter, Rect, Color32)| {
            let (rect, resp) = ui.allocate_exact_size(vec2(14.0, ROW), Sense::click());
            let resp = resp.on_hover_text(tip);
            let color = if !enabled {
                theme::text_disabled()
            } else if resp.hovered() {
                theme::text_strong()
            } else {
                theme::text_dim()
            };
            paint(ui.painter(), rect, color);
            enabled && resp.clicked()
        };
    // Right to left: after, the diamond, before.
    if small(
        ui,
        animated && next.is_some(),
        tr("Next Keyframe"),
        &|p, r, c| {
            let m = r.center();
            p.add(Shape::convex_polygon(
                vec![
                    m + vec2(-3.0, -4.0),
                    m + vec2(3.0, 0.0),
                    m + vec2(-3.0, 4.0),
                ],
                c,
                Stroke::NONE,
            ));
        },
    ) && let Some(t) = next
    {
        app.seek(clip.start + t);
    }
    let on_key = at.is_some();
    let tip = if on_key {
        tr("Delete Keyframe")
    } else {
        tr("Add Keyframe")
    };
    if small(ui, true, tip, &|p, r, c| {
        diamond(
            p,
            r.center(),
            4.0,
            if on_key { c } else { Color32::TRANSPARENT },
            c,
        );
    }) {
        let id = clip.id;
        match at {
            Some(t) => app.modify(|p| {
                if let Some(c) = p.clip_mut(id) {
                    c.pan_crop.remove_key(t);
                }
            }),
            None => {
                let now = app.display_time();
                let place = app.project().clip_place_in(id);
                let key = app.project().clip_box_at(id, now);
                if let (Some(place), Some(key)) = (place, key) {
                    let new = app.settings.prefs.new_motion_keys();
                    app.modify(|p| {
                        if let Some(c) = p.clip_mut(id) {
                            c.pan_crop.set_at(
                                offset,
                                key,
                                place.media,
                                (new.0, f64::from(new.1 / 100.0)),
                            );
                        }
                    });
                }
            }
        }
    }
    if small(
        ui,
        animated && before.is_some(),
        tr("Previous Keyframe"),
        &|p, r, c| {
            let m = r.center();
            p.add(Shape::convex_polygon(
                vec![m + vec2(3.0, -4.0), m + vec2(3.0, 4.0), m + vec2(-3.0, 0.0)],
                c,
                Stroke::NONE,
            ));
        },
    ) && let Some(t) = before
    {
        app.seek(clip.start + t);
    }
}

/// A keyframe's diamond.
fn diamond(p: &egui::Painter, c: Pos2, r: f32, fill: Color32, line: Color32) {
    p.add(Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
            c + vec2(-r, 0.0),
        ],
        fill,
        Stroke::new(1.0, line),
    ));
}

/// The stopwatch: animates the box (on: keyframes) or holds the framing at
/// the cursor all along.
fn stopwatch(ui: &mut Ui, app: &mut BananaApp, clip: &Clip) {
    let animated = clip.pan_crop.keys.len() > 1;
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, ROW), Sense::click());
    let resp = resp.on_hover_text(tr(
        "Animate the framing with keyframes (on), or keep the framing at the cursor all along (off)",
    ));
    let color = if animated {
        theme::selection()
    } else if resp.hovered() {
        theme::text_strong()
    } else {
        theme::text_dim()
    };
    let c = rect.center() + vec2(0.0, 1.0);
    let p = ui.painter();
    p.circle_stroke(c, 5.5, Stroke::new(1.3, color));
    p.line_segment(
        [c + vec2(0.0, -8.5), c + vec2(0.0, -6.5)],
        Stroke::new(1.5, color),
    );
    p.line_segment([c, c + vec2(2.5, -3.0)], Stroke::new(1.2, color));
    if animated {
        p.circle_filled(c, 2.0, color);
    }
    if !resp.clicked() {
        return;
    }
    let id = clip.id;
    let now = app.display_time();
    let offset = (now - clip.start).clamp(Tick::ZERO, clip.duration);
    let (Some(place), Some(key)) = (
        app.project().clip_place_in(id),
        app.project().clip_box_at(id, now),
    ) else {
        return;
    };
    let new = app.settings.prefs.new_motion_keys();
    app.modify(|p| {
        let Some(c) = p.clip_mut(id) else {
            return;
        };
        if animated {
            // The framing at the cursor, all along.
            c.pan_crop.keys = vec![PanCropKey {
                at: Tick::ZERO,
                ..key
            }];
        } else {
            // A keyframe at the cursor (at the start: at the end too, so
            // that the box can change on its way).
            let smooth = f64::from(new.1 / 100.0);
            c.pan_crop.set_at(
                Tick::ZERO,
                PanCropKey {
                    at: Tick::ZERO,
                    ..key
                },
                place.media,
                (new.0, smooth),
            );
            let at = if offset > Tick::ZERO {
                offset
            } else {
                c.duration - Tick(1).min(c.duration)
            };
            c.pan_crop.set_at(at, key, place.media, (new.0, smooth));
        }
    });
}

fn motion(ui: &mut Ui, app: &mut BananaApp, id: ClipId, rows: &mut Rows) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    let Some((tf0, place, at)) = transform_ui::framing(app, id) else {
        return;
    };
    let offset = (app.display_time() - clip.start).clamp(Tick::ZERO, clip.duration);
    let rate = app.project().settings.frame_rate;
    let frame = rate.frame_duration();
    // The keyframe at the cursor (within half a frame).
    let on_key = clip
        .pan_crop
        .keys
        .iter()
        .filter(|_| clip.pan_crop.keys.len() > 1)
        .find(|k| (k.at - offset).0.abs() * 2 < frame.0.max(1))
        .map(|k| k.at);
    let whole = transform_ui::whole_framing(&place);
    let open = effect(
        ui,
        app,
        "motion",
        tr("Motion"),
        Lane::Motion,
        rows,
        |ui, app| {
            if reset_button(ui, tf0 != whole) {
                transform_ui::write(app, ui.id().with(("reset", id)), id, at, &whole, &place);
            }
            key_buttons(ui, app, &clip, on_key);
            stopwatch(ui, app, &clip);
        },
    );
    if !open {
        return;
    }
    let mut tf = tf0;
    let wid = ui.id().with(("motion", id));
    // Position: the picture's middle on the frame (pixels).
    let mut c = transform_ui::center_of(&tf, &place);
    let [ow, oh] = place.frame;
    let changed = {
        let mut ch = false;
        let reset = prop(
            ui,
            tr("Position"),
            Lane::Motion,
            rows,
            Some(c != [ow / 2.0, oh / 2.0]),
            |ui| {
                ch |= hot(ui, &mut c[0], 1, 1.0, "", -1e6..=1e6);
                ch |= hot(ui, &mut c[1], 1, 1.0, "", -1e6..=1e6);
            },
        );
        if reset {
            c = [ow / 2.0, oh / 2.0];
            ch = true;
        }
        ch
    };
    if changed {
        tf = transform_ui::centered_at(tf, c, &place);
    }
    // Scale: uniform, or its width and height apart.
    let uniform = match app.transform_panel.uniform {
        Some((c, u)) if c == id => u,
        _ => (tf.stretch - 1.0).abs() < 1e-4,
    };
    let mut scale = f64::from(tf.scale) * 100.0;
    let mut height = scale * f64::from(tf.stretch);
    let scale_name = if uniform {
        tr("Scale")
    } else {
        tr("Scale Width")
    };
    let mut ch = false;
    let reset = prop(
        ui,
        scale_name,
        Lane::Motion,
        rows,
        Some((scale - 100.0).abs() > 1e-6),
        |ui| {
            ch = hot(ui, &mut scale, 1, 0.5, " %", 0.1..=100_000.0);
        },
    );
    if reset {
        scale = 100.0;
        ch = true;
    }
    if ch {
        let s = f64::from(tf.scale) * 100.0;
        if s > 0.0 {
            tf = transform_ui::scaled_about(
                tf,
                scale / s,
                transform_ui::center_of(&tf, &place),
                &place,
            );
        }
    }
    if !uniform {
        let mut ch = false;
        let reset = prop(
            ui,
            tr("Scale Height"),
            Lane::Motion,
            rows,
            Some((tf.stretch - 1.0).abs() > 1e-4),
            |ui| {
                ch = hot(ui, &mut height, 1, 0.5, " %", 0.1..=100_000.0);
            },
        );
        if reset {
            tf.stretch = 1.0;
        } else if ch && tf.scale > 0.0 {
            tf.stretch = ((height / 100.0) / f64::from(tf.scale)).clamp(0.001, 1000.0) as f32;
        }
    }
    let mut u = uniform;
    prop(ui, tr("Uniform Scale"), Lane::Motion, rows, None, |ui| {
        if tick(ui, &mut u) {
            app.transform_panel.uniform = Some((id, u));
            if u {
                tf.stretch = 1.0;
            }
        }
    });
    // Rotation, about the anchor point.
    let mut rotation = f64::from(tf.rotation_deg);
    let mut ch = false;
    let reset = prop(
        ui,
        tr("Rotation"),
        Lane::Motion,
        rows,
        Some(rotation.abs() > 1e-6),
        |ui| {
            ch = hot(ui, &mut rotation, 1, 0.5, " °", -100_000.0..=100_000.0);
        },
    );
    if reset {
        rotation = 0.0;
        ch = true;
    }
    if ch {
        tf = transform_ui::turned(tf, rotation - f64::from(tf.rotation_deg));
    }
    // Crop: parts of the media taken off each side.
    let mut crops =
        [tf.crop_left, tf.crop_top, tf.crop_right, tf.crop_bottom].map(|v| f64::from(v) * 100.0);
    let names = [
        tr("Crop Left"),
        tr("Crop Top"),
        tr("Crop Right"),
        tr("Crop Bottom"),
    ];
    let mut crop_changed = false;
    for (v, name) in crops.iter_mut().zip(names) {
        let mut ch = false;
        let reset = prop(ui, name, Lane::Motion, rows, Some(*v > 1e-6), |ui| {
            ch = hot(ui, v, 1, 0.2, " %", 0.0..=99.0);
        });
        if reset {
            *v = 0.0;
            ch = true;
        }
        crop_changed |= ch;
    }
    if crop_changed {
        let [l, t, r, b] = crops.map(|v| (v / 100.0) as f32);
        // Opposite sides leave at least a hundredth of the picture.
        (tf.crop_left, tf.crop_right) = (l.min(0.99 - r), r.min(0.99 - l));
        (tf.crop_top, tf.crop_bottom) = (t.min(0.99 - b), b.min(0.99 - t));
    }
    prop(ui, tr("Flip Horizontal"), Lane::Motion, rows, None, |ui| {
        tick(ui, &mut tf.flip_h);
    });
    prop(ui, tr("Flip Vertical"), Lane::Motion, rows, None, |ui| {
        tick(ui, &mut tf.flip_v);
    });
    if tf != tf0 {
        transform_ui::write(app, wid, id, at, &tf, &place);
    }
    pan_crop_box(ui, app, &clip, at, rows);
    interpolation(ui, app, &clip, offset, rows);
    source(ui, app, &clip, rows);
}

/// The Pan/Crop window's own numbers: the box's Width, Height and Center
/// (the media's pixels), and where it turns (the Anchor Point).
fn pan_crop_box(ui: &mut Ui, app: &mut BananaApp, clip: &Clip, at: Tick, rows: &mut Rows) {
    let id = clip.id;
    let Some(key0) = app.project().clip_box_at(id, app.display_time()) else {
        return;
    };
    let Some(place) = app.project().clip_place_in(id) else {
        return;
    };
    // The Anchor Point: shown with the Motion, open or not.
    let mut key = key0;
    let mut pivot = key.pivot;
    let mid = key.center;
    let mut ch = false;
    let reset = prop(
        ui,
        tr("Anchor Point"),
        Lane::Motion,
        rows,
        Some(pivot != mid),
        |ui| {
            ch |= hot(ui, &mut pivot[0], 1, 1.0, "", -1e6..=1e6);
            ch |= hot(ui, &mut pivot[1], 1, 1.0, "", -1e6..=1e6);
        },
    );
    if reset {
        pivot = mid;
        ch = true;
    }
    if ch {
        key.pivot = pivot;
    }
    let open = !app.transform_panel.closed.contains("box");
    let r = ui.horizontal(|ui| {
        ui.set_height(ROW);
        ui.add_space(INDENT - 16.0);
        let (c, resp) = ui.allocate_exact_size(vec2(16.0, ROW), Sense::click());
        caret(ui.painter(), c.center(), !open, theme::text_dim());
        let label = ui.add(
            egui::Label::new(RichText::new(tr("Pan/Crop Box")).color(theme::text_dim()))
                .sense(Sense::click()),
        );
        if resp.clicked() || label.clicked() {
            toggle_fold(app, "box");
        }
    });
    rows.push((Lane::Motion, r.response.rect));
    if open {
        let mut w = key.width.abs();
        let mut h = key.height.abs();
        let mut center = key.center;
        let mut ch = false;
        prop(ui, tr("Width"), Lane::Motion, rows, None, |ui| {
            ch |= hot(ui, &mut w, 1, 1.0, "", 1.0..=place.media[0] * 50.0);
        });
        prop(ui, tr("Height"), Lane::Motion, rows, None, |ui| {
            ch |= hot(ui, &mut h, 1, 1.0, "", 1.0..=place.media[1] * 50.0);
        });
        prop(ui, tr("X Center"), Lane::Motion, rows, None, |ui| {
            ch |= hot(ui, &mut center[0], 1, 1.0, "", -1e6..=1e6);
        });
        prop(ui, tr("Y Center"), Lane::Motion, rows, None, |ui| {
            ch |= hot(ui, &mut center[1], 1, 1.0, "", -1e6..=1e6);
        });
        if ch {
            // Mirrored boxes keep their sign; the turning point moves
            // with the box.
            key.width = w.copysign(key.width);
            key.height = h.copysign(key.height);
            let moved = [center[0] - key.center[0], center[1] - key.center[1]];
            key.pivot = [key.pivot[0] + moved[0], key.pivot[1] + moved[1]];
            key.center = center;
        }
    }
    if key != key0 {
        let new = app.settings.prefs.new_motion_keys();
        app.modify_merged(ui.id().with(("box", id)), |p| {
            let _ = p.set_clip_box_at(id, at, key, new);
        });
    }
    let _ = place;
}

/// The keyframe's interpolation: the curve and smoothness of the keyframe
/// at the cursor or the one before it.
fn interpolation(ui: &mut Ui, app: &mut BananaApp, clip: &Clip, offset: Tick, rows: &mut Rows) {
    let keys = &clip.pan_crop.keys;
    if keys.len() < 2 {
        return;
    }
    let open = !app.transform_panel.closed.contains("interpolation");
    let r = ui.horizontal(|ui| {
        ui.set_height(ROW);
        ui.add_space(INDENT - 16.0);
        let (c, resp) = ui.allocate_exact_size(vec2(16.0, ROW), Sense::click());
        caret(ui.painter(), c.center(), !open, theme::text_dim());
        let label = ui.add(
            egui::Label::new(RichText::new(tr("Keyframe interpolation")).color(theme::text_dim()))
                .sense(Sense::click()),
        );
        if resp.clicked() || label.clicked() {
            toggle_fold(app, "interpolation");
        }
    });
    rows.push((Lane::Motion, r.response.rect));
    if !open {
        return;
    }
    let Some(i) = keys.iter().rposition(|k| k.at <= offset) else {
        return;
    };
    let mut curve = keys[i].curve;
    let mut smooth = keys[i].smoothness * 100.0;
    let at = keys[i].at;
    let id = clip.id;
    let mut ch = false;
    prop(ui, tr("Curve"), Lane::Motion, rows, None, |ui| {
        ch |= choice(ui, "transform-curve", &mut curve, &key_curves());
    });
    prop(ui, tr("Smoothness"), Lane::Motion, rows, None, |ui| {
        ch |= hot(ui, &mut smooth, 1, 0.5, "", 0.0..=100.0);
    });
    if ch {
        app.modify_merged(ui.id().with(("interpolation", id, at)), |p| {
            if let Some(c) = p.clip_mut(id)
                && let Some(k) = c.pan_crop.keys.iter_mut().find(|k| k.at == at)
            {
                k.curve = curve;
                k.smoothness = (smooth / 100.0).clamp(0.0, 1.0);
            }
        });
    }
}

/// The Pan/Crop's source settings and Reduce Interlace Flicker.
fn source(ui: &mut Ui, app: &mut BananaApp, clip: &Clip, rows: &mut Rows) {
    let id = clip.id;
    let (mut keep, mut fill, mut flicker) = (
        clip.pan_crop.keep_aspect,
        clip.pan_crop.fill,
        clip.reduce_flicker,
    );
    let mut ch = false;
    prop(
        ui,
        tr("Maintain aspect ratio"),
        Lane::Motion,
        rows,
        None,
        |ui| {
            ch |= tick(ui, &mut keep);
        },
    );
    prop(
        ui,
        tr("Stretch to fill frame"),
        Lane::Motion,
        rows,
        None,
        |ui| {
            ch |= tick(ui, &mut fill);
        },
    );
    prop(
        ui,
        tr("Reduce interlace flicker"),
        Lane::Motion,
        rows,
        None,
        |ui| {
            ch |= tick(ui, &mut flicker);
        },
    );
    if ch {
        app.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.pan_crop.keep_aspect = keep;
                c.pan_crop.fill = fill;
                c.reduce_flicker = flicker;
            }
        });
    }
}

// ---- Opacity and Time -----------------------------------------------------

fn opacity(ui: &mut Ui, app: &mut BananaApp, id: ClipId, rows: &mut Rows) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    // The fades' ramps beside the Opacity's row, or its header when it is
    // folded.
    let folded = app.transform_panel.closed.contains("opacity");
    let open = effect(
        ui,
        app,
        "opacity",
        tr("Opacity"),
        if folded { Lane::Fades } else { Lane::Plain },
        rows,
        |ui, app| {
            let changed =
                clip.level != 1.0 || clip.fade_in > Tick::ZERO || clip.fade_out > Tick::ZERO;
            if reset_button(ui, changed) {
                app.modify(|p| {
                    let _ = p.set_clip_level(id, 1.0);
                    let _ = p.set_fades(id, Some(Tick::ZERO), Some(Tick::ZERO));
                });
            }
        },
    );
    if !open {
        return;
    }
    let mut level = f64::from(clip.level) * 100.0;
    let mut ch = false;
    let reset = prop(
        ui,
        tr("Opacity"),
        Lane::Fades,
        rows,
        Some((level - 100.0).abs() > 1e-6),
        |ui| {
            ch = hot(ui, &mut level, 1, 0.5, " %", 0.0..=100.0);
        },
    );
    if reset {
        level = 100.0;
        ch = true;
    }
    if ch {
        app.modify_merged(ui.id().with(("level", id)), |p| {
            let _ = p.set_clip_level(id, (level / 100.0) as f32);
        });
    }
    fades(ui, app, &clip, Lane::Plain, rows);
}

/// An event's fades: their lengths (seconds) and curves.
fn fades(ui: &mut Ui, app: &mut BananaApp, clip: &Clip, lane: Lane, rows: &mut Rows) {
    let id = clip.id;
    let max = clip.duration.as_seconds_f64();
    for (out, name, curve_name) in [
        (false, tr("Fade In"), tr("Fade-in curve")),
        (true, tr("Fade Out"), tr("Fade-out curve")),
    ] {
        let len = if out { clip.fade_out } else { clip.fade_in };
        let mut s = len.as_seconds_f64();
        let mut ch = false;
        let reset = prop(ui, name, lane, rows, Some(len > Tick::ZERO), |ui| {
            ch = hot(ui, &mut s, 2, 0.02, " s", 0.0..=max);
        });
        if reset {
            s = 0.0;
            ch = true;
        }
        if ch {
            let t = Tick::from_seconds_f64(s);
            app.modify_merged(ui.id().with(("fade", id, out)), |p| {
                let _ = if out {
                    p.set_fades(id, None, Some(t))
                } else {
                    p.set_fades(id, Some(t), None)
                };
            });
        }
        if len > Tick::ZERO {
            let mut curve = if out {
                clip.fade_out_curve
            } else {
                clip.fade_in_curve
            };
            let mut ch = false;
            prop(ui, curve_name, lane, rows, None, |ui| {
                ch = choice(
                    ui,
                    if out {
                        "fade-out-curve"
                    } else {
                        "fade-in-curve"
                    },
                    &mut curve,
                    &fade_curves(),
                );
            });
            if ch {
                app.modify(|p| {
                    let _ = p.set_fade_curve(id, out, curve);
                });
            }
        }
    }
}

fn time(ui: &mut Ui, app: &mut BananaApp, id: ClipId, rows: &mut Rows) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    let open = effect(ui, app, "time", tr("Time"), Lane::Plain, rows, |ui, app| {
        let changed = clip.rate != 1.0 || clip.undersample != 1.0;
        if reset_button(ui, changed) {
            app.modify(|p| {
                if let Some(c) = p.clip_mut(id) {
                    c.rate = 1.0;
                    c.undersample = 1.0;
                }
            });
        }
    });
    if !open {
        return;
    }
    let mut rate = clip.rate;
    let mut ch = false;
    prop(ui, tr("Playback rate"), Lane::Plain, rows, None, |ui| {
        ch = hot(ui, &mut rate, 3, 0.005, " ×", 0.05..=20.0);
    });
    if ch {
        app.modify_merged(ui.id().with(("rate", id)), |p| {
            if let Some(c) = p.clip_mut(id) {
                c.rate = rate;
            }
        });
    }
    let mut under = f64::from(clip.undersample);
    let mut ch = false;
    prop(ui, tr("Undersample rate"), Lane::Plain, rows, None, |ui| {
        ch = hot(ui, &mut under, 3, 0.005, "", 0.01..=1.0);
    });
    if ch {
        app.modify_merged(ui.id().with(("undersample", id)), |p| {
            if let Some(c) = p.clip_mut(id) {
                c.undersample = under as f32;
            }
        });
    }
    let (mut looped, mut hold) = (clip.looped, clip.hold_last);
    let mut ch = false;
    prop(ui, tr("Loop"), Lane::Plain, rows, None, |ui| {
        ch |= tick(ui, &mut looped);
    });
    prop(ui, tr("Hold last frame"), Lane::Plain, rows, None, |ui| {
        ch |= tick(ui, &mut hold);
    });
    if ch {
        app.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.looped = looped;
                c.hold_last = hold && !looped;
            }
        });
    }
    let mut reversed = clip.reverse.is_some();
    prop(ui, tr("Reverse"), Lane::Plain, rows, None, |ui| {
        if tick(ui, &mut reversed) {
            app.modify(|p| {
                let _ = p.reverse_event(id);
            });
        }
    });
}

// ---- Audio ----------------------------------------------------------------

fn volume(ui: &mut Ui, app: &mut BananaApp, id: ClipId, rows: &mut Rows) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    let open = effect(
        ui,
        app,
        "volume",
        tr("Volume"),
        Lane::Plain,
        rows,
        |ui, app| {
            if reset_button(ui, clip.level != 1.0) {
                app.modify(|p| {
                    let _ = p.set_clip_level(id, 1.0);
                });
            }
        },
    );
    if !open {
        return;
    }
    let (mut muted, mut invert) = (clip.muted, clip.invert_phase);
    let mut ch = false;
    prop(ui, tr("Mute"), Lane::Plain, rows, None, |ui| {
        ch |= tick(ui, &mut muted);
    });
    let mut db = if clip.level > 0.0 {
        20.0 * f64::from(clip.level).log10()
    } else {
        -60.0
    };
    let mut level_changed = false;
    let reset = prop(
        ui,
        tr("Level"),
        Lane::Plain,
        rows,
        Some(clip.level != 1.0),
        |ui| {
            level_changed = hot(ui, &mut db, 1, 0.1, " dB", -60.0..=24.0);
        },
    );
    if reset {
        db = 0.0;
        level_changed = true;
    }
    prop(ui, tr("Invert phase"), Lane::Plain, rows, None, |ui| {
        ch |= tick(ui, &mut invert);
    });
    if ch {
        app.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.muted = muted;
                c.invert_phase = invert;
            }
        });
    }
    if level_changed {
        let gain = if db <= -60.0 {
            0.0
        } else {
            10f64.powf(db / 20.0)
        };
        app.modify_merged(ui.id().with(("gain", id)), |p| {
            let _ = p.set_clip_level(id, gain as f32);
        });
    }
    fades(ui, app, &clip, Lane::Plain, rows);
}

fn channels(ui: &mut Ui, app: &mut BananaApp, id: ClipId, rows: &mut Rows) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    let open = effect(
        ui,
        app,
        "channels",
        tr("Channels"),
        Lane::Plain,
        rows,
        |ui, app| {
            if reset_button(ui, clip.channels != ChannelMap::Both) {
                app.modify(|p| {
                    if let Some(c) = p.clip_mut(id) {
                        c.channels = ChannelMap::Both;
                    }
                });
            }
        },
    );
    if !open {
        return;
    }
    let mut map = clip.channels;
    let options = [
        (ChannelMap::Both, tr("Both")),
        (ChannelMap::LeftOnly, tr("Left Only")),
        (ChannelMap::RightOnly, tr("Right Only")),
        (ChannelMap::Combine, tr("Combine")),
        (ChannelMap::Swap, tr("Swap")),
    ];
    let mut ch = false;
    prop(ui, tr("Channels"), Lane::Plain, rows, None, |ui| {
        ch = choice(ui, "transform-channels", &mut map, &options);
    });
    if ch {
        app.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.channels = map;
            }
        });
    }
}

fn pitch(ui: &mut Ui, app: &mut BananaApp, id: ClipId, rows: &mut Rows) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    let open = effect(
        ui,
        app,
        "pitch",
        tr("Pitch"),
        Lane::Plain,
        rows,
        |ui, app| {
            if reset_button(ui, clip.pitch != 0.0) {
                app.modify(|p| {
                    if let Some(c) = p.clip_mut(id) {
                        c.pitch = 0.0;
                    }
                });
            }
        },
    );
    if !open {
        return;
    }
    let mut semis = f64::from(clip.pitch);
    let mut keep = clip.preserve_pitch;
    let (mut ch, mut kch) = (false, false);
    prop(ui, tr("Pitch change"), Lane::Plain, rows, None, |ui| {
        ch = hot(ui, &mut semis, 2, 0.05, "", -48.0..=48.0);
    });
    prop(ui, tr("Preserve pitch"), Lane::Plain, rows, None, |ui| {
        kch = tick(ui, &mut keep);
    });
    if ch {
        app.modify_merged(ui.id().with(("pitch", id)), |p| {
            if let Some(c) = p.clip_mut(id) {
                c.pitch = semis as f32;
            }
        });
    }
    if kch {
        app.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.preserve_pitch = keep;
            }
        });
    }
}

// ---- the keyframes' strip ---------------------------------------------------

/// The event's time beside the rows: its name over it, the cursor, the
/// Motion's keyframes on its rows (a click goes there, a drag moves them in
/// time, their menu gives their curve or deletes them) and the fades on
/// the Opacity's.
fn strip(
    ui: &mut Ui,
    app: &mut BananaApp,
    id: ClipId,
    width: f32,
    top: f32,
    bottom: f32,
    rows: &Rows,
) {
    let Some(clip) = app.project().clip(id).cloned() else {
        return;
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, bottom - top), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::bg());
    let span = clip.duration.as_seconds_f64().max(1e-6);
    let x_of = |t: Tick| {
        rect.left() + ((t.as_seconds_f64() / span) as f32).clamp(0.0, 1.0) * (rect.width() - 1.0)
    };
    let t_of = |x: f32| {
        Tick::from_seconds_f64(f64::from(((x - rect.left()) / rect.width()).clamp(0.0, 1.0)) * span)
    };
    // The rows' bands, as the properties beside them.
    for (lane, r) in rows {
        let band = Rect::from_min_max(pos2(rect.left(), r.top()), pos2(rect.right(), r.bottom()));
        if *lane == Lane::Header {
            painter.rect_filled(band, 0.0, theme::panel_dark());
        }
        painter.line_segment(
            [band.left_bottom(), band.right_bottom()],
            Stroke::new(1.0, theme::panel()),
        );
    }
    // The event's bar with its name, over the first row.
    if let Some((_, first)) = rows.first() {
        let bar = Rect::from_min_max(
            pos2(rect.left(), first.top() + 2.0),
            pos2(rect.right(), first.bottom() - 2.0),
        );
        painter.rect_filled(bar, 2.0, theme::tab_on());
        let name = app
            .project()
            .media(clip.media)
            .map(crate::i18n::media_name)
            .unwrap_or_default();
        painter.text(
            bar.left_center() + vec2(5.0, 0.0),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(11.5),
            theme::text_strong(),
        );
    }
    // The fades on the Opacity's rows: their ramps.
    for (lane, r) in rows.iter().filter(|(l, _)| *l == Lane::Fades).take(1) {
        let _ = lane;
        let (y0, y1) = (r.bottom() - 3.0, r.top() + 3.0);
        let line = Stroke::new(1.5, theme::selection());
        let a = x_of(clip.fade_in);
        let b = x_of(clip.duration - clip.fade_out);
        painter.add(Shape::line(
            vec![
                pos2(rect.left(), y0),
                pos2(a, y1),
                pos2(b, y1),
                pos2(rect.right(), y0),
            ],
            line,
        ));
    }
    // The Motion's keyframes, on each of its rows.
    let keys = &clip.pan_crop.keys;
    let animated = keys.len() > 1;
    let mut spots = Vec::new();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let mut hot_key: Option<Tick> = None;
    if animated {
        for (_, r) in rows.iter().filter(|(l, _)| *l == Lane::Motion) {
            let y = r.center().y;
            for k in keys {
                let c = pos2(x_of(k.at), y);
                let near = pointer.is_some_and(|p| (p - c).length() <= 6.0);
                if near {
                    hot_key = Some(k.at);
                }
                let fill = if near {
                    theme::text_strong()
                } else {
                    theme::text_dim()
                };
                diamond(&painter, c, 4.5, fill, theme::panel_dark());
                spots.push((k.at, c));
            }
        }
    }
    app.transform_panel.spots = spots;
    // The cursor.
    let now = app.display_time() - clip.start;
    if now >= Tick::ZERO && now <= clip.duration {
        let x = x_of(now).round() + 0.5;
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, theme::selection()),
        );
    }
    // A keyframe taken: dragged in time (snapped to frames, between the
    // ones around it).
    if resp.drag_started()
        && let Some(at) = hot_key
    {
        let serial = app.transform_panel.drags;
        app.transform_panel.drags += 1;
        app.transform_panel.drag = Some((id, at, serial));
    }
    if resp.dragged()
        && let Some((cid, from, serial)) = app.transform_panel.drag
        && cid == id
        && let Some(p) = pointer
    {
        let rate = app.project().settings.frame_rate;
        let to = rate.snap_nearest(t_of(p.x));
        let i = keys.iter().position(|k| k.at == from);
        if let Some(i) = i {
            let frame = rate.frame_duration();
            let lo = if i == 0 {
                Tick::ZERO
            } else {
                keys[i - 1].at + frame
            };
            let hi = keys.get(i + 1).map_or(clip.duration, |k| k.at - frame);
            let to = to.clamp(lo, hi.max(lo));
            if to != from {
                app.modify_merged(egui::Id::new(("transform-key-drag", serial)), |p| {
                    if let Some(c) = p.clip_mut(id)
                        && let Some(k) = c.pan_crop.keys.iter_mut().find(|k| k.at == from)
                    {
                        k.at = to;
                    }
                });
                app.transform_panel.drag = Some((id, to, serial));
            }
        }
    }
    if resp.drag_stopped() {
        app.transform_panel.drag = None;
    }
    // A click: the cursor there (on a keyframe, at it).
    if resp.clicked()
        && let Some(p) = pointer
    {
        let t = hot_key.unwrap_or_else(|| t_of(p.x));
        app.seek(clip.start + t);
    }
    // A keyframe's menu: its curve, or away with it.
    if let Some(at) = hot_key.filter(|_| resp.secondary_clicked()) {
        ui.memory_mut(|m| m.data.insert_temp(egui::Id::new("transform-key-menu"), at));
    }
    let menu_at: Option<Tick> = ui.memory(|m| m.data.get_temp(egui::Id::new("transform-key-menu")));
    resp.context_menu(|ui| {
        let Some(at) = menu_at else {
            ui.close();
            return;
        };
        for (curve, name) in key_curves() {
            let now = keys.iter().find(|k| k.at == at).map(|k| k.curve);
            if ui.selectable_label(now == Some(curve), name).clicked() {
                app.modify(|p| {
                    if let Some(c) = p.clip_mut(id)
                        && let Some(k) = c.pan_crop.keys.iter_mut().find(|k| k.at == at)
                    {
                        k.curve = curve;
                    }
                });
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Delete Keyframe")).clicked() {
            app.modify(|p| {
                if let Some(c) = p.clip_mut(id) {
                    c.pan_crop.remove_key(at);
                }
            });
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's gradient event at the cursor, selected.
    fn app() -> (BananaApp, ClipId) {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let id = transform_ui::target(&app).expect("an event at the cursor");
        (app, id)
    }

    #[test]
    fn the_stopwatch_animates_the_framing_and_holds_it_again() {
        let (mut app, id) = app();
        assert!(app.project().clip(id).unwrap().pan_crop.keys.len() <= 1);
        let clip = app.project().clip(id).unwrap().clone();
        // As a click on it: keyframes at the start and at the cursor.
        let now = app.display_time();
        let key = app.project().clip_box_at(id, now).unwrap();
        let place = app.project().clip_place_in(id).unwrap();
        let offset = now - clip.start;
        app.modify(|p| {
            let c = p.clip_mut(id).unwrap();
            c.pan_crop.set_at(
                Tick::ZERO,
                PanCropKey {
                    at: Tick::ZERO,
                    ..key
                },
                place.media,
                (KeyCurve::Smooth, 0.0),
            );
            c.pan_crop
                .set_at(offset, key, place.media, (KeyCurve::Smooth, 0.0));
        });
        let keys = &app.project().clip(id).unwrap().pan_crop.keys;
        assert_eq!(keys.len(), 2, "animated");
        assert_eq!(keys[1].at, offset);
        // Taking the cursor's away leaves one framing all along.
        app.modify(|p| p.clip_mut(id).unwrap().pan_crop.remove_key(offset));
        let keys = &app.project().clip(id).unwrap().pan_crop.keys;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].at, Tick::ZERO);
    }

    #[test]
    fn the_window_draws_every_section() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        let mut drawn = 0;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut rows = Rows::new();
            let id = transform_ui::target(&app).unwrap();
            properties(ui, &mut app, id, &mut rows);
            drawn = rows.len();
        });
        out.textures_delta.clear();
        assert!(drawn >= 20, "the rows: {drawn}");
    }
}
