//! File › Render As…: VEGAS 23's Render window.
//!
//! A tree on the left (search, favorites, use cases, video formats, image
//! sequences, audio formats), the templates of the selected node in the
//! middle (a list in columns or a details table, filtered by how they match
//! the project), the selected template's details, folder and name at the
//! bottom, and the render options. Rendering opens the progress window.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bv_core::{Tick, TimeRange};
use bv_engine::{Availability, Category, FileFormat, RenderSettings};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{
    self, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use crate::i18n::tr;
use crate::theme;

mod channel_mapping;
mod customize;
pub(crate) mod folder;
mod paint;
pub(crate) mod progress;
pub(crate) mod templates;

use paint::FolderKind;
use templates::{CustomTemplate, Template, UseCase};

/// Which project settings a template must match to be listed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filters {
    pub resolution: bool,
    pub frame_rate: bool,
    pub ratio: bool,
    pub field_order: bool,
    pub color_depth: bool,
    pub hdr: bool,
    pub color_space: bool,
    pub pixel_ratio: bool,
    pub sample_rate: bool,
    pub channels: bool,
    pub bit_depth: bool,
}

impl Filters {
    fn video(&mut self) -> [&mut bool; 8] {
        [
            &mut self.resolution,
            &mut self.frame_rate,
            &mut self.ratio,
            &mut self.field_order,
            &mut self.color_depth,
            &mut self.hdr,
            &mut self.color_space,
            &mut self.pixel_ratio,
        ]
    }

    fn audio(&mut self) -> [&mut bool; 3] {
        [
            &mut self.sample_rate,
            &mut self.channels,
            &mut self.bit_depth,
        ]
    }
}

/// The Render dialog's choices, kept between sessions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RenderPrefs {
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub custom: Vec<CustomTemplate>,
    #[serde(default)]
    pub filters: Filters,
    #[serde(default)]
    pub show_all: bool,
    /// The details table instead of the list.
    #[serde(default)]
    pub details_view: bool,
    #[serde(default)]
    pub sort_by_name: bool,
    #[serde(default)]
    pub last_template: Option<String>,
    #[serde(default)]
    pub recent_folders: Vec<String>,
    #[serde(default)]
    pub options: bv_engine::RenderOptions,
    #[serde(default)]
    pub open_folder_when_done: bool,
    #[serde(default)]
    pub close_when_done: bool,
    /// "Show Video in Preview Window" off (it is on by default, like
    /// VEGAS's).
    #[serde(default)]
    pub hide_render_video: bool,
}

/// A node of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Favorites,
    UseCases,
    UseCase(UseCase),
    Video,
    Format(FileFormat),
    Images,
    Audio,
}

/// A top-down area of exactly `r`.
fn top_down(r: Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(r)
        .layout(Layout::top_down(Align::Min))
}

/// BananaV's templates (built once; names are written when shown).
fn builtin() -> &'static [Template] {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    ALL.get_or_init(templates::builtin)
}

/// Every template: BananaV's and the user's.
fn all_templates(app: &BananaApp) -> Vec<Template> {
    let mut v: Vec<Template> = builtin().to_vec();
    v.extend(app.settings.render.custom.iter().map(Template::from));
    v
}

/// Whether `t` passes the filters for the project.
fn matches(t: &Template, p: &bv_timeline::ProjectSettings, f: &Filters) -> bool {
    if let Some(v) = t.video_for(p) {
        let (pw, ph) = p.output_size();
        if f.resolution && (v.width, v.height) != (pw, ph) {
            return false;
        }
        if f.frame_rate && v.frame_rate != p.frame_rate {
            return false;
        }
        if f.ratio && v.width as u64 * ph as u64 != v.height as u64 * pw as u64 {
            return false;
        }
        if f.color_depth && (v.bit_depth > 8) != p.pixel_format.is_float() {
            return false;
        }
    }
    if let Some(a) = t.audio_for(p) {
        if f.sample_rate && a.sample_rate != p.sample_rate.0 {
            return false;
        }
        if f.channels && a.channels as usize != p.channels.channel_count() {
            return false;
        }
        if f.bit_depth && !t.format.compressed_audio() && a.bit_depth != p.bit_depth {
            return false;
        }
    }
    true
}

/// "105.2 GB": a size as the dialog writes it.
pub(crate) fn size_text(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes.max(0.0);
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{} {}", crate::fmt::num(v, 1), UNITS[u])
}

/// What the file of `t` would weigh for `seconds` of the project.
fn estimate(t: &Template, p: &bv_timeline::ProjectSettings, seconds: f64) -> f64 {
    let v = t.video_for(p);
    let a = t.audio_for(p);
    let audio = a.map_or(0.0, |a| {
        if t.format.compressed_audio() {
            a.bitrate as f64 / 8.0
        } else {
            let pcm = a.sample_rate as f64 * a.channels as f64 * a.bit_depth as f64 / 8.0;
            if t.format == FileFormat::Flac {
                pcm * 0.6
            } else {
                pcm
            }
        }
    }) * seconds;
    let Some(v) = v else {
        return audio;
    };
    let pixels = v.width as f64 * v.height as f64;
    let frames = seconds * v.frame_rate.as_f64();
    let video = match t.format {
        FileFormat::ProRes => {
            v.prores.megabits_1080p30() * 1e6 / 8.0 * pixels / (1920.0 * 1080.0)
                * v.frame_rate.as_f64()
                / 29.97
                * seconds
        }
        FileFormat::Bmp => frames * (pixels * if v.alpha { 4.0 } else { 3.0 } + 54.0),
        FileFormat::Dds => frames * (pixels * 4.0 + 128.0),
        FileFormat::Dpx => frames * (pixels * 4.0 + 2048.0),
        FileFormat::Exr => frames * pixels * 8.0 * 0.6,
        FileFormat::Tiff | FileFormat::Png => frames * pixels * 3.0 * 0.5,
        FileFormat::Jpeg | FileFormat::WmPhoto => frames * pixels * 0.35,
        FileFormat::Heic => frames * pixels * 0.15,
        _ => v.bitrate as f64 / 8.0 * seconds,
    };
    video + audio
}

/// Render to New Track's end: the rendered file added to the project and
/// put on new tracks — a video and an audio one, as the file has them —
/// at the time it was rendered from, its events picked. A canceled or
/// failed render adds nothing.
pub(crate) fn place_new_track(app: &mut BananaApp) {
    let Some(at) = app.new_track_render else {
        return;
    };
    let Some(p) = app.engine.render_progress() else {
        return;
    };
    if !p.finished {
        return;
    }
    app.new_track_render = None;
    let file = match p.files.first() {
        Some(f) if p.error.is_none() && !p.cancelled => f.clone(),
        _ => return,
    };
    let Some(&m) = app.import_paths(&[file]).first() else {
        return;
    };
    let Some(info) = app.project().media(m).cloned() else {
        return;
    };
    let placed = app.edit(|p| {
        let v = info
            .video
            .is_some()
            .then(|| p.add_track(bv_timeline::TrackKind::Video));
        let a = info
            .audio
            .is_some()
            .then(|| p.add_track(bv_timeline::TrackKind::Audio));
        p.insert_media(m, at, v, a)
    });
    if let Some(ids) = placed {
        app.selection = ids.into_iter().collect();
    }
}

