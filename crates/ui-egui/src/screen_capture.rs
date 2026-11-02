//! File › Screen Capture (VEGAS's Vegas Capture, "Screen Recording with
//! Vegas Capture"): sources — displays, application windows, cameras,
//! inputs and what the computer plays — added as cards that show them
//! live, each with its own settings; Start Recording records them all at
//! once, each into a file of its own in the recording folder (`bv_engine`'s
//! `capture.rs`); afterwards Add to Project puts each on a track of its own
//! at the cursor, and Open Folder shows the files. The Settings: the
//! recording folder and the codecs and quality new sources start with.

use std::collections::HashMap;
use std::path::PathBuf;

use bv_engine::{
    Capture, CaptureAudioCodec, CaptureDefaults, CaptureQuality, CaptureSource, CaptureVideoCodec,
    Captured, SourceKind,
};
use bv_timeline::TrackKind;
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, Color32, RichText, vec2};
use crate::i18n::{tr, trc};
use crate::icons::Icon;
use crate::theme;

/// The Screen Capture's settings, kept between sessions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CaptureSettings {
    /// Where recordings go (`None`: the Videos folder's "BananaV Captures").
    pub folder: Option<PathBuf>,
    pub defaults: CaptureDefaults,
}

/// The recording folder.
pub(crate) fn capture_folder(s: &CaptureSettings) -> PathBuf {
    s.folder.clone().unwrap_or_else(|| {
        bv_platform::user_dir(bv_platform::UserDir::Videos)
            .or_else(bv_platform::home_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join("BananaV Captures")
    })
}

/// The Add Capture Sources dialog's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddTab {
    Video,
    Audio,
    Screen,
    Window,
}

/// The Add Capture Sources dialog: the sources found on its tab, and the
/// ones picked.
struct Adding {
    tab: AddTab,
    found: Vec<CaptureSource>,
    picked: Vec<bool>,
}

/// The Screen Capture window.
#[derive(Default)]
pub(crate) struct ScreenCaptureWindow {
    pub open: bool,
    capture: Option<Capture>,
    /// The card whose settings show under it.
    editing: Option<usize>,
    adding: Option<Adding>,
    settings: bool,
    /// The last recording's files.
    recorded: Vec<Captured>,
    /// Each card's live picture, and the number of the picture it shows.
    textures: HashMap<usize, (u64, egui::TextureHandle)>,
    /// Cameras' formats, by their link.
    modes: HashMap<String, Vec<bv_engine::CameraMode>>,
}

impl std::fmt::Debug for ScreenCaptureWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScreenCaptureWindow")
            .field("open", &self.open)
            .finish()
    }
}

impl BananaApp {
    /// File › Screen Capture.
    pub(crate) fn open_screen_capture(&mut self) {
        let w = &mut self.screen_capture;
        w.open = true;
        if w.capture.is_none() {
            w.capture = Some(Capture::new());
        }
    }

    pub(crate) fn screen_capture_window(&mut self, ctx: &egui::Context) {
        if !self.screen_capture.open {
            return;
        }
        let mut open = true;
        crate::floating::Window::new(tr("Screen Capture"))
            .id(egui::Id::new("screen-capture"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(760.0);
                window_ui(ui, self);
            });
        add_sources_dialog(ctx, self);
        settings_dialog(ctx, self);
        if self
            .screen_capture
            .capture
            .as_ref()
            .is_some_and(Capture::is_recording)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        if !open {
            let w = &mut self.screen_capture;
            w.open = false;
            w.adding = None;
            w.settings = false;
            w.textures.clear();
            if let Some(mut c) = w.capture.take() {
                let files = c.close();
                if !files.is_empty() {
                    w.recorded = files;
                }
            }
        }
    }

    /// Add to Project: each file recorded on a new track of its kind, named
    /// after its source, at the cursor (one undo step).
    fn add_captures_to_project(&mut self) {
        let files = self.screen_capture.recorded.clone();
        let at = self.cursor;
        self.as_one_undo(|app| {
            for f in &files {
                let Some(&m) = app.import_paths(std::slice::from_ref(&f.path)).first() else {
                    continue;
                };
                let kind = if f.video {
                    TrackKind::Video
                } else {
                    TrackKind::Audio
                };
                app.modify(|p| {
                    let t = p.insert_track_at(kind, p.tracks.len());
                    if let Some(track) = p.tracks.iter_mut().find(|x| x.id == t) {
                        track.name.clone_from(&f.name);
                    }
                    let (v, a) = if f.video {
                        (Some(t), None)
                    } else {
                        (None, Some(t))
                    };
                    let _ = p.insert_media(m, at, v, a);
                });
            }
        });
    }
}

