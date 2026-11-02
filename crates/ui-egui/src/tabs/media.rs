//! Project Media: VEGAS's window of the project's media. Its toolbar; the
//! bins on the left (All Media, Media By Type, Tagged Media — a bin per
//! tag —, the custom bins, the smart bins and a search's results;
//! storyboards are to come); the chosen bin's media on the right, as a
//! list, details or thumbnails (a thumbnail scrubs through its video under
//! the pointer), the Media Tags panel under them (`media_tags.rs`); what
//! the selected media is at the bottom.

use std::collections::{HashMap, HashSet};

use crate::i18n::tr;
use bv_core::{Tick, TimeRange, timecode};
use bv_timeline::{BinId, ClipId, MediaId, MediaInfo, MediaSource, TrackKind};

use crate::app::{BananaApp, MediaDrag};
use crate::dock::Tab;
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets::{icon_button, icon_button_ex, icon_toggle};

/// Which media the right side shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinChoice {
    All,
    Type(MediaType),
    Custom(BinId),
    /// Tagged Media's bin of a tag.
    Tag(String),
    /// A smart bin: what its search finds now.
    Smart(BinId),
    /// What the last Search Media Bins found.
    Results,
    /// The Main Timeline storyboard: the timeline's sequence.
    MainTimeline,
    /// A storyboard of Storyboard Bins.
    Storyboard(BinId),
}

/// Media By Type's bins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Video,
    Audio,
    Image,
}

impl MediaType {
    fn of(m: &MediaInfo) -> MediaType {
        if m.still {
            MediaType::Image
        } else if m.video.is_some() {
            MediaType::Video
        } else {
            MediaType::Audio
        }
    }

    fn name(self) -> &'static str {
        match self {
            MediaType::Video => tr("Video"),
            MediaType::Audio => tr("Audio"),
            MediaType::Image => tr("Images"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            MediaType::Video => Icon::FileVideo,
            MediaType::Audio => Icon::FileAudio,
            MediaType::Image => Icon::FileImage,
        }
    }
}

/// Views: how the media show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaView {
    List,
    Details,
    Thumbnail,
}

#[derive(Debug)]
pub struct MediaWindow {
    pub bin: BinChoice,
    pub view: MediaView,
    by_type_open: bool,
    tags_open: bool,
    custom_open: bool,
    smart_open: bool,
    storyboards_open: bool,
    open_bins: HashSet<BinId>,
    /// A bin being renamed (a custom or a smart one), and its name so far.
    renaming: Option<(BinId, String)>,
    /// The last Search Media Bins' search: the Search Results bin.
    pub(crate) results: Option<bv_timeline::MediaSearch>,
    /// The Search Media Bins dialog, while open.
    pub(crate) search_dialog: Option<super::media_tags::SearchDialog>,
    /// The Media Tags panel's Add Tags box.
    pub(crate) tag_text: String,
    /// The Edit Quick Tags dialog's texts, while open.
    pub(crate) quick_tags_dialog: Option<Vec<String>>,
    /// The storyboards' picks, the Main Timeline's last refresh.
    pub(crate) story: super::storyboard::StoryboardState,
    /// Bypass Hover Scrub: thumbnails do not scrub.
    pub(crate) bypass_scrub: bool,
    /// The media picked with Ctrl and Shift (with the selected one):
    /// dragged together.
    pub(crate) picked: Vec<bv_timeline::MediaId>,
    /// The parts picked on thumbnails (dragged across, or I and O while
    /// hovering): what those media add to the timeline.
    pub(crate) parts: HashMap<MediaId, TimeRange>,
    /// The thumbnail under the pointer and the media time there, noted as
    /// it is drawn (the keys read it on the next frame).
    pub(crate) hovered: Option<(MediaId, Tick)>,
    /// A thumbnail's part being dragged.
    part_drag: Option<(MediaId, super::thumb_part::PartDrag)>,
    /// Where each thumbnail's picture was drawn (the tests' pointer).
    pub(crate) thumb_rects: HashMap<MediaId, Rect>,
    /// Where the media being dragged came from: a custom bin or a
    /// storyboard's cells, which a drop on a storyboard moves them out of.
    pub(crate) drag_from: Option<DragFrom>,
    /// The bins' row media are held over, and since when (Storyboard Bins
    /// and a storyboard open after a moment).
    dwell: Option<(u64, f64)>,
}

/// Where media dragged from Project Media came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DragFrom {
    Bin(BinId),
    /// A storyboard's cells, by place.
    Cells(BinId, Vec<usize>),
}

/// How long media are held over Storyboard Bins (or a storyboard) before
/// it opens.
const DWELL: f64 = 0.7;

impl Default for MediaWindow {
    fn default() -> MediaWindow {
        MediaWindow {
            bin: BinChoice::All,
            view: MediaView::Thumbnail,
            by_type_open: false,
            tags_open: true,
            custom_open: true,
            smart_open: true,
            storyboards_open: true,
            open_bins: HashSet::new(),
            renaming: None,
            results: None,
            search_dialog: None,
            tag_text: String::new(),
            quick_tags_dialog: None,
            story: Default::default(),
            bypass_scrub: false,
            picked: Vec::new(),
            parts: HashMap::new(),
            hovered: None,
            part_drag: None,
            drag_from: None,
            dwell: None,
            thumb_rects: HashMap::new(),
        }
    }
}

const TREE_W: f32 = 168.0;
const ROW_H: f32 = 20.0;
const INFO_H: f32 = 36.0;
/// A thumbnail of the Thumbnail view.
const THUMB: egui::Vec2 = vec2(160.0, 90.0);

fn channels(n: u16) -> String {
    match n {
        1 => "mono".to_string(),
        2 => tr("stereo").to_string(),
        n => trn!(n, "{} channel", "{} channels", n),
    }
}

fn length(m: &MediaInfo) -> String {
    let rate = m.video.map_or(bv_core::FrameRate::FPS_30, |v| v.frame_rate);
    timecode::format(m.duration, rate)
}