#[derive(Debug)]
pub struct RenderDialog {
    node: Node,
    expanded: [bool; 3],
    search: String,
    selected: Option<String>,
    folder: String,
    name: String,
    loop_only: bool,
    customize: Option<customize::CustomizeWindow>,
    /// The options' Channel Mapping (multichannel mapping).
    channels: Option<channel_mapping::ChannelMapping>,
    /// The render waiting for "replace the file?".
    confirm: Option<(RenderSettings, String)>,
    /// The folder takes no files (Windows' Controlled folder access…).
    problem: Option<folder::FolderProblem>,
    free: Option<(String, Option<u64>)>,
    /// Tools › Render to New Track: the file goes on new tracks once
    /// rendered; where its render started (its events' start).
    to_new_track: bool,
    started_at: Option<Tick>,
}

/// What the footer asked for.
enum Action {
    None,
    Render,
    Close,
}

/// What became of the dialog this frame.
pub enum Outcome {
    Open,
    Closed,
    /// A render of this file started: the progress window takes over.
    Started(PathBuf),
}

impl RenderDialog {
    pub fn new(app: &BananaApp) -> RenderDialog {
        let p = app.project();
        let folder = app
            .settings
            .render
            .recent_folders
            .first()
            .cloned()
            .or_else(|| {
                app.project_path
                    .as_ref()
                    .and_then(|p| p.parent().map(|d| d.display().to_string()))
            })
            .or_else(|| {
                bv_platform::user_dir(bv_platform::UserDir::Videos).map(|d| d.display().to_string())
            })
            .unwrap_or_else(|| ".".into());
        let name = app
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or(tr("Untitled").to_string(), |s| {
                s.to_string_lossy().to_string()
            });
        let all = all_templates(app);
        let last = app
            .settings
            .render
            .last_template
            .clone()
            .filter(|id| all.iter().any(|t| &t.id == id));
        let node = match last
            .as_ref()
            .and_then(|id| all.iter().find(|t| &t.id == id))
        {
            Some(t) => match t.format.category() {
                Category::Video => Node::Video,
                Category::ImageSequence => Node::Images,
                Category::Audio => Node::Audio,
            },
            None => Node::UseCases,
        };
        RenderDialog {
            node,
            expanded: [false; 3],
            search: String::new(),
            selected: last.or_else(|| Some("avc:1920x1080:30000-1001:cpu".into())),
            folder,
            name,
            loop_only: p.loop_region.is_some(),
            customize: None,
            channels: None,
            confirm: None,
            problem: None,
            free: None,
            to_new_track: false,
            started_at: None,
        }
    }

    /// Tools › Render to New Track (VEGAS's): the same window, its file
    /// put on new tracks once rendered — the soloed tracks mixed, or all
    /// of the project as the Master bus plays it, over the time selection
    /// or the whole project.
    pub fn to_new_track(app: &BananaApp) -> RenderDialog {
        let mut d = RenderDialog::new(app);
        d.to_new_track = true;
        d.name = format!("{} {}", d.name, tr("Mixdown"));
        d
    }

    /// Where the events of a Render to New Track go: its render's start.
    pub(crate) fn new_track_at(&self) -> Option<Tick> {
        self.started_at.filter(|_| self.to_new_track)
    }

    /// Where the render started, on the timeline.
    pub(crate) fn started_at(&self) -> Option<Tick> {
        self.started_at
    }

