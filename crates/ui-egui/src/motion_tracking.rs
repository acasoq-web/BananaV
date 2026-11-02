//! The Motion Tracking window (Tools › Video › Motion Tracking, Alt+M; an
//! event's menu and its More button), as VEGAS 2026's Help shows it: the
//! event tracked, its regions (Mask: add, remove), Track outside Region,
//! Precision, Mode, Show Motion Vectors, the Motion tracked bar (green
//! where frames were tracked) with the last run's status, and the track
//! buttons — backward to the event's start, a frame back, both ways, a
//! frame on, forward to its end — with the motion transfer menu: the
//! event itself or one beside it in time, its effects (or a new Bézier
//! Masking), their location parameters, which get the region's middle at
//! each frame tracked as keyframes. The region is drawn on the Video
//! Preview while the window is open, its corners and its inside dragged.
//! Its import menu (shown when the event has a Picture In Picture or a
//! Bézier Masking) makes a region following that effect's picture or mask.

use bv_core::Tick;
use bv_engine::track::{Mode, Precision};
use bv_timeline::{ClipId, MotionTrack, PluginFx, TrackKind, motion_tracks::Quad};

use crate::app::BananaApp;
use crate::egui::{self, Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::widgets::{icon_button, icon_button_ex};

/// The window's state between frames.
#[derive(Debug, Default)]
pub(crate) struct MotionWindow {
    pub open: bool,
    /// The event tracked (picked when the window opened).
    pub clip: Option<ClipId>,
    /// Its region picked (its place among its motion tracks).
    pub region: usize,
    pub precision: Precision,
    pub mode: Mode,
    pub show_vectors: bool,
    /// Tracking going on, and the undo it adds to.
    pub running: Option<egui::Id>,
    /// What the last tracking ended with.
    pub status: Option<String>,
    runs: u64,
    vectors: Vec<([f32; 2], [f32; 2])>,
    /// A drag on the preview: a corner (0–3) or the whole region (4), the
    /// pointer then (0–1 of the frame) and the region then.
    drag: Option<(usize, [f32; 2], Quad)>,
}

/// The location parameter names of BananaV's Bézier Masking's first mask.
const BEZIER_ID: &str = "org.bananav.BezierMasking";
/// BananaV's Picture In Picture.
const PIP_ID: &str = "org.bananav.PictureInPicture";

impl BananaApp {
    /// Tools › Video › Motion Tracking: the window on the picked video
    /// event (on `clip` when given).
    pub(crate) fn open_motion_tracking(&mut self, clip: Option<ClipId>) {
        let clip = clip.or_else(|| self.first_selected(TrackKind::Video));
        self.motion.clip = clip;
        self.motion.region = 0;
        self.motion.status = None;
        self.motion.vectors.clear();
        self.motion.open = true;
    }

    /// Each frame: the regions tracking found, into the event's region
    /// (one undo for a run), and how it ended.
    pub(crate) fn follow_motion_tracking(&mut self) {
        let Some(key) = self.motion.running else {
            return;
        };
        let Some(t) = self.engine.tracking() else {
            self.motion.running = None;
            return;
        };
        let (Some(clip), region) = (self.motion.clip, self.motion.region) else {
            return;
        };
        if !t.keys.is_empty() {
            let frame = self.frame_rate().frame_duration();
            self.modify_merged(key, |p| {
                if let Some(track) = p
                    .clip_mut(clip)
                    .and_then(|c| c.motion_tracks.get_mut(region))
                {
                    for (at, quad) in &t.keys {
                        track.set_key(*at, *quad, frame);
                    }
                }
            });
        }
        self.motion.vectors = t.vectors;
        if t.finished {
            self.motion.running = None;
            self.motion.status = Some(match t.lost {
                Some(l) => tr(l.text()).to_string(),
                None => tr("Successful").to_string(),
            });
        }
    }

    /// Tracks the picked region from the cursor as `direction` says.
    fn start_tracking(&mut self, direction: bv_engine::TrackDirection, one_frame: bool) {
        let Some(clip) = self
            .motion
            .clip
            .and_then(|c| self.project().clip(c))
            .cloned()
        else {
            return;
        };
        let Some(track) = clip.motion_tracks.get(self.motion.region).cloned() else {
            return;
        };
        let rate = self.frame_rate();
        let local = (rate.frame_start(rate.frame_at(self.cursor)) - clip.start)
            .clamp(Tick::ZERO, (clip.duration - Tick(1)).max(Tick::ZERO));
        let quad = track.quad_at(local);
        self.motion.runs += 1;
        let key = egui::Id::new(("rastreamento", self.motion.runs));
        // The frame it starts on keeps the region as it is there.
        let region = self.motion.region;
        let frame = rate.frame_duration();
        self.modify_merged(key, |p| {
            if let Some(t) = p
                .clip_mut(clip.id)
                .and_then(|c| c.motion_tracks.get_mut(region))
            {
                t.set_key(local, quad, frame);
            }
        });
        let started = self.engine.start_tracking(bv_engine::TrackRequest {
            clip: clip.id,
            from: local,
            direction,
            one_frame,
            quad,
            outside: track.outside,
            mode: self.motion.mode,
            precision: self.motion.precision,
        });
        if started {
            self.motion.running = Some(key);
            self.motion.status = Some(tr("Tracking…").to_string());
        }
    }

    /// The window, while open.
    pub(crate) fn motion_tracking_window(&mut self, ctx: &egui::Context) {
        if !self.motion.open {
            return;
        }
        let mut open = true;
        crate::floating::Window::new(tr("Motion Tracking"))
            .id(egui::Id::new("rastreamento-de-movimento"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(270.0);
                window_ui(ui, self);
            });
        if !open {
            self.motion.open = false;
            self.engine.cancel_tracking();
            self.motion.running = None;
        }
    }
}

/// An event's name: its active take's.
fn event_name(app: &BananaApp, clip: ClipId) -> String {
    let (names, i) = crate::takes::take_names(app, clip);
    names.get(i).cloned().unwrap_or_default()
}

fn window_ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    ui.spacing_mut().item_spacing.y = 6.0;
    let clip = app.motion.clip.and_then(|c| app.project().clip(c)).cloned();
    let Some(clip) = clip else {
        ui.label(tr("Pick a video event to track the motion in it."));
        return;
    };
    ui.label(egui::RichText::new(event_name(app, clip.id)).strong());
    let tracks = clip.motion_tracks.clone();
    if app.motion.region >= tracks.len() {
        app.motion.region = tracks.len().saturating_sub(1);
    }
    let running = app.motion.running.is_some();
    egui::Grid::new("rastreamento-grade")
        .num_columns(2)
        .spacing(vec2(10.0, 6.0))
        .show(ui, |ui| {
            ui.label(tr("Mask:"));
            ui.horizontal(|ui| {
                let picked = tracks
                    .get(app.motion.region)
                    .map_or(String::new(), |t| region_name(&t.name));
                egui::ComboBox::from_id_salt("rastreamento-regiao")
                    .width(110.0)
                    .selected_text(picked)
                    .show_ui(ui, |ui| {
                        for (i, t) in tracks.iter().enumerate() {
                            ui.selectable_value(&mut app.motion.region, i, region_name(&t.name));
                        }
                    });
                // A new region, in the middle of the frame.
                if icon_button(ui, Icon::Plus, tr("Add a region to track")).clicked() {
                    let id = clip.id;
                    let n = app.modify(|p| {
                        let c = p.clip_mut(id)?;
                        let name = bv_timeline::motion_tracks::next_region_name(&c.motion_tracks);
                        c.motion_tracks.push(MotionTrack::new(name));
                        Some(c.motion_tracks.len() - 1)
                    });
                    if let Some(n) = n {
                        app.motion.region = n;
                    }
                }
                let can_remove = !tracks.is_empty() && !running;
                if icon_button_ex(ui, Icon::Delete, tr("Remove the region"), false, can_remove)
                    .clicked()
                {
                    let (id, i) = (clip.id, app.motion.region);
                    app.modify(|p| {
                        if let Some(c) = p.clip_mut(id)
                            && i < c.motion_tracks.len()
                        {
                            c.motion_tracks.remove(i);
                        }
                    });
                }
                // VEGAS's import menu, when the event has an effect to
                // import from.
                let sources = import_sources(&clip);
                if !sources.is_empty() {
                    let menu = icon_button_ex(
                        ui,
                        Icon::Import,
                        tr("Import motion from an effect"),
                        false,
                        !running,
                    );
                    egui::Popup::menu(&menu).show(|ui| {
                        for (label, index, mask) in &sources {
                            if crate::menus::item(ui, label) {
                                app.import_motion(clip.id, *index, *mask);
                            }
                        }
                    });
                }
            });
            ui.end_row();
            ui.label("");
            let mut outside = tracks.get(app.motion.region).is_some_and(|t| t.outside);
            let enabled = !tracks.is_empty();
            if ui
                .add_enabled(
                    enabled,
                    egui::Checkbox::new(&mut outside, tr("Track outside Region")),
                )
                .changed()
            {
                let (id, i) = (clip.id, app.motion.region);
                app.modify(|p| {
                    if let Some(t) = p.clip_mut(id).and_then(|c| c.motion_tracks.get_mut(i)) {
                        t.outside = outside;
                    }
                });
            }
            ui.end_row();
            ui.label(tr("Precision:"));
            egui::ComboBox::from_id_salt("rastreamento-precisao")
                .width(170.0)
                .selected_text(tr(app.motion.precision.name()))
                .show_ui(ui, |ui| {
                    for p in Precision::ALL {
                        ui.selectable_value(&mut app.motion.precision, p, tr(p.name()));
                    }
                });
            ui.end_row();
            ui.label(tr("Mode:"));
            egui::ComboBox::from_id_salt("rastreamento-modo")
                .width(170.0)
                .selected_text(tr(app.motion.mode.name()))
                .show_ui(ui, |ui| {
                    for m in Mode::ALL {
                        ui.selectable_value(&mut app.motion.mode, m, tr(m.name()));
                    }
                });
            ui.end_row();
            ui.label("");
            crate::widgets::checkbox(ui, &mut app.motion.show_vectors, tr("Show Motion Vectors"));
            ui.end_row();
        });
    // Motion tracked: the event's length, green where frames were tracked.
    ui.label(tr("Motion tracked:"));
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(bar, 1.0, crate::theme::panel_dark());
    if let Some(t) = tracks.get(app.motion.region) {
        let len = clip.duration.0.max(1) as f32;
        let x = |at: Tick| bar.left() + bar.width() * (at.0 as f32 / len).clamp(0.0, 1.0);
        let frame = app.frame_rate().frame_duration();
        for (a, b) in t.tracked_spans(frame) {
            let r = Rect::from_min_max(
                pos2(x(a), bar.top()),
                pos2(x(b + frame).max(x(a) + 1.0), bar.bottom()),
            );
            painter.rect_filled(r, 0.0, Color32::from_rgb(40, 200, 60));
        }
        let c = x(app.cursor - clip.start);
        painter.vline(c, bar.y_range(), Stroke::new(1.0, Color32::WHITE));
    }
    let status = app.motion.status.clone().unwrap_or_default();
    ui.label(egui::RichText::new(status).color(crate::theme::text_dim()));
    // The track buttons and the motion transfer menu.
    use bv_engine::TrackDirection::*;
    let can = !tracks.is_empty();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let buttons = [
            (Icon::GoStart, tr("Track backward"), Backward, false),
            (
                Icon::PrevFrame,
                tr("Track one frame backward"),
                Backward,
                true,
            ),
            (Icon::Swap, tr("Track both directions"), Both, false),
            (
                Icon::NextFrame,
                tr("Track one frame forward"),
                Forward,
                true,
            ),
            (Icon::GoEnd, tr("Track forward"), Forward, false),
        ];
        for (icon, tip, dir, one) in buttons {
            if icon_button_ex(ui, icon, tip, false, can && !running).clicked() {
                app.start_tracking(dir, one);
            }
        }
        if running && ui.button(tr("Cancel")).clicked() {
            app.engine.cancel_tracking();
            app.motion.running = None;
            app.motion.status = Some(tr("Canceled").to_string());
        }
        let has_keys = tracks
            .get(app.motion.region)
            .is_some_and(|t| !t.keys.is_empty());
        ui.add_enabled_ui(has_keys && !running, |ui| {
            ui.menu_button(egui::RichText::new("⏷"), |ui| {
                transfer_menu(ui, app, &clip)
            })
            .response
            .on_hover_text(tr("Transfer the motion to an effect"));
        });
    });
}