/// The two lines about the selected media at the bottom, as VEGAS's.
fn info_lines(m: &MediaInfo) -> [String; 2] {
    let file = match &m.source {
        MediaSource::File { path } => std::path::Path::new(path)
            .extension()
            .map(|x| format!("; {}", x.to_string_lossy().to_uppercase()))
            .unwrap_or_default(),
        MediaSource::Generated(_) => format!("; {}", tr("generated")),
    };
    // Generated media lasts as long as it is made to: no length.
    let generated = matches!(m.source, MediaSource::Generated(_));
    let len = if generated || m.still {
        String::new()
    } else {
        format!("; {}", length(m))
    };
    let video = match m.video {
        Some(v) if m.still => trf!("Video: {}x{}; image{}", v.width, v.height, file),
        Some(v) => trf!(
            "Video: {}x{}; {}{}{}",
            v.width,
            v.height,
            crate::fmt::fps(v.frame_rate),
            len,
            file
        ),
        None => String::new(),
    };
    let audio = match m.audio {
        Some(a) => trf!(
            "Audio: {} Hz, {}{}",
            a.sample_rate.0,
            channels(a.channels),
            len
        ),
        None => String::new(),
    };
    if video.is_empty() {
        [audio, String::new()]
    } else {
        [video, audio]
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    // No drag going on: none to move out of where it came from.
    if app.media_drag.is_none() {
        app.media_window.drag_from = None;
        app.media_window.dwell = None;
    }
    toolbar(ui, app);
    let area = ui.available_rect_before_wrap();
    let info = Rect::from_min_max(
        pos2(area.left(), (area.bottom() - INFO_H).max(area.top())),
        area.max,
    );
    let body = Rect::from_min_max(area.min, pos2(area.right(), info.top()));
    let id = ui.id().with("media-bins");
    let w = crate::widgets::pane_width(ui, id, body, TREE_W, 80.0, 160.0);
    let tree_right = body.left() + w;
    let tree = Rect::from_min_max(body.min, pos2(tree_right, body.bottom()));
    let list = Rect::from_min_max(pos2(tree_right + 4.0, body.top()), body.max);
    let painter = ui.painter();
    painter.hline(
        area.x_range(),
        info.top() + 0.5,
        Stroke::new(1.0, theme::panel_dark()),
    );
    ui.scope_builder(
        UiBuilder::new().max_rect(tree.shrink2(vec2(4.0, 3.0))),
        |ui| {
            egui::ScrollArea::both()
                .id_salt("project-media-bins")
                .auto_shrink([false, false])
                .show(ui, |ui| bins_ui(ui, app));
        },
    );
    // The Media Tags panel under the media (Views › Media Tags).
    let panel_h = if app.settings.media_tags_panel {
        super::media_tags::PANEL_H.min(list.height() * 0.6)
    } else {
        0.0
    };
    let (list, panel) = (
        Rect::from_min_max(list.min, pos2(list.right(), list.bottom() - panel_h)),
        Rect::from_min_max(pos2(list.left(), list.bottom() - panel_h), list.max),
    );
    // A storyboard shows as its cells.
    if matches!(
        app.media_window.bin,
        BinChoice::MainTimeline | BinChoice::Storyboard(_)
    ) {
        super::storyboard::ui(ui, app, list);
    } else {
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| list_ui(ui, app));
    }
    if panel_h > 0.0 {
        super::media_tags::tags_panel(ui, app, panel);
    }
    if let Some(m) = app.selected_media.and_then(|id| app.project().media(id)) {
        let lines = info_lines(m);
        let painter = ui.painter_at(info);
        for (k, line) in lines.iter().enumerate() {
            crate::widgets::fit_text(
                &painter,
                pos2(info.left() + 6.0, info.top() + 3.0 + 15.0 * k as f32),
                Align2::LEFT_TOP,
                line,
                FontId::proportional(12.0),
                theme::text(),
                info.width() - 12.0,
            );
        }
    }
    ui.allocate_rect(area, Sense::hover());
}

impl BananaApp {
    /// Project Media's Select Timeline Events: the events playing the
    /// media (their active takes; a subclip's, those of its parent's part)
    /// picked — added to what is with `add` (Ctrl or Shift held) —, the
    /// cursor at the first and the timeline brought to it.
    pub(crate) fn select_timeline_events(&mut self, media: MediaId, add: bool) {
        let p = self.project();
        let (media, part) = match p.subclip(media) {
            Some(s) => (s.parent, Some(s.range)),
            None => (media, None),
        };
        let mut events: Vec<(Tick, ClipId)> = p
            .clips()
            .filter(|(_, c)| c.media == media)
            .filter(|(_, c)| {
                part.is_none_or(|r| {
                    let (a, b) = (c.source_in, c.source_in + c.source_offset(c.duration));
                    a.min(b) < r.end() && a.max(b) > r.start
                })
            })
            .map(|(_, c)| (c.start, c.id))
            .collect();
        events.sort();
        if !add {
            self.selection.clear();
        }
        self.selection.extend(events.iter().map(|&(_, id)| id));
        let Some(&(t, _)) = events.first() else {
            self.notify(tr("No event of the timeline uses this media."));
            return;
        };
        self.seek(t);
        let v = &self.timeline;
        let shown = Tick((v.lanes_width as f64 * v.ticks_per_px) as i64);
        if t < v.view_start || t > v.view_start + shown {
            self.center_on_cursor();
        }
    }

    /// Project Media's Add at Project Frame Rate: the media's event at the
    /// cursor, played so that each of its frames is one of the project's
    /// (a high-frame-rate clip in slow motion: `conform_to_project_rate`),
    /// as long as all of it then lasts; one undo.
    pub(crate) fn add_at_project_frame_rate(&mut self, media: MediaId) {
        let edits = self.history.edits();
        let at = self.cursor;
        self.place_media(media, at, None);
        if self.history.edits() == edits {
            return;
        }
        let p = self.project();
        let ids: Vec<ClipId> = self.selection.iter().copied().collect();
        let first = ids
            .iter()
            .copied()
            .find(|&id| p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Video))
            .or(ids.first().copied());
        if let Some(id) = first {
            self.history.amend(|p| {
                let _ = p.conform_to_project_rate(id, true);
            });
        }
    }

    /// Project Media's Match Project Video Settings: the project's frame
    /// size, rate and pixel aspect ratio taken from the media's video, as
    /// Project Properties' Match Media Video Settings does.
    pub(crate) fn match_project_video_settings(&mut self, media: MediaId) {
        let p = self.project();
        let info = p.media(media).cloned().or_else(|| p.subclip_info(media));
        let Some((name, v)) = info.and_then(|m| Some((crate::i18n::media_name(&m), m.video?)))
        else {
            return;
        };
        let (width, height) = ((v.width & !1).max(16), (v.height & !1).max(16));
        self.modify(|p| {
            p.settings.width = width;
            p.settings.height = height;
            p.settings.frame_rate = v.frame_rate;
            p.settings.pixel_aspect = v.pixel_aspect;
        });
        self.notify(trf!(
            "The project's video settings now match {}: {}×{}, {}.",
            name,
            width,
            height,
            crate::fmt::fps(v.frame_rate)
        ));
    }

    /// A bin of the project's first two media, the first one chosen (for
    /// screen tests).
    pub fn bins_for_review(&mut self) {
        let media: Vec<MediaId> = self.project().media.iter().take(2).map(|m| m.id).collect();
        let Ok(bin) = self.modify(|p| p.add_bin(None, "Shots")) else {
            return;
        };
        for &m in &media {
            self.modify(|p| {
                let _ = p.add_to_bin(bin, m);
            });
        }
        self.selected_media = media.first().copied();
    }

    /// Project Media's Thumbnail view with `video` imported and its
    /// thumbnail's middle third picked, and a storyboard of that part
    /// (screen tests). The media, or `None` when it could not be read.
    pub fn media_parts_for_review(&mut self, video: &std::path::Path) -> Option<MediaId> {
        let id = *self.import_paths(&[video.to_path_buf()]).first()?;
        let len = self.project().media(id)?.duration;
        let part = TimeRange::from_bounds(Tick(len.0 / 3), Tick(len.0 * 2 / 3));
        self.media_window.view = MediaView::Thumbnail;
        self.media_window.bin = BinChoice::All;
        self.media_window.parts.insert(id, part);
        self.selected_media = Some(id);
        self.modify(|p| {
            let board = p.add_storyboard("Cut");
            p.storyboard_drop(board, 0, bv_timeline::StoryCell::of(id, Some(part)), false);
        });
        Some(id)
    }

    /// Project Media on the storyboard called `name` (screen tests).
    pub fn storyboard_for_review(&mut self, name: &str) {
        if let Some(s) = self.project().storyboards.iter().find(|s| s.name == name) {
            self.media_window.bin = BinChoice::Storyboard(s.id);
        }
    }

    /// Project Media's Details view, its columns to Tags (screen tests).
    pub fn media_details_for_review(&mut self) {
        self.media_window.view = MediaView::Details;
    }

    /// Project Media's Thumbnail view on All Media (screen tests).
    pub fn media_thumbnails_for_review(&mut self) {
        self.media_window.view = MediaView::Thumbnail;
        self.media_window.bin = BinChoice::All;
    }

    /// Where the thumbnail of the media of `file` (`None`: the first
    /// generated media) was drawn last (screen tests).
    pub fn media_thumb_rect_for_review(&self, file: Option<&std::path::Path>) -> Option<Rect> {
        let m = self
            .project()
            .media
            .iter()
            .find(|m| match (&m.source, file) {
                (MediaSource::File { path }, Some(f)) => std::path::Path::new(path) == f,
                (MediaSource::Generated(_), None) => true,
                _ => false,
            })?;
        self.media_window.thumb_rects.get(&m.id).copied()
    }
}

