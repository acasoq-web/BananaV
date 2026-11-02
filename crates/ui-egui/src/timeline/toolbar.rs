//! The timeline's bottom bar (the customizable toolbar of transport and
//! editing buttons), the playback rate and the scroll bars.

use crate::i18n::tr;
use bv_core::Tick;

use super::{Layout, MAX_TICKS_PER_PX, MIN_TICKS_PER_PX, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::theme;

pub(crate) fn ui(view: &mut TimelineView, ui: &mut egui::Ui, app: &mut BananaApp, l: &Layout) {
    let painter = ui.painter();
    painter.rect_filled(l.toolbar, 0.0, theme::panel());
    // Apart from the scroll bar above, as VEGAS's.
    painter.line_segment(
        [l.toolbar.left_top(), l.toolbar.right_top()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    // Right-click on the toolbar: customize it, like VEGAS.
    let bg = ui.interact(l.toolbar, ui.id().with("barra-timeline"), Sense::click());
    bg.context_menu(|ui| {
        if ui.button(tr("Customize Timeline Toolbar…")).clicked() {
            crate::commands::Cmd::CustomizeTimelineToolbar.run(app);
            ui.close();
        }
    });
    let boxes = time_boxes(view, ui, app, l);
    // The buttons of Options › Customize Timeline Toolbar, those past the
    // time boxes cut off.
    let room = Rect::from_min_max(l.toolbar.min, pos2(boxes - 12.0, l.toolbar.bottom()));
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(room.shrink2(vec2(6.0, 3.0))),
        |ui| {
            ui.set_clip_rect(room.intersect(ui.clip_rect()));
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                crate::toolbar_buttons::paint(
                    ui,
                    app,
                    crate::toolbar_buttons::ToolbarKind::Timeline,
                );
            });
        },
    );
}

/// One of the time boxes at the toolbar's right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TimeBox {
    /// Where the cursor is (out of the time selection).
    Cursor,
    Start,
    End,
    Length,
}

/// A time box's icon and the gaps around its time.
const BOX_ICON: f32 = 14.0;
const BOX_GAP: f32 = 8.0;
const BOX_SPACE: f32 = 16.0;

/// The time boxes at the toolbar's right, as VEGAS 23's (its Help's
/// "Creating selections and positioning the cursor"): with the cursor in
/// the time selection, its start, end and length; otherwise where the
/// cursor is, the end's icon alone and the selection's length dimmed. A
/// double-click types into a lit one — a time in the ruler's format, or +
/// or − and a length from what it shows. Returns their left edge.
fn time_boxes(view: &mut TimelineView, ui: &mut egui::Ui, app: &mut BananaApp, l: &Layout) -> f32 {
    let now = app.display_time();
    let selection = app
        .project()
        .loop_region
        .filter(|r| r.duration > Tick::ZERO);
    let inside = selection.filter(|r| now >= r.start && now <= r.end());
    let length = |app: &BananaApp, d: Tick| crate::timefmt::length_text(app.project(), d);
    let boxes = match inside {
        Some(r) => [
            (TimeBox::Start, Some(app.ruler_text(r.start)), true),
            (TimeBox::End, Some(app.ruler_text(r.end())), true),
            (TimeBox::Length, Some(length(app, r.duration)), true),
        ],
        None => [
            (TimeBox::Cursor, Some(app.ruler_text(now)), true),
            (TimeBox::End, None, false),
            (
                TimeBox::Length,
                Some(length(app, selection.map_or(Tick::ZERO, |r| r.duration))),
                false,
            ),
        ],
    };
    // Each as wide as a long time of the ruler's format, so they keep their
    // places as the times change.
    let font = FontId::proportional(12.5);
    let painter = ui.painter().clone();
    let measure = |t: &str| {
        painter
            .layout_no_wrap(t.to_string(), font.clone(), theme::text())
            .size()
            .x
    };
    let widest = boxes
        .iter()
        .filter_map(|b| b.1.as_deref())
        .map(measure)
        .fold(
            measure(&app.ruler_text(Tick::from_seconds(36_000))),
            f32::max,
        );
    let slot = BOX_ICON + BOX_GAP + widest.ceil();
    let first = l.toolbar.right() - 6.0 - 3.0 * slot - 2.0 * BOX_SPACE;
    let cy = l.toolbar.center().y;
    for (i, (kind, text, lit)) in boxes.into_iter().enumerate() {
        let x = first + i as f32 * (slot + BOX_SPACE);
        let icon = Rect::from_center_size(pos2(x + BOX_ICON / 2.0, cy), vec2(BOX_ICON, BOX_ICON));
        paint_box_icon(&painter, icon, kind);
        let field = Rect::from_min_max(
            pos2(icon.right() + BOX_GAP, cy - 10.0),
            pos2(x + slot, cy + 10.0),
        );
        if let Some((editing, typed)) = &mut view.time_edit
            && *editing == kind
        {
            let resp = ui.put(
                field.expand2(vec2(3.0, 0.0)),
                egui::TextEdit::singleline(typed)
                    .font(font.clone())
                    .margin(vec2(2.0, 1.0)),
            );
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if resp.lost_focus() || cancel {
                if !cancel {
                    type_into(app, kind, typed);
                }
                view.time_edit = None;
            }
            continue;
        }
        if let Some(text) = &text {
            let color = if lit {
                theme::text()
            } else {
                theme::text_disabled()
            };
            painter.text(
                field.left_center(),
                Align2::LEFT_CENTER,
                text,
                font.clone(),
                color,
            );
        }
        let area = Rect::from_min_max(pos2(icon.left(), field.top()), field.max);
        let resp = ui.interact(area, ui.id().with(("caixa-de-tempo", kind)), Sense::click());
        if lit && resp.double_clicked() {
            view.time_edit = Some((kind, text.unwrap_or_default()));
        }
        let name = match kind {
            TimeBox::Cursor => tr("Cursor Position"),
            TimeBox::Start => tr("Selection Start"),
            TimeBox::End => tr("Selection End"),
            TimeBox::Length => tr("Selection Length"),
        };
        if lit {
            resp.on_hover_text(trf!("{} · double-click to type it", name));
        } else {
            resp.on_hover_text(name);
        }
    }
    first
}

