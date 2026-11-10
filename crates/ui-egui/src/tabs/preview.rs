//! Video Preview: the project's monitor, laid out as VEGAS's — its toolbar
//! (project properties, external monitor, output FX, Split-Screen View,
//! quality, zoom, overlays, snapshots), the frame, the transport and the
//! status line (the project's, the preview's and the display's sizes and
//! the frame shown).

use crate::i18n::{tr, trc};
use bv_core::Tick;
use bv_engine::{Channel, PreviewLook};
use bv_timeline::RenderQuality;
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{
    self, Color32, ColorImage, Pos2, Rect, Stroke, TextureHandle, TextureOptions, pos2, vec2,
};
use crate::icons::Icon;
use crate::menus::{check_item, keyed_item, radio_sub, sub};
use crate::settings::PreviewBackground;
use crate::theme;
use crate::widgets::{icon_button, icon_button_ex, icon_toggle};

/// The size the preview's frames are drawn at: fitting the window, or the
/// project's frame size, half or a quarter of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewSize {
    #[default]
    Auto,
    Full,
    Half,
    Quarter,
}

impl PreviewSize {
    const ALL: [PreviewSize; 4] = [
        PreviewSize::Auto,
        PreviewSize::Full,
        PreviewSize::Half,
        PreviewSize::Quarter,
    ];

    fn name(self) -> &'static str {
        // Words other windows translate otherwise (a half note…).
        match self {
            PreviewSize::Auto => trc("preview size", "Auto"),
            PreviewSize::Full => trc("preview size", "Full"),
            PreviewSize::Half => trc("preview size", "Half"),
            PreviewSize::Quarter => trc("preview size", "Quarter"),
        }
    }
}

/// The Video Preview's quality, as VEGAS's: how pictures are scaled
/// (Draft, Preview, Good or Best) and the size frames are drawn at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewQuality {
    pub level: RenderQuality,
    pub size: PreviewSize,
}

impl Default for PreviewQuality {
    /// Preview (Auto), as VEGAS starts.
    fn default() -> PreviewQuality {
        PreviewQuality {
            level: RenderQuality::Preview,
            size: PreviewSize::Auto,
        }
    }
}

fn level_name(level: RenderQuality) -> &'static str {
    match level {
        RenderQuality::Draft => tr("Draft"),
        RenderQuality::Preview => tr("Preview"),
        RenderQuality::Good => tr("Good"),
        RenderQuality::Best => tr("Best"),
    }
}

impl PreviewQuality {
    /// "Best (Full)".
    fn label(self) -> String {
        format!("{} ({})", level_name(self.level), self.size.name())
    }
}

/// What Split-Screen View shows in its part of the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitMode {
    /// The frame with every video effect bypassed.
    #[default]
    FxBypassed,
    /// The snapshot copied last (Copy Snapshot to Clipboard).
    Clipboard,
}

/// Split-Screen View: on or off, what it shows and where (left, top,
/// right, bottom of the frame, 0–1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplitScreen {
    pub on: bool,
    pub mode: SplitMode,
    pub area: [f32; 4],
}

impl Default for SplitScreen {
    fn default() -> SplitScreen {
        SplitScreen {
            on: false,
            mode: SplitMode::FxBypassed,
            area: RIGHT_HALF,
        }
    }
}

const LEFT_HALF: [f32; 4] = [0.0, 0.0, 0.5, 1.0];
const RIGHT_HALF: [f32; 4] = [0.5, 0.0, 1.0, 1.0];
const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

pub struct PreviewState {
    texture: Option<TextureHandle>,
    /// The image the texture was last given (its pixels used again for
    /// the next frame of its size).
    image: Option<std::sync::Arc<ColorImage>>,
    pub(crate) shown: Option<(u64, Tick, u32, u32)>,
    /// Overlays (the # button), and which: the grid, the safe areas, the
    /// closed captioning channels (CC1 to CC4), one channel of the picture.
    pub(crate) overlays: bool,
    grid: bool,
    safe_areas: bool,
    pub(crate) captions: [bool; 4],
    channel: Channel,
    /// The zoom, in percent of the size that fits the window; Bypass Zoom
    /// shows the frame fitting whatever it says.
    zoom: u32,
    bypass_zoom: bool,
    /// The frame's point (0–1 across and down) in the middle of the window:
    /// where the zoom looks (dragging with the mouse wheel held moves it).
    center: Pos2,
    /// Mouse wheel notches not turned into zoom yet (smooth wheels).
    wheel_rest: f32,
    /// The zoom being typed.
    zoom_text: Option<String>,
    /// The width the frame fits the window at, in pixels (for Original
    /// resolution).
    fit_width: f32,
    /// Split-Screen View, kept between sessions when "Retain Split Screen
    /// Setting" is on.
    split: SplitScreen,
    retain_split: bool,
    /// Where a drag making Split-Screen View's part started (0–1 of the
    /// frame).
    split_from: Option<Pos2>,
    /// The last snapshot copied: what Split-Screen View's Clipboard shows.
    snapshot: Option<TextureHandle>,
    /// The size the last frame shown was drawn at (the status line's
    /// "Preview").
    last_frame_size: (u32, u32),
    /// The pixels the frame was last shown in: frames drawn larger come
    /// back reduced to them (Full, Half and Quarter in a smaller window).
    room: (u32, u32),
    /// When the frames shown during the last second of playback came: the
    /// rate the preview keeps up with (VEGAS shows it beside "Preview").
    shown_at: std::collections::VecDeque<std::time::Instant>,
    /// "Adjust Size and Quality for Optimal Playback": how many times the
    /// frames are halved (at Draft quality) to keep up with playback, and
    /// when the preview's rate is looked at next.
    slow: u8,
    slow_check: Option<std::time::Instant>,
    /// Expanded Edit Mode's split screen: the frames on either side of the
    /// edit point.
    sides: [Option<TextureHandle>; 2],
    sides_shown: [Option<(u64, Tick, u32, u32)>; 2],
    /// Where the frame was last drawn (the window less its bars).
    area: Rect,
    /// The frame itself, as last drawn.
    frame: Rect,
    /// The main window's pass that last asked the engine for a frame (the
    /// Video Scopes ask themselves while the preview does not).
    pub(crate) requested_at: u64,
    /// Transform mode (the button by Preview Quality): the event at the
    /// cursor moved, resized and turned on the picture; the drag under
    /// way.
    pub(crate) transform: bool,
    pub(crate) transform_drag: Option<crate::transform_ui::Drag>,
    /// The 360° View (360° projects): the frame seen through a camera
    /// looking around in it, and where the camera looks; and whether the
    /// texture repeats across for it.
    pub(crate) view_360: bool,
    pub(crate) look: crate::view360::Look,
    repeats: bool,
}

impl Default for PreviewState {
    fn default() -> PreviewState {
        PreviewState {
            texture: None,
            image: None,
            shown: None,
            overlays: false,
            grid: true,
            safe_areas: false,
            captions: [false; 4],
            channel: Channel::All,
            zoom: 100,
            bypass_zoom: false,
            center: pos2(0.5, 0.5),
            wheel_rest: 0.0,
            zoom_text: None,
            fit_width: 0.0,
            split: SplitScreen::default(),
            retain_split: false,
            split_from: None,
            snapshot: None,
            last_frame_size: (0, 0),
            room: (0, 0),
            shown_at: std::collections::VecDeque::new(),
            slow: 0,
            slow_check: None,
            sides: [None, None],
            sides_shown: [None, None],
            area: Rect::NOTHING,
            frame: Rect::NOTHING,
            requested_at: 0,
            transform: false,
            transform_drag: None,
            view_360: false,
            look: Default::default(),
            repeats: false,
        }
    }
}

