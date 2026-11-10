//! Master Bus: the master's fader, mute and peak meters, as VEGAS lays
//! them out.

use crate::i18n::tr;
use std::time::Instant;

use crate::app::BananaApp;
use crate::dock::Tab;
use crate::egui::{self, Color32, Rect, RichText, Sense, Stroke, pos2, vec2};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets::{icon_button, icon_button_ex, vertical_fader};

/// The levels a meter shows, falling smoothly, with their peaks held.
#[derive(Debug)]
pub struct MeterState {
    levels: Vec<f32>,
    holds: Vec<(f32, Instant)>,
    clipped: Vec<bool>,
    last: Instant,
}

impl Default for MeterState {
    fn default() -> MeterState {
        MeterState {
            levels: vec![-96.0; 2],
            holds: vec![(-96.0, Instant::now()); 2],
            clipped: vec![false; 2],
            last: Instant::now(),
        }
    }
}

fn to_db(x: f32) -> f32 {
    if x <= 1e-5 { -96.0 } else { 20.0 * x.log10() }
}

impl MeterState {
    /// The levels at the peaks heard since the last frame: they fall
    /// 24 dB a second, and peaks hold 1.5 s. No peaks, or none (the
    /// interface draws faster than the sound arrives), is no news.
    pub fn update(&mut self, peaks: Option<Vec<f32>>) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32();
        self.last = now;
        for l in &mut self.levels {
            *l = (*l - 24.0 * dt).max(-96.0);
        }
        if let Some(p) = peaks.filter(|p| !p.is_empty()) {
            let n = p.len();
            if self.levels.len() != n {
                self.levels = vec![-96.0; n];
                self.holds = vec![(-96.0, now); n];
                self.clipped = vec![false; n];
            }
            for (i, v) in p.iter().enumerate() {
                let db = to_db(*v);
                self.levels[i] = self.levels[i].max(db);
                if db >= self.holds[i].0 {
                    self.holds[i] = (db, now);
                }
                if *v >= 1.0 {
                    self.clipped[i] = true;
                }
            }
        }
        for (i, h) in self.holds.iter_mut().enumerate() {
            if now.duration_since(h.1).as_secs_f32() > 1.5 {
                *h = (self.levels[i], now);
            }
        }
    }

    /// How many channels it shows.
    pub fn channels(&self) -> usize {
        self.levels.len()
    }

    pub fn active(&self) -> bool {
        self.levels.iter().any(|&l| l > -90.0)
    }

    /// Level of channel `i`, in dB (−96 when silent).
    pub fn level(&self, i: usize) -> f32 {
        self.levels.get(i).copied().unwrap_or(-96.0)
    }

    /// Peak hold of channel `i`, in dB.
    pub fn hold(&self, i: usize) -> f32 {
        self.holds.get(i).map_or(-96.0, |h| h.0)
    }

    /// The meter's Reset Clip: the clip indicators off, the peaks held
    /// from now.
    pub fn reset_clip(&mut self) {
        let now = Instant::now();
        for (h, l) in self.holds.iter_mut().zip(&self.levels) {
            *h = (*l, now);
        }
        self.clipped.fill(false);
    }
}

/// The meters' width, the scale's between them, the room after the fader.
const METER_W: f32 = 10.0;
const SCALE_W: f32 = 24.0;
const GAP: f32 = 12.0;

/// The meters' range: 0 dB at the top, −60 at the bottom.
const FLOOR: f32 = -60.0;

/// The meters' colors down their height, as VEGAS paints them: red at the
/// top, amber, then green from −21 dB — dim where the sound does not
/// reach, lit up to the level.
const STOPS: [(f32, [u8; 3], [u8; 3]); 5] = [
    (0.0, [0x2B, 0x1C, 0x18], [0xE5, 0x3B, 0x2C]),
    (-9.0, [0x2B, 0x27, 0x18], [0xE3, 0xB8, 0x2A]),
    (-15.0, [0x2B, 0x27, 0x18], [0xE3, 0xB8, 0x2A]),
    (-21.0, [0x16, 0x29, 0x1C], [0x3D, 0xC2, 0x5A]),
    (-60.0, [0x16, 0x29, 0x1C], [0x3D, 0xC2, 0x5A]),
];

/// How far down the meter `db` is, 0–1.
fn depth(db: f32) -> f32 {
    depth_to(db, FLOOR)
}