/// A time box's icon, BananaV's own: the cursor's needle as the ruler
/// shows it, or the time selection as a box with the corner it names
/// marked in the loop bar's color — both corners for its length.
pub(crate) fn paint_box_icon(p: &egui::Painter, r: Rect, kind: TimeBox) {
    paint_box_icon_in(p, r, kind, theme::loop_bar());
}

/// [`paint_box_icon`] with its corners in `corner` (the Trimmer's: VEGAS's
/// yellow).
pub(crate) fn paint_box_icon_in(p: &egui::Painter, r: Rect, kind: TimeBox, corner: egui::Color32) {
    if kind == TimeBox::Cursor {
        let (x, top) = (r.center().x.round() + 0.5, r.top() + 1.0);
        p.line_segment(
            [pos2(x, top + 8.0), pos2(x, r.bottom())],
            Stroke::new(1.0, theme::cursor()),
        );
        p.add(egui::Shape::convex_polygon(
            vec![
                pos2(x - 4.0, top),
                pos2(x + 4.0, top),
                pos2(x + 4.0, top + 6.0),
                pos2(x, top + 10.0),
                pos2(x - 4.0, top + 6.0),
            ],
            theme::cursor(),
            Stroke::NONE,
        ));
        return;
    }
    let b = Rect::from_min_max(
        pos2(r.left() + 0.5, r.top() + 2.5),
        pos2(r.right() - 0.5, r.bottom() - 1.5),
    );
    p.rect_stroke(
        b,
        0.0,
        Stroke::new(1.0, theme::text()),
        egui::StrokeKind::Middle,
    );
    let corner = |left: bool| {
        let (x, dx) = if left {
            (b.left(), 7.0)
        } else {
            (b.right(), -7.0)
        };
        egui::Shape::convex_polygon(
            vec![
                pos2(x, b.top()),
                pos2(x + dx, b.top()),
                pos2(x, b.top() + 7.0),
            ],
            corner,
            Stroke::NONE,
        )
    };
    if kind != TimeBox::End {
        p.add(corner(true));
    }
    if kind != TimeBox::Start {
        p.add(corner(false));
    }
}