/// A big button of the window's toolbar.
fn bar_button(ui: &mut egui::Ui, icon: Icon, text: &str, fill: Option<Color32>) -> egui::Response {
    let mut b =
        egui::Button::new(RichText::new(format!("  {text}")).size(13.0)).min_size(vec2(0.0, 30.0));
    if let Some(c) = fill {
        b = b.fill(c);
    }
    let r = ui.add(b);
    let at = egui::Rect::from_center_size(
        egui::pos2(r.rect.left() + 14.0, r.rect.center().y),
        vec2(12.0, 12.0),
    );
    crate::icons::paint(ui.painter(), at, icon, theme::text());
    r
}

/// "hh:mm:ss:ff" of a recording's time (frames at 30 a second).
fn clock(d: std::time::Duration) -> String {
    let s = d.as_secs();
    let f = (d.subsec_millis() * 30 / 1000) as u64;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, f)
}

fn window_ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let recording = app
        .screen_capture
        .capture
        .as_ref()
        .and_then(Capture::recording_time);
    let has_sources = app
        .screen_capture
        .capture
        .as_ref()
        .is_some_and(|c| !c.sources().is_empty());
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::icon_button(ui, Icon::Settings, tr("Settings")).clicked() {
                app.screen_capture.settings = true;
            }
        });
    });
    // The toolbar, centered.
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            let count = 2 + 2 * usize::from(!app.screen_capture.recorded.is_empty());
            let room = ui.available_width() - 170.0 * count as f32;
            ui.add_space((room / 2.0).max(0.0));
            ui.add_enabled_ui(recording.is_none(), |ui| {
                if bar_button(ui, Icon::Plus, tr("Add Capture Sources"), None).clicked() {
                    open_adding(app, AddTab::Screen);
                }
            });
            if has_sources {
                match recording {
                    None => {
                        if bar_button(ui, Icon::Record, tr("Start Recording"), None).clicked() {
                            start(app);
                        }
                    }
                    Some(t) => {
                        let stop = format!("{}  {}", tr("Stop"), clock(t));
                        if bar_button(ui, Icon::Stop, &stop, Some(Color32::from_rgb(232, 100, 50)))
                            .clicked()
                        {
                            stop_recording(app);
                        }
                    }
                }
            }
            if !app.screen_capture.recorded.is_empty() && recording.is_none() {
                if bar_button(ui, Icon::Import, tr("Add to Project"), None).clicked() {
                    app.add_captures_to_project();
                }
                if bar_button(ui, Icon::Open, tr("Open Folder"), None).clicked() {
                    let folder = capture_folder(&app.settings.capture);
                    let _ = std::process::Command::new("explorer").arg(folder).spawn();
                }
            }
        });
    });
    ui.add_space(12.0);
    if !has_sources {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), egui::Sense::hover());
            crate::icons::paint(ui.painter(), r, Icon::Capture, theme::text());
            ui.add_space(8.0);
            ui.label(RichText::new(tr("Screen Capture")).size(20.0).strong());
            ui.label(
                RichText::new(tr("Select capture sources to start."))
                    .size(16.0)
                    .color(theme::text_disabled()),
            );
            ui.add_space(24.0);
        });
        return;
    }
    cards(ui, app, recording.is_some());
}

/// The sources' cards, two a row.
fn cards(ui: &mut egui::Ui, app: &mut BananaApp, recording: bool) {
    let Some(capture) = app.screen_capture.capture.as_ref() else {
        return;
    };
    let mut sources = capture.sources().to_vec();
    let lives: Vec<bv_engine::SourceLive> = (0..sources.len()).map(|i| capture.live(i)).collect();
    let mut changed = false;
    let mut remove = None;
    let width = (ui.available_width() - 12.0) / 2.0;
    egui::ScrollArea::vertical()
        .max_height(560.0)
        .show(ui, |ui| {
            egui::Grid::new("screen-capture-cards")
                .num_columns(2)
                .spacing(vec2(12.0, 12.0))
                .show(ui, |ui| {
                    for (i, (s, live)) in sources.iter_mut().zip(&lives).enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            let edited = card(ui, app, i, s, live, width, recording, &mut remove);
                            changed |= edited;
                        });
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
        });
    if let Some(i) = remove {
        sources.remove(i);
        app.screen_capture.editing = None;
        app.screen_capture.textures.clear();
        changed = true;
    }
    if changed && let Some(c) = app.screen_capture.capture.as_mut() {
        c.set_sources(sources);
    }
}