/// VEGAS's toolbar: Remove All Unused Media, Import Media and its menu,
/// Swap Media, Capture Video, Extract Audio from CD, Remove Selected Media,
/// Properties, Media FX, the previews, Views, Search and Bypass Hover
/// Scrub.
fn toolbar(ui: &mut Ui, app: &mut BananaApp) {
    let selected = app
        .selected_media
        .filter(|&id| app.project().media(id).is_some());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let used: Vec<MediaId> = app.project().clips().map(|(_, c)| c.media).collect();
        let unused: Vec<MediaId> = app
            .project()
            .media
            .iter()
            .filter(|m| !used.contains(&m.id))
            .map(|m| m.id)
            .collect();
        if icon_button_ex(
            ui,
            Icon::RemoveUnused,
            tr("Remove All Unused Media from Project"),
            false,
            !unused.is_empty(),
        )
        .clicked()
        {
            for id in &unused {
                app.remove_media(*id);
            }
            app.notify(trn!(
                unused.len(),
                "{} unused media file removed.",
                "{} unused media files removed.",
                unused.len()
            ));
        }
        // Import Media…, with its menu.
        if text_button(ui, Icon::Import, tr("Import Media…"))
            .on_hover_text(tr("Import Media (Ctrl+I)"))
            .clicked()
        {
            app.import_dialog();
        }
        let more = arrow_button(ui);
        egui::Popup::menu(&more).show(|ui| {
            crate::menus::cmd(ui, app, crate::commands::Cmd::ImportMedia);
            crate::menus::cmd(ui, app, crate::commands::Cmd::ImportImageSequence);
            crate::menus::cmd(ui, app, crate::commands::Cmd::ImportDvdCamcorder);
            ui.separator();
            if crate::menus::item(ui, tr("Screen Capture…")) {
                app.open_screen_capture();
            }
            crate::menus::soon(ui, tr("Extract Audio from CD"), "");
            crate::menus::soon(ui, tr("Get Media from the Web"), "");
        });
        // Swap Media: the project's video files swapped for those of the
        // same names in a folder (as the menu's Swap Video Files).
        if icon_button_ex(ui, Icon::Swap, tr("Swap Media"), false, true).clicked() {
            app.swap_video_files_dialog();
        }
        if icon_button_ex(ui, Icon::Capture, tr("Start Screen Capture"), false, true).clicked() {
            app.open_screen_capture();
        }
        icon_button_ex(ui, Icon::Disc, tr("Extract Audio from CD"), false, false)
            .on_disabled_hover_text(tr("Coming soon to BananaV"));
        if icon_button_ex(
            ui,
            Icon::Delete,
            tr("Remove Selected Media from Project"),
            false,
            selected.is_some(),
        )
        .clicked()
            && let Some(id) = selected
        {
            app.request_remove_media(id);
        }
        if icon_button_ex(
            ui,
            Icon::Settings,
            tr("Properties"),
            false,
            selected.is_some(),
        )
        .clicked()
            && let Some(id) = selected
        {
            let picked = &app.media_window.picked;
            app.dialogs.media_properties = if picked.len() > 1 && picked.contains(&id) {
                crate::media_properties::MediaPropertiesDialog::new_many(app, &picked.clone())
            } else {
                crate::media_properties::MediaPropertiesDialog::new(app, id)
            };
        }
        if icon_button_ex(ui, Icon::Fx, tr("Media FX"), false, selected.is_some()).clicked()
            && let Some(id) = selected
        {
            let audio_only = app.project().media(id).is_some_and(|m| m.video.is_none());
            app.clip_windows.media_fx = Some((id, audio_only));
        }
        // Previews: the media alone through the preview bus.
        let storyboard = matches!(
            app.media_window.bin,
            BinChoice::MainTimeline | BinChoice::Storyboard(_)
        );
        if icon_button_ex(
            ui,
            Icon::Play,
            tr("Start Preview"),
            false,
            selected.is_some() || storyboard,
        )
        .clicked()
        {
            if storyboard {
                app.preview_storyboard();
            } else if let Some(id) = selected {
                preview(app, id);
            }
        }
        if icon_button_ex(ui, Icon::Stop, tr("Stop Preview"), false, app.previewing()).clicked() {
            app.stop_preview(false);
        }
        let auto = app.settings.media_auto_preview;
        if crate::widgets::icon_toggle(ui, Icon::AutoPreview, auto, tr("Auto Preview")).clicked() {
            app.settings.media_auto_preview = !auto;
            let _ = app.settings.save();
        }
        // Refresh Storyboard, lit while the Main Timeline storyboard shows
        // an older state of the timeline; its menu's automatic sync.
        let stale = app.storyboard_stale();
        if crate::widgets::icon_button_ex(ui, Icon::Refresh, tr("Refresh Storyboard"), stale, true)
            .clicked()
        {
            app.refresh_storyboard();
        }
        let sync = arrow_button(ui);
        egui::Popup::menu(&sync).show(|ui| {
            let on = app.settings.storyboard_auto_sync;
            if crate::menus::toggle_item(
                ui,
                tr("Automatically Synchronize Main Timeline Storyboard"),
                on,
            ) {
                app.settings.storyboard_auto_sync = !on;
                let _ = app.settings.save();
                app.refresh_storyboard();
                ui.close();
            }
        });
        let views = icon_button(ui, Icon::Views, tr("Views"));
        egui::Popup::menu(&views).show(|ui| {
            let w = &mut app.media_window;
            for (view, name) in [
                (MediaView::List, tr("List")),
                (MediaView::Details, tr("Details")),
                (MediaView::Thumbnail, tr("Thumbnail")),
            ] {
                if crate::widgets::radio(ui, w.view == view, name).clicked() {
                    w.view = view;
                    ui.close();
                }
            }
            ui.separator();
            let shown = app.settings.media_tags_panel;
            if crate::menus::toggle_item(ui, tr("Media Tags"), shown) {
                app.settings.media_tags_panel = !shown;
                let _ = app.settings.save();
                ui.close();
            }
        });
        // Search Media Bins: in the custom bin chosen, else in all.
        if icon_button(ui, Icon::ZoomTool, tr("Search Media Bins")).clicked() {
            let within = match app.media_window.bin {
                BinChoice::Custom(b) => bv_timeline::SearchIn::Bin(b),
                _ => bv_timeline::SearchIn::All,
            };
            super::media_tags::open_search(app, within);
        }
        let w = &mut app.media_window;
        if icon_toggle(ui, Icon::BypassFx, w.bypass_scrub, tr("Bypass Hover Scrub")).clicked() {
            w.bypass_scrub = !w.bypass_scrub;
        }
    });
}

/// A toolbar button with an icon and a name (Import Media…).
fn text_button(ui: &mut Ui, icon: Icon, text: &str) -> egui::Response {
    let font = FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, theme::text());
    let size = vec2(galley.size().x + 32.0, crate::widgets::BUTTON.y);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let ir = Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(16.0, 16.0));
    icons::paint(painter, ir, icon, theme::text());
    painter.galley(
        pos2(ir.right() + 6.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::text(),
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    resp
}

/// The ▾ that opens a toolbar button's menu.
fn arrow_button(ui: &mut Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(14.0, crate::widgets::BUTTON.y), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let c = rect.center();
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-4.0, -2.0),
            c + vec2(4.0, -2.0),
            c + vec2(0.0, 3.0),
        ],
        theme::text(),
        Stroke::NONE,
    ));
    resp
}

/// A line of the bins' tree; what it was asked.
#[derive(Default)]
struct RowResponse {
    toggled: bool,
    clicked: bool,
    /// Media dropped on it.
    dropped: Option<MediaId>,
    response: Option<egui::Response>,
}