/// Expanded Edit Mode on a video track: the events on either side of the
/// edit point, with the times to show (the last frame before it, the first
/// after it).
fn edit_point_sides(app: &BananaApp) -> Option<[Option<(bv_timeline::ClipId, Tick)>; 2]> {
    let e = app.timeline.edge_trim.filter(|e| e.expanded)?;
    let p = app.project();
    if p.track(e.track)?.kind != bv_timeline::TrackKind::Video {
        return None;
    }
    let (before, after) = crate::timeline::edit_point_events(p, e);
    let rate = p.settings.frame_rate;
    // The last frame starting inside the event before, the first one
    // starting inside the event after.
    let first = |t: Tick| {
        let k = rate.frame_at(t);
        if rate.frame_start(k) < t {
            rate.frame_start(k + 1)
        } else {
            rate.frame_start(k)
        }
    };
    let a = before
        .and_then(|id| p.clip(id))
        .map(|c| (c.id, rate.snap_floor(c.end() - Tick(1)).max(first(c.start))));
    let b = after
        .and_then(|id| p.clip(id))
        .map(|c| (c.id, first(c.start).min(rate.snap_floor(c.end() - Tick(1)))));
    Some([a, b])
}

/// The split screen: the frame before the edit point on the left, the one
/// after it on the right, each fitting its half of `area`.
fn paint_split(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    area: Rect,
    sides: [Option<(bv_timeline::ClipId, Tick)>; 2],
) {
    let ppp = ui.ctx().pixels_per_point();
    let half = egui::vec2(area.width() / 2.0 - 2.0, area.height());
    let (out_w, out_h) = app.project().settings.output_size();
    let aspect = out_w as f32 / out_h as f32;
    let mut size = half;
    if size.x / size.y > aspect {
        size.x = size.y * aspect;
    } else {
        size.y = size.x / aspect;
    }
    app.engine.request_edit_point(
        sides,
        (size.x * ppp).round().max(2.0) as u32,
        (size.y * ppp).round().max(2.0) as u32,
    );
    let frames = app.engine.edit_point_frames();
    let painter = ui.painter();
    for (i, f) in frames.into_iter().enumerate() {
        let center = if i == 0 {
            pos2(area.left() + area.width() / 4.0, area.center().y)
        } else {
            pos2(area.right() - area.width() / 4.0, area.center().y)
        };
        let rect = Rect::from_center_size(center, size);
        painter.rect_filled(rect, 0.0, Color32::BLACK);
        let wanted = sides[i];
        if let (Some(f), Some((_, t))) = (f, wanted)
            && f.time == app.frame_rate().snap_floor(t)
        {
            let key = (f.generation, f.time, f.width, f.height);
            if app.preview.sides_shown[i] != Some(key) {
                let image = frame_image([f.width as usize, f.height as usize], &f.rgba);
                match &mut app.preview.sides[i] {
                    Some(t) => t.set(image, TextureOptions::LINEAR),
                    None => {
                        app.preview.sides[i] = Some(ui.ctx().load_texture(
                            format!("edicao-{i}"),
                            image,
                            TextureOptions::LINEAR,
                        ));
                    }
                }
                app.preview.sides_shown[i] = Some(key);
            }
            if let Some(tex) = &app.preview.sides[i] {
                painter.image(
                    tex.id(),
                    rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        if wanted.is_some() {
            painter.text(
                rect.left_top() + egui::vec2(6.0, 4.0),
                egui::Align2::LEFT_TOP,
                if i == 0 { "A" } else { "B" },
                egui::FontId::proportional(13.0),
                Color32::from_white_alpha(200),
            );
        }
    }
    painter.vline(
        area.center().x,
        area.y_range(),
        egui::Stroke::new(1.0, Color32::from_rgb(0xE8, 0x2A, 0x2A)),
    );
}

impl PreviewState {
    /// Split-Screen View on or off (a control surface's button).
    pub(crate) fn toggle_split_screen(&mut self) {
        self.split.on ^= true;
    }

    /// The size of the frames the preview shows (at least a little).
    pub fn size(&self) -> (u32, u32) {
        let (w, h) = self.last_frame_size;
        (w.max(320), h.max(180))
    }

    /// The pixels the frames are shown in (see `room`).
    pub(crate) fn room(&self) -> (u32, u32) {
        self.room
    }

    /// Different frames shown a second during the last second of
    /// playback (`None` before there are a few).
    pub(crate) fn shown_rate(&self) -> Option<f64> {
        let shown = &self.shown_at;
        match (shown.front(), shown.back()) {
            (Some(a), Some(b)) if shown.len() > 2 && *b > *a => {
                Some((shown.len() - 1) as f64 / (*b - *a).as_secs_f64())
            }
            _ => None,
        }
    }

    /// The zoom `step` percent in (or out), on the middle of the window;
    /// 0: back to 100 %, the frame centered.
    pub(crate) fn zoom_by(&mut self, step: i32) {
        if step == 0 {
            self.reset_zoom();
        } else {
            self.zoom = clamp_zoom(self.shown_zoom() as f32 + step as f32);
            self.bypass_zoom = false;
        }
    }

    /// 100 %, the whole frame centered (a double-click on the frame).
    fn reset_zoom(&mut self) {
        self.zoom = 100;
        self.bypass_zoom = false;
        self.center = pos2(0.5, 0.5);
    }

    /// The zoom the frame is shown at (100 % while Bypass Zoom is on).
    fn shown_zoom(&self) -> u32 {
        if self.bypass_zoom { 100 } else { self.zoom }
    }

    /// Bypass Zoom: 100 % and back.
    pub(crate) fn toggle_bypass_zoom(&mut self) {
        self.bypass_zoom = !self.bypass_zoom;
    }

    /// Split-Screen View as it was left, when it is retained.
    pub(crate) fn restore(&mut self, prefs: &crate::settings::PreviewPrefs) {
        if let Some(s) = prefs.split {
            self.split = s;
            self.retain_split = true;
        }
    }

    /// The look the engine draws the frames with: the quality, Split-Screen
    /// View's FX Bypassed and the channel the overlays show.
    fn look(&self, quality: PreviewQuality) -> PreviewLook {
        let s = self.split;
        PreviewLook {
            quality: Some(quality.level),
            bypass: (s.on && s.mode == SplitMode::FxBypassed).then_some(s.area),
            channel: if self.overlays {
                self.channel
            } else {
                Channel::All
            },
        }
    }
}

impl BananaApp {
    /// The middle of the Video Preview's frame area (as last drawn), for
    /// screen tests.
    pub fn preview_spot_for_review(&self) -> Pos2 {
        self.preview.area.center()
    }

    /// Where the Video Preview drew the frame (for screen tests).
    pub fn preview_frame_for_review(&self) -> Rect {
        self.preview.frame
    }

    /// The Video Preview's room for the frame, between its bars (for
    /// screen tests).
    pub fn preview_area_for_review(&self) -> Rect {
        self.preview.area
    }

    /// Split-Screen View on, FX Bypassed on the right half, the first video
    /// event with an effect to tell them apart (for screen tests).
    pub fn split_screen_for_review(&mut self) {
        let first = self
            .project()
            .clips()
            .find(|(t, _)| t.kind == bv_timeline::TrackKind::Video)
            .map(|(_, c)| c.id);
        if let Some(clip) = first {
            let sepia = bv_timeline::Effect::new(bv_timeline::EffectKind::Sepia);
            super::effects::apply_to_clips(self, &[clip], std::slice::from_ref(&sepia));
        }
        self.preview.split = SplitScreen {
            on: true,
            mode: SplitMode::FxBypassed,
            area: RIGHT_HALF,
        };
    }
}

impl std::fmt::Debug for PreviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewState")
            .field("zoom", &self.zoom)
            .field("split", &self.split)
            .finish()
    }
}

fn save_snapshot(app: &mut BananaApp) {
    let t = app.display_time();
    if let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Save Snapshot"))
        .add_filter("PNG", &["png"])
        .set_file_name(format!("snapshot_{}.png", app.frame_rate().frame_at(t)))
        .save_file()
    {
        let Some(png) = app.engine.snapshot_png(t) else {
            app.report_error(tr("Could not draw the snapshot."));
            return;
        };
        match std::fs::write(&path, png) {
            Ok(()) => app.notify(trf!("Snapshot saved to {}", path.display())),
            Err(e) => app.report_save_error(&path, &e),
        }
    }
}

/// The zooms the menu lists, in percent of the size that fits.
const ZOOMS: [u32; 7] = [50, 100, 125, 150, 200, 400, 800];

/// The zoom's limits, in percent.
const MIN_ZOOM: u32 = 10;
const MAX_ZOOM: u32 = 1600;

fn clamp_zoom(z: f32) -> u32 {
    z.round().clamp(MIN_ZOOM as f32, MAX_ZOOM as f32) as u32
}

/// Where a frame of `size` goes in `area` with its point `center` (0–1 of
/// the frame) in the middle of the area. Dragged with the wheel held, it
/// goes anywhere while the middle of the window still shows some of it:
/// the center kept within the frame comes with it.
fn place_frame(area: Rect, size: egui::Vec2, center: Pos2) -> (Rect, Pos2) {
    let c = pos2(center.x.clamp(0.0, 1.0), center.y.clamp(0.0, 1.0));
    let min = area.center() - vec2(c.x * size.x, c.y * size.y);
    (Rect::from_min_size(min, size), c)
}

/// The frame's navigation, as VEGAS's: the mouse wheel zooms toward the
/// pointer (10 % a notch, 1 % with Ctrl), dragging with the wheel held
/// pans, and a double-click with it (or the left button, outside
/// Split-Screen View) shows the whole frame at 100 % again.
fn navigate(ui: &egui::Ui, app: &mut BananaApp, nav: &egui::Response, area: Rect, fit: egui::Vec2) {
    let p = &mut app.preview;
    if nav.hovered() {
        let (notches, fine) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        p.wheel_rest += notches;
        let whole = p.wheel_rest.trunc();
        p.wheel_rest -= whole;
        if whole != 0.0
            && let Some(at) = nav.hover_pos()
        {
            let from = p.shown_zoom();
            let to = clamp_zoom(from as f32 + whole * if fine { 1.0 } else { 10.0 });
            // The point under the pointer stays under it.
            let (before, _) = place_frame(area, fit * from as f32 / 100.0, p.center);
            let u = (at - before.min) / before.size();
            let size = fit * to as f32 / 100.0;
            p.center = pos2(
                (area.center().x - at.x) / size.x + u.x,
                (area.center().y - at.y) / size.y + u.y,
            );
            p.zoom = to;
            p.bypass_zoom = false;
        }
    } else {
        p.wheel_rest = 0.0;
    }
    let dragging = nav.dragged_by(egui::PointerButton::Middle);
    if dragging {
        let size = fit * p.shown_zoom() as f32 / 100.0;
        let d = nav.drag_delta();
        p.center -= vec2(d.x / size.x.max(1.0), d.y / size.y.max(1.0));
    }
    // A double click a plug-in's overlay took is its own (a point put
    // back), not the zoom's; nor is a Color Grading picker's click.
    let overlay = app.ofx.overlay_took_press() || app.color_panel.picking.is_some();
    if nav.double_clicked_by(egui::PointerButton::Middle)
        || (nav.double_clicked() && !p.split.on && !overlay)
    {
        p.reset_zoom();
    }
    // The pointer over the picture as it is now (`view_cursor`).
    let size = fit * app.preview.shown_zoom() as f32 / 100.0;
    let (frame, _) = place_frame(area, size, app.preview.center);
    let larger = size.x > area.width() + 0.5 || size.y > area.height() + 0.5;
    let over = nav.hover_pos().is_some_and(|q| frame.contains(q));
    if let Some(c) = view_cursor(dragging, larger, over) {
        crate::cursors::set(ui.ctx(), c);
    }
}

/// The Video Preview's pointer, as VEGAS's: dragged with the wheel held
/// (the 360° View: with either button), the closed hand; over a picture
/// larger than the window (zoomed in; the 360° View's, always), the open
/// hand; else none of BananaV's.
fn view_cursor(dragging: bool, larger: bool, over: bool) -> Option<crate::cursors::Cursor> {
    use crate::cursors::Cursor;
    if dragging {
        Some(Cursor::HandClosed)
    } else if larger && over {
        Some(Cursor::HandOpen)
    } else {
        None
    }
}

/// The 360° View's mouse: dragging (with either button) turns the camera,
/// the mouse wheel zooms (a tenth of the view a notch, a hundredth with
/// Ctrl) and a double-click looks ahead again.
fn navigate_360(ui: &egui::Ui, app: &mut BananaApp, nav: &egui::Response, width: f32) {
    let look = &mut app.preview.look;
    if nav.hovered() {
        let (notches, fine) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches != 0.0 {
            look.zoom(if fine { notches / 10.0 } else { notches });
        }
    }
    let middle = egui::PointerButton::Middle;
    let dragging = nav.dragged_by(egui::PointerButton::Primary) || nav.dragged_by(middle);
    if dragging {
        look.drag(nav.drag_delta(), width);
    }
    // The camera's view is dragged round wherever it shows.
    if let Some(c) = view_cursor(dragging, true, nav.hovered()) {
        crate::cursors::set(ui.ctx(), c);
    }
    if nav.double_clicked() || nav.double_clicked_by(middle) {
        *look = crate::view360::Look::default();
    }
}

/// "Adjust Size and Quality for Optimal Playback": how many times to halve
/// the frames. Each second playback shows fewer than nine in ten of the
/// project's frames, once more (twice at most); none once it stops.
fn keep_up(app: &mut BananaApp) -> u8 {
    use std::time::{Duration, Instant};
    // A display shows 60 frames a second, whatever the project's rate.
    let rate = app.frame_rate().as_f64().min(60.0);
    let on = app.settings.preview.optimal_playback;
    let playing = app.is_playing();
    let p = &mut app.preview;
    let now = Instant::now();
    while p
        .shown_at
        .front()
        .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1))
    {
        p.shown_at.pop_front();
    }
    if !playing {
        p.shown_at.clear();
    }
    if !(playing && on) {
        p.slow = 0;
        p.slow_check = None;
        return 0;
    }
    // The first look a little later: playback takes a moment to start.
    let check = *p
        .slow_check
        .get_or_insert(now + Duration::from_millis(1500));
    if now >= check {
        if p.slow < 2 && (p.shown_at.len() as f64) < 0.9 * rate {
            p.slow += 1;
        }
        p.slow_check = Some(now + Duration::from_secs(1));
    }
    p.slow
}