/// How far down a meter reaching `floor` dB `db` is, 0–1.
fn depth_to(db: f32, floor: f32) -> f32 {
    (db / floor).clamp(0.0, 1.0)
}

/// Paints the part of a meter from `from_db` down to `to_db` with its
/// colors (`lit` or dim), as a vertical gradient.
fn gradient(painter: &egui::Painter, rect: Rect, from_db: f32, to_db: f32, lit: bool) {
    meter_gradient(painter, rect, from_db, to_db, lit, FLOOR);
}

/// [`gradient`] on a meter reaching down to `floor` dB (the track headers'
/// meters show less than the Master Bus's).
pub(crate) fn meter_gradient(
    painter: &egui::Painter,
    rect: Rect,
    from_db: f32,
    to_db: f32,
    lit: bool,
    floor: f32,
) {
    let y = |db: f32| rect.top() + depth_to(db, floor) * rect.height();
    let color = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
    let at = |db: f32| -> Color32 {
        // The color at `db`, between its two stops.
        let pick = |(_, dim, bright): (f32, [u8; 3], [u8; 3])| if lit { bright } else { dim };
        for w in STOPS.windows(2) {
            let (hi, lo) = (w[0], w[1]);
            if db <= hi.0 && db >= lo.0 {
                let t = (hi.0 - db) / (hi.0 - lo.0);
                let (a, b) = (pick(hi), pick(lo));
                let mix = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
                return color([mix(0), mix(1), mix(2)]);
            }
        }
        color(pick(STOPS[STOPS.len() - 1]))
    };
    // The stops within the part, its ends included.
    let mut marks = vec![from_db];
    marks.extend(
        STOPS
            .iter()
            .map(|s| s.0)
            .filter(|&d| d < from_db && d > to_db),
    );
    marks.push(to_db);
    let mut mesh = egui::Mesh::default();
    for w in marks.windows(2) {
        let (top, bottom) = (y(w[0]), y(w[1]));
        if bottom <= top {
            continue;
        }
        let i = mesh.vertices.len() as u32;
        for (p, c) in [
            (pos2(rect.left(), top), at(w[0])),
            (pos2(rect.right(), top), at(w[0])),
            (pos2(rect.right(), bottom), at(w[1])),
            (pos2(rect.left(), bottom), at(w[1])),
        ] {
            mesh.colored_vertex(p, c);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The pointer over a meter whose click clears its clip lights and peaks:
/// the hand, as VEGAS's meters.
pub(crate) fn reset_pointer(ui: &egui::Ui, meter: &egui::Response) {
    if meter.hovered() {
        crate::cursors::set(ui.ctx(), crate::cursors::Cursor::HandPoint);
    }
}

/// A meter as VEGAS's Master Bus draws it: framed, dim down its height,
/// lit from the bottom up to `db`; the peak held as a line.
fn vegas_meter(painter: &egui::Painter, rect: Rect, db: f32, hold: f32) {
    painter.rect_filled(rect.expand(1.0), 0.0, Color32::from_gray(0x14));
    gradient(painter, rect, 0.0, FLOOR, false);
    if db > FLOOR {
        gradient(painter, rect, db.min(0.0), FLOOR, true);
    }
    if hold > FLOOR {
        let y = rect.top() + depth(hold) * rect.height();
        painter.hline(
            rect.x_range(),
            y,
            Stroke::new(1.0, Color32::from_gray(0xE6)),
        );
    }
}

/// The scale between the meters: every 3 dB from 3 to 57, each with a dot
/// against both meters (numbers too close to the one above are left out).
fn vegas_scale(painter: &egui::Painter, rect: Rect) {
    let dot = Color32::from_gray(0x80);
    // On whole pixels, or the dots blur.
    let (left, right) = (rect.left().round(), rect.right().round() - 1.0);
    let mut last: Option<f32> = None;
    // A mesh: shapes this small would feather away.
    let mut dots = egui::Mesh::default();
    for k in (3..60).step_by(3) {
        let row = (rect.top() + depth(-(k as f32)) * rect.height()).round();
        for x in [left, right] {
            dots.add_colored_rect(Rect::from_min_size(pos2(x, row), vec2(1.0, 1.0)), dot);
        }
        let y = row + 0.5;
        if last.is_some_and(|l| y - l < 12.0) {
            continue;
        }
        last = Some(y);
        painter.text(
            pos2(rect.center().x, y),
            egui::Align2::CENTER_CENTER,
            k.to_string(),
            egui::FontId::proportional(10.5),
            Color32::from_gray(0x80),
        );
    }
    painter.add(egui::Shape::mesh(dots));
}

/// The Master Bus, laid out as VEGAS's: its toolbar (properties, downmix,
/// dim, the Mixing Console), its name, FX, routing, mute and solo; the
/// fader beside the two meters with their scale between them; the lock
/// under the fader and each channel's peak under its meter (red once it
/// clipped; a click on the meters clears it).
/// The window: the Master's strip, and with View › Mixer Preview Fader
/// the preview bus's strip left of it, as VEGAS's Help shows them.
pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    if !app.settings.preview_fader {
        master_strip(ui, app);
        return;
    }
    let area = ui.available_rect_before_wrap();
    let half = (area.width() / 2.0).floor();
    let left = Rect::from_min_size(area.min, vec2(half - 1.0, area.height()));
    let right = Rect::from_min_max(pos2(area.left() + half + 1.0, area.top()), area.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
        preview_strip(ui, app)
    });
    ui.painter().vline(
        area.left() + half,
        area.y_range(),
        egui::Stroke::new(1.0, theme::separator()),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
        master_strip(ui, app)
    });
    ui.allocate_rect(area, Sense::hover());
}