#[allow(clippy::too_many_arguments)]
fn bin_row(
    ui: &mut Ui,
    app: &BananaApp,
    depth: usize,
    name: &str,
    icon: Icon,
    open: Option<bool>,
    selected: bool,
    ready: bool,
    drop_target: bool,
) -> RowResponse {
    let indent = 2.0 + 14.0 * depth as f32;
    let width = (indent + 48.0 + name.chars().count() as f32 * 7.5).max(ui.available_width());
    let sense = if ready {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), sense);
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, ready, selected, name)
    });
    // Media dragged over a bin it can go into.
    let dragging = match app.media_drag {
        Some(MediaDrag::Media(id) | MediaDrag::MediaPart(id, _)) if drop_target => Some(id),
        _ => None,
    };
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let under = dragging.is_some() && pointer.is_some_and(|p| rect.contains(p));
    let painter = ui.painter();
    if under {
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.5, theme::banana()),
            egui::StrokeKind::Inside,
        );
    } else if selected {
        painter.rect_filled(rect, 0.0, theme::header_selected());
    } else if resp.hovered() && ready {
        painter.rect_filled(rect, 0.0, theme::hover());
    }
    let color = if ready {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let arrow = Rect::from_center_size(
        pos2(rect.left() + indent + 6.0, rect.center().y),
        vec2(12.0, 12.0),
    );
    if let Some(open) = open {
        let c = arrow.center();
        let pts = if open {
            vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ]
        } else {
            vec![
                c + vec2(-2.0, -4.0),
                c + vec2(3.0, 0.0),
                c + vec2(-2.0, 4.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(
            pts,
            theme::text_dim(),
            Stroke::NONE,
        ));
    }
    let ir = Rect::from_center_size(
        pos2(arrow.right() + 11.0, rect.center().y),
        vec2(15.0, 15.0),
    );
    let icon_color = match (icon, ready) {
        (Icon::Folder, true) => theme::banana(),
        _ => color,
    };
    icons::paint(painter, ir, icon, icon_color);
    painter.text(
        pos2(ir.right() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        color,
    );
    let on_arrow = resp
        .interact_pointer_pos()
        .is_some_and(|p| p.x <= arrow.right() + 2.0);
    let released = ui.input(|i| i.pointer.any_released());
    let resp = if ready {
        resp
    } else {
        resp.on_hover_text(tr("Coming soon to BananaV"))
    };
    RowResponse {
        toggled: open.is_some() && (resp.double_clicked() || (resp.clicked() && on_arrow)),
        clicked: resp.clicked() && !on_arrow,
        dropped: if under && released { dragging } else { None },
        response: Some(resp),
    }
}

/// The bins: All Media, Media By Type, Tagged Media, Custom Bins (the
/// user's, made with the menu, filled by dragging media onto them), Smart
/// Bins and Storyboard Bins.
fn bins_ui(ui: &mut Ui, app: &mut BananaApp) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let w = &app.media_window;
    let all = bin_row(
        ui,
        app,
        0,
        tr("All Media"),
        Icon::FilmStrip,
        None,
        w.bin == BinChoice::All,
        true,
        false,
    );
    if all.clicked {
        app.media_window.bin = BinChoice::All;
    }
    if let Some(resp) = all.response {
        resp.context_menu(|ui| {
            if ui.button(tr("Search Media Bins…")).clicked() {
                super::media_tags::open_search(app, bv_timeline::SearchIn::All);
                ui.close();
            }
        });
    }
    let open = app.media_window.by_type_open;
    let by_type = bin_row(
        ui,
        app,
        0,
        tr("Media By Type"),
        Icon::FilmStrip,
        Some(open),
        false,
        true,
        false,
    );
    if by_type.toggled || by_type.clicked {
        app.media_window.by_type_open = !open;
    }
    if open {
        for kind in [MediaType::Video, MediaType::Audio, MediaType::Image] {
            let sel = app.media_window.bin == BinChoice::Type(kind);
            if bin_row(ui, app, 1, kind.name(), kind.icon(), None, sel, true, false).clicked {
                app.media_window.bin = BinChoice::Type(kind);
            }
        }
    }
    // Tagged Media: a bin per tag.
    let tags = app.project().all_tags();
    let open = app.media_window.tags_open;
    let tagged = bin_row(
        ui,
        app,
        0,
        tr("Tagged Media"),
        Icon::FilmStrip,
        (!tags.is_empty()).then_some(open),
        false,
        true,
        false,
    );
    if tagged.toggled || tagged.clicked {
        app.media_window.tags_open = !open;
    }
    if open {
        for tag in tags {
            let choice = BinChoice::Tag(tag.clone());
            let sel = app.media_window.bin == choice;
            let row = bin_row(ui, app, 1, &tag, Icon::Marker, None, sel, true, true);
            if row.clicked {
                app.media_window.bin = choice;
            }
            // Media let go on a tag get it.
            if let Some(media) = row.dropped {
                app.media_drag = None;
                app.modify(|p| p.add_tag(media, &tag));
            }
        }
    }
    let open = app.media_window.custom_open;
    let custom = bin_row(
        ui,
        app,
        0,
        tr("Custom Bins"),
        Icon::Folder,
        Some(open),
        false,
        true,
        false,
    );
    if custom.toggled || custom.clicked {
        app.media_window.custom_open = !open;
    }
    if let Some(resp) = custom.response {
        resp.context_menu(|ui| {
            if ui.button(tr("Create New Bin")).clicked() {
                new_bin(app, None);
                ui.close();
            }
        });
    }
    if open {
        custom_bins(ui, app, None, 1);
    }
    smart_bins(ui, app);
    let open = app.media_window.storyboards_open;
    let boards = bin_row(
        ui,
        app,
        0,
        tr("Storyboard Bins"),
        Icon::Folder,
        Some(open),
        false,
        true,
        false,
    );
    if boards.toggled || boards.clicked {
        app.media_window.storyboards_open = !open;
    }
    if dwelt(ui, app, &boards, 0) {
        app.media_window.storyboards_open = true;
    }
    if let Some(resp) = boards.response {
        resp.context_menu(|ui| {
            if ui.button(tr("Create New Bin")).clicked() {
                let name = tr("New Storyboard");
                let id = app.modify(|p| p.add_storyboard(name));
                app.media_window.storyboards_open = true;
                app.media_window.bin = BinChoice::Storyboard(id);
                app.media_window.renaming = Some((id, name.to_string()));
                ui.close();
            }
        });
    }
    if open {
        storyboard_bins(ui, app);
    }
    results_bin(ui, app);
}

/// Whether media dragged have been held over `row` for a moment (once
/// each time they come over it); `key` names the row.
fn dwelt(ui: &Ui, app: &mut BananaApp, row: &RowResponse, key: u64) -> bool {
    let Some(resp) = &row.response else {
        return false;
    };
    let dragging = app.media_drag.as_ref().is_some_and(MediaDrag::is_media);
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let over = dragging && pointer.is_some_and(|p| resp.rect.contains(p));
    let now = ui.input(|i| i.time);
    let w = &mut app.media_window;
    match w.dwell {
        Some((k, _)) if k == key && !over => w.dwell = None,
        Some((k, since)) if k == key => {
            if since.is_finite() {
                if now - since >= DWELL {
                    w.dwell = Some((key, f64::INFINITY));
                    return true;
                }
                ui.ctx().request_repaint();
            }
        }
        _ if over => {
            w.dwell = Some((key, now));
            ui.ctx().request_repaint();
        }
        _ => {}
    }
    false
}

impl BananaApp {
    /// `media` let go on storyboard `to`: out of the custom bin or the
    /// other storyboard they were dragged from, unless `copy` (Ctrl).
    pub(crate) fn moved_into_storyboard(&mut self, to: BinId, copy: bool, media: &[MediaId]) {
        let from = self.media_window.drag_from.take();
        if copy {
            return;
        }
        match from {
            Some(DragFrom::Bin(bin)) => {
                self.history.amend(|p| {
                    for &m in media {
                        let _ = p.remove_from_bin(bin, m);
                    }
                });
            }
            Some(DragFrom::Cells(board, mut cells)) if board != to => {
                cells.sort_unstable();
                self.history.amend(|p| {
                    for &i in cells.iter().rev() {
                        p.storyboard_remove(board, i);
                    }
                });
            }
            _ => {}
        }
    }
}

/// Storyboard Bins' storyboards: the Main Timeline's, then the project's
/// (renamed, deleted, dragged onto the timeline whole; media let go on
/// one go to its end).
fn storyboard_bins(ui: &mut Ui, app: &mut BananaApp) {
    let sel = app.media_window.bin == BinChoice::MainTimeline;
    let main = bin_row(
        ui,
        app,
        1,
        tr("Main Timeline"),
        Icon::FilmStrip,
        None,
        sel,
        true,
        false,
    );
    if main.clicked || dwelt(ui, app, &main, 1) {
        app.media_window.bin = BinChoice::MainTimeline;
        app.media_window.story.picked.clear();
    }
    let boards: Vec<(BinId, String)> = app
        .project()
        .storyboards
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    for (id, name) in boards {
        if renamed(ui, app, id, 1, |p, new| {
            if let Some(s) = p.storyboard_mut(id) {
                s.name = new.to_string();
            }
        }) {
            continue;
        }
        let sel = app.media_window.bin == BinChoice::Storyboard(id);
        let row = bin_row(ui, app, 1, &name, Icon::FilmStrip, None, sel, true, true);
        if row.clicked || dwelt(ui, app, &row, 2 + id.0) {
            app.media_window.bin = BinChoice::Storyboard(id);
            app.media_window.story.picked.clear();
        }
        if let Some(media) = row.dropped {
            let part = match app.media_drag.take() {
                Some(MediaDrag::MediaPart(_, r)) => Some(r),
                _ => None,
            };
            let copy = ui.input(|i| i.modifiers.command);
            app.modify(|p| {
                let end = p.storyboard(id).map_or(0, |s| s.cells.len());
                p.storyboard_drop(id, end, bv_timeline::StoryCell::of(media, part), false);
            });
            app.moved_into_storyboard(id, copy, &[media]);
        }
        let Some(resp) = row.response else {
            continue;
        };
        // Dragged onto the timeline: all its cells.
        let drag = ui.interact(resp.rect, resp.id.with("drag"), Sense::drag());
        if drag.drag_started()
            && let Some(s) = app.project().storyboard(id)
        {
            app.media_drag = Some(MediaDrag::Cells(s.cells.clone()));
        }
        resp.context_menu(|ui| {
            if ui.button(tr("Rename Bin")).clicked() {
                app.media_window.renaming = Some((id, name.clone()));
                ui.close();
            }
            if ui.button(tr("Delete Bin")).clicked() {
                if app.media_window.bin == BinChoice::Storyboard(id) {
                    app.media_window.bin = BinChoice::MainTimeline;
                }
                app.modify(|p| p.remove_storyboard(id));
                ui.close();
            }
        });
    }
}

/// Smart Bins: searches kept, each showing what it finds now; Edit Search,
/// Rename and Delete in their menus. Then the last search's results, to
/// keep as a bin or a smart bin.
fn smart_bins(ui: &mut Ui, app: &mut BananaApp) {
    let bins: Vec<(BinId, String)> = app
        .project()
        .smart_bins
        .iter()
        .map(|b| (b.id, b.name.clone()))
        .collect();
    let open = app.media_window.smart_open;
    let row = bin_row(
        ui,
        app,
        0,
        tr("Smart Bins"),
        Icon::Folder,
        (!bins.is_empty()).then_some(open),
        false,
        true,
        false,
    );
    if row.toggled || row.clicked {
        app.media_window.smart_open = !open;
    }
    if open {
        for (id, name) in bins {
            if renamed(ui, app, id, 1, |p, new| {
                if let Some(b) = p.smart_bin_mut(id) {
                    b.name = new.to_string();
                }
            }) {
                continue;
            }
            let sel = app.media_window.bin == BinChoice::Smart(id);
            let row = bin_row(ui, app, 1, &name, Icon::ZoomTool, None, sel, true, false);
            if row.clicked {
                app.media_window.bin = BinChoice::Smart(id);
            }
            if let Some(resp) = row.response {
                resp.context_menu(|ui| {
                    if ui.button(tr("Edit Search…")).clicked() {
                        super::media_tags::edit_smart_bin(app, id);
                        ui.close();
                    }
                    if ui.button(tr("Rename Bin")).clicked() {
                        app.media_window.renaming = Some((id, name.clone()));
                        ui.close();
                    }
                    if ui.button(tr("Delete Bin")).clicked() {
                        if app.media_window.bin == BinChoice::Smart(id) {
                            app.media_window.bin = BinChoice::All;
                        }
                        app.modify(|p| p.remove_smart_bin(id));
                        ui.close();
                    }
                });
            }
        }
    }
}

/// The last search's results, at the tree's end while there are any.
fn results_bin(ui: &mut Ui, app: &mut BananaApp) {
    let Some(search) = app.media_window.results.clone() else {
        return;
    };
    let sel = app.media_window.bin == BinChoice::Results;
    let row = bin_row(
        ui,
        app,
        0,
        tr("Search Results"),
        Icon::ZoomTool,
        None,
        sel,
        true,
        false,
    );
    if row.clicked {
        app.media_window.bin = BinChoice::Results;
    }
    let Some(resp) = row.response else {
        return;
    };
    let resp = resp.on_hover_text(super::media_tags::search_summary(&search));
    resp.context_menu(|ui| {
        if ui.button(tr("Save As Bin")).clicked() {
            let name = tr("Search Results");
            let id = app.modify(|p| p.save_search_as_bin(name, &search));
            app.media_window.custom_open = true;
            if let Some(parent) = app.project().bin(id).and_then(|b| b.parent) {
                app.media_window.open_bins.insert(parent);
            }
            app.media_window.bin = BinChoice::Custom(id);
            app.media_window.renaming = Some((id, name.to_string()));
            ui.close();
        }
        if ui.button(tr("Save As Smart Bin")).clicked() {
            let name = tr("New Smart Bin");
            let s = search.clone();
            let id = app.modify(|p| p.add_smart_bin(name, s));
            app.media_window.smart_open = true;
            app.media_window.bin = BinChoice::Smart(id);
            app.media_window.renaming = Some((id, name.to_string()));
            ui.close();
        }
        if ui.button(tr("Edit Search…")).clicked() {
            app.media_window.search_dialog = Some(super::media_tags::SearchDialog {
                search: search.clone(),
                editing: None,
            });
            ui.close();
        }
    });
}

/// While bin `id` is being renamed: its name in a box, `rename` setting
/// it when done. Whether it is.
fn renamed(
    ui: &mut Ui,
    app: &mut BananaApp,
    id: BinId,
    depth: usize,
    rename: impl FnOnce(&mut bv_timeline::Project, &str),
) -> bool {
    let Some((rid, text)) = &mut app.media_window.renaming else {
        return false;
    };
    if *rid != id {
        return false;
    }
    let mut done = None;
    ui.horizontal(|ui| {
        ui.add_space(2.0 + 14.0 * depth as f32 + 30.0);
        let r = ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
        r.request_focus();
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            done = Some(None);
        } else if r.lost_focus() {
            done = Some(Some(text.trim().to_string()));
        }
    });
    match done {
        Some(Some(new)) if !new.is_empty() => {
            app.media_window.renaming = None;
            app.modify(|p| rename(p, &new));
        }
        Some(_) => app.media_window.renaming = None,
        None => {}
    }
    true
}