fn open_preferences(app: &mut BananaApp, tab: crate::preferences::PrefTab) {
    let mut d = crate::preferences::PreferencesDialog::new(app);
    d.tab = tab;
    app.menu_dialogs.more.preferences = Some(d);
}

/// The window's shortcut menu (a right-click on it), as VEGAS's.
fn context_menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    if ui.button(tr("Video Output Color Grading")).clicked() {
        super::color::open_at(app, super::color::Level::VideoOutput);
        ui.close();
    }
    ui.separator();
    let background = app.settings.prefs.preview_background;
    for (b, name) in [
        (PreviewBackground::Default, tr("Default Background")),
        (PreviewBackground::Black, tr("Black Background")),
        (PreviewBackground::White, tr("White Background")),
    ] {
        if check_item(ui, name, background == b, true, "") && background != b {
            app.settings.prefs.preview_background = b;
            let _ = app.settings.save();
        }
    }
    ui.separator();
    let prefs = &mut app.settings.prefs;
    if check_item(
        ui,
        tr("Simulate Device Aspect Ratio"),
        prefs.simulate_device_aspect,
        false,
        "",
    ) {
        prefs.simulate_device_aspect = !prefs.simulate_device_aspect;
        let _ = app.settings.save();
    }
    let p = &mut app.settings.preview;
    let mut changed = false;
    if check_item(ui, tr("Enable Preview Scaling"), !p.scaling_off, false, "") {
        p.scaling_off = !p.scaling_off;
        changed = true;
    }
    if check_item(
        ui,
        tr("Adjust Size and Quality for Optimal Playback"),
        p.optimal_playback,
        false,
        "",
    ) {
        p.optimal_playback = !p.optimal_playback;
        changed = true;
    }
    ui.separator();
    for (name, hidden) in [
        (tr("Show Toolbar"), &mut p.hide_toolbar),
        (tr("Show Status Bar"), &mut p.hide_status_bar),
        (tr("Show Transport Bar"), &mut p.hide_transport_bar),
    ] {
        if check_item(ui, name, !*hidden, false, "") {
            *hidden = !*hidden;
            changed = true;
        }
    }
    if changed {
        let _ = app.settings.save();
    }
    ui.separator();
    if keyed_item(ui, tr("Video Preview Preferences…"), "") {
        open_preferences(app, crate::preferences::PrefTab::Video);
    }
    if keyed_item(ui, tr("Preview Device Preferences…"), "") {
        open_preferences(app, crate::preferences::PrefTab::PreviewDevice);
    }
    ui.separator();
    sub(ui, tr("Preview Quality"), true, |ui| quality_items(ui, app));
}