/// What a time box typed into sets: the cursor (the view following it),
/// or the time selection's start, end or length — past the other end it
/// keeps its length, and the cursor goes to its start when it would be
/// left out of it. `+` or `−` and a length count from what the box
/// showed; text that names no time leaves things as they were.
fn type_into(app: &mut BananaApp, kind: TimeBox, text: &str) {
    let selection = app
        .project()
        .loop_region
        .filter(|r| r.duration > Tick::ZERO);
    let shown = match kind {
        TimeBox::Cursor => app.display_time(),
        TimeBox::Start => selection.map_or(app.cursor, |r| r.start),
        TimeBox::End => selection.map_or(app.cursor, |r| r.end()),
        TimeBox::Length => selection.map_or(Tick::ZERO, |r| r.duration),
    };
    let project = app.project();
    let text = text.trim();
    let typed = if let Some(rest) = text.strip_prefix('+') {
        crate::timefmt::parse_length(project, rest).map(|d| shown + d)
    } else if let Some(rest) = text.strip_prefix('-').or_else(|| text.strip_prefix('−')) {
        crate::timefmt::parse_length(project, rest).map(|d| shown - d)
    } else if kind == TimeBox::Length {
        crate::timefmt::parse_length(project, text)
    } else {
        crate::timefmt::parse_position(project, text)
    };
    let Some(t) = typed else {
        return;
    };
    let Some(r) = selection.filter(|_| kind != TimeBox::Cursor) else {
        app.seek_playback(t);
        app.center_on_cursor();
        return;
    };
    let (start, end) = match kind {
        TimeBox::Start if t < r.end() => (t, r.end()),
        TimeBox::Start => (t, t + r.duration),
        TimeBox::End if t > r.start => (r.start, t),
        TimeBox::End => (t - r.duration, t),
        _ => (r.start, r.start + t),
    };
    let start = start.max(Tick::ZERO);
    if end > start {
        app.set_loop_region(Some(bv_core::TimeRange::from_bounds(start, end)));
        if app.cursor < start || app.cursor > end {
            app.seek(start);
        }
    }
}

/// Fastest shuttle of the scrub control, and fastest normal rate.
pub(super) const MAX_RATE: f64 = 4.0;