/// A new bin in `parent`, named "New Bin", ready to be renamed.
fn new_bin(app: &mut BananaApp, parent: Option<BinId>) {
    let name = tr("New Bin");
    if let Ok(id) = app.modify(|p| p.add_bin(parent, name)) {
        if let Some(p) = parent {
            app.media_window.open_bins.insert(p);
        }
        app.media_window.custom_open = true;
        app.media_window.bin = BinChoice::Custom(id);
        app.media_window.renaming = Some((id, name.to_string()));
    }
}

/// The custom bins in `parent`, and the bins in each that is open.
fn custom_bins(ui: &mut Ui, app: &mut BananaApp, parent: Option<BinId>, depth: usize) {
    let bins: Vec<(BinId, String, bool)> = app
        .project()
        .bins_in(parent)
        .iter()
        .map(|b| {
            (
                b.id,
                b.name.clone(),
                !app.project().bins_in(Some(b.id)).is_empty(),
            )
        })
        .collect();
    for (id, name, has_bins) in bins {
        // Renaming: the name in a box.
        if renamed(ui, app, id, depth, |p, new| {
            let _ = p.rename_bin(id, new);
        }) {
            continue;
        }
        let open = app.media_window.open_bins.contains(&id);
        let sel = app.media_window.bin == BinChoice::Custom(id);
        let row = bin_row(
            ui,
            app,
            depth,
            &name,
            Icon::Folder,
            has_bins.then_some(open),
            sel,
            true,
            true,
        );
        if row.toggled {
            if open {
                app.media_window.open_bins.remove(&id);
            } else {
                app.media_window.open_bins.insert(id);
            }
        } else if row.clicked {
            app.media_window.bin = BinChoice::Custom(id);
        }
        if let Some(media) = row.dropped {
            app.media_drag = None;
            app.modify(|p| {
                let _ = p.add_to_bin(id, media);
            });
        }
        if let Some(resp) = row.response {
            resp.context_menu(|ui| {
                if ui.button(tr("Create New Bin")).clicked() {
                    new_bin(app, Some(id));
                    ui.close();
                }
                if ui.button(tr("Search Media Bins…")).clicked() {
                    super::media_tags::open_search(app, bv_timeline::SearchIn::Bin(id));
                    ui.close();
                }
                if ui.button(tr("Rename Bin")).clicked() {
                    app.media_window.renaming = Some((id, name.clone()));
                    ui.close();
                }
                if ui.button(tr("Delete Bin")).clicked() {
                    if app.media_window.bin == BinChoice::Custom(id) {
                        app.media_window.bin = BinChoice::All;
                    }
                    app.modify(|p| {
                        let _ = p.remove_bin(id);
                    });
                    ui.close();
                }
            });
        }
        if open && depth < 16 {
            custom_bins(ui, app, Some(id), depth + 1);
        }
    }
}