/// A small ▾ beside a toolbar button, opening its menu.
fn arrow(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0, 24.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 2.0, theme::hover());
    }
    triangle(ui, rect.center());
    resp
}

fn triangle(ui: &egui::Ui, c: Pos2) {
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-4.0, -2.0),
            c + vec2(4.0, -2.0),
            c + vec2(0.0, 3.0),
        ],
        theme::text(),
        Stroke::NONE,
    ));
}

/// A flat button with `text` and a ▾, opening a menu (the quality's).
fn text_arrow(ui: &mut egui::Ui, text: &str, tip: &str) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, theme::text());
    let size = vec2(galley.size().x + 24.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 2.0, theme::hover());
    }
    let y = rect.center().y - galley.size().y / 2.0;
    ui.painter()
        .galley(pos2(rect.left() + 5.0, y), galley, theme::text());
    triangle(ui, pos2(rect.right() - 10.0, rect.center().y));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip)
}

/// Preview Quality: Draft, Preview, Good and Best, each with its sizes.
fn quality_menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    let q = app.settings.preview.quality;
    let button = text_arrow(ui, &q.label(), tr("Preview Quality"));
    egui::Popup::menu(&button).show(|ui| quality_items(ui, app));
}

/// The quality's levels, each a submenu of its sizes.
fn quality_items(ui: &mut egui::Ui, app: &mut BananaApp) {
    let q = app.settings.preview.quality;
    let mut chosen = None;
    for level in RenderQuality::ALL {
        radio_sub(ui, level_name(level), q.level == level, |ui| {
            for size in PreviewSize::ALL {
                let on = q.level == level && q.size == size;
                if check_item(ui, size.name(), on, true, "") {
                    chosen = Some(PreviewQuality { level, size });
                }
            }
        });
    }
    if let Some(c) = chosen
        && c != q
    {
        app.settings.preview.quality = c;
        let _ = app.settings.save();
    }
}

/// The zoom: typed in, or from its menu.
fn zoom_box(ui: &mut egui::Ui, app: &mut BananaApp) {
    let (out_w, _) = app.project().settings.output_size();
    let p = &mut app.preview;
    let mut text = p
        .zoom_text
        .clone()
        .unwrap_or_else(|| format!("{} %", p.zoom));
    let edit = ui.add(
        egui::TextEdit::singleline(&mut text)
            .desired_width(56.0)
            .horizontal_align(egui::Align::Center),
    );
    if edit.changed() {
        p.zoom_text = Some(text.clone());
    }
    if edit.lost_focus() {
        let digits: String = text
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
            .map(|c| if c == ',' { '.' } else { c })
            .collect();
        if let Ok(z) = digits.parse::<f32>() {
            p.zoom = clamp_zoom(z);
            p.bypass_zoom = false;
        }
        p.zoom_text = None;
    }
    edit.on_hover_text(tr("Zoom"));
    let more = arrow(ui);
    egui::Popup::menu(&more).show(|ui| {
        // Five percent in or out, as VEGAS's.
        if keyed_item(ui, "+", "Ctrl+Num +") {
            p.zoom_by(5);
        }
        if keyed_item(ui, "−", "Ctrl+Num -") {
            p.zoom_by(-5);
        }
        if keyed_item(ui, tr("Original resolution"), "") && p.fit_width > 0.0 {
            // One pixel of the frame on one pixel of the screen.
            p.zoom = clamp_zoom(out_w as f32 / p.fit_width * 100.0);
            p.bypass_zoom = false;
        }
        for z in ZOOMS {
            let keys = if z == 100 { "Ctrl+Num 1" } else { "" };
            if keyed_item(ui, &format!("{z} %"), keys) {
                if z == 100 {
                    p.zoom_by(0);
                } else {
                    p.zoom = z;
                    p.bypass_zoom = false;
                }
            }
        }
        if check_item(
            ui,
            tr("Bypass Zoom"),
            p.bypass_zoom,
            false,
            "Ctrl+Shift+Alt+B",
        ) {
            p.bypass_zoom = !p.bypass_zoom;
        }
    });
}

/// Overlays: the button shows them; its menu chooses which.
fn overlays_menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    let p = &mut app.preview;
    if icon_toggle(ui, Icon::Grid, p.overlays, tr("Overlays")).clicked() {
        p.overlays = !p.overlays;
    }
    let more = arrow(ui);
    egui::Popup::menu(&more).show(|ui| {
        if check_item(ui, tr("Grid"), p.grid, false, "") {
            p.grid = !p.grid;
            p.overlays = true;
        }
        if check_item(ui, tr("Safe Areas"), p.safe_areas, false, "") {
            p.safe_areas = !p.safe_areas;
            p.overlays = true;
        }
        ui.separator();
        // The closed captioning channels drawn over the picture.
        for (k, name) in [
            tr("Closed Captioning CC1 (Primary)"),
            tr("Closed Captioning CC2"),
            tr("Closed Captioning CC3 (Secondary)"),
            tr("Closed Captioning CC4"),
        ]
        .into_iter()
        .enumerate()
        {
            if check_item(ui, name, p.captions[k], false, "") {
                p.captions[k] = !p.captions[k];
                p.overlays = true;
            }
        }
        ui.separator();
        for (channel, name) in [
            (Channel::Red, tr("Red")),
            (Channel::Green, tr("Green")),
            (Channel::Blue, tr("Blue")),
            (Channel::RedGray, tr("Red as Grayscale")),
            (Channel::GreenGray, tr("Green as Grayscale")),
            (Channel::BlueGray, tr("Blue as Grayscale")),
            (Channel::AlphaGray, tr("Alpha as Grayscale")),
        ] {
            if check_item(ui, name, p.channel == channel, false, "") {
                p.channel = if p.channel == channel {
                    Channel::All
                } else {
                    channel
                };
                p.overlays = true;
            }
        }
    });
}