/// The scrub control at the bottom of the track list, beside the scroll
/// bar and the toolbar, like VEGAS 23's. Dragging the knob shuttles
/// backward or forward from the cursor, the further the faster; it springs
/// back to 0 when released. The triangle under it is the normal rate, the
/// speed of Play. Double-click the slider: normal rate 1; double-click
/// "Rate": type the normal rate.
pub(crate) fn rate(view: &mut TimelineView, ui: &mut egui::Ui, app: &mut BananaApp, l: &Layout) {
    let r = l.rate.shrink2(vec2(10.0, 2.0));
    let painter = ui.painter().clone();
    painter.rect_filled(l.rate, 0.0, theme::panel());
    // Apart from the tracks above, as VEGAS's.
    painter.line_segment(
        [l.rate.left_top(), l.rate.right_top()],
        Stroke::new(1.0, theme::panel_dark()),
    );
    // As VEGAS's: nothing at the color bars' width, the rate alone while
    // the track list is narrow, its slider from 200 pixels.
    if l.rate.width() < 64.0 {
        app.scrub_rate = 0.0;
        return;
    }
    let label_w = 72.0;
    let label_rect = Rect::from_min_size(
        pos2(r.left(), r.center().y - 9.0),
        vec2(label_w - 8.0, 18.0),
    );
    if let Some(text) = &mut view.rate_text {
        let resp = ui.put(
            label_rect,
            egui::TextEdit::singleline(text)
                .font(FontId::proportional(11.0))
                .margin(vec2(3.0, 1.0)),
        );
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if resp.lost_focus() || cancel {
            if !cancel && let Some(v) = crate::fmt::parse(text) {
                app.play_rate = v.clamp(0.1, MAX_RATE);
                app.restart_if_playing();
            }
            view.rate_text = None;
        }
    } else {
        painter.text(
            pos2(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            trf!("Rate: {}", crate::fmt::num(app.scrub_rate, 2)),
            FontId::proportional(11.0),
            theme::text(),
        );
        let label = ui.interact(label_rect, ui.id().with("taxa-rotulo"), Sense::click());
        if label.double_clicked() {
            view.rate_text = Some(crate::fmt::num(app.play_rate, 2));
        }
        label.on_hover_text(trf!(
            "Normal rate {}× · double-click to type it",
            crate::fmt::num(app.play_rate, 2)
        ));
    }

    if l.rate.width() < 200.0 {
        app.scrub_rate = 0.0;
        return;
    }
    // The line, the normal rate triangle under it and the knob.
    let line_y = r.center().y - 2.0;
    let line = Rect::from_min_max(
        pos2(r.left() + label_w, line_y - 1.0),
        pos2(r.right() - 6.0, line_y + 1.0),
    );
    let half = line.width() / 2.0;
    // The rate grows with the distance from the middle: in proportion, or
    // (Options › Preferences › General: "Use linear scrub range" off) more
    // sharply toward the ends.
    let linear = app.settings.prefs.linear_scrub;
    let curve = |f: f64| if linear { f } else { f * f.abs() };
    let uncurve = |f: f64| {
        if linear {
            f
        } else {
            f.signum() * f.abs().sqrt()
        }
    };
    let to_x = |v: f64| line.center().x + uncurve(v / MAX_RATE) as f32 * half;
    let from_x = |x: f32| curve(((x - line.center().x) / half).clamp(-1.0, 1.0) as f64) * MAX_RATE;
    painter.rect_filled(line, 1.0, theme::text_dim().gamma_multiply(0.7));
    let tx = to_x(app.play_rate);
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(tx, line_y + 5.0),
            pos2(tx + 4.5, line_y + 10.0),
            pos2(tx - 4.5, line_y + 10.0),
        ],
        theme::text(),
        egui::Stroke::NONE,
    ));
    let knob = Rect::from_center_size(pos2(to_x(app.scrub_rate), line_y), vec2(10.0, 13.0));
    crate::widgets::paint_knob(&painter, knob, false);

    let knob_zone = Rect::from_min_max(
        pos2(line.left() - 6.0, r.top()),
        pos2(line.right() + 6.0, line_y + 4.0),
    );
    let shuttle = ui.interact(
        knob_zone,
        ui.id().with("taxa-scrub"),
        Sense::click_and_drag(),
    );
    if shuttle.drag_started() && app.is_playing() {
        app.stop();
    }
    if shuttle.dragged()
        && let Some(p) = shuttle.interact_pointer_pos()
    {
        app.scrub_rate = (from_x(p.x) * 100.0).round() / 100.0;
    }
    if shuttle.drag_stopped() {
        app.scrub_rate = 0.0;
    }
    if shuttle.double_clicked() {
        app.play_rate = 1.0;
        app.restart_if_playing();
    }
    shuttle.on_hover_text(tr(
        "Scrub: drag to shuttle from the cursor, the further the faster · double-click: normal rate 1",
    ));
    let rate_zone = Rect::from_min_max(
        pos2(line.left() - 6.0, line_y + 4.0),
        pos2(line.right() + 6.0, r.bottom() + 2.0),
    );
    let normal = ui.interact(
        rate_zone,
        ui.id().with("taxa-normal"),
        Sense::click_and_drag(),
    );
    if (normal.dragged() || normal.clicked())
        && let Some(p) = normal.interact_pointer_pos()
    {
        let v = from_x(p.x).clamp(0.1, MAX_RATE);
        // Pulls to 1.
        app.play_rate = if (v - 1.0).abs() < 0.06 {
            1.0
        } else {
            (v * 100.0).round() / 100.0
        };
    }
    if normal.drag_stopped() || normal.clicked() {
        app.restart_if_playing();
    }
    if normal.double_clicked() {
        app.play_rate = 1.0;
        app.restart_if_playing();
    }
    normal.on_hover_text(tr("Normal rate: the speed of Play (double-click: 1)"));

    // While the knob is held away from 0, the cursor moves at that rate.
    if app.scrub_rate != 0.0 {
        let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
        let end = app.project().duration().max(Tick::ZERO);
        let t = app.cursor + Tick::from_seconds_f64(app.scrub_rate * dt);
        app.cursor = t.clamp(Tick::ZERO, end);
        let x = view.x_of(l, app.cursor);
        if x > l.lanes.right() - 10.0 || x < l.lanes.left() {
            let width = Tick((l.lanes.width() as f64 * view.ticks_per_px) as i64);
            view.view_start = (app.cursor - Tick(width.0 / 2)).max(Tick::ZERO);
        }
        ui.ctx().request_repaint();
    }
}

