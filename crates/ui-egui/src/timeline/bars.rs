//! Barras acima das faixas: região de loop, régua e marcadores/regiões,
//! e a caixa de timecode no canto.

use crate::i18n::tr;
use bv_core::{FrameRate, Tick, TimeRange};
use bv_timeline::{MarkerId, Project};

use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::egui::{self, Align2, CursorIcon, FontId, Rect, Sense, Shape, Stroke, pos2};
use crate::theme;

pub(crate) enum BarDrag {
    LoopCreate {
        anchor: Tick,
    },
    LoopMove {
        grab: Tick,
        len: Tick,
    },
    LoopStart,
    LoopEnd,
    Scrub,
    Marker {
        id: MarkerId,
        region_end: bool,
    },
    /// A tag of the CD layout bar (`whole`: Alt moves the whole track);
    /// `grab`: how far from its time it was taken.
    Cd {
        tag: super::marker_bars::CdTag,
        whole: bool,
        grab: Tick,
    },
    /// A command marker's flag.
    Command {
        id: MarkerId,
        grab: Tick,
    },
    /// The Marker Tool's picked tags, together.
    Picked,
}

/// Marcador ou borda de região sob o ponteiro na barra de marcadores.
fn marker_at(view: &TimelineView, l: &Layout, p: &Project, x: f32) -> Option<(MarkerId, bool)> {
    let near = |t: Tick| (view.x_of(l, t) - x).abs() <= 6.0;
    p.markers
        .iter()
        .find(|m| near(m.time) || (x > view.x_of(l, m.time) && x < view.x_of(l, m.time) + 19.0))
        .map(|m| (m.id, false))
        .or_else(|| {
            p.regions.iter().find_map(|r| {
                if near(r.range.start) {
                    Some((r.id, false))
                } else if near(r.range.end()) {
                    Some((r.id, true))
                } else {
                    None
                }
            })
        })
}

/// The loop region a double-click on its bar leaves: the whole project,
/// or nothing when it was the whole project already.
fn whole_or_none(project: &Project) -> Option<TimeRange> {
    let all = TimeRange::new(Tick::ZERO, project.duration());
    (all.duration > Tick::ZERO && project.loop_region != Some(all)).then_some(all)
}