/// Split-Screen View: the button turns it on; its menu chooses what it
/// shows and where. Whether it changed.
fn split_menu(ui: &mut egui::Ui, app: &mut BananaApp) -> bool {
    let p = &mut app.preview;
    let mut changed = false;
    if icon_toggle(ui, Icon::SplitScreen, p.split.on, tr("Split-Screen View")).clicked() {
        p.split.on = !p.split.on;
        changed = true;
    }
    let more = arrow(ui);
    egui::Popup::menu(&more).show(|ui| {
        for (mode, name) in [
            (SplitMode::FxBypassed, tr("FX Bypassed")),
            (SplitMode::Clipboard, tr("Clipboard")),
        ] {
            if check_item(ui, name, p.split.mode == mode, true, "") {
                p.split.mode = mode;
                changed = true;
            }
        }
        ui.separator();
        for (area, name) in [
            (LEFT_HALF, tr("Select Left Half")),
            (RIGHT_HALF, tr("Select Right Half")),
            (WHOLE, tr("Select All")),
        ] {
            if keyed_item(ui, name, "") {
                p.split.area = area;
                p.split.on = true;
                changed = true;
            }
        }
        ui.separator();
        if check_item(
            ui,
            tr("Retain Split Screen Setting"),
            p.retain_split,
            false,
            "",
        ) {
            p.retain_split = !p.retain_split;
            changed = true;
        }
    });
    changed
}

/// The part of `frame` Split-Screen View's `area` covers.
fn area_rect(frame: Rect, area: [f32; 4]) -> Rect {
    Rect::from_min_max(
        frame.lerp_inside(vec2(area[0], area[1])),
        frame.lerp_inside(vec2(area[2], area[3])),
    )
}