    /// The templates the middle shows.
    fn listed(&self, app: &BananaApp, all: &[Template]) -> Vec<Template> {
        let prefs = &app.settings.render;
        let settings = &app.project().settings;
        let query = self.search.trim().to_lowercase();
        let mut out: Vec<Template> = all
            .iter()
            .filter(|t| {
                if !query.is_empty() {
                    return t.search_text().contains(&query);
                }
                match self.node {
                    Node::Favorites => prefs.favorites.contains(&t.id),
                    Node::UseCases => false,
                    Node::UseCase(c) => t.use_cases.contains(&c),
                    Node::Video => t.format.category() == Category::Video,
                    Node::Format(f) => t.format == f,
                    Node::Images => t.format.category() == Category::ImageSequence,
                    Node::Audio => t.format.category() == Category::Audio,
                }
            })
            .filter(|t| prefs.show_all || t.availability() != Availability::NoEncoder)
            .filter(|t| matches(t, settings, &prefs.filters))
            .cloned()
            .collect();
        if prefs.sort_by_name {
            out.sort_by_key(|t| t.name().to_lowercase());
        }
        out
    }

    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> Outcome {
        let mut open = true;
        let mut action = Action::None;
        let all = all_templates(app);
        let title = if self.to_new_track {
            tr("Render to New Track")
        } else {
            tr("Render")
        };
        crate::floating::Window::new(title)
            .id(egui::Id::new("render-dialog"))
            .collapsible(false)
            .resizable(true)
            .default_size([920.0, 700.0])
            .min_size([760.0, 560.0])
            .open(&mut open)
            .show(ctx, |ui| {
                // The first (measuring) pass sees no size limit: the window
                // starts at its default size instead of growing to it.
                if ui.is_sizing_pass() {
                    ui.allocate_space(vec2(900.0, 640.0));
                    return;
                }
                ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0, theme::separator());
                // Everything is laid out in the window's area: it keeps its
                // size.
                let mut area = ui.available_rect_before_wrap();
                if ui.ctx().embed_viewports() {
                    // Inside the main window (no OS windows): its default size.
                    area = Rect::from_min_size(area.min, area.size().min(vec2(900.0, 660.0)));
                }
                self.top_bar(ui, app);
                let footer_h = 34.0;
                let body = Rect::from_min_max(
                    pos2(area.left(), ui.cursor().top()),
                    pos2(
                        area.right(),
                        (area.bottom() - footer_h - 6.0).max(area.top() + 200.0),
                    ),
                );
                let tree = Rect::from_min_size(body.min, vec2(250.0, body.height()));
                let right = Rect::from_min_max(pos2(tree.right() + 6.0, body.top()), body.max);
                let footer = Rect::from_min_max(pos2(area.left(), body.bottom() + 6.0), area.max);
                ui.scope_builder(top_down(tree), |ui| self.tree(ui, app));
                ui.scope_builder(top_down(right), |ui| self.right(ui, app, &all));
                ui.scope_builder(top_down(footer), |ui| {
                    action = self.footer(ui, app, &all);
                });
                ui.allocate_rect(area, Sense::hover());
            });
        if let Some(c) = &mut self.customize {
            match c.show(ctx, app) {
                customize::Outcome::Open => {}
                customize::Outcome::Closed => self.customize = None,
                customize::Outcome::Chosen(id) => {
                    self.selected = Some(id);
                    self.customize = None;
                }
            }
        }
        if let Some(c) = &mut self.channels
            && !c.show(ctx, app)
        {
            self.channels = None;
        }
        if let Some(path) = self.confirm_window(ctx, app) {
            return Outcome::Started(path);
        }
        if let Some(p) = &self.problem
            && !p.window(ctx, "render-folder-problem")
        {
            self.problem = None;
        }
        match action {
            Action::Render => {
                if let Some(s) = self.settings(app, &all) {
                    // A folder that takes no files is explained before the
                    // render starts (not as "file not found" when it fails).
                    if let Some(p) = s.path.parent().and_then(folder::FolderProblem::of) {
                        self.problem = Some(p);
                        return Outcome::Open;
                    }
                    let first = if s.format.category() == Category::ImageSequence {
                        bv_engine::sequence_path(&s.path, 0)
                    } else {
                        s.path.clone()
                    };
                    if first.exists() {
                        let name = first
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().to_string());
                        self.confirm = Some((s, name));
                    } else {
                        return Outcome::Started(self.start(app, s));
                    }
                }
            }
            Action::Close => {
                self.remember(app);
                return Outcome::Closed;
            }
            Action::None => {}
        }
        if open { Outcome::Open } else { Outcome::Closed }
    }

    /// The dialog's choices go into the settings.
    fn remember(&self, app: &mut BananaApp) {
        let prefs = &mut app.settings.render;
        if self.selected.is_some() {
            prefs.last_template = self.selected.clone();
        }
        let folder = self.folder.trim().to_string();
        if !folder.is_empty() {
            prefs.recent_folders.retain(|f| f != &folder);
            prefs.recent_folders.insert(0, folder);
            prefs.recent_folders.truncate(10);
        }
        let _ = app.settings.save();
    }

    /// Shows why the folder takes no files (for screen tests).
    pub(crate) fn set_problem(&mut self, problem: folder::FolderProblem) {
        self.problem = Some(problem);
    }

    /// The selected template's render settings.
    fn settings(&self, app: &BananaApp, all: &[Template]) -> Option<RenderSettings> {
        let t = all.iter().find(|t| Some(&t.id) == self.selected.as_ref())?;
        if t.availability() != Availability::Ready {
            return None;
        }
        let p = app.project();
        let range = match (self.loop_only, p.loop_region) {
            (true, Some(r)) => r,
            _ => TimeRange::new(Tick::ZERO, p.duration()),
        };
        if range.duration <= Tick::ZERO {
            return None;
        }
        let name = self.name.trim();
        let name = if name.is_empty() {
            tr("Untitled")
        } else {
            name
        };
        let file = format!("{name}.{}", t.format.extension());
        Some(RenderSettings {
            format: t.format,
            path: Path::new(self.folder.trim()).join(file),
            range,
            video: t.video_for(&p.settings),
            audio: t.audio_for(&p.settings),
            options: bv_engine::RenderOptions {
                no_recompress: app.settings.prefs.no_recompress,
                ..app.settings.render.options
            },
            project_file: app.project_path.clone(),
        })
    }

    /// Starts the render; returns its file.
    fn start(&mut self, app: &mut BananaApp, s: RenderSettings) -> PathBuf {
        self.started_at = Some(s.range.start);
        self.remember(app);
        app.engine.set_project(app.history.current());
        let file = s.path.clone();
        crate::logs::log(
            crate::logs::LogKind::Renders,
            &crate::logs::render_lines(&s),
        );
        app.engine.start_render(s);
        file
    }

    /// "Replace the file?"; the file of the render it started, if any.
    fn confirm_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> Option<PathBuf> {
        let (settings, name) = self.confirm.clone()?;
        let mut answer = None;
        crate::floating::Window::new(tr("Render"))
            .id(egui::Id::new("render-replace"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(trf!("{} already exists. Do you want to replace it?", name));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("No")).clicked() {
                            answer = Some(false);
                        }
                        if ui.button(RichText::new(tr("Yes")).strong()).clicked() {
                            answer = Some(true);
                        }
                    });
                });
            });
        match answer {
            Some(true) => {
                self.confirm = None;
                Some(self.start(app, settings))
            }
            Some(false) => {
                self.confirm = None;
                None
            }
            None => None,
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Render:")).strong());
            let project = app
                .project_path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map_or(tr("Untitled").to_string(), |s| {
                    s.to_string_lossy().to_string()
                });
            ui.label(project);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.node != Node::UseCases || !self.search.trim().is_empty() {
                    self.view_button(ui, app);
                }
                self.filter_button(ui, app);
            });
        });
        ui.add_space(2.0);
    }

    /// An icon button with a "▾", opening `menu`.
    fn menu_icon(
        ui: &mut Ui,
        id: &str,
        tip: &str,
        draw: impl Fn(&egui::Painter, Rect, Color32),
        keep_open: bool,
        menu: impl FnOnce(&mut Ui),
    ) {
        let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 24.0), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
        let resp = resp.on_hover_text(tip);
        if resp.hovered() {
            ui.painter().rect_filled(rect, 3.0, theme::hover());
        }
        let icon = Rect::from_min_size(rect.min + vec2(3.0, 3.0), vec2(18.0, 18.0));
        draw(ui.painter(), icon, theme::text());
        paint::drop_arrow(
            ui.painter(),
            pos2(rect.right() - 7.0, rect.center().y),
            theme::text(),
        );
        let _ = id;
        let mut popup = egui::Popup::menu(&resp);
        if keep_open {
            popup = popup.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        }
        popup.show(menu);
    }

    fn filter_button(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        let prefs = &mut app.settings.render;
        Self::menu_icon(
            ui,
            "render-filter",
            tr("Filter the templates"),
            paint::funnel,
            true,
            |ui| {
                ui.set_min_width(230.0);
                let group = |ui: &mut Ui, title: &str, items: &mut [&mut bool], names: &[&str]| {
                    let all_on = items.iter().all(|b| **b);
                    let mut on = all_on;
                    if crate::widgets::checkbox(ui, &mut on, title).changed() {
                        for b in items.iter_mut() {
                            **b = on;
                        }
                    }
                    for (b, name) in items.iter_mut().zip(names) {
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            crate::widgets::checkbox(ui, b, *name);
                        });
                    }
                };
                let mut f = prefs.filters;
                group(
                    ui,
                    tr("Match project video settings"),
                    &mut f.video(),
                    &[
                        tr("Resolution"),
                        tr("Framerate"),
                        tr("Ratio"),
                        tr("Field order"),
                        tr("Color depth"),
                        "HDR",
                        tr("Color space"),
                        tr("Pixel ratio"),
                    ],
                );
                ui.separator();
                group(
                    ui,
                    tr("Match project audio settings"),
                    &mut f.audio(),
                    &[tr("Sample rate"), tr("Channel count"), tr("Bit depth")],
                );
                ui.separator();
                crate::widgets::checkbox(ui, &mut prefs.show_all, tr("Show all formats"))
                    .on_hover_text(tr(
                        "Also list the formats this computer has no encoder for.",
                    ));
                prefs.filters = f;
            },
        );
    }

    fn view_button(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        let prefs = &mut app.settings.render;
        Self::menu_icon(
            ui,
            "render-view",
            tr("Template view"),
            paint::view_grid,
            false,
            |ui| {
                if crate::widgets::radio(ui, !prefs.details_view, tr("List")).clicked() {
                    prefs.details_view = false;
                    ui.close();
                }
                if crate::widgets::radio(ui, prefs.details_view, tr("Details")).clicked() {
                    prefs.details_view = true;
                    ui.close();
                }
                ui.separator();
                if crate::widgets::radio(ui, !prefs.sort_by_name, tr("Sort by format")).clicked() {
                    prefs.sort_by_name = false;
                    ui.close();
                }
                if crate::widgets::radio(ui, prefs.sort_by_name, tr("Sort by name")).clicked() {
                    prefs.sort_by_name = true;
                    ui.close();
                }
            },
        );
    }

    fn tree_row(
        ui: &mut Ui,
        depth: usize,
        expanded: Option<bool>,
        icon: Option<FolderKind>,
        label: &str,
        selected: bool,
        enabled: bool,
    ) -> (egui::Response, bool) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, selected, label)
        });
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 0.0, paint::BLUE);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, theme::hover());
        }
        let mut x = rect.left() + 8.0 + depth as f32 * 18.0;
        let mut toggled = false;
        if let Some(open) = expanded {
            let c = pos2(x + 4.0, rect.center().y);
            let pts = if open {
                vec![
                    pos2(c.x - 4.0, c.y - 2.0),
                    pos2(c.x + 4.0, c.y - 2.0),
                    pos2(c.x, c.y + 3.0),
                ]
            } else {
                vec![
                    pos2(c.x - 2.0, c.y - 4.0),
                    pos2(c.x + 3.0, c.y),
                    pos2(c.x - 2.0, c.y + 4.0),
                ]
            };
            p.add(egui::Shape::convex_polygon(
                pts,
                theme::text_dim(),
                Stroke::NONE,
            ));
            let arrow = Rect::from_center_size(c, vec2(16.0, 22.0));
            if resp.clicked()
                && resp
                    .interact_pointer_pos()
                    .is_some_and(|pos| arrow.contains(pos))
            {
                toggled = true;
            }
        }
        x += 16.0;
        if let Some(kind) = icon {
            paint::folder(
                p,
                Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(18.0, 16.0)),
                kind,
            );
            x += 26.0;
        }
        let color = if !enabled {
            theme::text_disabled()
        } else if selected {
            theme::text_strong()
        } else {
            theme::text()
        };
        p.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            color,
        );
        (resp, toggled)
    }

    fn tree(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        let area = ui.max_rect();
        ui.painter().rect(
            area,
            0.0,
            theme::panel_dark(),
            Stroke::new(1.0, theme::separator()),
            egui::StrokeKind::Inside,
        );
        let inner = area.shrink(1.0);
        ui.scope_builder(top_down(inner), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // Search.
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 26.0), Sense::hover());
                let c = r.center() - vec2(1.0, 1.0);
                ui.painter()
                    .circle_stroke(c, 5.0, Stroke::new(1.3, theme::text_dim()));
                ui.painter().line_segment(
                    [c + vec2(3.5, 3.5), c + vec2(7.0, 7.0)],
                    Stroke::new(1.5, theme::text_dim()),
                );
                let w = (ui.available_width() - 30.0).max(40.0);
                ui.add_sized(
                    vec2(w, 22.0),
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(tr("Search"))
                        .frame(egui::Frame::NONE),
                );
                let (r, x) = ui.allocate_exact_size(vec2(18.0, 22.0), Sense::click());
                x.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        tr("Clear the search"),
                    )
                });
                let c = r.center();
                let s = Stroke::new(1.6, theme::error());
                ui.painter()
                    .line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s);
                ui.painter()
                    .line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], s);
                if x.on_hover_text(tr("Clear the search")).clicked() {
                    self.search.clear();
                }
            });
            ui.add_space(2.0);
            let searching = !self.search.trim().is_empty();
            let sel = |n: Node| !searching && self.node == n;
            let mut pick = None;
            if !app.settings.render.favorites.is_empty() {
                let (r, _) = Self::tree_row(
                    ui,
                    0,
                    None,
                    Some(FolderKind::Star),
                    tr("Favorites"),
                    sel(Node::Favorites),
                    true,
                );
                if r.clicked() {
                    pick = Some(Node::Favorites);
                }
            }
            let groups = [
                (0usize, Node::UseCases, FolderKind::Plain, tr("Use Case")),
                (1, Node::Video, FolderKind::Video, tr("Video")),
                (
                    usize::MAX,
                    Node::Images,
                    FolderKind::Images,
                    tr("Image Sequence"),
                ),
                (2, Node::Audio, FolderKind::Audio, tr("Audio")),
            ];
            for (slot, node, kind, label) in groups {
                let expandable = slot != usize::MAX;
                let open = expandable && self.expanded[slot];
                let (r, toggled) = Self::tree_row(
                    ui,
                    0,
                    expandable.then_some(open),
                    Some(kind),
                    label,
                    sel(node),
                    true,
                );
                if toggled {
                    self.expanded[slot] = !open;
                } else if r.clicked() {
                    pick = Some(node);
                }
                if r.double_clicked() && expandable {
                    self.expanded[slot] = !open;
                }
                if !open {
                    continue;
                }
                let children: Vec<(Node, String, bool)> = match node {
                    Node::UseCases => UseCase::ALL
                        .iter()
                        .map(|c| (Node::UseCase(*c), c.title().to_string(), true))
                        .collect(),
                    Node::Video | Node::Audio => {
                        let cat = if node == Node::Video {
                            Category::Video
                        } else {
                            Category::Audio
                        };
                        FileFormat::ALL
                            .iter()
                            .filter(|f| f.category() == cat)
                            .filter(|f| {
                                app.settings.render.show_all
                                    || f.availability() != Availability::NoEncoder
                            })
                            .map(|f| {
                                (
                                    Node::Format(*f),
                                    tr(f.name()).to_string(),
                                    f.availability() == Availability::Ready,
                                )
                            })
                            .collect()
                    }
                    _ => Vec::new(),
                };
                for (child, label, ready) in children {
                    let (r, _) = Self::tree_row(ui, 1, None, None, &label, sel(child), ready);
                    let r = if ready {
                        r
                    } else if let Node::Format(f) = child {
                        r.on_hover_text(unavailable_text(f.availability()))
                    } else {
                        r
                    };
                    if r.clicked() {
                        pick = Some(child);
                    }
                }
            }
            if let Some(n) = pick {
                self.node = n;
                self.search.clear();
            }
        });
    }

    fn right(&mut self, ui: &mut Ui, app: &mut BananaApp, all: &[Template]) {
        let area = ui.max_rect();
        let details_h = 200.0;
        let list = Rect::from_min_max(
            area.min,
            pos2(
                area.right(),
                (area.bottom() - details_h - 6.0).max(area.top() + 120.0),
            ),
        );
        let details = Rect::from_min_max(pos2(area.left(), list.bottom() + 6.0), area.max);
        ui.painter().rect(
            list,
            0.0,
            theme::panel_dark(),
            Stroke::new(1.0, theme::separator()),
            egui::StrokeKind::Inside,
        );
        ui.scope_builder(top_down(list.shrink(1.0)), |ui| {
            if self.node == Node::UseCases && self.search.trim().is_empty() {
                self.use_case_grid(ui);
            } else {
                let listed = self.listed(app, all);
                if app.settings.render.details_view {
                    self.details_table(ui, app, &listed);
                } else {
                    self.column_list(ui, app, &listed);
                }
            }
        });
        ui.scope_builder(top_down(details), |ui| self.details(ui, app, all));
    }

    fn use_case_grid(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .id_salt("render-use-cases")
            .show(ui, |ui| {
                let w = ((ui.available_width() - 8.0) / 2.0).max(120.0);
                for pair in UseCase::ALL.chunks(2) {
                    ui.horizontal(|ui| {
                        for case in pair {
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(w, 84.0), Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    case.title(),
                                )
                            });
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(rect.shrink(2.0), 3.0, theme::hover());
                            }
                            let icon =
                                Rect::from_min_size(rect.min + vec2(10.0, 14.0), vec2(56.0, 56.0));
                            paint::use_case(p, icon, *case);
                            let text_x = icon.right() + 14.0;
                            p.text(
                                pos2(text_x, rect.top() + 18.0),
                                Align2::LEFT_TOP,
                                case.title(),
                                FontId::proportional(14.0),
                                theme::text_strong(),
                            );
                            let galley = p.layout(
                                case.description().to_string(),
                                FontId::proportional(12.5),
                                theme::text_dim(),
                                rect.right() - text_x - 10.0,
                            );
                            p.galley(pos2(text_x, rect.top() + 38.0), galley, theme::text_dim());
                            if resp.clicked() {
                                self.node = Node::UseCase(*case);
                                self.expanded[0] = true;
                            }
                        }
                    });
                }
            });
    }

    /// One template in the list: page, name, and the star when it is the
    /// selected or hovered one (or a favorite).
    fn template_cell(&mut self, ui: &mut Ui, app: &mut BananaApp, rect: Rect, t: &Template) {
        let id = ui.id().with(("template", &t.id));
        let resp = ui.interact(rect, id, Sense::click());
        let selected = self.selected.as_ref() == Some(&t.id);
        let ready = t.availability() == Availability::Ready;
        let name = t.name();
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, ready, selected, &name)
        });
        let favorite = app.settings.render.favorites.contains(&t.id);
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 0.0, paint::BLUE);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, theme::hover());
        }
        paint::page(
            p,
            Rect::from_min_size(rect.min + vec2(6.0, 3.0), vec2(16.0, rect.height() - 6.0)),
            ready,
        );
        let color = if !ready {
            theme::text_disabled()
        } else if selected {
            theme::text_strong()
        } else {
            theme::text()
        };
        let star =
            Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), vec2(18.0, 18.0));
        crate::widgets::fit_text(
            p,
            pos2(rect.left() + 28.0, rect.center().y),
            Align2::LEFT_CENTER,
            t.name(),
            FontId::proportional(13.0),
            color,
            rect.width() - 52.0,
        );
        if selected || resp.hovered() || favorite {
            let c = if favorite {
                theme::banana()
            } else {
                theme::text_dim()
            };
            paint::star(p, star.center(), 6.5, c, favorite);
        }
        let resp = if ready {
            resp
        } else {
            resp.on_hover_text(unavailable_text(t.availability()))
        };
        if resp.clicked() {
            let on_star = resp
                .interact_pointer_pos()
                .is_some_and(|pos| star.contains(pos));
            if on_star && (selected || resp.hovered()) {
                let favs = &mut app.settings.render.favorites;
                if favorite {
                    favs.retain(|f| f != &t.id);
                } else {
                    favs.push(t.id.clone());
                }
                let _ = app.settings.save();
            } else {
                self.selected = Some(t.id.clone());
            }
        }
        if resp.double_clicked() && ready {
            self.selected = Some(t.id.clone());
            self.customize = Some(customize::CustomizeWindow::new(t.clone(), app));
        }
    }

    /// Templates flowing down in columns, scrolled sideways (VEGAS's list).
    fn column_list(&mut self, ui: &mut Ui, app: &mut BananaApp, listed: &[Template]) {
        if listed.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(tr("No template matches.")).color(theme::text_dim()));
            });
            return;
        }
        let row_h = 24.0;
        let col_w = 320.0;
        let height = (ui.available_height() - 14.0).max(24.0);
        let rows = ((height / row_h).floor() as usize).max(1);
        let cols = listed.len().div_ceil(rows);
        egui::ScrollArea::horizontal()
            .id_salt(("render-list", format!("{:?}", self.node)))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (area, _) = ui.allocate_exact_size(
                    vec2(cols as f32 * col_w, rows as f32 * row_h),
                    Sense::hover(),
                );
                for (i, t) in listed.iter().enumerate() {
                    let (c, r) = (i / rows, i % rows);
                    let rect = Rect::from_min_size(
                        area.min + vec2(c as f32 * col_w, r as f32 * row_h),
                        vec2(col_w - 4.0, row_h),
                    );
                    self.template_cell(ui, app, rect, t);
                }
            });
    }

    /// The details view: a table to compare templates.
    fn details_table(&mut self, ui: &mut Ui, app: &mut BananaApp, listed: &[Template]) {
        let p = app.project().settings;
        let w = ui.available_width();
        let cols = [0.0, w * 0.40, w * 0.55, w * 0.80];
        let header = |ui: &mut Ui| {
            let (r, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
            ui.painter().rect_filled(r, 0.0, theme::panel());
            for (x, name) in cols
                .iter()
                .zip([tr("Name"), tr("Format"), tr("Video"), tr("Audio")])
            {
                ui.painter().text(
                    pos2(r.left() + x + 8.0, r.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(12.5),
                    theme::text_dim(),
                );
            }
        };
        header(ui);
        egui::ScrollArea::vertical()
            .id_salt("render-details")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for t in listed {
                    let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
                    let name_rect =
                        Rect::from_min_max(rect.min, pos2(rect.left() + cols[1], rect.bottom()));
                    self.template_cell(ui, app, name_rect, t);
                    let dim = if t.availability() == Availability::Ready {
                        theme::text_dim()
                    } else {
                        theme::text_disabled()
                    };
                    let v = t.video_for(&p).map_or("—".to_string(), |v| {
                        format!(
                            "{}x{}, {}",
                            v.width,
                            v.height,
                            crate::fmt::fps(v.frame_rate)
                        )
                    });
                    let a = t.audio_for(&p).map_or("—".to_string(), |a| {
                        format!(
                            "{} Hz, {}",
                            crate::fmt::grouped(a.sample_rate as u64),
                            templates::channels_text(a.channels)
                        )
                    });
                    for (x, text, width) in [
                        (cols[1], tr(t.format.name()).to_string(), cols[2] - cols[1]),
                        (cols[2], v, cols[3] - cols[2]),
                        (cols[3], a, w - cols[3]),
                    ] {
                        crate::widgets::fit_text(
                            ui.painter(),
                            pos2(rect.left() + x + 8.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            &text,
                            FontId::proportional(12.5),
                            dim,
                            width - 12.0,
                        );
                    }
                }
            });
    }

    fn details(&mut self, ui: &mut Ui, app: &mut BananaApp, all: &[Template]) {
        let p = app.project().settings;
        let t = all
            .iter()
            .find(|t| Some(&t.id) == self.selected.as_ref())
            .cloned();
        egui::Frame::new()
            .fill(theme::panel())
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                // As wide as the widest label in the interface's language.
                let label_w = [
                    tr("Notes:"),
                    tr("Video:"),
                    tr("Audio:"),
                    tr("Folder:"),
                    tr("Name:"),
                ]
                .iter()
                .map(|l| {
                    ui.painter()
                        .layout_no_wrap(l.to_string(), FontId::proportional(13.0), theme::text())
                        .size()
                        .x
                })
                .fold(0.0f32, f32::max)
                    + 10.0;
                // Name, codec, help and Customize.
                ui.horizontal(|ui| {
                    let name = t.as_ref().map_or(String::new(), |t| t.name());
                    ui.label(RichText::new(name).strong().color(theme::text_strong()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ready = t
                            .as_ref()
                            .is_some_and(|t| t.availability() == Availability::Ready);
                        if ui
                            .add_enabled(ready, egui::Button::new(tr("Customize…")))
                            .clicked()
                            && let Some(t) = &t
                        {
                            self.customize = Some(customize::CustomizeWindow::new(t.clone(), app));
                        }
                        if let Some(t) = &t {
                            let (r, resp) =
                                ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                            ui.painter().circle_stroke(
                                r.center(),
                                7.5,
                                Stroke::new(1.2, theme::text()),
                            );
                            ui.painter().text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                "?",
                                FontId::proportional(11.5),
                                theme::text(),
                            );
                            resp.on_hover_text(format_help(t.format));
                            ui.label(tr(t.format.codec_name()));
                        }
                    });
                });
                let row = |ui: &mut Ui, label: &str, add: &mut dyn FnMut(&mut Ui)| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            vec2(label_w, 18.0),
                            egui::Label::new(RichText::new(label).color(theme::text_dim())),
                        );
                        add(ui);
                    });
                };
                if let Some(t) = &t {
                    let notes = t.notes();
                    if !notes.is_empty() {
                        row(ui, tr("Notes:"), &mut |ui| {
                            ui.label(&notes);
                        });
                    }
                    if let Some(v) = t.video_for(&p) {
                        let job = video_summary(t, &v, &p);
                        row(ui, tr("Video:"), &mut |ui| {
                            ui.label(job.clone());
                        });
                    }
                    if let Some(a) = t.audio_for(&p) {
                        let job = audio_summary(t, &a, &p);
                        row(ui, tr("Audio:"), &mut |ui| {
                            ui.label(job.clone());
                        });
                    }
                }
                ui.add_space(2.0);
                // Folder.
                let recent = app.settings.render.recent_folders.clone();
                let project_dir = app
                    .project_path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|d| d.display().to_string());
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(label_w, 22.0), egui::Label::new(tr("Folder:")));
                    let w = (ui.available_width() - 76.0).max(80.0);
                    ui.add_sized(vec2(w, 22.0), egui::TextEdit::singleline(&mut self.folder));
                    if crate::widgets::icon_button(ui, crate::icons::Icon::Folder, tr("Browse…"))
                        .clicked()
                    {
                        let mut d = rfd::FileDialog::new().set_title(tr("Folder"));
                        let start = PathBuf::from(self.folder.trim());
                        if start.is_dir() {
                            d = d.set_directory(start);
                        }
                        if let Some(dir) = d.pick_folder() {
                            self.folder = dir.display().to_string();
                        }
                    }
                    if crate::widgets::icon_button_ex(
                        ui,
                        crate::icons::Icon::Up,
                        tr("Project Location"),
                        false,
                        project_dir.is_some(),
                    )
                    .clicked()
                        && let Some(d) = &project_dir
                    {
                        self.folder = d.clone();
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(14.0, 22.0), Sense::click());
                    paint::drop_arrow(ui.painter(), r.center(), theme::text());
                    egui::Popup::menu(&resp).show(|ui| {
                        if recent.is_empty() {
                            ui.add_enabled(false, egui::Button::new(tr("No recent folders")));
                        }
                        for f in &recent {
                            if ui.button(f).clicked() {
                                self.folder = f.clone();
                                ui.close();
                            }
                        }
                    });
                });
                // Name, with the extension shown at the end of the box.
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(label_w, 22.0), egui::Label::new(tr("Name:")));
                    let w = (ui.available_width() - 4.0).max(80.0);
                    let r = ui.add_sized(vec2(w, 22.0), egui::TextEdit::singleline(&mut self.name));
                    if let Some(t) = &t {
                        ui.painter().text(
                            pos2(r.rect.right() - 8.0, r.rect.center().y),
                            Align2::RIGHT_CENTER,
                            format!(".{}", t.format.extension()),
                            FontId::proportional(12.5),
                            theme::text_disabled(),
                        );
                    }
                });
                // Free space and the estimated size.
                let folder = self.folder.trim().to_string();
                if self.free.as_ref().is_none_or(|(f, _)| *f != folder) {
                    self.free = Some((folder.clone(), bv_platform::free_space(Path::new(&folder))));
                }
                let free = self.free.as_ref().and_then(|f| f.1);
                let seconds = {
                    let pr = app.project();
                    match (self.loop_only, pr.loop_region) {
                        (true, Some(r)) => r.duration.as_seconds_f64(),
                        _ => pr.duration().as_seconds_f64(),
                    }
                };
                let size = t.as_ref().map_or(0.0, |t| estimate(t, &p, seconds));
                ui.horizontal(|ui| {
                    ui.add_space(label_w + 8.0);
                    ui.label(tr("Free disk space:"));
                    ui.label(free.map_or("—".into(), |b| size_text(b as f64)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let big = free.is_some_and(|f| size > f as f64);
                        ui.label(RichText::new(size_text(size)).color(if big {
                            theme::error()
                        } else {
                            theme::text()
                        }));
                        ui.label(tr("Estimated file size:"));
                    });
                });
            });
    }

    fn footer(&mut self, ui: &mut Ui, app: &mut BananaApp, all: &[Template]) -> Action {
        let mut action = Action::None;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(tr("Cancel")).min_size(vec2(96.0, 26.0)))
                    .clicked()
                {
                    action = Action::Close;
                }
                let ready = self.settings(app, all).is_some();
                let tip = if ready {
                    String::new()
                } else if app.project().duration() <= Tick::ZERO {
                    tr("Nothing to render: the timeline is empty.").to_string()
                } else {
                    tr("Pick a template that can be rendered.").to_string()
                };
                let r = ui.add_enabled(
                    ready,
                    egui::Button::new(RichText::new(tr("Render…")).strong())
                        .min_size(vec2(96.0, 26.0)),
                );
                if r.on_disabled_hover_text(tip).clicked() {
                    action = Action::Render;
                }
                let format = all
                    .iter()
                    .find(|t| Some(&t.id) == self.selected.as_ref())
                    .map(|t| t.format);
                self.options_button(ui, app, format);
                // AI Auto Reframe, offered when the shape changes.
                let t = all.iter().find(|t| Some(&t.id) == self.selected.as_ref());
                let p = &app.project().settings;
                if let Some(v) = t.and_then(|t| t.video_for(p)) {
                    let (pw, ph) = p.output_size();
                    if v.width as u64 * ph as u64 != v.height as u64 * pw as u64 {
                        ui.add_enabled(
                            false,
                            egui::Label::new(trf!(
                                "AI Auto Reframe from {} to {}",
                                templates::aspect(pw, ph),
                                templates::aspect(v.width, v.height)
                            )),
                        )
                        .on_disabled_hover_text(tr("Coming soon to BananaV"));
                    }
                }
                let has_loop = app.project().loop_region.is_some();
                ui.add_enabled_ui(has_loop, |ui| {
                    crate::widgets::checkbox(
                        ui,
                        &mut self.loop_only,
                        tr("Render loop region only"),
                    );
                    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    paint::loop_bracket(ui.painter(), r, self.loop_only && has_loop);
                })
                .response
                .on_disabled_hover_text(tr("Make a loop region on the timeline first."));
            });
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.customize.is_none() {
            action = Action::Close;
        }
        action
    }

    fn options_button(&mut self, ui: &mut Ui, app: &mut BananaApp, format: Option<FileFormat>) {
        // The path reference: kept by the files BananaV writes itself, of
        // a project saved.
        let saved = app.project_path.is_some();
        let keeps = format.is_some_and(|f| {
            matches!(
                f,
                FileFormat::AvcMp4
                    | FileFormat::HevcMp4
                    | FileFormat::Av1Mp4
                    | FileFormat::ProRes
                    | FileFormat::Aac
                    | FileFormat::Wav
            )
        });
        let o = &mut app.settings.render.options;
        let wave = matches!(format, Some(FileFormat::Wav | FileFormat::Wave64));
        let mut channels = false;
        Self::menu_icon(
            ui,
            "render-options",
            tr("Render options"),
            |p, r, c| crate::icons::paint(p, r, crate::icons::Icon::Settings, c),
            true,
            |ui| {
                ui.set_min_width(330.0);
                crate::widgets::checkbox(
                    ui,
                    &mut o.stretch,
                    tr("Stretch video to fill output frame size (do not letterbox)"),
                );
                crate::widgets::checkbox(
                    ui,
                    &mut o.use_rotation,
                    tr("Use project output rotation setting"),
                );
                // Multichannel mapping: Wave and Wave64 files get the
                // busses the Channel Mapping picks.
                ui.add_enabled(
                    wave,
                    egui::Checkbox::new(&mut o.multichannel, tr("Enable multichannel mapping")),
                )
                .on_disabled_hover_text(tr("Wave and Wave64 files only."));
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    if ui
                        .add_enabled(wave && o.multichannel, egui::Button::new(tr("Channels…")))
                        .clicked()
                    {
                        channels = true;
                        ui.close();
                    }
                });
                crate::widgets::checkbox(
                    ui,
                    &mut o.save_markers,
                    tr("Save project markers in media file"),
                );
                let why = if !saved {
                    tr("Save the project first.")
                } else {
                    tr("Files of this format keep no path reference.")
                };
                ui.add_enabled(
                    saved && keeps,
                    egui::Checkbox::new(
                        &mut o.path_reference,
                        tr("Save project as path reference in rendered file"),
                    ),
                )
                .on_disabled_hover_text(why);
                crate::widgets::checkbox(
                    ui,
                    &mut o.loudness_log,
                    tr("Save loudness log file next to media file"),
                );
                crate::widgets::checkbox(ui, &mut o.swap_files, tr("Swap video files on render"))
                    .on_hover_text(tr(
                        "A file already there is replaced only once the render has finished well.",
                    ));
            },
        );
        if channels {
            self.channels = Some(channel_mapping::ChannelMapping::new(app.project()));
        }
    }
}