pub(crate) fn interact(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
) {
    let project = app.project().clone();
    let pointer = ui.input(|i| i.pointer.latest_pos());

    // The time display's menu: what it shows.
    let tc = ui.interact(l.tc_box, ui.id().with("caixa-tempo-menu"), Sense::click());
    tc.context_menu(|ui| {
        use crate::sync::TimeDisplay;
        for (mode, label) in [
            (TimeDisplay::Cursor, tr("Time at Cursor")),
            (TimeDisplay::MtcIn, tr("MIDI Timecode In")),
            (TimeDisplay::MtcOut, tr("MIDI Timecode Out")),
            (TimeDisplay::ClockOut, tr("MIDI Clock Out")),
        ] {
            if crate::widgets::radio(ui, app.time_display == mode, label).clicked() {
                app.time_display = mode;
                ui.close();
            }
        }
        ui.separator();
        // The ruler's formats, as Options › Ruler Time Format.
        crate::menus::sub(ui, tr("Time Format"), true, |ui| {
            use crate::timefmt::RulerFormatExt;
            for r in crate::timefmt::RulerFormat::ALL {
                if crate::widgets::radio(ui, app.effective_ruler_format() == r, r.label()).clicked()
                {
                    app.modify(|p| p.settings.ruler_format = Some(r));
                    ui.close();
                }
            }
        });
        // Colors of their own, for the interface type in use.
        let key = format!("{:?}", theme::interface());
        let colors = app
            .settings
            .time_display_colors
            .get(&key)
            .copied()
            .unwrap_or_default();
        for (background, label, now) in [
            (false, tr("Text Color"), colors.text),
            (true, tr("Background Color"), colors.background),
        ] {
            crate::menus::sub(ui, label, true, |ui| {
                if crate::widgets::radio(ui, now.is_none(), tr("Default")).clicked() {
                    let c = app
                        .settings
                        .time_display_colors
                        .entry(key.clone())
                        .or_default();
                    if background {
                        c.background = None;
                    } else {
                        c.text = None;
                    }
                    let _ = app.settings.save();
                    ui.close();
                }
                if crate::widgets::radio(ui, now.is_some(), tr("Custom…")).clicked() {
                    let start = now.unwrap_or(if background {
                        theme::panel_dark().to_array()
                    } else {
                        theme::text().to_array()
                    });
                    app.time_display_color = Some((
                        background,
                        crate::color_picker::ColorPicker::new(label, start),
                    ));
                    ui.close();
                }
            });
        }
    });

    // Barra de loop.
    let lb = ui.interact(
        l.loop_bar,
        ui.id().with("barra-loop"),
        Sense::click_and_drag(),
    );
    let region = project.loop_region;
    let edge_hit = |x: f32, t: Tick| (view.x_of(l, t) - x).abs() <= 5.0;
    if lb.hovered()
        && let (Some(p), Some(r)) = (pointer, region)
    {
        if edge_hit(p.x, r.start) || edge_hit(p.x, r.end()) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        } else if p.x > view.x_of(l, r.start) && p.x < view.x_of(l, r.end()) {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
    if lb.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
    {
        let t = view.time_at(l, o.x).max(Tick::ZERO);
        view.bar_drag = Some(match region {
            Some(r) if edge_hit(o.x, r.start) => BarDrag::LoopStart,
            Some(r) if edge_hit(o.x, r.end()) => BarDrag::LoopEnd,
            Some(r) if r.contains(t) => BarDrag::LoopMove {
                grab: t - r.start,
                len: r.duration,
            },
            _ => {
                let anchor = view.snap(app, &project, t, &[Tick::ZERO], &[]);
                BarDrag::LoopCreate { anchor }
            }
        });
    }
    if lb.dragged()
        && let Some(p) = pointer
        && view.bar_drag.is_some()
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let Some(drag) = &view.bar_drag else {
            return;
        };
        let t = view.time_at(l, x).max(Tick::ZERO);
        let new = match *drag {
            BarDrag::LoopCreate { anchor } => {
                let t = view.snap(app, &project, t, &[Tick::ZERO], &[]);
                Some(TimeRange::from_bounds(anchor.min(t), anchor.max(t)))
            }
            BarDrag::LoopMove { grab, len } => {
                let s = view.snap(
                    app,
                    &project,
                    (t - grab).max(Tick::ZERO),
                    &[Tick::ZERO, len],
                    &[],
                );
                Some(TimeRange::new(s, len))
            }
            BarDrag::LoopStart => region.map(|r| {
                let s = view.snap(app, &project, t, &[Tick::ZERO], &[]).min(r.end());
                TimeRange::from_bounds(s, r.end())
            }),
            BarDrag::LoopEnd => region.map(|r| {
                let e = view.snap(app, &project, t, &[Tick::ZERO], &[]).max(r.start);
                TimeRange::from_bounds(r.start, e)
            }),
            _ => None,
        };
        if let Some(r) = new {
            let r = (r.duration > Tick::ZERO).then_some(r);
            app.modify_merged(lb.id, |p| p.loop_region = r);
        }
    }
    if lb.drag_stopped() {
        view.bar_drag = None;
        view.snap_line = None;
        app.restart_if_playing();
    }
    if lb.double_clicked() {
        app.set_loop_region(whole_or_none(&project));
    }
    lb.on_hover_text(tr(
        "Drag to create the loop region · double-click: the whole project, again: none",
    ));

    // Ruler: a click or a drag moves the cursor, and playback with it.
    // Dragging holds playback; it goes on from where the mouse is let go,
    // like VEGAS's. While playing, each needle's head can be grabbed on its
    // own: the playback position's triangle, the edit cursor's flag.
    let ruler = ui.interact(l.ruler, ui.id().with("regua"), Sense::click_and_drag());
    if ruler.hovered()
        && let Some(p) = pointer
        && view.needle_head(app, l, p.x).is_some()
    {
        ui.ctx()
            .set_cursor_icon(crate::egui::CursorIcon::ResizeHorizontal);
    }
    // From the press on, as VEGAS's: the needle comes to the mouse at once
    // and follows every move (egui's drag starts only some pixels away,
    // which left it behind, then made it jump). A click is a press let go
    // where it was: both needles go there, playback going on from there.
    let held = ruler.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
    if held && view.needle.is_none() {
        let which = ui
            .input(|i| i.pointer.press_origin())
            .and_then(|o| view.needle_head(app, l, o.x))
            .unwrap_or(super::Needle::Both);
        view.needle = Some(which);
        if which != super::Needle::Cursor {
            app.hold_playback();
        }
    }
    if held && let Some(p) = pointer {
        // Onto event edges and markers, like VEGAS's (Shift: free); at the
        // tracks' edge the view runs on.
        let x = view.edge_scroll_x(ui, l, p.x);
        let t = view.snap_cursor(app, &project, view.time_at(l, x).max(Tick::ZERO));
        match view.needle {
            Some(super::Needle::Cursor) => app.seek(t),
            Some(super::Needle::Playhead) => app.move_playhead(t),
            _ => {
                app.seek(t);
                app.move_playhead(t);
            }
        }
        view.bar_drag = Some(BarDrag::Scrub);
    }
    if !held && view.needle.is_some() && matches!(view.bar_drag, Some(BarDrag::Scrub)) {
        app.release_playback();
        view.needle = None;
        view.bar_drag = None;
        view.snap_line = None;
    }
    // Its shortcut menu, as VEGAS's: the formats and the ruler's times.
    ruler.context_menu(|ui| {
        use crate::commands::Cmd;
        for r in crate::timefmt::RulerFormat::ALL {
            crate::menus::cmd(ui, app, Cmd::Ruler(r));
        }
        ui.separator();
        crate::menus::cmd(ui, app, Cmd::SetTimeAtCursor);
        crate::menus::cmd(ui, app, Cmd::SetProjectTime);
        crate::menus::cmd(ui, app, Cmd::SetProjectTempo);
    });

    // Barra de marcadores.
    let mb = ui.interact(
        l.marker_bar,
        ui.id().with("barra-marcadores"),
        Sense::click_and_drag(),
    );
    if mb.hovered()
        && let Some(p) = pointer
        && marker_at(view, l, &project, p.x).is_some()
    {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if mb.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && let Some((id, region_end)) = marker_at(view, l, &project, o.x)
    {
        if view.marker_tool.active {
            // The Marker Tool moves its picked tags together.
            let at = view.time_at(l, o.x);
            super::marker_tool::start_drag(
                view,
                &project,
                super::marker_tool::MarkerBar::Markers,
                id,
                at,
            );
        } else {
            view.bar_drag = Some(BarDrag::Marker { id, region_end });
        }
    }
    if mb.dragged()
        && let Some(p) = pointer
        && matches!(view.bar_drag, Some(BarDrag::Picked))
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let now = view.time_at(l, x);
        super::marker_tool::drag(view, app, &project, now, mb.id);
    }
    if mb.dragged()
        && let Some(p) = pointer
        && let Some(BarDrag::Marker { id, region_end }) = view.bar_drag
    {
        let x = view.edge_scroll_x(ui, l, p.x);
        let t = view.snap(
            app,
            &project,
            view.time_at(l, x).max(Tick::ZERO),
            &[Tick::ZERO],
            &[],
        );
        app.modify_merged(mb.id, |p| {
            if let Some(r) = p.regions.iter_mut().find(|r| r.id == id) {
                r.range = if region_end {
                    TimeRange::from_bounds(r.range.start, t.max(r.range.start))
                } else {
                    TimeRange::from_bounds(t.min(r.range.end()), r.range.end())
                };
            } else {
                p.move_marker(id, t);
            }
        });
    }
    if mb.drag_stopped() {
        super::marker_tool::end_drag(view);
        view.bar_drag = None;
        view.snap_line = None;
    }
    if let Some(p) = pointer {
        let hit = marker_at(view, l, &project, p.x);
        if mb.clicked() {
            super::marker_bars::pick(
                view,
                ui,
                &project,
                super::marker_tool::MarkerBar::Markers,
                hit.map(|h| h.0),
            );
        }
        if mb.clicked()
            && let Some((id, end)) = hit
        {
            let t = project
                .markers
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.time)
                .or_else(|| {
                    project
                        .regions
                        .iter()
                        .find(|r| r.id == id)
                        .map(|r| if end { r.range.end() } else { r.range.start })
                });
            if let Some(t) = t {
                app.seek(t);
            }
        }
        if mb.double_clicked() {
            match hit {
                Some((id, _)) => {
                    let name = project
                        .markers
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.name.clone())
                        .or_else(|| {
                            project
                                .regions
                                .iter()
                                .find(|r| r.id == id)
                                .map(|r| r.name.clone())
                        })
                        .unwrap_or_default();
                    app.dialogs.rename = Some((id, name));
                }
                None => {
                    let t = project
                        .settings
                        .frame_rate
                        .snap_floor(view.time_at(l, p.x).max(Tick::ZERO));
                    app.modify(|p| p.add_marker(t));
                }
            }
        }
        if mb.secondary_clicked() {
            view.context =
                super::lanes::ContextTarget::Marker(hit.map(|h| h.0), view.time_at(l, p.x));
        }
    }
    mb.on_hover_text(tr(
        "Markers (M) and regions (R) · double-click creates/renames · drag to move",
    ))
    .context_menu(|ui| {
        if let super::lanes::ContextTarget::Marker(hit, t) = view.context {
            match hit {
                Some(id) => {
                    if ui.button(tr("Rename…")).clicked() {
                        let name = project
                            .markers
                            .iter()
                            .find(|m| m.id == id)
                            .map(|m| m.name.clone())
                            .or_else(|| {
                                project
                                    .regions
                                    .iter()
                                    .find(|r| r.id == id)
                                    .map(|r| r.name.clone())
                            })
                            .unwrap_or_default();
                        app.dialogs.rename = Some((id, name));
                        ui.close();
                    }
                    if ui.button(tr("Delete")).clicked() {
                        app.remove_marker(id);
                        ui.close();
                    }
                }
                None => {
                    if ui.button(tr("Insert Marker Here")).clicked() {
                        let t = project.settings.frame_rate.snap_floor(t.max(Tick::ZERO));
                        app.modify(|p| p.add_marker(t));
                        ui.close();
                    }
                }
            }
        } else {
            ui.close();
        }
    });
}