/// A dashed outline around `r`.
fn dashed_rect(painter: &egui::Painter, r: Rect, stroke: Stroke) {
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(&corners, stroke, 4.0, 3.0));
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let bars = &app.settings.preview;
    let (toolbar, status_bar, transport_bar) = (
        !bars.hide_toolbar,
        !bars.hide_status_bar,
        !bars.hide_transport_bar,
    );
    // The toolbar, as VEGAS's. In a narrow window what does not fit is cut
    // off at its edge, as VEGAS's is: the bar never widens the window, so
    // the frame below always fits it.
    let mut split_changed = false;
    if toolbar {
        let row = Rect::from_min_size(
            ui.cursor().min,
            vec2(ui.available_width(), ui.spacing().interact_size.y),
        );
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        {
            let ui = &mut bar;
            ui.spacing_mut().item_spacing.x = 2.0;
            if icon_button(ui, Icon::Settings, tr("Project Properties")).clicked() {
                app.open_project_settings();
            }
            let external = app.external.is_some();
            if icon_toggle(
                ui,
                Icon::ExternalMonitor,
                external,
                tr("Video Preview on External Monitor (Alt+Shift+4)"),
            )
            .clicked()
            {
                app.toggle_external_monitor();
            }
            let fx = !app.project().video_output_fx.is_empty();
            if icon_toggle(ui, Icon::Fx, fx, tr("Video Output FX…")).clicked() {
                app.clip_windows.output_fx = true;
            }
            split_changed = split_menu(ui, app);
            quality_menu(ui, app);
            zoom_box(ui, app);
            overlays_menu(ui, app);
            crate::transform_ui::toolbar_button(ui, app);
            if icon_button(ui, Icon::Copy, tr("Copy Snapshot to Clipboard")).clicked() {
                // Over black, as the preview shows it.
                match app
                    .engine
                    .snapshot(app.display_time(), [0.0, 0.0, 0.0, 1.0])
                {
                    Some((w, h, rgba)) => {
                        let size = [w as usize, h as usize];
                        let image = ColorImage::from_rgba_unmultiplied(size, &rgba);
                        // Kept for Split-Screen View's Clipboard too.
                        app.preview.snapshot = Some(ui.ctx().load_texture(
                            "snapshot",
                            image.clone(),
                            TextureOptions::LINEAR,
                        ));
                        ui.ctx().copy_image(image);
                        app.notify(tr("Snapshot copied."));
                    }
                    None => app.report_error(tr("Could not draw the snapshot.")),
                }
            }
            if icon_button(ui, Icon::Save, tr("Save Snapshot to File…")).clicked() {
                save_snapshot(app);
            }
            // The 360° View, for 360° projects; never on by itself: the
            // flat frame shows how the sphere is stitched.
            let is_360 = app.project().settings.output_360;
            let tip = if is_360 {
                tr("360° View")
            } else {
                tr("360° View: turn on 360 Output in Project Properties' Advanced tab first.")
            };
            if icon_button_ex(
                ui,
                Icon::View360,
                tip,
                is_360 && app.preview.view_360,
                is_360,
            )
            .clicked()
            {
                app.preview.view_360 = !app.preview.view_360;
            }
            icon_button_ex(ui, Icon::Hdr, tr("HDR Preview"), false, false)
                .on_disabled_hover_text(tr("Coming soon to BananaV"));
        }
        let height = bar.min_rect().height().max(row.height());
        ui.advance_cursor_after_rect(Rect::from_min_size(row.min, vec2(row.width(), height)));
    }

    let status_h = if status_bar { 24.0 } else { 0.0 };
    let transport_h = if transport_bar { 30.0 } else { 0.0 };
    let full = ui.available_rect_before_wrap();
    let image_area = Rect::from_min_max(
        full.min + vec2(4.0, 2.0),
        pos2(full.max.x - 4.0, full.max.y - status_h - transport_h),
    );

    let ppp = ui.ctx().pixels_per_point();
    // The Trimmer's picture (its own video monitor off, Show Video in
    // Preview Window on) takes the frame's place while the Trimmer has the
    // focus, as VEGAS's.
    if let Some((info, at)) = app.trimmer_in_preview() {
        super::trimmer::paint_in_preview(ui, app, &info, at, image_area);
        let transport = Rect::from_min_max(
            pos2(full.min.x, full.max.y - status_h - transport_h),
            pos2(full.max.x, full.max.y - status_h),
        );
        if transport_bar {
            transport_ui(ui, app, transport);
        }
        ui.allocate_rect(full, egui::Sense::hover());
        return;
    }
    // The output size: the project turned by its output rotation.
    let (out_w, out_h) = app.project().settings.output_size();
    // The frame fills the monitor; no larger than the project's size when
    // Options › Preferences › Video › "Display at project size" is on.
    let par = if app.settings.prefs.simulate_device_aspect {
        app.project().settings.pixel_aspect.ratio()
    } else {
        1.0
    };
    let aspect = out_w as f32 * par / out_h as f32;
    let mut fit = image_area.size();
    if fit.x / fit.y > aspect {
        fit.x = fit.y * aspect;
    } else {
        fit.y = fit.x / aspect;
    }
    let native = vec2(out_w as f32 * par, out_h as f32) / ppp;
    if app.settings.prefs.display_at_project_size && fit.x > native.x {
        fit = native;
    }
    let quality = app.settings.preview.quality;
    // "Enable Preview Scaling" off: one pixel of the frame on one of the
    // screen, at most.
    if app.settings.preview.scaling_off {
        let k = match quality.size {
            PreviewSize::Auto | PreviewSize::Full => 1.0,
            PreviewSize::Half => 0.5,
            PreviewSize::Quarter => 0.25,
        };
        if fit.x > native.x * k {
            fit = native * k;
        }
    }
    app.preview.fit_width = fit.x * ppp;
    app.preview.area = image_area;
    // Expanded Edit Mode shows both sides of the edit point instead.
    let split = edit_point_sides(app);
    // Multicamera editing: every take of the multicamera event at the
    // cursor, in tiles (none there: the program, as without it). The
    // external monitor shows them too, unless Show Full Frame on External
    // Monitor: then it shows the program and this window asks for them.
    let multicam = if split.is_none() {
        app.multicam_view()
    } else {
        None
    };
    // The 360° View: the camera's view of a 360° project's frame, where
    // the frame fits.
    let view_360 = app.preview.view_360
        && app.project().settings.output_360
        && split.is_none()
        && multicam.is_none();
    // The mouse over the frame: zoom, pan and the shortcut menu (in the
    // 360° View: looking around and zooming).
    let nav = ui.interact(
        image_area,
        ui.id().with("preview-navigation"),
        egui::Sense::click_and_drag(),
    );
    let size = if view_360 {
        navigate_360(ui, app, &nav, fit.x);
        fit
    } else {
        navigate(ui, app, &nav, image_area, fit);
        fit * app.preview.shown_zoom() as f32 / 100.0
    };
    let center = if view_360 {
        pos2(0.5, 0.5)
    } else {
        app.preview.center
    };
    let (frame_rect, center) = place_frame(image_area, size, center);
    app.preview.frame = frame_rect;
    if !app.preview.bypass_zoom && !view_360 {
        app.preview.center = center;
    }
    // Preview Quality: the frames at the size shown (Auto; zoomed in, no
    // more pixels than the project has), or at the project's, half or a
    // quarter of it.
    let (mut max_w, mut max_h) = match quality.size {
        // The 360° View magnifies the part of the frame it shows: the
        // frame as big as that part needs.
        PreviewSize::Auto if view_360 => {
            let w = ((frame_rect.width() * ppp * 360.0 / app.preview.look.fov).round() as u32)
                .min(out_w);
            (w, (out_h as u64 * w as u64 / out_w.max(1) as u64) as u32)
        }
        PreviewSize::Auto => (
            ((frame_rect.width() * ppp).round() as u32).min(out_w),
            ((frame_rect.height() * ppp).round() as u32).min(out_h),
        ),
        PreviewSize::Full => (out_w, out_h),
        PreviewSize::Half => (out_w / 2, out_h / 2),
        PreviewSize::Quarter => (out_w / 4, out_h / 4),
    };
    // Halved (at Draft quality) to keep up with playback, when asked to.
    let mut level = quality.level;
    let slow = keep_up(app);
    if slow > 0 {
        max_w >>= slow;
        max_h >>= slow;
        level = RenderQuality::Draft;
    }
    // Split-Screen View: drag out the part it shows; a double-click shows
    // it all.
    if app.preview.split.on && split.is_none() && !view_360 {
        let left = egui::PointerButton::Primary;
        let over = nav.hover_pos().is_some_and(|q| frame_rect.contains(q));
        if (over && !nav.dragged()) || nav.dragged_by(left) {
            // The system's crosshair (VEGAS's preview uses the system's).
            crate::cursors::system(ui.ctx(), egui::CursorIcon::Crosshair);
        }
        let in_frame = |q: Pos2| {
            pos2(
                ((q.x - frame_rect.left()) / frame_rect.width()).clamp(0.0, 1.0),
                ((q.y - frame_rect.top()) / frame_rect.height()).clamp(0.0, 1.0),
            )
        };
        if nav.drag_started_by(left) {
            app.preview.split_from = ui
                .input(|i| i.pointer.press_origin())
                .filter(|q| frame_rect.contains(*q))
                .map(in_frame);
        }
        if nav.dragged_by(left)
            && let (Some(a), Some(b)) = (
                app.preview.split_from,
                nav.interact_pointer_pos().map(in_frame),
            )
            && (a.x - b.x).abs() > 0.01
            && (a.y - b.y).abs() > 0.01
        {
            app.preview.split.area = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
        }
        if nav.drag_stopped_by(left) && app.preview.split_from.take().is_some() {
            split_changed = true;
        }
        if nav.double_clicked() {
            app.preview.split.area = WHOLE;
            split_changed = true;
        }
    }
    nav.context_menu(|ui| context_menu(ui, app));
    if split_changed {
        let keep = app.preview.retain_split.then_some(app.preview.split);
        if app.settings.preview.split != keep {
            app.settings.preview.split = keep;
            let _ = app.settings.save();
        }
    }
    // While a render shows its frames ("Show Video in Preview Window"),
    // they take the preview's place.
    let rendered = app.engine.render_frame();
    // With the external monitor on, it asks for the frames (at its size)
    // and this window shows them too, unless Options › Preferences ›
    // Preview Device says not to while playing.
    let external = app.external.is_some();
    let own = !external || (multicam.is_some() && app.settings.multicam_full_frame);
    let hold = !own && !app.settings.prefs.preview_in_window && app.is_playing();
    if rendered.is_none() && own && image_area.width() > 8.0 && image_area.height() > 8.0 {
        match &multicam {
            Some((tiles, _, _)) => app.engine.request_multicam(
                tiles.clone(),
                app.display_time(),
                max_w.max(2),
                max_h.max(2),
            ),
            None => {
                let look = app.preview.look(PreviewQuality { level, ..quality });
                app.engine.set_preview_look(look);
                // Drawn at the quality's size, the frame comes back at the
                // size it shows at: the screen's pixels it covers (zoomed
                // in, more than the window; the 360° View magnifies the
                // part it shows, so it takes the whole frame).
                let room = if view_360 || quality.size == PreviewSize::Auto {
                    (0, 0)
                } else {
                    (
                        (frame_rect.width() * ppp).ceil().max(2.0) as u32,
                        (frame_rect.height() * ppp).ceil().max(2.0) as u32,
                    )
                };
                app.preview.room = room;
                app.engine
                    .request_preview_in(app.display_time(), max_w.max(2), max_h.max(2), room);
            }
        }
        app.preview.requested_at = ui.ctx().cumulative_pass_nr_for(egui::ViewportId::ROOT);
    }
    let frame = if multicam.is_some() {
        app.engine.multicam_frame()
    } else {
        app.engine.preview_frame()
    };
    if let Some(f) = rendered.or(frame).filter(|_| !hold) {
        let key = (f.generation, f.time, f.width, f.height);
        if app.preview.shown != Some(key) || app.preview.repeats != view_360 {
            let image = frame_image_reusing(
                [f.width as usize, f.height as usize],
                &f.rgba,
                &mut app.preview.image,
            );
            // Draft shows the pixels as they are; the 360° View's frame
            // repeats across (its left and right edges meet).
            let options = match (quality.level == RenderQuality::Draft, view_360) {
                (true, false) => TextureOptions::NEAREST,
                (false, false) => TextureOptions::LINEAR,
                (true, true) => TextureOptions::NEAREST_REPEAT,
                (false, true) => TextureOptions::LINEAR_REPEAT,
            };
            app.preview.repeats = view_360;
            match &mut app.preview.texture {
                Some(t) => t.set(image, options),
                None => {
                    app.preview.texture = Some(ui.ctx().load_texture("monitor", image, options));
                }
            }
            app.preview.shown = Some(key);
            app.preview.last_frame_size = f.drawn;
            if app.is_playing() {
                app.preview.shown_at.push_back(std::time::Instant::now());
            }
        }
    }

    let painter = ui.painter();
    let background = match app.settings.prefs.preview_background {
        PreviewBackground::Default => theme::panel_dark(),
        PreviewBackground::White => Color32::WHITE,
        PreviewBackground::Black => Color32::BLACK,
    };
    painter.rect_filled(image_area, 0.0, background);
    if let Some(sides) = split {
        paint_split(ui, app, image_area, sides);
    }
    let painter = ui.painter().with_clip_rect(image_area);
    if split.is_none() {
        painter.rect_filled(frame_rect, 0.0, Color32::BLACK);
    }
    if split.is_none()
        && let Some(t) = &app.preview.texture
    {
        if view_360 {
            crate::view360::paint(&painter, frame_rect, t.id(), &app.preview.look);
        } else {
            painter.image(
                t.id(),
                frame_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
    if let Some((_, tiles, look)) = &multicam {
        crate::multicam::overlay(ui, app, frame_rect, tiles, look, &nav);
    }
    // Split-Screen View: the snapshot copied in its part (Clipboard; FX
    // Bypassed comes drawn), and the part outlined.
    let s = app.preview.split;
    if s.on && split.is_none() && !view_360 {
        let part = area_rect(frame_rect, s.area);
        if s.mode == SplitMode::Clipboard {
            match &app.preview.snapshot {
                Some(t) => {
                    painter.image(
                        t.id(),
                        part,
                        Rect::from_min_max(pos2(s.area[0], s.area[1]), pos2(s.area[2], s.area[3])),
                        Color32::WHITE,
                    );
                }
                None => {
                    painter.text(
                        part.center(),
                        egui::Align2::CENTER_CENTER,
                        tr("Copy a snapshot to compare it here."),
                        egui::FontId::proportional(12.0),
                        Color32::from_white_alpha(170),
                    );
                }
            }
        }
        if s.area != WHOLE {
            dashed_rect(
                &painter,
                part,
                Stroke::new(1.0, Color32::from_white_alpha(200)),
            );
        }
    }
    let prefs = &app.settings.prefs;
    let overlays = app.preview.overlays && split.is_none();
    if overlays && app.preview.grid {
        // The frame's width and height divided as the preferences say.
        let st = Stroke::new(1.0, Color32::from_white_alpha(90));
        let (across, down) = (prefs.grid_horizontal.max(1), prefs.grid_vertical.max(1));
        for i in 1..across {
            let x = frame_rect.left() + frame_rect.width() * i as f32 / across as f32;
            painter.vline(x.round() + 0.5, frame_rect.y_range(), st);
        }
        for i in 1..down {
            let y = frame_rect.top() + frame_rect.height() * i as f32 / down as f32;
            painter.hline(frame_rect.x_range(), y.round() + 0.5, st);
        }
    }
    if overlays && app.preview.safe_areas {
        // The borders take the preferences' share of the frame: 10% action
        // safe leaves 90% of it inside. Dashed, as VEGAS draws them.
        for pct in [prefs.action_safe, prefs.title_safe] {
            let k = 1.0 - pct.min(100) as f32 / 100.0;
            let r = Rect::from_center_size(frame_rect.center(), frame_rect.size() * k);
            dashed_rect(&painter, r.shrink(0.5), Stroke::new(1.0, Color32::WHITE));
        }
    }
    // The closed captions of the channels turned on, at the frame shown.
    if overlays && app.preview.captions.contains(&true) {
        let t = app.display_time();
        let p = app.project();
        for (ch, on) in app.preview.captions.iter().enumerate() {
            if *on {
                crate::captions_ui::paint_captions(&painter, frame_rect, &p.captions_at(ch, t));
            }
        }
    }
    // An OFX plug-in's on-screen controls, while its settings are open;
    // a Color Grading picker's click instead while one waits (on the flat
    // frame).
    if split.is_none() && !app.preview.split.on && !view_360 {
        if app.color_panel.picking.is_some() {
            super::color::pick_on_preview(ui, app, &nav, frame_rect);
        } else if app.motion.open {
            // Motion Tracking's region, while its window is open.
            crate::motion_tracking::overlay(ui, app, frame_rect, image_area);
        } else if app.preview.transform {
            // Transform mode: the event's box over its picture.
            crate::transform_ui::overlay(ui, app, frame_rect, image_area);
        } else {
            crate::ofx::overlay_ui(ui, app, &nav, frame_rect, image_area);
        }
    }

    // The transport: loop, play, pause, stop and More.
    let transport = Rect::from_min_max(
        pos2(full.min.x, full.max.y - status_h - transport_h),
        pos2(full.max.x, full.max.y - status_h),
    );
    if transport_bar {
        transport_ui(ui, app, transport);
    }
    if status_bar {
        status_ui(ui, app, full, status_h, fit);
    }
    ui.allocate_rect(full, egui::Sense::hover());
}

/// The transport bar: the buttons Edit Visible Button Set ticks (VEGAS's:
/// loop, play, pause, stop) and More, with the others.
fn transport_ui(ui: &mut egui::Ui, app: &mut BananaApp, transport: Rect) {
    use crate::transport_buttons::{Bar, TransportButton as B};
    let mut shown = app.transport_shown(Bar::Preview);
    let mut hidden = app.transport_hidden(Bar::Preview);
    // In a narrow window the buttons that do not fit go to More, which
    // stays, as VEGAS's.
    let slots = ((transport.width() - 8.0) / 24.0).floor().max(1.0) as usize;
    if shown.len() + 1 > slots {
        let over = shown.split_off(slots - 1);
        hidden.splice(0..0, over);
    }
    let mut run = None;
    let mut edit = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(transport), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let avail = ui.available_width();
            let width = (shown.len() + 1) as f32 * 24.0;
            ui.add_space((avail / 2.0 - width / 2.0).max(0.0));
            for b in &shown {
                let r = match b {
                    B::Loop => icon_toggle(ui, b.icon(), app.loop_enabled, tr("Loop Playback (Q)")),
                    B::Play => icon_toggle(ui, b.icon(), app.is_playing(), tr("Play (Space)")),
                    B::Pause => icon_button(ui, b.icon(), tr("Pause (Enter)")),
                    _ => icon_button(ui, b.icon(), b.name()),
                };
                if r.clicked() {
                    run = Some(*b);
                }
            }
            let more = icon_button(ui, Icon::More, tr("More"));
            egui::Popup::menu(&more).show(|ui| {
                for b in &hidden {
                    if ui.button(b.name()).clicked() {
                        run = Some(*b);
                        ui.close();
                    }
                }
                if !hidden.is_empty() {
                    ui.separator();
                }
                if ui.button(tr("Edit Visible Button Set…")).clicked() {
                    edit = true;
                    ui.close();
                }
            });
        });
    });
    match run {
        Some(B::Loop) => {
            app.loop_enabled = !app.loop_enabled;
            app.restart_if_playing();
        }
        Some(B::PlayFromStart) => app.play_from_start(),
        Some(B::Play) => app.toggle_play(),
        Some(B::Pause) => app.toggle_pause(),
        Some(B::Stop) => app.stop(),
        Some(B::GoStart) => app.seek_playback(Tick::ZERO),
        Some(B::PrevFrame) => app.step_frames(-1),
        Some(B::NextFrame) => app.step_frames(1),
        Some(B::GoEnd) => app.go_end(),
        // The Trimmer's own: never on this bar.
        Some(_) | None => {}
    }
    if edit {
        app.open_transport_buttons(Bar::Preview);
    }
}