/// "Region 1" in the interface's language.
fn region_name(name: &str) -> String {
    match name.strip_prefix("Region ") {
        Some(n) if n.parse::<u32>().is_ok() => trf!("Region {}", n),
        _ => name.to_string(),
    }
}

/// A position parameter of a plug-in: its name and label, and whether it
/// counts 0–1 of the frame.
fn location_params(fx: &PluginFx) -> Vec<(String, String, bool)> {
    let Some(info) = bv_ofx::registry()
        .known()
        .into_iter()
        .find(|i| i.identifier == fx.id)
    else {
        return Vec::new();
    };
    let own = bv_engine::plugins::is_own(&info);
    let mut out: Vec<(String, String, bool)> = Vec::new();
    for p in info.described.iter().flat_map(|d| &d.params) {
        if crate::ofx::is_position(p) && out.iter().all(|(n, ..)| *n != p.name) {
            let label = if p.label.is_empty() {
                &p.name
            } else {
                &p.label
            };
            out.push((
                p.name.clone(),
                crate::ofx::say(own, label),
                p.double_type.contains("Normalised"),
            ));
        }
    }
    out
}

/// What the import menu offers: each Picture In Picture of the event, and
/// each of its Bézier Maskings' masks turned on — their names, the
/// effect's place in the chain and the mask (1–5).
fn import_sources(clip: &bv_timeline::Clip) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    for (k, e) in clip.effects.iter().enumerate() {
        let Some(fx) = &e.plugin else {
            continue;
        };
        let name = crate::ofx::fx_name(e);
        if fx.id == PIP_ID {
            out.push((name, k, 0));
        } else if fx.id == BEZIER_ID {
            for n in 1..=5 {
                let on = fx
                    .params
                    .get(&format!("enable{n}"))
                    .and_then(|p| match &p.value {
                        bv_timeline::PluginValue::Num(v) => v.first().map(|&v| v != 0.0),
                        _ => None,
                    })
                    .unwrap_or(n == 1);
                if on {
                    out.push((trf!("{}: Mask {}", name, n), k, n));
                }
            }
        }
    }
    out
}