/// Passo das marcas da régua em quadros (maior, menor).
pub(crate) fn ruler_step(view: &TimelineView, rate: FrameRate) -> (i64, i64) {
    let base = rate.timecode_base() as i64;
    let candidates = [
        1,
        2,
        5,
        10,
        base,
        2 * base,
        5 * base,
        10 * base,
        15 * base,
        30 * base,
        60 * base,
        120 * base,
        300 * base,
        600 * base,
        1800 * base,
        3600 * base,
    ];
    let frame_px = rate.frame_duration().0 as f64 / view.ticks_per_px;
    let major = candidates
        .iter()
        .copied()
        .find(|&c| c as f64 * frame_px >= 95.0)
        .unwrap_or(3600 * base);
    let minor = match major {
        m if m <= 2 => 1,
        m if m == base => (base / 6).max(1),
        m => m / 5,
    };
    (major, minor.max(1))
}

/// Steps of a Measures & Beats ruler, in beats (major, minor): majors a
/// whole number of measures apart, minors on beats when they fit.
pub(crate) fn beat_steps(view: &TimelineView, tempo: &bv_timeline::Tempo) -> (i64, i64) {
    let per_measure = tempo.beats_per_measure.max(1) as i64;
    let beat_px = tempo.beat().0 as f64 / view.ticks_per_px;
    let mut measures = 1i64;
    while (measures * per_measure) as f64 * beat_px < 95.0 && measures < 1 << 20 {
        measures *= 2;
    }
    let major = measures * per_measure;
    let minor = if beat_px >= 8.0 {
        1
    } else if per_measure as f64 * beat_px >= 8.0 {
        per_measure
    } else {
        (major / 4).max(1)
    };
    (major, minor)
}