/// The preview bus's strip: its name, its fader (the level previews play
/// at) and its meters.
pub(crate) fn preview_strip(ui: &mut egui::Ui, app: &mut BananaApp) {
    // The rows the Master's strip has above its fader, empty here.
    ui.horizontal(|ui| {
        ui.allocate_exact_size(vec2(1.0, 22.0), Sense::hover());
    });
    ui.horizontal(|ui| {
        ui.allocate_exact_size(vec2(4.0, 18.0), Sense::hover());
        ui.label(RichText::new(tr("Preview")).color(theme::text()));
    });
    ui.horizontal(|ui| {
        ui.allocate_exact_size(vec2(1.0, 22.0), Sense::hover());
    });
    ui.add_space(4.0);
    let bottom_h = 22.0;
    let area = ui.available_rect_before_wrap();
    let h = (area.height() - bottom_h - 4.0).max(80.0);
    let fader = Rect::from_min_size(pos2(area.left() + 2.0, area.top()), vec2(26.0, h));
    ui.scope_builder(egui::UiBuilder::new().max_rect(fader), |ui| {
        let mut db = app.settings.preview_gain_db;
        let r = vertical_fader(ui, &mut db, -60.0, 12.0, h);
        if r.changed() {
            let db = if db <= -59.5 { -96.0 } else { db };
            app.settings.preview_gain_db = db;
            app.engine.set_preview_gain(db);
        }
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            let _ = app.settings.save();
        }
        r.on_hover_text(trf!(
            "{} dB (double-click: 0 dB)",
            crate::fmt::signed(app.settings.preview_gain_db.max(-60.0) as f64, 1)
        ));
    });
    let left = (fader.right() + GAP).round();
    let top = (area.top() + 6.0).round();
    let bottom = (area.top() + h - 6.0).round();
    let lanes = [
        Rect::from_min_max(pos2(left, top), pos2(left + METER_W, bottom)),
        Rect::from_min_max(
            pos2(left + METER_W + SCALE_W, top),
            pos2(left + 2.0 * METER_W + SCALE_W, bottom),
        ),
    ];
    let meters = &mut app.previews.meters;
    let painter = ui.painter();
    for (i, lane) in lanes.iter().enumerate() {
        vegas_meter(painter, *lane, meters.level(i), meters.hold(i));
    }
    vegas_scale(
        painter,
        Rect::from_min_max(
            pos2(lanes[0].right() + 1.0, top),
            pos2(lanes[1].left() - 1.0, bottom),
        ),
    );
    let r = ui
        .interact(
            lanes[0].union(lanes[1]),
            ui.id().with("preview-clip-reset"),
            Sense::click(),
        )
        .on_hover_text(tr("Click to clear the clip lights"));
    reset_pointer(ui, &r);
    if r.clicked() {
        meters.clipped.iter_mut().for_each(|c| *c = false);
    }
    let row = Rect::from_min_size(
        pos2(area.left(), area.top() + h),
        vec2(area.width(), bottom_h),
    );
    for (i, lane) in lanes.iter().enumerate() {
        let hold = meters.hold(i);
        let text = if hold <= -90.0 {
            "−∞".to_string()
        } else {
            crate::fmt::num(hold as f64, 1)
        };
        let clipped = meters.clipped.get(i).copied().unwrap_or(false);
        ui.painter().text(
            pos2(lane.center().x, row.center().y),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(11.0),
            if clipped {
                theme::error()
            } else {
                theme::text()
            },
        );
    }
    ui.allocate_rect(area, Sense::hover());
}