/// The media of the chosen bin, in the project's order.
fn shown(app: &BananaApp) -> Vec<MediaInfo> {
    let w = &app.media_window;
    let p = app.project();
    // A tag's, a smart bin's or the results' media.
    let found: Option<Vec<MediaId>> = match &w.bin {
        BinChoice::Tag(t) => Some(p.media_tagged(t)),
        BinChoice::Smart(b) => Some(
            p.smart_bin(*b)
                .map_or_else(Vec::new, |b| p.search_media(&b.search)),
        ),
        BinChoice::Results => Some(
            w.results
                .as_ref()
                .map_or_else(Vec::new, |s| p.search_media(s)),
        ),
        _ => None,
    };
    p.media
        .iter()
        .flat_map(|m| {
            // Each media, then its subclips.
            std::iter::once(m.clone()).chain(
                p.subclips
                    .iter()
                    .filter(|s| s.parent == m.id)
                    .filter_map(|s| p.subclip_info(s.id)),
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|m| match &w.bin {
            BinChoice::All => true,
            BinChoice::Type(t) => MediaType::of(m) == *t,
            BinChoice::Custom(b) => p.bin(*b).is_some_and(|b| b.media.contains(&m.id)),
            _ => found.as_ref().is_some_and(|f| f.contains(&m.id)),
        })
        .collect()
}

/// The chosen bin's media, as the view says.
fn list_ui(ui: &mut Ui, app: &mut BananaApp) {
    let media = shown(app);
    let view = app.media_window.view;
    egui::ScrollArea::vertical()
        .id_salt("project-media-list")
        .auto_shrink([false, false])
        .show(ui, |ui| match view {
            MediaView::Thumbnail => {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 10.0);
                    ui.add_space(6.0);
                    for m in &media {
                        thumbnail_cell(ui, app, m);
                    }
                });
            }
            MediaView::List | MediaView::Details => {
                ui.spacing_mut().item_spacing.y = 0.0;
                let details = view == MediaView::Details;
                if details {
                    details_header(ui);
                }
                for m in &media {
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), ROW_H + 2.0),
                        Sense::click_and_drag(),
                    );
                    if ui.is_rect_visible(rect) {
                        paint_row(ui, app, m, rect, &resp, details);
                    }
                    media_interactions(app, m, resp, false);
                }
            }
        });
}

/// The details' columns: name, length, video, audio, tags.
fn columns(width: f32) -> [f32; 5] {
    let (length, video, audio, tags) = (90.0, 130.0, 120.0, 120.0);
    [
        (width - length - video - audio - tags).max(120.0),
        length,
        video,
        audio,
        tags,
    ]
}

fn details_header(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::panel_dark());
    let cols = columns(rect.width());
    let mut x = rect.left();
    for (k, name) in [
        tr("Name"),
        tr("Length"),
        tr("Video"),
        tr("Audio"),
        tr("Tags"),
    ]
    .into_iter()
    .enumerate()
    {
        crate::widgets::fit_text(
            ui.painter(),
            pos2(x + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(12.0),
            theme::text_dim(),
            cols[k] - 10.0,
        );
        x += cols[k];
        ui.painter().vline(
            x - 0.5,
            rect.y_range(),
            Stroke::new(1.0, theme::separator()),
        );
    }
}

/// A line of the List and Details views.
fn paint_row(
    ui: &Ui,
    app: &BananaApp,
    m: &MediaInfo,
    rect: Rect,
    resp: &egui::Response,
    details: bool,
) {
    let painter = ui.painter_at(rect);
    let selected = app.selected_media == Some(m.id) || app.media_window.picked.contains(&m.id);
    if selected {
        painter.rect_filled(rect, 0.0, theme::selection());
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::hover());
    }
    let color = if selected {
        Color32::WHITE
    } else {
        theme::text()
    };
    icons::paint(
        &painter,
        Rect::from_min_size(rect.min + vec2(6.0, 3.0), vec2(16.0, 16.0)),
        MediaType::of(m).icon(),
        color,
    );
    let cols = columns(rect.width());
    let room = if details {
        cols[0] - 32.0
    } else {
        rect.width() - 34.0
    };
    crate::widgets::fit_text(
        &painter,
        rect.left_center() + vec2(28.0, 0.0),
        Align2::LEFT_CENTER,
        crate::i18n::media_name(m),
        FontId::proportional(13.0),
        color,
        room,
    );
    if details {
        let video = match m.video {
            Some(v) if m.still => format!("{}×{}", v.width, v.height),
            Some(v) => format!("{}×{} {}", v.width, v.height, crate::fmt::fps(v.frame_rate)),
            None => String::new(),
        };
        let audio = m
            .audio
            .map(|a| format!("{} Hz {}", a.sample_rate.0, channels(a.channels)))
            .unwrap_or_default();
        let len = if m.still || matches!(m.source, MediaSource::Generated(_)) {
            String::new()
        } else {
            length(m)
        };
        let tags = app.project().tags_of(m.id).join(", ");
        let mut x = rect.left() + cols[0];
        for (k, text) in [len, video, audio, tags].into_iter().enumerate() {
            crate::widgets::fit_text(
                &painter,
                pos2(x + 6.0, rect.center().y),
                Align2::LEFT_CENTER,
                &text,
                FontId::proportional(12.0),
                if selected {
                    Color32::WHITE
                } else {
                    theme::text_dim()
                },
                cols[k + 1] - 10.0,
            );
            x += cols[k + 1];
        }
    }
}