fn unavailable_text(a: Availability) -> &'static str {
    match a {
        Availability::Soon => tr("Coming soon to BananaV"),
        Availability::NoEncoder => tr(
            "This computer has no encoder for this format (a Windows extension or driver is missing).",
        ),
        Availability::Ready => "",
    }
}

/// The "?" next to the codec: what the format is good for.
fn format_help(f: FileFormat) -> &'static str {
    use FileFormat::*;
    match f {
        AvcMp4 => tr(
            "H.264 video and AAC audio in MP4: plays everywhere. Encoded by the processor or by a graphics card (NVENC, AMD, Quick Sync).",
        ),
        HevcMp4 => tr(
            "HEVC (H.265) video and AAC audio in MP4: about half the size of H.264 at the same quality.",
        ),
        Av1Mp4 => tr(
            "AV1 video and AAC audio in MP4: the most efficient web codec, encoded by recent graphics cards.",
        ),
        ProRes => tr(
            "Apple ProRes in QuickTime: an editing codec of constant quality. BananaV's own encoder.",
        ),
        Wmv => tr("Windows Media Video 9 with Windows Media Audio, encoded by Windows."),
        Mpeg1 => tr("MPEG-1 for Video CD authoring."),
        Mpeg2 => tr("MPEG-2 for DVD and Blu-ray authoring."),
        Bmp | Dds | Dpx | Exr | Heic | Jpeg | Png | Tiff | WmPhoto => {
            tr("One picture per frame, numbered: name_000000, name_000001…")
        }
        Aac => tr("AAC audio in an MPEG-4 file (.m4a)."),
        Ac3 => tr("Dolby Digital (AC-3), for DVD and Blu-ray."),
        Aiff => tr("Uncompressed audio in Apple's AIFF."),
        Flac => tr("Lossless compressed audio. BananaV's own encoder."),
        Mp3 => tr("MPEG Layer 3 audio, encoded by Windows."),
        OggVorbis => tr("Ogg Vorbis audio."),
        Wav => tr("Uncompressed audio in a Wave file (RF64 past 4 GB)."),
        Wave64 => tr("Uncompressed audio in Wave64, for files of any size."),
        Wma => tr("Windows Media Audio, encoded by Windows."),
    }
}