/// Effect `fx`'s values at `at` (from its event's start), its plug-in's
/// defaults where it has none of its own.
pub(crate) fn values_at(
    fx: &PluginFx,
    at: Tick,
    rate: bv_core::FrameRate,
) -> Vec<(String, bv_ofx::Value)> {
    let Some(info) = bv_ofx::registry()
        .known()
        .into_iter()
        .find(|i| i.identifier == fx.id)
    else {
        return Vec::new();
    };
    let mut out: Vec<(String, bv_ofx::Value)> = Vec::new();
    for p in info.described.iter().flat_map(|d| &d.params) {
        if !p.ty.has_value() || out.iter().any(|(n, _)| *n == p.name) {
            continue;
        }
        let v = match fx.params.get(&p.name) {
            Some(v) => bv_engine::plugins::animation(v, rate)
                .at(bv_engine::plugins::frames(at, rate), p.ty),
            None => p.default.clone(),
        };
        out.push((p.name.clone(), v));
    }
    out
}

/// The motion transfer menu: the events (the tracked one and those beside
/// it in time), their effects (and a new Bézier Masking), their location
/// parameters.
fn transfer_menu(ui: &mut egui::Ui, app: &mut BananaApp, clip: &bv_timeline::Clip) {
    let p = app.project();
    let span = clip.range();
    let mut events: Vec<ClipId> = vec![clip.id];
    for (t, c) in p.clips() {
        if t.kind == TrackKind::Video && c.id != clip.id && c.range().intersects(span) {
            events.push(c.id);
        }
    }
    for id in events {
        let name = event_name(app, id);
        crate::menus::sub(ui, &name, true, |ui| {
            let Some(target) = app.project().clip(id).cloned() else {
                return;
            };
            for (k, e) in target.effects.iter().enumerate() {
                let Some(fx) = &e.plugin else {
                    continue;
                };
                let params = location_params(fx);
                if params.is_empty() {
                    continue;
                }
                crate::menus::sub(ui, &crate::ofx::fx_name(e), true, |ui| {
                    for (param, label, normalised) in &params {
                        if ui.button(label).clicked() {
                            app.transfer_motion(id, k, param, *normalised);
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            if ui.button(tr("Add Bézier Masking")).clicked() {
                app.transfer_to_new_mask(id);
                ui.close();
            }
        });
    }
}

impl BananaApp {
    /// The picked region's middle at each frame tracked, as keyframes of
    /// parameter `param` of effect `index` of event `target` (its times
    /// from that event's start).
    fn transfer_motion(&mut self, target: ClipId, index: usize, param: &str, normalised: bool) {
        let Some((track, start)) = self.motion_source() else {
            return;
        };
        let Some(t) = self.project().clip(target).map(|c| (c.start, c.duration)) else {
            return;
        };
        let size = (
            self.project().settings.width,
            self.project().settings.height,
        );
        let Some(keys) = track.location_keys(t.0 - start, t.1, normalised, size) else {
            return;
        };
        let param = param.to_string();
        self.modify(|p| {
            if let Some(fx) = p
                .clip_mut(target)
                .and_then(|c| c.effects.get_mut(index))
                .and_then(|e| e.plugin.as_deref_mut())
            {
                fx.params.insert(param, keys);
            }
        });
    }

    /// A new Bézier Masking on event `target`, its first mask the region's
    /// size, following the region's middle.
    fn transfer_to_new_mask(&mut self, target: ClipId) {
        let Some((track, start)) = self.motion_source() else {
            return;
        };
        let Some(info) = bv_ofx::registry()
            .known()
            .into_iter()
            .find(|i| i.identifier == BEZIER_ID)
        else {
            return;
        };
        let Some(t) = self.project().clip(target).map(|c| (c.start, c.duration)) else {
            return;
        };
        let size = (
            self.project().settings.width,
            self.project().settings.height,
        );
        let Some(keys) = track.location_keys(t.0 - start, t.1, true, size) else {
            return;
        };
        let q = track.keys.first().map_or(track.region, |k| k.quad);
        let (lo, hi) = q.iter().fold(([1f32, 1.0], [0f32, 0.0]), |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        });
        let mut fx = crate::ofx::plugin_fx(&info);
        let num = |v: f64| bv_timeline::PluginParam {
            value: bv_timeline::PluginValue::Num(vec![v]),
            keys: Vec::new(),
        };
        fx.params
            .insert("width1".into(), num((hi[0] - lo[0]) as f64));
        fx.params
            .insert("height1".into(), num((hi[1] - lo[1]) as f64));
        fx.params.insert("location1".into(), keys);
        self.modify(|p| {
            if let Some(c) = p.clip_mut(target) {
                c.effects.push(bv_timeline::Effect::plugin(fx));
            }
        });
    }

    /// A new region of event `clip` following its effect `index`'s picture
    /// (a Picture In Picture) or mask `mask` (a Bézier Masking): at each of
    /// the event's frames when the effect is animated, else drawn there.
    fn import_motion(&mut self, clip: ClipId, index: usize, mask: usize) {
        let p = self.project();
        let Some(c) = p.clip(clip) else {
            return;
        };
        let Some(fx) = c
            .effects
            .get(index)
            .and_then(|e| e.plugin.as_deref())
            .cloned()
        else {
            return;
        };
        let s = &p.settings;
        let aspect = s.width as f64 * f64::from(s.pixel_aspect.ratio()) / s.height.max(1) as f64;
        let rate = s.frame_rate;
        let region = |at: Tick| {
            bv_engine::Engine::tracked_region(&fx.id, &values_at(&fx, at, rate), aspect, mask)
                .map(|q| q.map(|c| c.map(|v| v as f32)))
        };
        let mut track = MotionTrack::new(bv_timeline::motion_tracks::next_region_name(
            &c.motion_tracks,
        ));
        if fx.params.values().any(|p| !p.keys.is_empty()) {
            let mut i = 0;
            loop {
                let at = rate.frame_start(i);
                if at >= c.duration || i > 1_000_000 {
                    break;
                }
                if let Some(quad) = region(at) {
                    track
                        .keys
                        .push(bv_timeline::motion_tracks::TrackKey { at, quad });
                }
                i += 1;
            }
            if track.keys.is_empty() {
                return;
            }
        } else {
            match region(Tick::ZERO) {
                Some(q) => track.region = q,
                None => return,
            }
        }
        let n = self.modify(|p| {
            let c = p.clip_mut(clip)?;
            c.motion_tracks.push(track);
            Some(c.motion_tracks.len() - 1)
        });
        if let Some(n) = n {
            self.motion.region = n;
        }
    }

    /// The picked region and the start of the event it was tracked in.
    fn motion_source(&self) -> Option<(MotionTrack, Tick)> {
        let c = self.project().clip(self.motion.clip?)?;
        Some((c.motion_tracks.get(self.motion.region)?.clone(), c.start))
    }
}

/// The picked region on the Video Preview (`frame` is where the frame is
/// drawn, `area` the preview's picture area: the region shows and is
/// grabbed there too, past the frame's edges), its corners and inside
/// dragged; its motion vectors when asked.
pub(crate) fn overlay(ui: &mut egui::Ui, app: &mut BananaApp, frame: Rect, area: Rect) {
    let Some(clip) = app.motion.clip.and_then(|c| app.project().clip(c)).cloned() else {
        return;
    };
    let Some(track) = clip.motion_tracks.get(app.motion.region).cloned() else {
        return;
    };
    let local = app.display_time() - clip.start;
    if local < Tick::ZERO || local >= clip.duration {
        return;
    }
    let screen = |p: [f32; 2]| {
        pos2(
            frame.left() + p[0] * frame.width(),
            frame.top() + p[1] * frame.height(),
        )
    };
    let unit = |p: Pos2| {
        [
            (p.x - frame.left()) / frame.width().max(1.0),
            (p.y - frame.top()) / frame.height().max(1.0),
        ]
    };
    let quad = track.quad_at(local);
    let frame_len = app.frame_rate().frame_duration();
    let tracked = track.key_at(local, frame_len).is_some();
    let resp = ui.interact(
        area,
        ui.id().with("regiao-rastreada"),
        Sense::click_and_drag(),
    );
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    // A press: a corner, else inside the region.
    if resp.drag_started()
        && let Some(p) = pointer
    {
        let corner = quad.iter().position(|c| screen(*c).distance(p) <= 8.0);
        let inside = bv_engine::track::inside(&quad, unit(p));
        app.motion.drag = match (corner, inside) {
            (Some(k), _) => Some((k, unit(p), quad)),
            (None, true) => Some((4, unit(p), quad)),
            _ => None,
        };
    }
    if resp.dragged()
        && let (Some((k, from, start)), Some(p)) = (app.motion.drag, pointer)
        && app.motion.running.is_none()
    {
        let at = unit(p);
        let d = [at[0] - from[0], at[1] - from[1]];
        let mut q = start;
        for (i, c) in q.iter_mut().enumerate() {
            if k == 4 || k == i {
                *c = [c[0] + d[0], c[1] + d[1]];
            }
        }
        let (id, region) = (clip.id, app.motion.region);
        let key = egui::Id::new(("regiao-arrastada", id, region));
        app.modify_merged(key, |p| {
            if let Some(t) = p.clip_mut(id).and_then(|c| c.motion_tracks.get_mut(region)) {
                t.set_region_at(local, q, frame_len);
            }
        });
    }
    if resp.drag_stopped() {
        app.motion.drag = None;
    }
    // The region: green on a frame tracked, else white; its corners.
    let painter = ui.painter().with_clip_rect(area);
    let color = if tracked {
        Color32::from_rgb(60, 220, 80)
    } else {
        Color32::WHITE
    };
    let pts: Vec<Pos2> = quad.iter().map(|c| screen(*c)).collect();
    let mut closed = pts.clone();
    closed.push(pts[0]);
    painter.add(egui::Shape::line(
        closed.clone(),
        Stroke::new(3.0, Color32::from_black_alpha(140)),
    ));
    painter.add(egui::Shape::line(closed, Stroke::new(1.5, color)));
    for c in &pts {
        let r = Rect::from_center_size(*c, vec2(7.0, 7.0));
        painter.rect_filled(r, 0.0, color);
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, Color32::BLACK),
            egui::StrokeKind::Outside,
        );
    }
    if app.motion.show_vectors {
        for (a, b) in &app.motion.vectors {
            let (a, b) = (screen(*a), screen(*b));
            painter.line_segment([a, b], Stroke::new(1.0, Color32::from_rgb(250, 210, 40)));
            painter.circle_filled(b, 1.5, Color32::from_rgb(250, 210, 40));
        }
    }
}

impl BananaApp {
    /// The demo's first video event with a region tracked over its first
    /// thirty frames (moving to the right), the Motion Tracking window
    /// open on it, the cursor among them and the motion vectors shown
    /// (screen tests).
    pub fn motion_tracking_for_review(&mut self) -> bool {
        let Some((id, start)) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .and_then(|t| t.clips.first())
            .map(|c| (c.id, c.start))
        else {
            return false;
        };
        let frame = self.frame_rate().frame_duration();
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                let mut t = MotionTrack::new("Region 1".into());
                for n in 0..30 {
                    let dx = n as f32 * 0.004;
                    let quad = bv_timeline::motion_tracks::NEW_REGION.map(|q| [q[0] + dx, q[1]]);
                    t.set_key(Tick(frame.0 * n), quad, frame);
                }
                c.motion_tracks.push(t);
            }
        });
        self.select_clip(id, false);
        self.open_motion_tracking(Some(id));
        self.motion.show_vectors = true;
        self.motion.status = Some(tr("Successful").to_string());
        self.motion.vectors = (0..12)
            .map(|i| {
                let x = 0.44 + (i % 4) as f32 * 0.06;
                let y = 0.4 + (i / 4) as f32 * 0.08;
                ([x, y], [x + 0.004, y])
            })
            .collect();
        self.seek(start + Tick(frame.0 * 20));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Picture In Picture's picture comes in as a region: its corners
    /// where the effect puts them, and at each frame when it moves.
    #[test]
    fn a_region_comes_from_a_picture_in_picture() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let Some(info) = bv_ofx::registry()
            .known()
            .into_iter()
            .find(|i| i.identifier == PIP_ID)
        else {
            panic!("Picture In Picture is registered");
        };
        let clip = app
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .and_then(|t| t.clips.first())
            .map(|c| c.id)
            .expect("a video event");
        let mut fx = crate::ofx::plugin_fx(&info);
        let num = |v: &[f64]| bv_timeline::PluginParam {
            value: bv_timeline::PluginValue::Num(v.to_vec()),
            keys: Vec::new(),
        };
        fx.params.insert("location".into(), num(&[0.5, 0.5]));
        fx.params.insert("scaleX".into(), num(&[0.5]));
        let index = app.modify(|p| {
            let c = p.clip_mut(clip)?;
            c.effects.push(bv_timeline::Effect::plugin(fx));
            Some(c.effects.len() - 1)
        });
        let index = index.expect("the effect added");
        let c = app.project().clip(clip).unwrap().clone();
        assert_eq!(import_sources(&c).len(), 1);
        app.import_motion(clip, index, 0);
        let c = app.project().clip(clip).unwrap();
        let region = c.motion_tracks.last().expect("a region");
        assert!(region.keys.is_empty(), "the picture stands still");
        let q = region.region;
        assert!(
            (q[0][0] - 0.25).abs() < 1e-4 && (q[0][1] - 0.25).abs() < 1e-4,
            "{q:?}"
        );
        assert!(
            (q[2][0] - 0.75).abs() < 1e-4 && (q[2][1] - 0.75).abs() < 1e-4,
            "{q:?}"
        );
    }
}