/// The Master's strip: its buttons, fader and meters.
pub(crate) fn master_strip(ui: &mut egui::Ui, app: &mut BananaApp) {
    let master = app.project().master;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if icon_button(ui, Icon::Settings, tr("Master Bus Properties")).clicked() {
            app.open_project_audio_settings();
        }
        // Playback only: what it is folded down to, and 20 dB down.
        let tip = match bv_engine::Engine::downmix() {
            bv_engine::Downmix::Off => tr("Downmix Output"),
            bv_engine::Downmix::Stereo => tr("Downmix Output: stereo"),
            bv_engine::Downmix::Mono => tr("Downmix Output: mono"),
        };
        let on = crate::commands::Cmd::Downmix.checked(app).unwrap_or(false);
        if crate::widgets::icon_toggle(ui, Icon::Downmix, on, tip).clicked() {
            crate::commands::Cmd::Downmix.run(app);
        }
        let dim = bv_engine::Engine::dim();
        if crate::widgets::icon_toggle(ui, Icon::Dim, dim, tr("Dim Output")).clicked() {
            crate::commands::Cmd::Dim.run(app);
        }
        if icon_button(ui, Icon::MixingConsole, tr("Mixing Console")).clicked() {
            app.show_tab(Tab::MixingConsole, false);
        }
    });
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        icons::paint(ui.painter(), r.shrink(1.0), Icon::BusSelect, theme::text());
        ui.label(RichText::new(tr("Master")).color(theme::text()));
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // Its effects: the Plug-In Chooser while it has none; a right-click
        // puts them after its volume.
        let has_fx = !app.project().master_fx.is_empty();
        let fx = crate::widgets::icon_toggle(ui, Icon::Fx, has_fx, tr("Master Bus FX"));
        if fx.clicked() {
            app.clip_windows.fx_target = Some(crate::fx_window::FxTarget::Master);
        }
        fx.context_menu(|ui| {
            let mut post = app.project().master_fx_post_fader;
            if crate::widgets::checkbox(ui, &mut post, tr("Post Volume")).clicked() {
                app.modify(|p| p.master_fx_post_fader = post);
                ui.close();
            }
        });
        // The Audio Device Selector: where the Master plays on the output
        // device (playback only) — its pair, or in 5.1 each of its three.
        let routing = icon_button(ui, Icon::Routing, tr("Audio Device Selector"));
        egui::Popup::menu(&routing).show(|ui| device_menu(ui, app));
        let muted = master.muted;
        if letter(ui, "M", muted, true, tr("Mute the master")).clicked() {
            app.modify(|p| p.master.muted = !muted);
        }
        letter(ui, "S", false, false, tr("Coming soon to BananaV"));
    });
    // 5.1: the Surround Master's channel faders (trims), locked together
    // unless Lock Fader is off.
    let surround = app.project().settings.channels == bv_core::ChannelLayout::Surround51;
    if surround {
        surround_faders(ui, app);
    }
    // The fader and the meters, down to the lock and the peaks.
    ui.add_space(4.0);
    let bottom_h = 22.0;
    let area = ui.available_rect_before_wrap();
    let h = (area.height() - bottom_h - 4.0).max(80.0);
    let fader = Rect::from_min_size(pos2(area.left() + 2.0, area.top()), vec2(26.0, h));
    ui.scope_builder(egui::UiBuilder::new().max_rect(fader), |ui| {
        let mut db = master.gain_db;
        let r = vertical_fader(ui, &mut db, -60.0, 12.0, h);
        if r.changed() {
            let key = r.id;
            let db = if db <= -59.5 { -96.0 } else { db };
            app.modify_merged(key, |p| p.master.gain_db = db);
        }
        // Its volume before the effects (Pre FX) or after them (Post FX).
        let post_fader = app.project().master_fx_post_fader;
        r.context_menu(|ui| {
            for (label, post) in [(tr("Pre FX"), true), (tr("Post FX"), false)] {
                if crate::widgets::radio(ui, post_fader == post, label).clicked() {
                    app.modify(|p| p.master_fx_post_fader = post);
                    ui.close();
                }
            }
        });
        r.on_hover_text(trf!(
            "{} dB (double-click: 0 dB)",
            crate::fmt::signed(master.gain_db.max(-60.0) as f64, 1)
        ));
    });
    // Meter, scale, meter, right of the fader (5.1: the scale, then a
    // meter for each channel).
    let left = (fader.right() + if surround { GAP / 3.0 } else { GAP }).round();
    let top = (area.top() + 6.0).round();
    let bottom = (area.top() + h - 6.0).round();
    let lanes: Vec<Rect> = if surround {
        let w = 4.0;
        (0..6)
            .map(|i| {
                let x = left + SCALE_W + i as f32 * (w + 1.0);
                Rect::from_min_max(pos2(x, top), pos2(x + w, bottom))
            })
            .collect()
    } else {
        vec![
            Rect::from_min_max(pos2(left, top), pos2(left + METER_W, bottom)),
            Rect::from_min_max(
                pos2(left + METER_W + SCALE_W, top),
                pos2(left + 2.0 * METER_W + SCALE_W, bottom),
            ),
        ]
    };
    let painter = ui.painter();
    for (i, lane) in lanes.iter().enumerate() {
        vegas_meter(painter, *lane, app.meters.level(i), app.meters.hold(i));
    }
    let scale = if surround {
        Rect::from_min_max(pos2(left, top), pos2(lanes[0].left() - 1.0, bottom))
    } else {
        Rect::from_min_max(
            pos2(lanes[0].right() + 1.0, top),
            pos2(lanes[1].left() - 1.0, bottom),
        )
    };
    vegas_scale(painter, scale);
    let meters = lanes.iter().fold(lanes[0], |all, lane| all.union(*lane));
    let r = ui
        .interact(meters, ui.id().with("clip-reset"), Sense::click())
        .on_hover_text(tr("Click to clear the clip lights"));
    reset_pointer(ui, &r);
    if r.clicked() {
        app.meters.clipped.iter_mut().for_each(|c| *c = false);
    }
    // The lock under the fader, each channel's peak under its meter.
    let row = Rect::from_min_size(
        pos2(area.left(), area.top() + h),
        vec2(area.width(), bottom_h),
    );
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(Rect::from_center_size(
            pos2(fader.center().x, row.center().y),
            vec2(22.0, 22.0),
        )),
        |ui| {
            if surround {
                // The channel faders together, or one by one.
                let locked = !app.project().master.unlocked;
                if crate::widgets::icon_toggle(ui, Icon::Lock, locked, tr("Lock Fader")).clicked() {
                    app.modify(|p| p.master.unlocked = locked);
                }
            } else {
                icon_button_ex(ui, Icon::Lock, tr("Lock Fader"), false, false)
                    .on_disabled_hover_text(tr("Coming soon to BananaV"));
            }
        },
    );
    // Each channel's peak under its meter (5.1: the meters too narrow).
    for (i, lane) in lanes.iter().enumerate().filter(|_| !surround) {
        let hold = app.meters.hold(i);
        let text = if hold <= -90.0 {
            "−∞".to_string()
        } else {
            crate::fmt::num(hold as f64, 1)
        };
        let clipped = app.meters.clipped.get(i).copied().unwrap_or(false);
        ui.painter().text(
            pos2(lane.center().x, row.center().y),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(11.0),
            if clipped {
                theme::error()
            } else {
                theme::text()
            },
        );
    }
    ui.allocate_rect(area, Sense::hover());
}