/// Text pieces, the ones that differ from the project in red.
fn summary(parts: &[(String, bool)]) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (text, off) in parts {
        let color = if *off { theme::error() } else { theme::text() };
        job.append(
            &format!("{text}; "),
            0.0,
            egui::TextFormat::simple(FontId::proportional(12.5), color),
        );
    }
    job
}

fn video_summary(
    t: &Template,
    v: &bv_engine::VideoParams,
    p: &bv_timeline::ProjectSettings,
) -> egui::text::LayoutJob {
    let (pw, ph) = p.output_size();
    let mut parts = vec![
        (crate::fmt::fps(v.frame_rate), v.frame_rate != p.frame_rate),
        (
            format!("{}x{}", v.width, v.height),
            (v.width, v.height) != (pw, ph),
        ),
        (templates::aspect(v.width, v.height), false),
        (
            match v.field_order {
                bv_core::FieldOrder::Progressive => tr("Progressive"),
                bv_core::FieldOrder::UpperFirst => tr("Upper field first"),
                bv_core::FieldOrder::LowerFirst => tr("Lower field first"),
            }
            .to_string(),
            v.field_order != p.field_order,
        ),
        (crate::fmt::num(1.0, 3), false),
        (trf!("{} bit", v.bit_depth), false),
        ("Rec. 709".to_string(), false),
    ];
    match t.format {
        FileFormat::ProRes => parts.push((format!("ProRes {}", v.prores.name()), false)),
        f if f.category() == Category::Video => {
            let mode = match v.rate_mode {
                bv_engine::RateMode::Cbr => "CBR",
                bv_engine::RateMode::Vbr => "VBR",
                bv_engine::RateMode::Quality => "CQ",
            };
            parts.push((
                format!("{} Mb/s {mode}", crate::fmt::num(v.bitrate as f64 / 1e6, 1)),
                false,
            ));
            parts.push((tr(v.encoder.short_name()).to_string(), false));
        }
        _ => {}
    }
    if v.alpha && t.format.supports_alpha() {
        parts.push((tr("Alpha").to_string(), false));
    }
    summary(&parts)
}