pub(crate) fn paint(
    view: &TimelineView,
    ui: &egui::Ui,
    app: &BananaApp,
    l: &Layout,
    project: &Project,
    prerendered: &[TimeRange],
) {
    let painter = ui.painter();
    let rate = project.settings.frame_rate;

    // The time display, like VEGAS's: white, on the right, with the grip
    // dots on the left.
    let own = app
        .settings
        .time_display_colors
        .get(&format!("{:?}", theme::interface()))
        .copied()
        .unwrap_or_default();
    let rgba = |c: [u8; 4]| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    let tc_back = own.background.map_or_else(theme::panel_dark, rgba);
    let tc_text = own.text.map_or_else(theme::text, rgba);
    painter.rect_filled(l.tc_box, 0.0, tc_back);
    for i in 0..5 {
        painter.circle_filled(
            pos2(l.tc_box.left() + 6.0, l.tc_box.top() + 8.0 + i as f32 * 4.0),
            1.0,
            theme::text_dim(),
        );
    }
    // What it shows (its right-click menu): the time, or MIDI timecode
    // coming in or going out.
    use crate::sync::{MtcStatus, TimeDisplay, timecode_text};
    let shown = match app.time_display {
        TimeDisplay::Cursor
            if app.effective_ruler_format() == crate::timefmt::RulerFormat::AudioCd =>
        {
            crate::timefmt::cd_track_time(project, app.display_time())
                .unwrap_or_else(|| app.ruler_text(app.display_time()))
        }
        TimeDisplay::Cursor => app.ruler_text(app.display_time()),
        TimeDisplay::MtcIn => match &app.chase.status {
            _ if !app.trigger_mtc => tr("MIDI Timecode In").to_string(),
            Some(MtcStatus::Locked(tc)) => timecode_text(*tc, app.settings.prefs.mtc_input_rate),
            Some(MtcStatus::WrongFormat) => tr("Wrong format").to_string(),
            _ => tr("Listening…").to_string(),
        },
        TimeDisplay::MtcOut => app
            .midi
            .generator
            .as_ref()
            .and_then(|g| g.sent.lock().ok().and_then(|s| *s))
            .map_or_else(
                || tr("MIDI Timecode Out").to_string(),
                |(tc, rate)| timecode_text(tc, rate),
            ),
        // MIDI clock counts beats: where it is, as measures and beats.
        TimeDisplay::ClockOut if app.generate_clock => {
            use crate::timefmt::RulerFormatExt;
            crate::timefmt::RulerFormat::MeasuresBeats.format(app.display_time(), project)
        }
        TimeDisplay::ClockOut => tr("MIDI Clock Out").to_string(),
    };
    // As VEGAS's: smaller when the track list is narrow, none at the color
    // bars' width.
    let full = if app.time_display == TimeDisplay::Cursor {
        22.0
    } else {
        16.0
    };
    let room = l.tc_box.width() - 16.0;
    let fits = |size: f32| {
        painter
            .layout_no_wrap(shown.clone(), FontId::proportional(size), tc_text)
            .size()
            .x
            <= room
    };
    if let Some(size) = [full, 13.0, 11.0].into_iter().find(|&s| fits(s)) {
        crate::widgets::fit_text(
            painter,
            pos2(l.tc_box.right() - 8.0, l.tc_box.center().y),
            Align2::RIGHT_CENTER,
            shown,
            FontId::proportional(size),
            tc_text,
            room,
        );
    }
    ui.interact(l.tc_box, ui.id().with("caixa-tempo"), egui::Sense::hover())
        .on_hover_text(trf!(
            "frame {} · {}",
            rate.frame_at(app.display_time()),
            crate::fmt::fps(rate)
        ));

    // Região de loop.
    painter.rect_filled(l.loop_bar, 0.0, theme::panel_dark());
    if let Some(r) = project.loop_region {
        let x0 = view.x_of(l, r.start).max(l.loop_bar.left());
        let x1 = view.x_of(l, r.end()).min(l.loop_bar.right());
        if x1 > x0 {
            let color = if app.loop_enabled {
                theme::loop_bar()
            } else {
                theme::loop_bar().gamma_multiply(0.5)
            };
            let bar = Rect::from_min_max(
                pos2(x0, l.loop_bar.top() + 1.0),
                pos2(x1, l.loop_bar.bottom() - 1.0),
            );
            painter.rect_filled(bar, 2.0, color);
            // Sombra da região sobre as faixas.
            let lanes = painter.with_clip_rect(l.lanes);
            lanes.rect_filled(
                Rect::from_min_max(pos2(x0, l.lanes.top()), pos2(x1, l.lanes.bottom())),
                0.0,
                theme::loop_bar().gamma_multiply(if app.loop_enabled { 0.10 } else { 0.05 }),
            );
        }
    }
    // Tools › Selectively Prerender Video: a bar at the top over each part
    // that plays from its file.
    for r in prerendered {
        let x0 = view.x_of(l, r.start).max(l.loop_bar.left());
        let x1 = view.x_of(l, r.end()).min(l.loop_bar.right());
        if x1 > x0 {
            let bar =
                Rect::from_min_max(pos2(x0, l.loop_bar.top()), pos2(x1, l.loop_bar.top() + 3.0));
            painter.rect_filled(bar, 0.0, theme::prerendered());
        }
    }

    // Régua.
    painter.rect_filled(l.ruler, 0.0, theme::panel());
    let rp = painter.with_clip_rect(l.ruler);
    let end = view.time_at(l, l.ruler.right());
    if app.effective_ruler_format() == crate::timefmt::RulerFormat::MeasuresBeats {
        // Marks on beats and measures of the project tempo, counted from
        // the ruler's start time.
        let tempo = project.settings.tempo;
        let (major, minor) = beat_steps(view, &tempo);
        let beat = tempo.beat().0 as f64;
        let offset = project.ruler_offset;
        let at = |k: i64| Tick((k as f64 * beat).round() as i64) - offset;
        let first = ((view.view_start + offset).0 as f64 / beat).floor() as i64;
        let mut k = first.div_euclid(minor) * minor;
        while at(k) <= end {
            let t = at(k);
            let x = view.x_of(l, t);
            let is_major = k.rem_euclid(major) == 0;
            let h = if is_major { 9.0 } else { 4.0 };
            rp.line_segment(
                [pos2(x, l.ruler.bottom() - h), pos2(x, l.ruler.bottom())],
                Stroke::new(1.0, theme::text_dim()),
            );
            if is_major {
                rp.text(
                    pos2(x + 3.0, l.ruler.top() + 2.0),
                    Align2::LEFT_TOP,
                    app.ruler_text(t),
                    FontId::monospace(10.5),
                    theme::text_dim(),
                );
            }
            k += minor;
        }
    }
    let rate = if app.effective_ruler_format() == crate::timefmt::RulerFormat::AudioCd {
        crate::timefmt::CD_FRAMES
    } else {
        rate
    };
    let (major, minor) = ruler_step(view, rate);
    let mut frame = rate.frame_at(view.view_start).div_euclid(minor) * minor;
    while app.effective_ruler_format() != crate::timefmt::RulerFormat::MeasuresBeats {
        let t = rate.frame_start(frame);
        if t > end {
            break;
        }
        let x = view.x_of(l, t);
        let is_major = frame % major == 0;
        let h = if is_major { 9.0 } else { 4.0 };
        rp.line_segment(
            [pos2(x, l.ruler.bottom() - h), pos2(x, l.ruler.bottom())],
            Stroke::new(1.0, theme::text_dim()),
        );
        if is_major {
            rp.text(
                pos2(x + 3.0, l.ruler.top() + 2.0),
                Align2::LEFT_TOP,
                app.ruler_text(t),
                FontId::monospace(10.5),
                theme::text_dim(),
            );
        }
        frame += minor;
    }

    // Markers and regions. Their lines cross the tracks always, or only
    // while snapping is on (Options › Preferences › General: "Always draw
    // marker lines").
    painter.rect_filled(l.marker_bar, 0.0, theme::panel_dark());
    let mp = painter.with_clip_rect(l.marker_bar);
    let lines = app.settings.prefs.marker_lines || app.snapping;
    let lanes = painter.with_clip_rect(if lines { l.lanes } else { Rect::NOTHING });
    // VEGAS's flags: a region's at both its ends, a marker's at it, each
    // with its number (a name that is one, else its place), a name that is
    // not one after it.
    use crate::marker_flag::{self as flag, Kind};
    let top = l.marker_bar.top();
    for (i, r) in project.regions.iter().enumerate() {
        let (x0, x1) = (view.x_of(l, r.range.start), view.x_of(l, r.range.end()));
        let picked = super::marker_tool::picked(view, super::marker_tool::MarkerBar::Markers, r.id);
        let (number, name) = flag_texts(&r.name, i);
        for (x, kind) in [(x0, Kind::RegionStart), (x1, Kind::RegionEnd)] {
            let f = flag::paint(&mp, x, top, &number, kind, flag::TIMELINE, picked);
            if kind == Kind::RegionStart {
                flag::paint_name(&mp, f, name, flag::TIMELINE, kind);
            }
            lanes.line_segment(
                [pos2(x, l.lanes.top()), pos2(x, l.lanes.bottom())],
                Stroke::new(1.0, flag::REGION.gamma_multiply(0.35)),
            );
        }
    }
    for (i, m) in project.markers.iter().enumerate() {
        let x = view.x_of(l, m.time);
        let picked = super::marker_tool::picked(view, super::marker_tool::MarkerBar::Markers, m.id);
        let (number, name) = flag_texts(&m.name, i);
        let f = flag::paint(&mp, x, top, &number, Kind::Marker, flag::TIMELINE, picked);
        flag::paint_name(&mp, f, name, flag::TIMELINE, Kind::Marker);
        lanes.add(Shape::dashed_line(
            &[pos2(x, l.lanes.top()), pos2(x, l.lanes.bottom())],
            Stroke::new(1.0, flag::MARKER.gamma_multiply(0.5)),
            4.0,
            4.0,
        ));
    }
}