/// Whether a media's thumbnail spans its video, so that a part of it can
/// be picked there.
fn has_parts(m: &MediaInfo) -> bool {
    m.video.is_some()
        && !m.still
        && m.duration > Tick::ZERO
        && !matches!(m.source, MediaSource::Generated(_))
}

/// A thumbnail's part: a drag across the picture picks it or moves an
/// end, the picture showing the frame there. Whether the drag was taken.
fn part_drag(app: &mut BananaApp, m: &MediaInfo, pic: Rect, resp: &egui::Response) -> bool {
    use super::thumb_part;
    if !has_parts(m) {
        return false;
    }
    let rate = m.video.map(|v| v.frame_rate);
    let w = &mut app.media_window;
    let part = w.parts.get(&m.id).copied();
    if resp.drag_started()
        && let Some(o) = thumb_part::across(resp).filter(|o| pic.contains(*o))
    {
        let drag = thumb_part::drag_at(pic, o.x, m.duration, rate, part);
        w.part_drag = Some((m.id, drag));
    }
    let Some((id, drag)) = w.part_drag.filter(|(id, _)| *id == m.id) else {
        return false;
    };
    if let Some(p) = resp.interact_pointer_pos() {
        let t = thumb_part::time_across(pic, p.x, m.duration, rate);
        match thumb_part::dragged(drag, part, t, m.duration) {
            Some(r) => w.parts.insert(id, r),
            None => w.parts.remove(&id),
        };
        w.hovered = Some((id, t));
    }
    if resp.drag_stopped() || !resp.dragged() {
        w.part_drag = None;
    }
    true
}

impl BananaApp {
    /// I and O while the pointer is over a thumbnail (last frame's): its
    /// part starts or ends there.
    pub(crate) fn media_part_keys(
        &mut self,
        ctx: &egui::Context,
        hovered: Option<(MediaId, Tick)>,
    ) {
        let Some((id, t)) = hovered else {
            return;
        };
        let p = self.project();
        let Some(length) = p
            .media(id)
            .map(|m| m.duration)
            .or_else(|| p.subclip_info(id).map(|m| m.duration))
        else {
            return;
        };
        for (key, start) in [(egui::Key::I, true), (egui::Key::O, false)] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                let parts = &mut self.media_window.parts;
                let part = super::thumb_part::mark(parts.get(&id).copied(), t, length, start);
                parts.insert(id, part);
            }
        }
    }
}

/// Media `id` (its thumbnail's part, when one is picked) placed at `at`.
fn place_part(app: &mut BananaApp, id: MediaId, at: Tick) {
    let part = app.media_window.parts.get(&id).copied();
    app.place_media_range(id, at, part, None);
}

/// A media of the Thumbnail view: its picture (scrubbing through the video
/// under the pointer; a part picked lighter) and its name.
fn thumbnail_cell(ui: &mut Ui, app: &mut BananaApp, m: &MediaInfo) {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(THUMB.x, THUMB.y + 32.0), Sense::click_and_drag());
    let pic = Rect::from_min_size(rect.min, THUMB);
    app.media_window.thumb_rects.insert(m.id, pic);
    let taken = part_drag(app, m, pic, &resp);
    if ui.is_rect_visible(rect) {
        let selected = app.selected_media == Some(m.id) || app.media_window.picked.contains(&m.id);
        let painter = ui.painter_at(rect);
        if selected {
            painter.rect_filled(
                rect.expand(3.0),
                2.0,
                theme::selection().gamma_multiply(0.5),
            );
        } else if resp.hovered() {
            painter.rect_filled(rect.expand(3.0), 2.0, theme::hover());
        }
        painter.rect_filled(pic, 0.0, Color32::BLACK);
        // Hover scrub: the frame under the pointer's place across.
        let scrub = !app.media_window.bypass_scrub
            && m.video.is_some()
            && !m.still
            && !matches!(m.source, MediaSource::Generated(_));
        let at = match resp.hover_pos() {
            Some(p) if scrub || taken => {
                let k = ((p.x - pic.left()) / pic.width()).clamp(0.0, 1.0);
                let step = (k * 40.0).round() / 40.0;
                Tick((m.duration.0 as f64 * step as f64) as i64)
            }
            _ => Tick::ZERO,
        };
        if has_parts(m)
            && !taken
            && let Some(p) = resp.hover_pos().filter(|p| pic.contains(*p))
        {
            let rate = m.video.map(|v| v.frame_rate);
            let t = super::thumb_part::time_across(pic, p.x, m.duration, rate);
            app.media_window.hovered = Some((m.id, t));
        }
        let h = (pic.height() * ui.ctx().pixels_per_point()).round() as u32;
        if let Some(t) = app.engine.thumbnail(m, at, h) {
            let id = app.textures.get(ui.ctx(), &t);
            let aspect = t.width as f32 / t.height as f32;
            let w = (pic.height() * aspect).min(pic.width());
            let r = Rect::from_center_size(pic.center(), vec2(w, w / aspect));
            painter.image(
                id,
                r,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            icons::paint(
                &painter,
                pic.shrink(28.0),
                MediaType::of(m).icon(),
                theme::text_dim(),
            );
        }
        if let Some(&part) = app.media_window.parts.get(&m.id) {
            super::thumb_part::paint(&painter, pic, m.duration, part);
        }
        if scrub && resp.hovered() && m.duration > Tick::ZERO {
            let k = at.0 as f32 / m.duration.0 as f32;
            let x = pic.left() + pic.width() * k;
            painter.vline(x, pic.y_range(), Stroke::new(1.0, theme::banana()));
        }
        let galley = painter.layout(
            crate::i18n::media_name(m).to_string(),
            FontId::proportional(12.0),
            theme::text(),
            THUMB.x,
        );
        let at = pos2(rect.center().x - galley.size().x / 2.0, pic.bottom() + 3.0);
        painter.galley(at, galley, theme::text());
    }
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::i18n::media_name(m))
    });
    media_interactions(app, m, resp, taken);
}

/// Start Preview on media `id` (a subclip: its part of its parent).
fn preview(app: &mut BananaApp, id: bv_timeline::MediaId) {
    match app.project().subclip(id).cloned() {
        Some(s) => app.preview_media(s.parent, s.range.start),
        None => app.preview_media(id, bv_core::Tick::ZERO),
    }
}