fn audio_summary(
    t: &Template,
    a: &bv_engine::AudioParams,
    p: &bv_timeline::ProjectSettings,
) -> egui::text::LayoutJob {
    let mut parts = vec![
        (
            format!("{} Hz", crate::fmt::grouped(a.sample_rate as u64)),
            a.sample_rate != p.sample_rate.0,
        ),
        (
            if a.float {
                tr("32 bit float").to_string()
            } else {
                trf!("{} bit", a.bit_depth)
            },
            false,
        ),
        (
            templates::channels_text(a.channels),
            a.channels as usize != p.channels.channel_count(),
        ),
        (t.format.audio_mime().to_string(), false),
    ];
    if t.format.compressed_audio() {
        parts.push((
            format!("{} kb/s", crate::fmt::num(a.bitrate as f64 / 1000.0, 1)),
            false,
        ));
    }
    summary(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render to New Track: the project's sound over a time selection into
    /// a WAV file, put on a new audio track where the selection starts.
    #[test]
    fn render_to_new_track_puts_the_file_on_new_tracks() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let dir = std::env::temp_dir().join(format!("bv-new-track-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let at = Tick::from_seconds(1);
        let s = RenderSettings {
            project_file: None,
            format: FileFormat::Wav,
            path: dir.join("Mixdown.wav"),
            range: TimeRange::new(at, Tick::from_seconds(1)),
            video: None,
            audio: Some(bv_engine::AudioParams::default()),
            options: bv_engine::RenderOptions::default(),
        };
        let tracks = app.project().tracks.len();
        app.engine.set_project(app.history.current());
        app.engine.start_render(s);
        app.new_track_render = Some(at);
        let started = std::time::Instant::now();
        while app.new_track_render.is_some() && started.elapsed().as_secs() < 60 {
            place_new_track(&mut app);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let p = app.project();
        assert_eq!(p.tracks.len(), tracks + 1, "a new audio track");
        let t = p.tracks.last().unwrap();
        assert_eq!(t.kind, bv_timeline::TrackKind::Audio);
        assert_eq!(t.clips[0].start, at);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes_in_binary_units() {
        assert_eq!(size_text(0.0), "0.0 B");
        assert_eq!(size_text(1536.0), "1.5 KB");
        assert_eq!(size_text(3.2 * 1024.0 * 1024.0 * 1024.0), "3.2 GB");
    }

    #[test]
    fn filters_follow_the_project() {
        let p = bv_timeline::ProjectSettings::default();
        let all = templates::builtin();
        let on = Filters {
            resolution: true,
            frame_rate: true,
            ..Filters::default()
        };
        let (w, h) = p.output_size();
        for t in all.iter().filter(|t| matches(t, &p, &on)) {
            if let Some(v) = t.video_for(&p) {
                assert_eq!(
                    (v.width, v.height, v.frame_rate),
                    (w, h, p.frame_rate),
                    "{}",
                    t.id
                );
            }
        }
    }
}