/// The status line, as VEGAS's: the project's size and rate, the
/// preview's (while playing, the rate it keeps up with), the frame and
/// the display's size — the frame fitting the window, whatever the zoom
/// (orange while it is not the preview's).
fn status_ui(ui: &mut egui::Ui, app: &mut BananaApp, full: Rect, status_h: f32, fit: egui::Vec2) {
    let ppp = ui.ctx().pixels_per_point();
    let status = Rect::from_min_max(pos2(full.min.x, full.max.y - status_h), full.max);
    let (fw, fh) = app.preview.last_frame_size;
    let frame_no = app.frame_rate().frame_at(app.display_time());
    let s = app.project().settings;
    let rate = crate::fmt::num(s.frame_rate.as_f64(), 3);
    let preview_rate = match app.preview.shown_rate() {
        Some(r) => crate::fmt::num(r, 3),
        None => rate.clone(),
    };
    let (dw, dh) = ((fit.x * ppp).round() as u32, (fit.y * ppp).round() as u32);
    let scaled = (dw, dh) != (fw, fh);
    // The view transform the preview shows through, as VEGAS's ("779x438
    // sRGB (ACES)").
    let display = if s.color_managed() {
        format!(
            "{dw}x{dh} {} (ACES)",
            crate::project_properties::view_label(s.color.version, s.color.view)
        )
    } else {
        format!("{dw}x{dh}")
    };
    let items = [
        (
            Icon::File,
            theme::text(),
            format!("{}x{}; {}p", s.width, s.height, rate),
            tr("Project"),
        ),
        (
            Icon::PreviewSize,
            theme::selection(),
            format!("{fw}x{fh}; {preview_rate}p"),
            tr("Preview"),
        ),
        (
            Icon::FrameNumber,
            theme::selection(),
            frame_no.to_string(),
            tr("Frame"),
        ),
        (
            Icon::DisplaySize,
            if scaled {
                theme::ripple()
            } else {
                theme::text()
            },
            display,
            tr("Display"),
        ),
    ];
    let font = egui::FontId::proportional(12.0);
    let widths: Vec<f32> = items
        .iter()
        .map(|(_, _, text, _)| {
            ui.painter()
                .layout_no_wrap(text.clone(), font.clone(), theme::text())
                .size()
                .x
                + 22.0
        })
        .collect();
    let gap = 28.0;
    // In a narrow window, the first items that fit.
    let mut count = items.len();
    let total = |n: usize| widths[..n].iter().sum::<f32>() + gap * n.saturating_sub(1) as f32;
    while count > 0 && total(count) > status.width() - 8.0 {
        count -= 1;
    }
    let mut x = status.center().x - total(count) / 2.0;
    for ((icon, icon_color, text, tip), w) in items.into_iter().zip(widths).take(count) {
        let r = Rect::from_min_size(pos2(x, status.top()), vec2(w, status.height()));
        let ir = Rect::from_center_size(pos2(r.left() + 8.0, r.center().y), vec2(14.0, 14.0));
        crate::icons::paint(ui.painter(), ir, icon, icon_color);
        let color = if icon == Icon::DisplaySize && scaled {
            theme::ripple()
        } else {
            theme::text()
        };
        ui.painter().text(
            pos2(ir.right() + 6.0, r.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            font.clone(),
            color,
        );
        ui.interact(
            r,
            ui.id().with(("estado-da-previa", tip)),
            egui::Sense::hover(),
        )
        .on_hover_text(tip);
        x += w + gap;
    }
}

/// Whether every pixel of RGBA bytes is opaque.
fn opaque(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>()
        .0
        .chunks(1024)
        .all(|c| c.iter().fold(255u8, |a, p| a & p[3]) == 255)
}

/// [`frame_image`] into the image the last frame was shown with, once the
/// texture took its pixels (no one else holds it): a 4K frame is 33 MB,
/// and fresh memory for each one cost more than the copy.
pub(crate) fn frame_image_reusing(
    size: [usize; 2],
    rgba: &[u8],
    last: &mut Option<std::sync::Arc<ColorImage>>,
) -> egui::ImageData {
    let reused = rgba.len() == size[0] * size[1] * 4
        && opaque(rgba)
        && match last.as_mut().and_then(std::sync::Arc::get_mut) {
            Some(img) if img.size == size => {
                bytemuck::cast_slice_mut::<Color32, u8>(&mut img.pixels).copy_from_slice(rgba);
                true
            }
            _ => false,
        };
    if !reused {
        *last = Some(std::sync::Arc::new(frame_image(size, rgba)));
    }
    match last {
        Some(img) => egui::ImageData::Color(img.clone()),
        None => egui::ImageData::Color(std::sync::Arc::new(frame_image(size, rgba))),
    }
}

/// A frame as an image for the screen. Almost every frame is opaque: no
/// color then needs working out, the pixels are copied.
pub(crate) fn frame_image(size: [usize; 2], rgba: &[u8]) -> ColorImage {
    let opaque = opaque(rgba);
    if opaque && rgba.len() == size[0] * size[1] * 4 {
        // Opaque: the bytes are premultiplied pixels already, egui's own
        // layout — copied whole (egui's conversion went pixel by pixel:
        // a quarter of the interface's time at 1080p).
        ColorImage::new(size, bytemuck::cast_slice::<u8, Color32>(rgba).to_vec())
    } else if opaque {
        ColorImage::from_rgba_premultiplied(size, rgba)
    } else {
        ColorImage::from_rgba_unmultiplied(size, rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_come_out_with_their_colors() {
        let opaque = [10, 20, 30, 255, 40, 50, 60, 255];
        let image = frame_image([2, 1], &opaque);
        assert_eq!(image.pixels[1], Color32::from_rgb(40, 50, 60));
        // A pixel seen through gets its color multiplied.
        let through = [10, 20, 30, 255, 200, 100, 50, 128];
        let image = frame_image([2, 1], &through);
        assert_eq!(image.pixels[0], Color32::from_rgb(10, 20, 30));
        assert_eq!(
            image.pixels[1],
            Color32::from_rgba_unmultiplied(200, 100, 50, 128)
        );
    }

    #[test]
    fn the_zoom_steps_five_percent_and_comes_back_centered() {
        let mut p = PreviewState::default();
        p.zoom_by(5);
        assert_eq!(p.zoom, 105);
        p.zoom_by(-5);
        p.zoom_by(-5);
        assert_eq!(p.zoom, 95);
        p.center = pos2(0.2, 0.7);
        p.zoom_by(0);
        assert_eq!((p.zoom, p.center), (100, pos2(0.5, 0.5)));
        p.zoom = 12;
        p.zoom_by(-5);
        assert_eq!(p.zoom, MIN_ZOOM);
    }

    #[test]
    fn the_frame_pans_while_the_window_middle_shows_it() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        // Centered.
        let (r, _) = place_frame(area, vec2(80.0, 40.0), pos2(0.5, 0.5));
        assert_eq!(r.center(), area.center());
        // Its left edge in the middle of the window, at most.
        let (r, c) = place_frame(area, vec2(200.0, 100.0), pos2(-0.5, 0.5));
        assert_eq!(c, pos2(0.0, 0.5));
        assert_eq!(r.left(), area.center().x);
        // Not zoomed in, it moves too.
        let (r, _) = place_frame(area, vec2(80.0, 40.0), pos2(0.25, 0.5));
        assert_eq!(r.center().x, area.center().x + 20.0);
    }

    #[test]
    fn the_wheel_zooms_and_dragging_with_it_held_pans() {
        use egui_kittest::Harness;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 900.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let at = h.state().preview_spot_for_review();
        h.event(egui::Event::PointerMoved(at));
        for _ in 0..3 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        assert_eq!(h.state().preview.zoom, 130);
        let before = h.state().preview.center;
        let middle = egui::PointerButton::Middle;
        h.event(egui::Event::PointerButton {
            pos: at,
            button: middle,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        for i in 1..=10 {
            h.event(egui::Event::PointerMoved(at + vec2(i as f32 * 8.0, 0.0)));
            h.step();
        }
        h.event(egui::Event::PointerButton {
            pos: at + vec2(80.0, 0.0),
            button: middle,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        let after = h.state().preview.center;
        assert!(
            after.x < before.x,
            "dragged right, the frame shows more of its left: {before:?} → {after:?}"
        );
    }

    #[test]
    fn the_quality_reads_as_vegas_shows_it() {
        let q = PreviewQuality {
            level: RenderQuality::Best,
            size: PreviewSize::Full,
        };
        assert_eq!(q.label(), "Best (Full)");
        assert_eq!(PreviewQuality::default().label(), "Preview (Auto)");
    }

    /// As VEGAS's preview: the open hand over a picture larger than the
    /// window, the closed one while it is dragged (larger or not), none
    /// otherwise.
    #[test]
    fn a_zoomed_picture_shows_the_hand() {
        use crate::cursors::Cursor;
        assert_eq!(view_cursor(false, true, true), Some(Cursor::HandOpen));
        assert_eq!(view_cursor(false, true, false), None);
        assert_eq!(view_cursor(false, false, true), None);
        assert_eq!(view_cursor(true, false, false), Some(Cursor::HandClosed));
        assert_eq!(view_cursor(true, true, true), Some(Cursor::HandClosed));
    }
}