/// A flag's number and the name shown after it: a name that is a number
/// is the flag's (VEGAS's numbered markers), else the flag takes the
/// marker's place (`index`, from 0) and the name follows it.
fn flag_texts(name: &str, index: usize) -> (String, &str) {
    let name = name.trim();
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        (name.to_string(), "")
    } else {
        ((index + 1).to_string(), name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_double_click_on_the_loop_bar_takes_the_whole_project_then_nothing() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let all = TimeRange::new(Tick::ZERO, app.project().duration());
        assert_ne!(app.project().loop_region, Some(all));
        assert_eq!(whole_or_none(app.project()), Some(all));
        app.set_loop_region(Some(all));
        assert_eq!(whole_or_none(app.project()), None, "again: none");
        assert_eq!(whole_or_none(&Project::default()), None, "nothing to take");
    }

    #[test]
    fn a_press_on_the_ruler_brings_the_needle_at_once() {
        use egui_kittest::Harness;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let mut h = Harness::builder()
            .with_size(crate::egui::vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let before = h.state().cursor_seconds();
        // The ruler, about 6.7 s in.
        let at = h.state().ruler_spot_for_review(6.7);
        h.event(crate::egui::Event::PointerMoved(at));
        h.event(crate::egui::Event::PointerButton {
            pos: at,
            button: crate::egui::PointerButton::Primary,
            pressed: true,
            modifiers: crate::egui::Modifiers::default(),
        });
        h.step();
        let pressed = h.state().cursor_seconds();
        assert!(
            (pressed - before).abs() > 0.5,
            "the press moved it: {before} to {pressed}"
        );
        // A small move follows at once (no drag threshold to pass).
        let next = at + crate::egui::vec2(3.0, 0.0);
        h.event(crate::egui::Event::PointerMoved(next));
        h.step();
        assert!(h.state().cursor_seconds() > pressed, "it followed");
        h.event(crate::egui::Event::PointerButton {
            pos: next,
            button: crate::egui::PointerButton::Primary,
            pressed: false,
            modifiers: crate::egui::Modifiers::default(),
        });
        h.step();
        assert!(h.state().timeline.needle.is_none(), "let go");
    }
}