/// A click selects (and opens the media in the Trimmer); a double-click
/// places it (its thumbnail's part) at the cursor, or in the Trimmer;
/// dragging takes it to the timeline or into a bin, unless `taken` (a
/// part picked on its thumbnail); the menu.
fn media_interactions(app: &mut BananaApp, m: &MediaInfo, resp: egui::Response, taken: bool) {
    if resp.clicked() {
        // Ctrl one more or less, Shift the run from the one selected.
        let mods = resp.ctx.input(|i| i.modifiers);
        let picked = &mut app.media_window.picked;
        if mods.ctrl || mods.command {
            if let Some(k) = picked.iter().position(|&p| p == m.id) {
                picked.remove(k);
            } else {
                if picked.is_empty()
                    && let Some(sel) = app.selected_media
                {
                    picked.push(sel);
                }
                picked.push(m.id);
            }
        } else if mods.shift
            && let Some(sel) = app.selected_media
        {
            let order: Vec<bv_timeline::MediaId> = shown(app).iter().map(|x| x.id).collect();
            let (a, b) = (
                order.iter().position(|&x| x == sel),
                order.iter().position(|&x| x == m.id),
            );
            if let (Some(a), Some(b)) = (a, b) {
                app.media_window.picked = order[a.min(b)..=a.max(b)].to_vec();
            }
        } else {
            picked.clear();
        }
        app.selected_media = Some(m.id);
        app.open_in_trimmer(m.id);
        if app.settings.media_auto_preview {
            preview(app, m.id);
        }
    }
    if resp.drag_started() && !taken {
        // Several picked: all of them, this one first, the others in the
        // window's order (VEGAS's Help); one alone with its part.
        let picked = &app.media_window.picked;
        let part = app.media_window.parts.get(&m.id).copied();
        app.media_drag = Some(if picked.len() > 1 && picked.contains(&m.id) {
            let order: Vec<bv_timeline::MediaId> = shown(app)
                .iter()
                .map(|x| x.id)
                .filter(|id| *id != m.id && app.media_window.picked.contains(id))
                .collect();
            MediaDrag::Many(std::iter::once(m.id).chain(order).collect())
        } else if let Some(r) = part {
            MediaDrag::MediaPart(m.id, r)
        } else {
            MediaDrag::Media(m.id)
        });
        app.media_window.drag_from = match app.media_window.bin {
            BinChoice::Custom(b) => Some(DragFrom::Bin(b)),
            _ => None,
        };
        app.media_drag_secondary = resp.dragged_by(egui::PointerButton::Secondary);
    }
    // Double-click: to the tracks, or (Options › Preferences › General) to
    // the Trimmer.
    if resp.double_clicked() {
        if app.settings.prefs.media_to_trimmer {
            app.open_in_trimmer(m.id);
            app.show_tab(Tab::Trimmer, false);
        } else {
            let at = app.cursor;
            place_part(app, m.id, at);
        }
    }
    // A right-click picks it (unless it is among those picked), and opens
    // VEGAS's menu (`media_menu.rs`).
    if resp.secondary_clicked() && !app.media_window.picked.contains(&m.id) {
        app.media_window.picked.clear();
        app.selected_media = Some(m.id);
    }
    let at = resp.rect.left_top();
    resp.context_menu(|ui| crate::media_menu::media_menu(ui, app, m, at));
}

impl BananaApp {
    /// Project Media's Delete key: the media picked (or selected) removed
    /// from the project, as its menu's Remove from Project.
    pub(crate) fn media_delete_key(&mut self, ctx: &egui::Context) {
        if self.keys_tab != Some(Tab::ProjectMedia)
            || !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
        {
            return;
        }
        let picked = self.media_window.picked.clone();
        let ids = if picked.is_empty() {
            self.selected_media.into_iter().collect()
        } else {
            picked
        };
        for id in ids {
            self.request_remove_media(id);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::egui::{Event, Modifiers, PointerButton};
    use egui_kittest::Harness;

    /// A video of ten seconds, offline (its thumbnail an icon).
    pub(crate) fn offline_video(app: &mut BananaApp, name: &str) -> MediaId {
        let settings = app.project().settings;
        let mut info = MediaInfo::generated(bv_timeline::Generator::ColorBars, &settings);
        info.source = MediaSource::File {
            path: format!("{name}.mp4"),
        };
        info.name = name.to_string();
        info.still = false;
        info.duration = Tick::from_seconds(10);
        app.modify(|p| p.add_media(info))
    }

    fn harness(app: BananaApp) -> Harness<'static, BananaApp> {
        let mut h = Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        h
    }

    fn button(h: &mut Harness<'_, BananaApp>, at: egui::Pos2, pressed: bool) {
        h.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }

    /// A drag across a thumbnail picks a part, I and O move its ends where
    /// the pointer is, a drag down takes the part to the timeline, and
    /// placed it is as long as the part.
    #[test]
    fn thumbnails_pick_parts_across_and_with_i_and_o() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        let id = offline_video(&mut app, "clip");
        app.media_window.view = MediaView::Thumbnail;
        app.show_tab(Tab::ProjectMedia, false);
        let mut h = harness(app);
        let pic = h.state().media_window.thumb_rects[&id];
        let at = |f: f32| pos2(pic.left() + pic.width() * f, pic.center().y);
        let near = |t: Tick, s: f64| (t.0 as f64 / Tick::from_seconds(1).0 as f64 - s).abs() < 0.15;
        h.event(Event::PointerMoved(at(0.2)));
        h.step();
        button(&mut h, at(0.2), true);
        for k in 1..=8 {
            h.event(Event::PointerMoved(at(0.2 + 0.05 * k as f32)));
            h.step();
        }
        button(&mut h, at(0.6), false);
        assert!(h.state().media_drag.is_none(), "across: no drag away");
        let part = h.state().media_window.parts[&id];
        assert!(near(part.start, 2.0) && near(part.end(), 6.0), "{part:?}");
        // O at 80 %, then I at 10 %.
        h.event(Event::PointerMoved(at(0.8)));
        h.step();
        h.key_press(egui::Key::O);
        h.step();
        h.event(Event::PointerMoved(at(0.1)));
        h.step();
        h.key_press(egui::Key::I);
        h.step();
        let part = h.state().media_window.parts[&id];
        assert!(near(part.start, 1.0) && near(part.end(), 8.0), "{part:?}");
        // Down: the part to the timeline.
        let from = at(0.5);
        h.event(Event::PointerMoved(from));
        h.step();
        button(&mut h, from, true);
        for k in 1..=6 {
            h.event(Event::PointerMoved(from + vec2(1.0, 12.0 * k as f32)));
            h.step();
        }
        assert!(
            matches!(h.state().media_drag, Some(MediaDrag::MediaPart(m, r)) if m == id && r == part)
        );
        h.state_mut().media_drag = None;
        button(&mut h, from + vec2(1.0, 72.0), false);
        let app = h.state_mut();
        place_part(app, id, Tick::ZERO);
        let placed: Vec<Tick> = app
            .project()
            .clips()
            .filter(|(_, c)| c.media == id)
            .map(|(_, c)| c.duration)
            .collect();
        assert!(!placed.is_empty() && placed.iter().all(|&d| d == part.duration));
    }

    /// Project Media's Select Timeline Events, Add at Project Frame Rate
    /// (one undo) and Match Project Video Settings.
    #[test]
    fn project_media_selects_adds_at_the_project_rate_and_matches() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let p = app.project();
        let media = p.clips().next().map(|(_, c)| c.media).expect("an event");
        let mut starts: Vec<Tick> = p
            .clips()
            .filter(|(_, c)| c.media == media)
            .map(|(_, c)| c.start)
            .collect();
        starts.sort();
        let settings = p.settings;
        app.select_timeline_events(media, false);
        assert_eq!(app.selection.len(), starts.len());
        assert_eq!(app.cursor, starts[0], "the cursor at the first");
        // A 100 fps clip: played at 0.2997 in the 29.97 fps demo.
        let mut info = MediaInfo::generated(bv_timeline::Generator::ColorBars, &settings);
        let video = info.video.as_mut().unwrap();
        (video.width, video.height) = (1280, 720);
        video.frame_rate = bv_core::FrameRate::new(100, 1);
        let fast = app.modify(|p| p.add_media(info));
        let before = app.project().clips().count();
        app.add_at_project_frame_rate(fast);
        let p = app.project();
        let added: Vec<_> = p.clips().filter(|(_, c)| c.media == fast).collect();
        assert_eq!(added.len(), 1);
        let want = p.settings.frame_rate.as_f64() / 100.0;
        assert!((added[0].1.rate - want).abs() < 1e-9, "{}", added[0].1.rate);
        app.undo();
        assert_eq!(app.project().clips().count(), before, "one undo");
        app.match_project_video_settings(fast);
        let s = app.project().settings;
        assert_eq!((s.width, s.height), (1280, 720));
        assert_eq!(s.frame_rate, bv_core::FrameRate::new(100, 1));
    }
}