/// One source's card: its live picture (or its levels), name, settings'
/// line, mute and remove; a click opens its settings under it. Whether
/// the source changed.
#[allow(clippy::too_many_arguments)]
fn card(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    i: usize,
    s: &mut CaptureSource,
    live: &bv_engine::SourceLive,
    width: f32,
    recording: bool,
    remove: &mut Option<usize>,
) -> bool {
    let mut changed = false;
    let video = s.kind.is_video();
    if video {
        let h = width * 9.0 / 16.0;
        let (r, _) = ui.allocate_exact_size(vec2(width, h), egui::Sense::hover());
        ui.painter().rect_filled(r, 4.0, Color32::from_gray(20));
        if let Some((pw, ph, px)) = &live.picture {
            let entry = app.screen_capture.textures.get(&i);
            if entry.is_none_or(|(serial, _)| *serial != live.serial) {
                let rgba: Vec<u8> = px
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[2], p[1], p[0], 255])
                    .collect();
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([*pw as usize, *ph as usize], &rgba);
                let tex = ui.ctx().load_texture(
                    format!("capture-{i}"),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                app.screen_capture.textures.insert(i, (live.serial, tex));
            }
            if let Some((_, tex)) = app.screen_capture.textures.get(&i) {
                let scale = (r.width() / *pw as f32).min(r.height() / *ph as f32);
                let at = egui::Rect::from_center_size(
                    r.center(),
                    vec2(*pw as f32 * scale, *ph as f32 * scale),
                );
                ui.painter().image(
                    tex.id(),
                    at,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    }
    let frame = egui::Frame::new()
        .fill(theme::panel_dark())
        .inner_margin(egui::Margin::same(10))
        .corner_radius(4.0);
    let resp = frame
        .show(ui, |ui| {
            ui.set_width(width - 20.0);
            if !video {
                levels(ui, &live.levels, width - 20.0);
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&s.name).strong());
                    ui.label(
                        RichText::new(s.summary(tr("Stereo"), tr("Mono")))
                            .color(theme::text_disabled()),
                    );
                    if let Some(e) = &live.error {
                        ui.label(RichText::new(e).color(theme::error()));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::widgets::icon_button_ex(
                        ui,
                        Icon::Delete,
                        tr("Remove from the capture list"),
                        false,
                        !recording,
                    )
                    .clicked()
                    {
                        *remove = Some(i);
                    }
                    let tip = if s.muted {
                        tr("Muted: not recorded")
                    } else {
                        tr("Mute")
                    };
                    let icon = if s.muted {
                        Icon::MuteAudio
                    } else {
                        Icon::Speaker
                    };
                    if crate::widgets::icon_button_ex(ui, icon, tip, s.muted, !recording).clicked()
                    {
                        s.muted = !s.muted;
                        changed = true;
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_text(tr("Open device settings"));
    if resp.clicked() && !recording {
        let e = &mut app.screen_capture.editing;
        *e = if *e == Some(i) { None } else { Some(i) };
    }
    if app.screen_capture.editing == Some(i) && !recording {
        changed |= device_settings(ui, app, s);
    }
    changed
}

/// A sound source's level meters, a bar a channel.
fn levels(ui: &mut egui::Ui, levels: &[f32], width: f32) {
    let rows = levels.len().max(2);
    for c in 0..rows {
        let v = levels
            .get(c)
            .or(levels.first())
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let (r, _) = ui.allocate_exact_size(vec2(width, 4.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 1.0, Color32::from_gray(60));
        // dB scale from -60.
        let db = 20.0 * v.max(1e-6).log10();
        let k = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let lit = egui::Rect::from_min_size(r.min, vec2(r.width() * k, r.height()));
        let color = if db > -6.0 {
            Color32::from_rgb(230, 120, 30)
        } else if db > -18.0 {
            Color32::from_rgb(200, 200, 40)
        } else {
            Color32::from_rgb(40, 170, 60)
        };
        ui.painter().rect_filled(lit, 1.0, color);
        ui.add_space(2.0);
    }
}

/// A combo box of `options` (their labels) for `value`; whether it
/// changed.
fn combo<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut T,
    options: &[(T, String)],
    width: f32,
) -> bool {
    let mut changed = false;
    let text = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or(String::new(), |(_, t)| t.clone());
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(text)
        .show_ui(ui, |ui| {
            for (v, t) in options {
                if ui.selectable_label(*v == *value, t).clicked() && *v != *value {
                    *value = *v;
                    changed = true;
                }
            }
        });
    changed
}

/// The device settings under a card: a video source's Resolution,
/// Framerate and Codec; a sound's Codec, Channels and Sample Rate.
fn device_settings(ui: &mut egui::Ui, app: &mut BananaApp, s: &mut CaptureSource) -> bool {
    let mut changed = false;
    let w = (ui.available_width() - 24.0) / 3.0;
    let frame = egui::Frame::new()
        .fill(theme::panel())
        .inner_margin(egui::Margin::same(8))
        .corner_radius(4.0);
    frame.show(ui, |ui| {
        egui::Grid::new(("capture-device", s.name.as_str()))
            .num_columns(3)
            .spacing(vec2(8.0, 4.0))
            .show(ui, |ui| {
                if s.kind.is_video() {
                    for t in [tr("Resolution"), tr("Framerate"), tr("Codec")] {
                        ui.vertical_centered(|ui| ui.label(t));
                    }
                    ui.end_row();
                    let sizes = video_sizes(app, s);
                    let mut size = (s.width, s.height);
                    let options: Vec<((u32, u32), String)> = sizes
                        .iter()
                        .map(|&(w, h)| ((w, h), format!("{w}×{h}")))
                        .collect();
                    if combo(ui, "capture-size", &mut size, &options, w) {
                        (s.width, s.height) = size;
                        changed = true;
                    }
                    let rates = video_rates(app, s);
                    let options: Vec<(bv_core::FrameRate, String)> =
                        rates.iter().map(|&r| (r, bv_engine::fps_text(r))).collect();
                    changed |= combo(ui, "capture-rate", &mut s.rate, &options, w);
                    let options: Vec<(CaptureVideoCodec, String)> = CaptureVideoCodec::ALL
                        .iter()
                        .map(|&c| (c, c.name().to_string()))
                        .collect();
                    changed |= combo(ui, "capture-vcodec", &mut s.video_codec, &options, w);
                } else {
                    for t in [tr("Codec"), tr("Channels"), tr("Sample Rate")] {
                        ui.vertical_centered(|ui| ui.label(t));
                    }
                    ui.end_row();
                    let options: Vec<(CaptureAudioCodec, String)> = CaptureAudioCodec::ALL
                        .iter()
                        .map(|&c| (c, c.name().to_string()))
                        .collect();
                    changed |= combo(ui, "capture-acodec", &mut s.audio_codec, &options, w);
                    let options = [
                        (1u16, tr("Mono").to_string()),
                        (2, tr("Stereo").to_string()),
                    ];
                    changed |= combo(ui, "capture-channels", &mut s.channels, &options, w);
                    let options: Vec<(u32, String)> = [44_100u32, 48_000, 96_000]
                        .iter()
                        .map(|&r| (r, format!("{r} Hz")))
                        .collect();
                    changed |= combo(ui, "capture-srate", &mut s.sample_rate, &options, w);
                }
                ui.end_row();
            });
    });
    changed
}

/// The sizes a video source offers: a camera's formats; a display's or a
/// window's own size and smaller ones of its shape.
fn video_sizes(app: &mut BananaApp, s: &CaptureSource) -> Vec<(u32, u32)> {
    if let SourceKind::Camera { link } = &s.kind {
        let modes = camera_modes(app, link);
        let mut v: Vec<(u32, u32)> = modes.iter().map(|m| (m.width, m.height)).collect();
        v.dedup();
        if !v.is_empty() {
            return v;
        }
    }
    let native = match &s.kind {
        SourceKind::Display { name } => bv_platform::displays()
            .into_iter()
            .find(|d| d.name == *name)
            .map(|d| (d.width, d.height)),
        _ => None,
    }
    .unwrap_or((s.width, s.height));
    let mut v: Vec<(u32, u32)> = [1.0, 0.75, 2.0 / 3.0, 0.5]
        .iter()
        .map(|k| {
            (
                ((native.0 as f64 * k) as u32 & !1).max(2),
                ((native.1 as f64 * k) as u32 & !1).max(2),
            )
        })
        .collect();
    if !v.contains(&(s.width, s.height)) {
        v.push((s.width, s.height));
    }
    v
}

/// The frame rates a video source offers: a camera's at its size; 15 to
/// 60 for the screen.
fn video_rates(app: &mut BananaApp, s: &CaptureSource) -> Vec<bv_core::FrameRate> {
    if let SourceKind::Camera { link } = &s.kind {
        let mut v: Vec<bv_core::FrameRate> = camera_modes(app, link)
            .iter()
            .filter(|m| (m.width, m.height) == (s.width, s.height))
            .map(|m| m.rate)
            .collect();
        v.dedup();
        if !v.is_empty() {
            return v;
        }
    }
    let mut v: Vec<bv_core::FrameRate> = [15, 24, 25, 30, 50, 60]
        .iter()
        .map(|&n| bv_core::FrameRate::new(n, 1))
        .collect();
    if !v.contains(&s.rate) {
        v.push(s.rate);
    }
    v
}

fn camera_modes(app: &mut BananaApp, link: &str) -> Vec<bv_engine::CameraMode> {
    app.screen_capture
        .modes
        .entry(link.to_string())
        .or_insert_with(|| bv_engine::camera_modes(link))
        .clone()
}

/// The sources of a tab of the Add Capture Sources dialog.
fn found_on(app: &BananaApp, tab: AddTab) -> Vec<CaptureSource> {
    let d = &app.settings.capture.defaults;
    match tab {
        AddTab::Video => bv_engine::camera_sources(d),
        AddTab::Audio => bv_engine::audio_sources(d, tr("Computer sound")),
        AddTab::Screen => bv_engine::display_sources(d, &|n| trf!("Screen {}", n)),
        AddTab::Window => bv_engine::window_sources(d),
    }
}

fn open_adding(app: &mut BananaApp, tab: AddTab) {
    let found = found_on(app, tab);
    app.screen_capture.adding = Some(Adding {
        tab,
        picked: vec![false; found.len()],
        found,
    });
}

/// Add Capture Sources: the tabs Video, Audio, Screen and Window, a tile
/// for each source found (click to pick), Add.
fn add_sources_dialog(ctx: &egui::Context, app: &mut BananaApp) {
    let Some(tab) = app.screen_capture.adding.as_ref().map(|a| a.tab) else {
        return;
    };
    let mut open = true;
    let mut switch = None;
    let mut add = false;
    crate::floating::Window::new(tr("Select Capture Sources"))
        .id(egui::Id::new("screen-capture-sources"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(520.0);
            ui.horizontal(|ui| {
                for (t, name) in [
                    (AddTab::Video, tr("Video")),
                    (AddTab::Audio, tr("Audio")),
                    (AddTab::Screen, tr("Screen")),
                    (AddTab::Window, tr("Window")),
                ] {
                    if crate::widgets::tab_button(ui, name, t == tab, true, 80.0).clicked()
                        && t != tab
                    {
                        switch = Some(t);
                    }
                }
            });
            ui.separator();
            let Some(adding) = app.screen_capture.adding.as_mut() else {
                return;
            };
            if adding.found.is_empty() {
                ui.label(RichText::new(tr("None found.")).color(theme::text_disabled()));
            }
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (s, picked) in adding.found.iter().zip(adding.picked.iter_mut()) {
                            let fill = if *picked {
                                theme::banana().gamma_multiply(0.5)
                            } else {
                                theme::panel_dark()
                            };
                            let b = egui::Button::new(RichText::new(fit(&s.name, 22)).size(12.0))
                                .fill(fill)
                                .min_size(vec2(160.0, 64.0));
                            if ui.add(b).on_hover_text(&s.name).clicked() {
                                *picked = !*picked;
                            }
                        }
                    });
                });
            ui.separator();
            ui.horizontal(|ui| {
                let any = adding.picked.iter().any(|&p| p);
                if ui.add_enabled(any, egui::Button::new(tr("Add"))).clicked() {
                    add = true;
                }
                if ui.button(tr("Refresh")).clicked() {
                    switch = Some(tab);
                }
            });
        });
    if add && let Some(adding) = app.screen_capture.adding.take() {
        let picked: Vec<CaptureSource> = adding
            .found
            .into_iter()
            .zip(adding.picked)
            .filter_map(|(s, p)| p.then_some(s))
            .collect();
        if let Some(c) = app.screen_capture.capture.as_mut() {
            let mut sources = c.sources().to_vec();
            sources.extend(picked);
            c.set_sources(sources);
        }
        return;
    }
    if let Some(t) = switch {
        open_adding(app, t);
    }
    if !open {
        app.screen_capture.adding = None;
    }
}

/// A name cut to `n` characters (an ellipsis after).
fn fit(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}

fn start(app: &mut BananaApp) {
    let folder = capture_folder(&app.settings.capture);
    let Some(c) = app.screen_capture.capture.as_mut() else {
        return;
    };
    app.screen_capture.editing = None;
    if let Err(e) = c.start(&folder) {
        app.report_error(trf!("Could not start recording: {}", e));
    }
}

fn stop_recording(app: &mut BananaApp) {
    let Some(c) = app.screen_capture.capture.as_mut() else {
        return;
    };
    let (files, errors) = c.stop();
    if !errors.is_empty() {
        app.report_error(errors.join(" · "));
    }
    if !files.is_empty() {
        app.notify(trn!(
            files.len(),
            "{} file recorded.",
            "{} files recorded.",
            files.len()
        ));
        app.screen_capture.recorded = files;
    }
}

/// "10 min Full HD at 30 fps ≈ 0.75 GB" for a video quality; "10 min
/// stereo audio ≈ 14.4 MB" for a sound's.
fn estimate(video: bool, d: &CaptureDefaults) -> String {
    if video {
        let rate = d
            .video_quality
            .video_bitrate(1920, 1080, bv_core::FrameRate::FPS_30);
        let gb = rate as f64 * 600.0 / 8.0 / 1e9;
        trf!("10 min Full HD at 30 fps ≈ {} GB", crate::fmt::num(gb, 2))
    } else {
        let rate = match d.audio_codec {
            CaptureAudioCodec::Pcm => 48_000 * 2 * 16,
            _ => d.audio_quality.audio_bitrate(2),
        };
        let mb = rate as f64 * 600.0 / 8.0 / 1e6;
        trf!("10 min stereo audio ≈ {} MB", crate::fmt::num(mb, 1))
    }
}

/// The Screen Capture's Settings: Recording Folder (and its free space),
/// the video and audio settings new sources start with, the version.
fn settings_dialog(ctx: &egui::Context, app: &mut BananaApp) {
    if !app.screen_capture.settings {
        return;
    }
    let mut open = true;
    let before = app.settings.capture.clone();
    crate::floating::Window::new(tr("Screen Capture - Settings"))
        .id(egui::Id::new("screen-capture-settings"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(560.0);
            let folder = capture_folder(&app.settings.capture);
            egui::Grid::new("capture-settings")
                .num_columns(2)
                .spacing(vec2(16.0, 8.0))
                .show(ui, |ui| {
                    ui.label(tr("Recording Folder:"));
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(folder.display().to_string()).truncate());
                        if crate::widgets::icon_button(ui, Icon::Open, tr("Browse…")).clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .set_title(tr("Recording Folder"))
                                .set_directory(&folder)
                                .pick_folder()
                        {
                            app.settings.capture.folder = Some(p);
                        }
                        if crate::widgets::icon_button(ui, Icon::Forward, tr("Open Folder"))
                            .clicked()
                        {
                            let _ = std::fs::create_dir_all(&folder);
                            let _ = std::process::Command::new("explorer").arg(&folder).spawn();
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    let free = bv_platform::free_space(&folder)
                        .map_or("—".into(), |b| crate::fmt::bytes(b as f64));
                    ui.label(
                        RichText::new(trf!("Free Disk Space: {}", free))
                            .color(theme::text_disabled()),
                    );
                    ui.end_row();
                });
            let d = &mut app.settings.capture.defaults;
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Video settings")).strong());
            egui::Grid::new("capture-settings-video")
                .num_columns(2)
                .spacing(vec2(16.0, 6.0))
                .show(ui, |ui| {
                    ui.label(tr("Codec:"));
                    let options: Vec<(CaptureVideoCodec, String)> = CaptureVideoCodec::ALL
                        .iter()
                        .map(|&c| (c, c.name().to_string()))
                        .collect();
                    combo(
                        ui,
                        "capture-settings-vcodec",
                        &mut d.video_codec,
                        &options,
                        360.0,
                    );
                    ui.end_row();
                    ui.label(tr("Quality:"));
                    combo(
                        ui,
                        "capture-settings-vq",
                        &mut d.video_quality,
                        &qualities(),
                        360.0,
                    );
                    ui.end_row();
                });
            ui.label(RichText::new(estimate(true, d)).color(theme::text_disabled()));
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Audio Settings")).strong());
            egui::Grid::new("capture-settings-audio")
                .num_columns(2)
                .spacing(vec2(16.0, 6.0))
                .show(ui, |ui| {
                    ui.label(tr("Codec:"));
                    let options: Vec<(CaptureAudioCodec, String)> = CaptureAudioCodec::ALL
                        .iter()
                        .map(|&c| (c, c.name().to_string()))
                        .collect();
                    combo(
                        ui,
                        "capture-settings-acodec",
                        &mut d.audio_codec,
                        &options,
                        360.0,
                    );
                    ui.end_row();
                    ui.label(tr("Quality:"));
                    combo(
                        ui,
                        "capture-settings-aq",
                        &mut d.audio_quality,
                        &qualities(),
                        360.0,
                    );
                    ui.end_row();
                });
            ui.label(RichText::new(estimate(false, d)).color(theme::text_disabled()));
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Installed Version")).strong());
            ui.label(
                RichText::new(format!("BananaV {}", env!("CARGO_PKG_VERSION")))
                    .color(theme::text_disabled()),
            );
        });
    if app.settings.capture != before {
        let _ = app.settings.save();
    }
    if !open {
        app.screen_capture.settings = false;
    }
}

fn qualities() -> Vec<(CaptureQuality, String)> {
    vec![
        (CaptureQuality::Low, trc("quality", "Low").to_string()),
        (CaptureQuality::Medium, trc("quality", "Medium").to_string()),
        (CaptureQuality::High, trc("quality", "High").to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_clock_counts_frames_at_thirty() {
        assert_eq!(
            clock(std::time::Duration::from_millis(13_433)),
            "00:00:13:12"
        );
        assert_eq!(clock(std::time::Duration::from_secs(3_725)), "01:02:05:00");
    }

    #[test]
    fn estimates_follow_the_quality() {
        let d = CaptureDefaults::default();
        crate::i18n::set_language(crate::i18n::Language::English);
        assert_eq!(estimate(true, &d), "10 min Full HD at 30 fps ≈ 0.75 GB");
        assert_eq!(estimate(false, &d), "10 min stereo audio ≈ 14.4 MB");
    }

    #[test]
    fn captured_files_go_on_tracks_of_their_own() {
        let dir = std::env::temp_dir().join(format!("bv-capture-add-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A second of silence as a "recording".
        let wav = dir.join("Microphone.wav");
        let f = std::fs::File::create(&wav).unwrap();
        let mut w = bv_wav::WavWriter::new(f, 48_000, 2, bv_wav::SampleFormat::Int(16)).unwrap();
        w.write_planar(&[vec![0.0; 48_000], vec![0.0; 48_000]])
            .unwrap();
        w.finish().unwrap();
        let mut app = BananaApp::new(egui::Context::default());
        let tracks = app.project().tracks.len();
        app.screen_capture.recorded = vec![Captured {
            name: "Microphone (USB)".into(),
            path: wav,
            video: false,
        }];
        app.add_captures_to_project();
        let p = app.project();
        assert_eq!(p.tracks.len(), tracks + 1);
        let t = p.tracks.last().unwrap();
        assert_eq!(t.name, "Microphone (USB)");
        assert_eq!(t.kind, TrackKind::Audio);
        assert_eq!(t.clips.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