pub(crate) fn scrollbars(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
) {
    let painter = ui.painter().clone();
    // Horizontal.
    let bar = Rect::from_min_max(
        l.hscroll.min,
        pos2(l.hscroll.right() - 60.0, l.hscroll.bottom()),
    );
    painter.rect_filled(l.hscroll, 0.0, theme::panel());
    // VEGAS's: its middle scrolls, its ends zoom, and however long the
    // project its thumb never disappears.
    let lanes = l.lanes.width().max(1.0) as f64;
    let visible = Tick((lanes * view.ticks_per_px) as i64);
    let total = (app.project().duration().max(view.view_start + visible) + Tick(visible.0 / 5))
        .max(Tick(1));
    if let Some((start, len)) = crate::widgets::zoom_scrollbar(
        ui,
        bar.shrink2(vec2(0.0, 1.0)),
        ui.id().with("rolagem-h"),
        (view.view_start.0 as f64, visible.0 as f64),
        total.0 as f64,
        (MIN_TICKS_PER_PX * lanes, MAX_TICKS_PER_PX * lanes),
    ) {
        view.view_start = Tick(start as i64).max(Tick::ZERO);
        view.ticks_per_px = len / lanes;
    }
    // Botões de zoom.
    let zr = Rect::from_min_max(pos2(bar.right() + 2.0, l.hscroll.top()), l.hscroll.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(zr), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().interact_size = vec2(18.0, 14.0);
            if ui
                .small_button("−")
                .on_hover_text(tr("Zoom Out (↓)"))
                .clicked()
            {
                view.zoom_around(1.5, app.display_time());
            }
            if ui
                .small_button("+")
                .on_hover_text(tr("Zoom In (↑)"))
                .clicked()
            {
                view.zoom_around(1.0 / 1.5, app.display_time());
            }
            if ui
                .small_button("⇔")
                .on_hover_text(tr("Fit to Project"))
                .clicked()
            {
                view.fit(app.project().duration());
            }
        });
    });
    // Vertical.
    painter.rect_filled(l.vscroll, 0.0, theme::panel());
    let content = l.content_height.max(1.0);
    if content > l.lanes.height() {
        let f0 = view.scroll_y / content;
        let f1 = (view.scroll_y + l.lanes.height()) / content;
        let thumb = Rect::from_min_max(
            pos2(
                l.vscroll.left() + 2.0,
                l.vscroll.top() + l.vscroll.height() * f0,
            ),
            pos2(
                l.vscroll.right() - 2.0,
                l.vscroll.top() + l.vscroll.height() * f1.min(1.0),
            ),
        );
        let resp = ui.interact(l.vscroll, ui.id().with("rolagem-v"), Sense::drag());
        painter.rect_filled(
            thumb,
            4.0,
            if resp.dragged() {
                theme::knob()
            } else {
                theme::separator()
            },
        );
        if resp.dragged() {
            let max = (content - l.lanes.height() + 20.0).max(0.0);
            view.scroll_y = (view.scroll_y + resp.drag_delta().y / l.vscroll.height() * content)
                .clamp(0.0, max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::TimeRange;

    /// Typed into, the time boxes set the cursor or the time selection's
    /// ends and length, as VEGAS's: a time in the ruler's format, or a
    /// sign and a length from what the box shows; past the other end the
    /// selection keeps its length, the cursor going to its start when left
    /// out of it; text that names no time changes nothing.
    #[test]
    fn the_time_boxes_set_the_cursor_and_the_selection() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        // Frames' times, which the ruler's texts name exactly.
        let rate = app.frame_rate();
        let at = |seconds: i64| rate.snap_floor(Tick::from_seconds(seconds));
        let selection = |app: &BananaApp| app.project().loop_region.expect("a selection");
        // The demo's: 0 to 5 s.
        let five = Tick::from_seconds(5);
        assert_eq!(selection(&app), TimeRange::from_bounds(Tick::ZERO, five));

        let text = app.ruler_text(at(8));
        type_into(&mut app, TimeBox::Cursor, &text);
        assert_eq!(app.cursor, at(8));

        let text = app.ruler_text(at(1));
        type_into(&mut app, TimeBox::Start, &text);
        assert_eq!(selection(&app), TimeRange::from_bounds(at(1), five));
        assert_eq!(app.cursor, at(1), "left out of it: to its start");

        let two = crate::timefmt::length_text(app.project(), at(2));
        type_into(&mut app, TimeBox::Length, &format!("+{two}"));
        assert_eq!(selection(&app), TimeRange::from_bounds(at(1), five + at(2)));
        type_into(&mut app, TimeBox::End, &format!("-{two}"));
        assert_eq!(selection(&app), TimeRange::from_bounds(at(1), five));

        // Its start typed past its end: as long, from there.
        let text = app.ruler_text(at(6));
        type_into(&mut app, TimeBox::Start, &text);
        let long = five - at(1);
        assert_eq!(selection(&app), TimeRange::from_bounds(at(6), at(6) + long));

        let before = selection(&app);
        type_into(&mut app, TimeBox::End, "soon");
        assert_eq!(selection(&app), before);
    }
}