/// The Audio Device Selector's menu: the output device's pairs of channels
/// the Master plays on (playback only) — its pair, or in 5.1 each of its
/// three.
pub(crate) fn device_menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    let now = app.project().master.device_pairs;
    let n = app
        .engine
        .hardware_outputs()
        .max(now.iter().copied().max().unwrap_or(0) + 1);
    let names: Vec<String> = (0..n)
        .map(|o| crate::i18n::output_name(app.project(), bv_timeline::Output::Hardware(o)))
        .collect();
    let surround = app.project().settings.channels == bv_core::ChannelLayout::Surround51;
    let mut picked = None;
    let list = |ui: &mut egui::Ui, k: usize, picked: &mut Option<(usize, u8)>| {
        for (o, name) in names.iter().enumerate() {
            let on = usize::from(now[k]) == o;
            if crate::widgets::radio(ui, on, name.as_str()).clicked() {
                *picked = Some((k, o as u8));
                ui.close();
            }
        }
    };
    if surround {
        for (k, label) in [
            (0, tr("Front (L/R)")),
            (1, tr("Center and LFE")),
            (2, tr("Rear (Ls/Rs)")),
        ] {
            ui.menu_button(label, |ui| list(ui, k, &mut picked));
        }
    } else {
        list(ui, 0, &mut picked);
    }
    if let Some((k, o)) = picked {
        app.modify(|p| p.master.device_pairs[k] = o);
    }
}

/// The Surround Master's channel faders: L, R, C, LFE, Ls and Rs, each a
/// trim in dB (−∞ at the bottom); locked, one moves them all as far.
pub(crate) fn surround_faders(ui: &mut egui::Ui, app: &mut BananaApp) {
    let master = app.project().master;
    ui.add_space(4.0);
    ui.label(RichText::new(tr("Surround Master")).color(theme::text_dim()));
    // Thin faders, so the six fit the docked window: a name, a rail and
    // a knob each, −60 to +12 dB.
    const W: f32 = 15.0;
    const H: f32 = 76.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (c, name) in ["L", "R", "C", "LFE", "Ls", "Rs"].into_iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(W, H), Sense::click_and_drag());
            let rail = Rect::from_min_max(
                pos2(rect.center().x - 1.0, rect.top() + 18.0),
                pos2(rect.center().x + 1.0, rect.bottom() - 4.0),
            );
            let db = master.channel_db[c].max(-60.0);
            let y = rail.bottom() - (db + 60.0) / 72.0 * rail.height();
            let painter = ui.painter();
            painter.text(
                pos2(rect.center().x, rect.top() + 6.0),
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::proportional(9.0),
                theme::text(),
            );
            painter.rect_filled(rail, 1.0, theme::panel_dark());
            let knob = Rect::from_center_size(pos2(rect.center().x, y), vec2(W - 3.0, 6.0));
            let lit = resp.hovered() || resp.dragged();
            painter.rect_filled(knob, 1.0, if lit { theme::banana() } else { theme::text() });
            let mut new = None;
            if resp.dragged()
                && let Some(p) = resp.interact_pointer_pos()
            {
                let t = ((rail.bottom() - p.y) / rail.height()).clamp(0.0, 1.0);
                new = Some(t * 72.0 - 60.0);
            }
            if resp.double_clicked() {
                new = Some(0.0);
            }
            if let Some(to) = new {
                let to = if to <= -59.5 { -96.0 } else { to };
                let moved = to.max(-60.0) - db;
                let locked = !master.unlocked;
                app.modify_merged(resp.id.with(locked), |p| {
                    if locked {
                        for d in &mut p.master.channel_db {
                            let v = (d.max(-60.0) + moved).min(12.0);
                            *d = if v <= -59.5 { -96.0 } else { v };
                        }
                    } else {
                        p.master.channel_db[c] = to;
                    }
                });
            }
            resp.on_hover_text(trf!(
                "{}: {} dB (double-click: 0 dB)",
                name,
                crate::fmt::signed(db as f64, 1)
            ));
        }
    });
}

impl BananaApp {
    /// The Master Bus's meters at these peaks (linear, one a channel), as
    /// if the project were playing (for screen tests).
    pub fn meters_for_review(&mut self, peaks: Vec<f32>) {
        self.meters.update(Some(peaks));
    }
}

/// "M" or "S": a letter lit when on.
fn letter(ui: &mut egui::Ui, text: &str, on: bool, enabled: bool, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(22.0, 24.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 2.0, theme::error());
    } else if resp.hovered() && enabled {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let color = match (on, enabled) {
        (true, _) => Color32::BLACK,
        (false, true) => theme::text(),
        (false, false) => theme::text_disabled(),
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(16.0),
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_without_new_peaks_keep_the_levels() {
        let mut m = MeterState::default();
        m.update(Some(vec![0.5, 0.25]));
        let (left, right) = (m.level(0), m.level(1));
        assert!(left > -7.0 && right > -13.0, "{left} {right}");
        // The interface drew again before the sound brought new peaks.
        for _ in 0..3 {
            m.update(Some(Vec::new()));
            m.update(None);
        }
        assert!(m.level(0) > left - 1.0, "{}", m.level(0));
        assert!(m.level(1) > right - 1.0, "{}", m.level(1));
        assert_eq!(m.hold(0), left);
    }
}
