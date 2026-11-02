//! File › Properties: VEGAS's Project Properties window, with OK, Cancel
//! and Apply. Changes are made on a draft and reach the project (as one
//! undoable edit) on OK/Apply.
//!
//! The Video tab has every control of VEGAS's. The ones BananaV cannot do
//! yet (interlaced output, non-square pixels, HDR,
//! deinterlacing, frame blending, prerendering) stay visible but greyed out
//! with the "Coming soon" tip; ACES's work with 32-bit floating point
//! (full range), as VEGAS's (`bv_timeline::color`). The Audio tab has VEGAS's controls too
//! (stereo busses and recording come later), and so do the Ruler and
//! Summary tabs, and so does Audio CD (kept for when BananaV burns CDs).
//! The Advanced tab's master display needs HDR projects: greyed out; its
//! 360 Output makes the project a 360° one.

use std::path::PathBuf;

use bv_core::{ChannelLayout, FrameRate, SampleRate, Tick, timecode};
use bv_timeline::PixelAspect;
use bv_timeline::{
    AcesDisplay, AcesLook, AcesSpace, AcesVersion, AcesView, AudioQuality, CompositingGamma,
    OutputRotation, PixelFormat, Project, ProjectSettings, RenderQuality, ViewTransform,
};

use crate::app::BananaApp;
use crate::egui::{self, RichText, Stroke, Ui, vec2};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::settings::{NewProjectDefaults, VideoTemplate};
use crate::theme;
use crate::timefmt::{RulerFormat, RulerFormatExt};
use crate::widgets;

/// Width of the tab pages.
const PAGE_WIDTH: f32 = 420.0;
/// Width of the drop-down lists in the lower half of the Video tab.
const LIST_WIDTH: f32 = 218.0;
/// The labels of the lower half of the Video tab.
const PROCESSING_LABELS: [&str; 10] = [
    "Pixel format:",
    "Compositing gamma:",
    "ACES version:",
    "ACES color space:",
    "View transform:",
    "Look modification transform:",
    "Full-resolution rendering quality:",
    "Motion blur type:",
    "Deinterlace method:",
    "Resample mode:",
];
/// Width of the drop-down lists next to Width/Height.
const SIDE_WIDTH: f32 = 172.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropTab {
    Video,
    Audio,
    Ruler,
    Summary,
    AudioCd,
    Advanced,
}

impl PropTab {
    const ALL: [PropTab; 6] = [
        PropTab::Video,
        PropTab::Audio,
        PropTab::Ruler,
        PropTab::Summary,
        PropTab::AudioCd,
        PropTab::Advanced,
    ];

    fn label(self) -> &'static str {
        match self {
            PropTab::Video => tr("Video"),
            PropTab::Audio => tr("Audio"),
            PropTab::Ruler => tr("Ruler"),
            PropTab::Summary => tr("Summary"),
            PropTab::AudioCd => tr("Audio CD"),
            PropTab::Advanced => tr("Advanced"),
        }
    }

    /// The tabs BananaV has already.
    fn ready(self) -> bool {
        true
    }
}

/// VEGAS's frame rate list (other rates can be typed).
pub(crate) const RATES: [(FrameRate, &str); 10] = [
    (FrameRate::new(12, 1), "Half Film"),
    (FrameRate::new(25, 2), "Half PAL"),
    (FrameRate::new(15000, 1001), "Half NTSC"),
    (FrameRate::new(15, 1), "Multimedia"),
    (FrameRate::FPS_23_976, "IVTC Film"),
    (FrameRate::FPS_24, "Film"),
    (FrameRate::FPS_25, "PAL"),
    (FrameRate::FPS_29_97, "NTSC"),
    (FrameRate::FPS_50, "Double PAL"),
    (FrameRate::FPS_59_94, "Double NTSC"),
];

/// The templates that come with BananaV: progressive, square pixels.
const TEMPLATES: [(&str, u32, u32, FrameRate); 17] = [
    ("HD 720-24p", 1280, 720, FrameRate::FPS_23_976),
    ("HD 720-25p", 1280, 720, FrameRate::FPS_25),
    ("HD 720-30p", 1280, 720, FrameRate::FPS_29_97),
    ("HD 720-50p", 1280, 720, FrameRate::FPS_50),
    ("HD 720-60p", 1280, 720, FrameRate::FPS_59_94),
    ("HD 1080-24p", 1920, 1080, FrameRate::FPS_23_976),
    ("HD 1080-25p", 1920, 1080, FrameRate::FPS_25),
    ("HD 1080-30p", 1920, 1080, FrameRate::FPS_29_97),
    ("HD 1080-50p", 1920, 1080, FrameRate::FPS_50),
    ("HD 1080-60p", 1920, 1080, FrameRate::FPS_59_94),
    ("UHD 2160-24p", 3840, 2160, FrameRate::FPS_23_976),
    ("UHD 2160-25p", 3840, 2160, FrameRate::FPS_25),
    ("UHD 2160-30p", 3840, 2160, FrameRate::FPS_29_97),
    ("UHD 2160-50p", 3840, 2160, FrameRate::FPS_50),
    ("UHD 2160-60p", 3840, 2160, FrameRate::FPS_59_94),
    ("Vertical HD 1080-30p", 1080, 1920, FrameRate::FPS_29_97),
    ("Square HD 1080-30p", 1080, 1080, FrameRate::FPS_29_97),
];

/// VEGAS's sample rate list (other rates can be typed).
const SAMPLE_RATES: [u32; 10] = [
    8_000, 11_025, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
];

/// The LFE cutoff presets (other frequencies can be typed).
const LFE_CUTOFFS: [(u32, &str); 2] = [(80, "Dolby consumer/DVD"), (120, "pro/film")];

/// Width of the small drop-down lists of the Audio tab.
const SMALL_LIST: f32 = 76.0;

/// A typed sample rate ("48.000", "44100"): 2,000 to 384,000 Hz.
fn parse_sample_rate(text: &str) -> Option<SampleRate> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | ' '))
        .filter(char::is_ascii_digit)
        .collect();
    let hz: u32 = digits.parse().ok()?;
    (2_000..=384_000).contains(&hz).then_some(SampleRate(hz))
}

/// "120 (pro/film)": an LFE cutoff as the list shows it.
fn cutoff_label(hz: u32) -> String {
    match LFE_CUTOFFS.iter().find(|(f, _)| *f == hz) {
        Some((_, name)) => format!("{hz} ({})", tr(name)),
        None => hz.to_string(),
    }
}

/// A typed LFE cutoff: 10 to 1,000 Hz.
fn parse_cutoff(text: &str) -> Option<u32> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let hz: u32 = digits.parse().ok()?;
    (10..=1_000).contains(&hz).then_some(hz)
}

/// Where recorded audio goes unless the project says otherwise (VEGAS uses
/// the Documents folder).
pub(crate) fn default_recorded_folder() -> PathBuf {
    bv_platform::user_dir(bv_platform::UserDir::Documents).unwrap_or_else(std::env::temp_dir)
}

/// The folder as the project stores it: empty when it is the default.
fn stored_folder(text: &str, default: PathBuf) -> String {
    let t = text.trim();
    if t.is_empty() || std::path::Path::new(t) == default {
        String::new()
    } else {
        t.to_string()
    }
}

/// The folder as the box shows it: the default one when none is stored.
fn shown_folder(stored: &str, default: PathBuf) -> String {
    if stored.is_empty() {
        default.display().to_string()
    } else {
        stored.to_string()
    }
}

/// A folder box: its label, the path with Browse…, and the free space of
/// its disk (read again when the path changes).
fn folder_box(
    ui: &mut Ui,
    label: &str,
    folder: &mut String,
    free: &mut Option<(String, Option<u64>)>,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        let browse_w = 74.0;
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            vec2(ui.available_width() - browse_w - 6.0, h),
            egui::TextEdit::singleline(folder),
        );
        if ui
            .add(egui::Button::new(tr("Browse…")).min_size(vec2(browse_w, 0.0)))
            .clicked()
        {
            let start = PathBuf::from(folder.trim());
            let mut dialog = rfd::FileDialog::new().set_title(label.trim_end_matches(':'));
            if start.is_dir() {
                dialog = dialog.set_directory(&start);
            }
            if let Some(dir) = dialog.pick_folder() {
                *folder = dir.display().to_string();
            }
        }
    });
    let path = folder.trim().to_string();
    if free.as_ref().is_none_or(|(f, _)| *f != path) {
        let bytes = bv_platform::free_space(std::path::Path::new(&path));
        *free = Some((path, bytes));
    }
    ui.horizontal(|ui| {
        ui.label(tr("Free storage space in selected folder:"));
        ui.add_space(12.0);
        let text = match free.as_ref().and_then(|(_, b)| *b) {
            Some(b) => storage_text(b),
            None => "—".into(),
        };
        ui.label(text);
    });
}

/// "29.970 (NTSC)": a frame rate as VEGAS lists it.
fn rate_label(rate: FrameRate) -> String {
    let n = crate::fmt::num(rate.as_f64(), 3);
    match RATES.iter().find(|(r, _)| *r == rate) {
        Some((_, name)) => format!("{n} ({})", tr(name)),
        None => n,
    }
}

/// A typed frame rate ("29,97", "30", "29.970 (NTSC)"). Rates near the NTSC
/// ones (×1000/1001) become exactly those.
fn parse_rate(text: &str) -> Option<FrameRate> {
    let number: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    let v = crate::fmt::parse(&number)?;
    if !(0.5..=300.0).contains(&v) {
        return None;
    }
    for k in [12, 15, 24, 30, 48, 60, 120] {
        if (v - k as f64 * 1000.0 / 1001.0).abs() < 0.0015 {
            return Some(FrameRate::new(k * 1000, 1001));
        }
    }
    let num = (v * 1000.0).round() as u32;
    let g = gcd(num, 1000);
    Some(FrameRate::new(num / g, 1000 / g))
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// "HD 1080-30p (1920x1080; 29.970 fps)".
fn template_label(name: &str, w: u32, h: u32, rate: FrameRate) -> String {
    format!(
        "{} ({w}x{h}; {} fps)",
        tr(name),
        crate::fmt::num(rate.as_f64(), 3)
    )
}

/// `true` when the Video tab's settings of `a` and `b` are the same.
fn same_video(a: &ProjectSettings, b: &ProjectSettings) -> bool {
    (
        a.width,
        a.height,
        a.frame_rate,
        a.pixel_format,
        a.compositing_gamma,
        a.rotation,
        a.render_quality,
    ) == (
        b.width,
        b.height,
        b.frame_rate,
        b.pixel_format,
        b.compositing_gamma,
        b.rotation,
        b.render_quality,
    )
}

/// The name the template box shows for `s`.
fn template_name(s: &ProjectSettings, saved: &[VideoTemplate]) -> String {
    if let Some(t) = saved.iter().find(|t| same_video(&t.settings, s)) {
        return t.name.clone();
    }
    TEMPLATES
        .iter()
        .find(|t| (t.1, t.2, t.3) == (s.width, s.height, s.frame_rate))
        .map_or_else(
            || tr("(Custom)").to_string(),
            |t| template_label(t.0, t.1, t.2, t.3),
        )
}

/// Where prerendered video goes unless the project says otherwise.
fn default_prerender_folder() -> PathBuf {
    bv_platform::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("BananaV")
}

/// "115.1 Gigabytes".
fn storage_text(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{} {}", crate::fmt::num(b / GB, 1), tr("Gigabytes"))
    } else {
        format!(
            "{} {}",
            crate::fmt::num(b / (1024.0 * 1024.0), 1),
            tr("Megabytes")
        )
    }
}

/// The values the window edits.
#[derive(Clone, Debug, PartialEq)]
struct Draft {
    settings: ProjectSettings,
    /// As the project stores it: empty is the default folder.
    prerender_folder: String,
    /// As the project stores it: empty is the default folder.
    recorded_folder: String,
    /// Ruler start time.
    ruler_offset: Tick,
    summary: bv_timeline::Summary,
    audio_cd: bv_timeline::AudioCd,
    /// Audio: the number of stereo busses.
    busses: usize,
}

impl Draft {
    fn of(p: &Project) -> Draft {
        Draft {
            settings: p.settings,
            prerender_folder: p.prerender_folder.clone(),
            recorded_folder: p.recorded_folder.clone(),
            ruler_offset: p.ruler_offset,
            summary: p.summary.clone(),
            audio_cd: p.audio_cd.clone(),
            busses: p.busses.len(),
        }
    }
}

enum TemplatePick {
    BuiltIn(usize),
    Saved(usize),
}

#[derive(Debug)]
pub struct ProjectPropertiesDialog {
    tab: PropTab,
    draft: Draft,
    /// The template box: a template's name, or a name being typed.
    template: String,
    width: String,
    height: String,
    rate: String,
    /// The prerendered files folder as shown (never empty).
    folder: String,
    /// Free space of the folder, read again when the folder changes.
    free: Option<(String, Option<u64>)>,
    /// The recorded files folder as shown (never empty).
    recorded: String,
    recorded_free: Option<(String, Option<u64>)>,
    /// The sample rate box ("48,000").
    sample_rate: String,
    /// The LFE cutoff box ("120 (pro/film)").
    cutoff: String,
    /// The ruler start time box ("00:00:00;00").
    ruler_start: String,
    start_new_projects: bool,
    /// The template box has the keyboard (a name is being typed).
    template_typing: bool,
    /// Height of the Video page, kept by the other tabs.
    page_height: f32,
    /// Width of the pages: wider than [`PAGE_WIDTH`] when the language's
    /// texts need it.
    page_width: f32,
}

impl ProjectPropertiesDialog {
    /// The dialog on its Ruler tab (the ruler's Set Project Tempo…).
    pub fn on_ruler(mut self) -> ProjectPropertiesDialog {
        self.tab = PropTab::Ruler;
        self
    }

    /// The dialog open on its Audio tab.
    pub fn on_audio_tab(mut self) -> ProjectPropertiesDialog {
        self.tab = PropTab::Audio;
        self
    }

    pub fn new(app: &BananaApp) -> ProjectPropertiesDialog {
        let draft = Draft::of(app.project());
        let s = draft.settings;
        let folder = shown_folder(&draft.prerender_folder, default_prerender_folder());
        let recorded = shown_folder(&draft.recorded_folder, default_recorded_folder());
        ProjectPropertiesDialog {
            tab: PropTab::Video,
            template: template_name(&s, &app.settings.video_templates),
            width: s.width.to_string(),
            height: s.height.to_string(),
            rate: rate_label(s.frame_rate),
            folder,
            free: None,
            recorded,
            recorded_free: None,
            sample_rate: crate::fmt::grouped(s.sample_rate.0 as u64),
            cutoff: cutoff_label(s.lfe_cutoff_hz),
            ruler_start: timecode::format(draft.ruler_offset, s.frame_rate),
            start_new_projects: false,
            template_typing: false,
            page_height: 0.0,
            page_width: PAGE_WIDTH,
            draft,
        }
    }

    /// Draws the window; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut close = false;
        crate::floating::Window::new(tr("Project Properties"))
            .id(egui::Id::new("project-properties"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                self.tab_strip(ui);
                egui::Frame::new()
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0, theme::separator()))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(self.page_width);
                        // Text boxes and lists with a border, like VEGAS's.
                        ui.visuals_mut().widgets.inactive.bg_stroke =
                            Stroke::new(1.0, theme::separator());
                        let top = ui.cursor().top();
                        match self.tab {
                            PropTab::Video => {
                                self.video(ui, app);
                                ui.separator();
                                self.page_height = ui.cursor().top() - top;
                                self.page_width = ui.min_rect().width().max(PAGE_WIDTH);
                            }
                            PropTab::Audio => self.audio(ui),
                            PropTab::Ruler => self.ruler(ui),
                            PropTab::Summary => self.summary(ui),
                            PropTab::AudioCd => self.audio_cd(ui),
                            PropTab::Advanced => advanced(ui, &mut self.draft.settings),
                        }
                        // The other tabs are as tall as the Video tab.
                        let used = ui.cursor().top() - top;
                        if used < self.page_height {
                            ui.add_space(self.page_height - used);
                        }
                        // At the bottom of every tab but Audio CD (a disc's
                        // codes are not for every project), like VEGAS: it
                        // takes the whole dialog's settings.
                        let shown = self.tab != PropTab::AudioCd;
                        ui.scope(|ui| {
                            // Hidden, it keeps its room: all tabs stay as tall.
                            if !shown {
                                ui.set_invisible();
                            }
                            widgets::checkbox(
                                ui,
                                &mut self.start_new_projects,
                                tr("Start all new projects with these settings"),
                            );
                        });
                    });
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let changed = self.changed(app);
                    if ui
                        .add_enabled(
                            changed,
                            egui::Button::new(tr("Apply")).min_size(vec2(72.0, 0.0)),
                        )
                        .clicked()
                    {
                        self.apply(app);
                    }
                    if ui
                        .add(egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("OK").strong())
                                .min_size(vec2(72.0, 0.0)),
                        )
                        .clicked()
                    {
                        self.apply(app);
                        close = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
        open && !close
    }

    fn tab_strip(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for tab in PropTab::ALL {
                if widgets::tab_button(ui, tab.label(), self.tab == tab, tab.ready(), 0.0).clicked()
                {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(2.0);
    }

    /// Something to apply: an edited value or "Start all new projects".
    fn changed(&self, app: &BananaApp) -> bool {
        self.start_new_projects || self.edited() != Draft::of(app.project())
    }

    /// The draft with the text boxes read.
    fn edited(&self) -> Draft {
        let mut d = self.draft.clone();
        if let Some(w) = parse_size(&self.width) {
            d.settings.width = w;
        }
        if let Some(h) = parse_size(&self.height) {
            d.settings.height = h;
        }
        if let Some(r) = parse_rate(&self.rate) {
            d.settings.frame_rate = r;
        }
        if let Some(r) = parse_sample_rate(&self.sample_rate) {
            d.settings.sample_rate = r;
        }
        if let Some(hz) = parse_cutoff(&self.cutoff) {
            d.settings.lfe_cutoff_hz = hz;
        }
        d.prerender_folder = stored_folder(&self.folder, default_prerender_folder());
        d.recorded_folder = stored_folder(&self.recorded, default_recorded_folder());
        if let Some(t) = timecode::parse(&self.ruler_start, d.settings.frame_rate) {
            d.ruler_offset = t;
        }
        d
    }

    fn apply(&mut self, app: &mut BananaApp) {
        self.draft = self.edited();
        self.refresh_texts(app);
        let d = self.draft.clone();
        if d != Draft::of(app.project()) {
            app.modify(|p| {
                p.settings = d.settings;
                p.prerender_folder = d.prerender_folder.clone();
                p.recorded_folder = d.recorded_folder.clone();
                p.ruler_offset = d.ruler_offset;
                p.summary = d.summary.clone();
                p.audio_cd = d.audio_cd.clone();
                p.set_bus_count(d.busses);
            });
        }
        if self.start_new_projects {
            app.settings.new_project = Some(NewProjectDefaults {
                settings: d.settings,
                prerender_folder: d.prerender_folder,
                recorded_folder: d.recorded_folder,
                ruler_offset: d.ruler_offset,
                summary: d.summary,
            });
            let _ = app.settings.save();
            self.start_new_projects = false;
        }
    }

    /// Writes the draft's values back into the text boxes.
    fn refresh_texts(&mut self, app: &BananaApp) {
        let s = &self.draft.settings;
        self.width = s.width.to_string();
        self.height = s.height.to_string();
        self.rate = rate_label(s.frame_rate);
        self.sample_rate = crate::fmt::grouped(s.sample_rate.0 as u64);
        self.cutoff = cutoff_label(s.lfe_cutoff_hz);
        self.ruler_start = timecode::format(self.draft.ruler_offset, s.frame_rate);
        self.template = template_name(s, &app.settings.video_templates);
    }

    fn video(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        ui.spacing_mut().item_spacing.y = 6.0;
        self.template_row(ui, app);
        ui.add_space(4.0);
        self.frame_grid(ui, app);
        ui.add_space(2.0);
        ui.separator();
        self.processing_grid(ui);
        ui.separator();
        self.prerender(ui);
        self.follow_template(app);
    }

    fn template_row(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        ui.horizontal(|ui| {
            ui.label(tr("Template:"));
            let saved = app.settings.video_templates.clone();
            let current = self.template.clone();
            let (text, picked) = widgets::editable_combo(
                ui,
                egui::Id::new("pp-template"),
                &mut self.template,
                250.0,
                |ui| {
                    let mut pick = None;
                    for (i, t) in TEMPLATES.iter().enumerate() {
                        let label = template_label(t.0, t.1, t.2, t.3);
                        if ui.selectable_label(label == current, label).clicked() {
                            pick = Some(TemplatePick::BuiltIn(i));
                        }
                    }
                    if !saved.is_empty() {
                        ui.separator();
                    }
                    for (i, t) in saved.iter().enumerate() {
                        if ui.selectable_label(t.name == current, &t.name).clicked() {
                            pick = Some(TemplatePick::Saved(i));
                        }
                    }
                    pick
                },
            );
            self.template_typing = text.has_focus();
            match picked {
                Some(TemplatePick::BuiltIn(i)) => {
                    let (_, w, h, r) = TEMPLATES[i];
                    let s = &mut self.draft.settings;
                    s.width = w;
                    s.height = h;
                    s.frame_rate = r;
                    self.refresh_texts(app);
                }
                Some(TemplatePick::Saved(i)) => {
                    let t = saved[i].settings;
                    let s = &mut self.draft.settings;
                    s.width = t.width;
                    s.height = t.height;
                    s.frame_rate = t.frame_rate;
                    s.pixel_format = t.pixel_format;
                    s.compositing_gamma = t.compositing_gamma;
                    s.rotation = t.rotation;
                    s.render_quality = t.render_quality;
                    self.refresh_texts(app);
                }
                None => {}
            }
            let name = self.template.trim().to_string();
            let built_in = TEMPLATES
                .iter()
                .any(|t| template_label(t.0, t.1, t.2, t.3) == name);
            let can_save = !name.is_empty() && !built_in && name != tr("(Custom)");
            if widgets::icon_button_ex(ui, Icon::Save, tr("Save Template"), false, can_save)
                .clicked()
                && can_save
            {
                self.draft = self.edited();
                let settings = self.draft.settings;
                let list = &mut app.settings.video_templates;
                match list.iter_mut().find(|t| t.name == name) {
                    Some(t) => t.settings = settings,
                    None => list.push(VideoTemplate {
                        name: name.clone(),
                        settings,
                    }),
                }
                let _ = app.settings.save();
                self.template = name.clone();
            }
            let deletable = app.settings.video_templates.iter().any(|t| t.name == name);
            if widgets::icon_button_tinted(
                ui,
                Icon::Delete,
                tr("Delete Template"),
                theme::error(),
                deletable,
            )
            .clicked()
                && deletable
            {
                app.settings.video_templates.retain(|t| t.name != name);
                let _ = app.settings.save();
                self.template = template_name(&self.draft.settings, &app.settings.video_templates);
            }
            ui.add_space(8.0);
            if widgets::icon_button(ui, Icon::MatchMedia, tr("Match Media Video Settings"))
                .clicked()
            {
                self.match_media(app);
            }
        });
    }

    /// Sets the frame size and rate from a media file the user picks.
    fn match_media(&mut self, app: &mut BananaApp) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Match Media Video Settings"))
            .pick_file()
        else {
            return;
        };
        match app.probe_cached(&path).and_then(|m| m.video) {
            Some(v) => {
                let s = &mut self.draft.settings;
                s.width = (v.width & !1).max(16);
                s.height = (v.height & !1).max(16);
                s.frame_rate = v.frame_rate;
                s.pixel_aspect = v.pixel_aspect;
                self.refresh_texts(app);
            }
            None => app.report_error(trf!("{}: no video to match.", path.display())),
        }
    }

    /// Width, height, HDR mode | field order, pixel aspect ratio, output
    /// rotation and frame rate.
    fn frame_grid(&mut self, ui: &mut Ui, app: &BananaApp) {
        let h = ui.spacing().interact_size.y;
        egui::Grid::new("pp-frame")
            .num_columns(4)
            .spacing(vec2(8.0, 6.0))
            .show(ui, |ui| {
                ui.label(tr("Width:"));
                let r = ui.add_sized(vec2(52.0, h), egui::TextEdit::singleline(&mut self.width));
                if r.lost_focus() {
                    self.commit_size(app);
                }
                // Media of another order is deinterlaced; rendering fields
                // is still to come.
                ui.label(tr("Field order:"));
                choice(
                    ui,
                    "pp-field-order",
                    SIDE_WIDTH,
                    &mut self.draft.settings.field_order,
                    &field_orders(),
                );
                ui.end_row();

                ui.label(tr("Height:"));
                let r = ui.add_sized(vec2(52.0, h), egui::TextEdit::singleline(&mut self.height));
                if r.lost_focus() {
                    self.commit_size(app);
                }
                ui.label(tr("Pixel aspect ratio:"));
                let (ratios, mut par) = pixel_aspects(self.draft.settings.pixel_aspect);
                choice(ui, "pp-par", SIDE_WIDTH, &mut par, &ratios);
                self.draft.settings.pixel_aspect = par;
                ui.end_row();

                ui.label(tr("HDR Mode:"));
                let mut hdr = 0;
                choice(
                    ui,
                    "pp-hdr",
                    52.0,
                    &mut hdr,
                    &[
                        (0, tr("Off").into(), true),
                        (1, "HDR 10".into(), false),
                        (2, "HLG".into(), false),
                    ],
                );
                ui.label(tr("Output rotation:"));
                let rotations: Vec<(OutputRotation, String, bool)> = OutputRotation::ALL
                    .iter()
                    .map(|&r| (r, rotation_label(r).to_string(), true))
                    .collect();
                choice(
                    ui,
                    "pp-rotation",
                    SIDE_WIDTH,
                    &mut self.draft.settings.rotation,
                    &rotations,
                );
                ui.end_row();

                ui.label("");
                ui.label("");
                ui.label(tr("Frame rate:"));
                let (r, picked) = widgets::editable_combo(
                    ui,
                    egui::Id::new("pp-rate"),
                    &mut self.rate,
                    SIDE_WIDTH,
                    |ui| {
                        let current = self.draft.settings.frame_rate;
                        let mut pick = None;
                        for (rate, _) in RATES {
                            if ui
                                .selectable_label(rate == current, rate_label(rate))
                                .clicked()
                            {
                                pick = Some(rate);
                            }
                        }
                        pick
                    },
                );
                if let Some(rate) = picked {
                    self.draft.settings.frame_rate = rate;
                    self.refresh_texts(app);
                } else if r.lost_focus() {
                    if let Some(rate) = parse_rate(&self.rate) {
                        self.draft.settings.frame_rate = rate;
                    }
                    self.refresh_texts(app);
                }
                ui.end_row();
            });
    }

    /// The template box follows the values, except while a new name is
    /// being typed (to be saved).
    fn follow_template(&mut self, app: &BananaApp) {
        if self.template_typing {
            return;
        }
        let known = self.template == tr("(Custom)")
            || TEMPLATES
                .iter()
                .any(|t| template_label(t.0, t.1, t.2, t.3) == self.template)
            || app
                .settings
                .video_templates
                .iter()
                .any(|t| t.name == self.template);
        if known {
            self.template = template_name(&self.edited().settings, &app.settings.video_templates);
        }
    }

    /// Reads the width and height boxes (even sizes from 16 to 8192).
    fn commit_size(&mut self, app: &BananaApp) {
        let s = &mut self.draft.settings;
        if let Some(w) = parse_size(&self.width) {
            s.width = w;
        }
        if let Some(h) = parse_size(&self.height) {
            s.height = h;
        }
        self.refresh_texts(app);
    }

    /// Pixel format down to resample mode.
    fn processing_grid(&mut self, ui: &mut Ui) {
        // The lists fill the page right of the longest label.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let label_w = PROCESSING_LABELS
            .iter()
            .map(|l| {
                ui.painter()
                    .layout_no_wrap(tr(l).to_string(), font.clone(), theme::text())
                    .size()
                    .x
            })
            .fold(0.0, f32::max)
            .ceil();
        let list_w = (self.page_width - label_w - 8.0).max(LIST_WIDTH);
        egui::Grid::new("pp-processing")
            .num_columns(2)
            .min_col_width(label_w)
            .spacing(vec2(8.0, 6.0))
            .show(ui, |ui| {
                let s = &mut self.draft.settings;
                ui.label(tr("Pixel format:"));
                let formats: Vec<(PixelFormat, String, bool)> = PixelFormat::ALL
                    .iter()
                    .map(|&f| (f, pixel_format_label(f).to_string(), true))
                    .collect();
                let before = s.pixel_format;
                choice(ui, "pp-pixel-format", list_w, &mut s.pixel_format, &formats);
                if s.pixel_format != before {
                    picked_pixel_format(s);
                }
                ui.end_row();

                ui.label(tr("Compositing gamma:"));
                let gammas = [
                    (
                        CompositingGamma::Linear,
                        format!("{} ({})", crate::fmt::num(1.0, 3), tr("Linear")),
                        true,
                    ),
                    (
                        CompositingGamma::Video,
                        format!("{} ({})", crate::fmt::num(2.222, 3), tr("Video")),
                        true,
                    ),
                ];
                // As VEGAS's: a choice with 32-bit floating point (full
                // range) only; the others composite in video gamma.
                if s.pixel_format == PixelFormat::Float32Full {
                    choice(ui, "pp-gamma", list_w, &mut s.compositing_gamma, &gammas);
                } else {
                    fixed_choice(
                        ui,
                        list_w,
                        "pp-gamma-8",
                        &gammas[1].1,
                        tr("Only 32-bit floating point (full range) can composite in linear light; the other formats composite in video gamma."),
                    );
                }
                ui.end_row();

                aces_rows(ui, s, list_w);

                ui.label(tr("Full-resolution rendering quality:"));
                let qualities: Vec<(RenderQuality, String, bool)> = RenderQuality::ALL
                    .iter()
                    .map(|&q| (q, quality_label(q).to_string(), true))
                    .collect();
                choice(ui, "pp-quality", list_w, &mut s.render_quality, &qualities);
                ui.end_row();

                // How the video bus's motion blur weighs its frames.
                ui.label(tr("Motion blur type:"));
                let names = [
                    tr("Gaussian"),
                    tr("Pyramid"),
                    tr("Box"),
                    tr("Gaussian (asymmetric)"),
                    tr("Pyramid (asymmetric)"),
                    tr("Box (asymmetric)"),
                ];
                let blurs: Vec<(bv_timeline::MotionBlurType, String, bool)> =
                    bv_timeline::MotionBlurType::ALL
                        .iter()
                        .zip(names)
                        .map(|(&k, t)| (k, t.to_string(), true))
                        .collect();
                choice(ui, "pp-motion-blur", list_w, &mut s.motion_blur, &blurs);
                ui.end_row();

                // How interlaced media becomes whole frames (Smart
                // adaptive, VEGAS's on the graphics card, still to come).
                ui.label(tr("Deinterlace method:"));
                use bv_timeline::DeinterlaceMethod as D;
                choice(
                    ui,
                    "pp-deinterlace",
                    list_w,
                    &mut s.deinterlace,
                    &[
                        (D::None, tr("None").into(), true),
                        (D::BlendFields, tr("Blend fields").into(), true),
                        (D::Interpolate, tr("Interpolate fields").into(), true),
                        (D::SmartAdaptive, tr("Smart adaptive (GPU only)").into(), true),
                    ],
                );
                ui.end_row();

                // How the frames a media lacks are made (an event's switch
                // may say otherwise).
                ui.label(tr("Resample mode:"));
                choice(
                    ui,
                    "pp-resample",
                    list_w,
                    &mut s.resample,
                    &[
                        (bv_timeline::ResampleMode::FrameBlend, tr("Frame Blend").into(), true),
                        (bv_timeline::ResampleMode::OpticalFlow, tr("Optical Flow").into(), true),
                        (bv_timeline::ResampleMode::Disable, tr("Disable resample").into(), true),
                    ],
                );
                ui.end_row();
            });
    }

    /// Source media adjustment, prerendering and the prerendered files
    /// folder.
    fn prerender(&mut self, ui: &mut Ui) {
        let soon = tr("Coming soon to BananaV");
        widgets::checkbox(
            ui,
            &mut self.draft.settings.adjust_source,
            tr("Adjust source media to better match project or render settings"),
        )
        .on_hover_text(tr(
            "Standard definition media a few percent off the frame's shape fill it instead of leaving thin bars (DV beside HD, 486 lines in 480).",
        ));
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                let mut off = false;
                widgets::checkbox(
                    ui,
                    &mut off,
                    tr("Override project settings when prerendering video"),
                )
            })
            .inner
            .on_disabled_hover_text(soon);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled(false, egui::Button::new(tr("Select…")))
                    .on_disabled_hover_text(soon);
            });
        });
        let s = self.edited().settings;
        let (w, h) = s.output_size();
        egui::Grid::new("pp-prerender-format")
            .num_columns(2)
            .spacing(vec2(24.0, 2.0))
            .show(ui, |ui| {
                let dim = |t: String| RichText::new(t).color(theme::text_dim());
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(dim(tr("Format:").into()));
                });
                ui.label(dim("AVC (H.264)".into()));
                ui.end_row();
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(dim(tr("Template:").into()));
                });
                ui.label(dim(trf!(
                    "Internet {}x{} {} fps progressive",
                    w,
                    h,
                    crate::fmt::num(s.frame_rate.as_f64(), 3)
                )));
                ui.end_row();
            });
        folder_box(
            ui,
            tr("Prerendered files folder:"),
            &mut self.folder,
            &mut self.free,
        );
    }

    /// The Audio tab: master bus, busses, sample rate, bit depth, resample
    /// quality, the LFE filter of 5.1 projects and the recorded files folder.
    fn audio(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let s = &mut self.draft.settings;
        audio_row(ui, tr("Master bus mode:"), 0.0, true, |ui| {
            let mut modes = vec![
                (ChannelLayout::Stereo, tr("Stereo").to_string(), true),
                (
                    ChannelLayout::Surround51,
                    tr("5.1 Surround").to_string(),
                    true,
                ),
            ];
            // Older BananaV projects may be mono.
            if s.channels == ChannelLayout::Mono {
                modes.insert(0, (ChannelLayout::Mono, "Mono".to_string(), true));
            }
            choice(ui, "pp-master-bus", 130.0, &mut s.channels, &modes);
        });
        audio_row(ui, tr("Number of stereo busses:"), 0.0, true, |ui| {
            let mut busses = self.draft.busses as f64;
            widgets::spin_box(ui, &mut busses, 0.0..=26.0, 1.0, 0, SMALL_LIST);
            self.draft.busses = busses.round().clamp(0.0, 26.0) as usize;
        });
        audio_row(ui, tr("Sample rate (Hz):"), 0.0, true, |ui| {
            let current = s.sample_rate;
            let (r, picked) = widgets::editable_combo(
                ui,
                egui::Id::new("pp-sample-rate"),
                &mut self.sample_rate,
                SMALL_LIST + 20.0,
                |ui| {
                    let mut pick = None;
                    for hz in SAMPLE_RATES {
                        let text = crate::fmt::grouped(hz as u64);
                        if ui.selectable_label(current.0 == hz, text).clicked() {
                            pick = Some(SampleRate(hz));
                        }
                    }
                    pick
                },
            );
            if let Some(rate) = picked {
                s.sample_rate = rate;
                self.sample_rate = crate::fmt::grouped(rate.0 as u64);
            } else if r.lost_focus() {
                if let Some(rate) = parse_sample_rate(&self.sample_rate) {
                    s.sample_rate = rate;
                }
                self.sample_rate = crate::fmt::grouped(s.sample_rate.0 as u64);
            }
        });
        audio_row(ui, tr("Bit depth:"), 0.0, true, |ui| {
            let depths: Vec<(u8, String, bool)> = [8u8, 16, 24]
                .iter()
                .map(|&b| (b, b.to_string(), true))
                .collect();
            choice(ui, "pp-bit-depth", SMALL_LIST, &mut s.bit_depth, &depths);
        });
        audio_row(ui, tr("Resample and stretch quality:"), 0.0, true, |ui| {
            choice(
                ui,
                "pp-resample-quality",
                SMALL_LIST,
                &mut s.resample_quality,
                &audio_qualities(),
            );
        });
        ui.add_space(4.0);
        // The LFE filter is for 5.1 projects only.
        let surround = s.channels == ChannelLayout::Surround51;
        ui.add_enabled_ui(surround, |ui| {
            widgets::checkbox(
                ui,
                &mut s.lfe_lowpass,
                tr("Enable low-pass filter on LFE (surround projects only)"),
            );
        });
        let lfe = surround && s.lfe_lowpass;
        audio_row(
            ui,
            tr("Cutoff frequency for low-pass filter (Hz):"),
            16.0,
            lfe,
            |ui| {
                let current = s.lfe_cutoff_hz;
                // As wide as the row leaves, up to VEGAS's width.
                let width = (ui.available_width() - 4.0).clamp(120.0, 180.0);
                let (r, picked) = widgets::editable_combo(
                    ui,
                    egui::Id::new("pp-lfe-cutoff"),
                    &mut self.cutoff,
                    width,
                    |ui| {
                        let mut pick = None;
                        for (hz, _) in LFE_CUTOFFS {
                            if ui
                                .selectable_label(current == hz, cutoff_label(hz))
                                .clicked()
                            {
                                pick = Some(hz);
                            }
                        }
                        pick
                    },
                );
                if let Some(hz) = picked {
                    s.lfe_cutoff_hz = hz;
                    self.cutoff = cutoff_label(hz);
                } else if r.lost_focus() {
                    if let Some(hz) = parse_cutoff(&self.cutoff) {
                        s.lfe_cutoff_hz = hz;
                    }
                    self.cutoff = cutoff_label(s.lfe_cutoff_hz);
                }
            },
        );
        audio_row(ui, tr("Low-pass filter quality:"), 16.0, lfe, |ui| {
            choice(
                ui,
                "pp-lfe-quality",
                SMALL_LIST,
                &mut s.lfe_quality,
                &audio_qualities(),
            );
        });
        ui.add_space(4.0);
        folder_box(
            ui,
            tr("Recorded files folder:"),
            &mut self.recorded,
            &mut self.recorded_free,
        );
    }
}

impl ProjectPropertiesDialog {
    /// The Ruler tab: ruler format and start time, and the tempo of the
    /// Measures & Beats ruler.
    fn ruler(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let rate = self.draft.settings.frame_rate;
        egui::Grid::new("pp-ruler")
            .num_columns(2)
            .spacing(vec2(10.0, 6.0))
            .show(ui, |ui| {
                let s = &mut self.draft.settings;
                ui.label(tr("Ruler time format:"));
                let mut format = s.effective_ruler_format();
                let formats: Vec<(RulerFormat, String, bool)> = RulerFormat::ALL
                    .iter()
                    .map(|&f| (f, f.dialog_label().to_string(), true))
                    .collect();
                let width = (ui.available_width() - 2.0).max(200.0);
                choice(ui, "pp-ruler-format", width, &mut format, &formats);
                if format != s.effective_ruler_format() {
                    s.ruler_format = Some(format);
                }
                ui.end_row();
                ui.label(tr("Ruler start time:"));
                let h = ui.spacing().interact_size.y;
                let r = ui.add_sized(
                    vec2(100.0, h),
                    egui::TextEdit::singleline(&mut self.ruler_start),
                );
                if r.lost_focus()
                    && let Some(t) = timecode::parse(&self.ruler_start, rate)
                {
                    self.draft.ruler_offset = t;
                }
                if !r.has_focus() {
                    self.ruler_start = timecode::format(self.draft.ruler_offset, rate);
                }
                ui.end_row();
            });
        ui.add_space(4.0);
        group_title(ui, tr("Measures & Beats"));
        let tempo = &mut self.draft.settings.tempo;
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            egui::Grid::new("pp-tempo")
                .num_columns(2)
                .spacing(vec2(16.0, 6.0))
                .show(ui, |ui| {
                    ui.label(tr("Beats per minute:"));
                    widgets::spin_box(
                        ui,
                        &mut tempo.beats_per_minute,
                        1.0..=999.0,
                        1.0,
                        3,
                        SMALL_LIST,
                    );
                    ui.end_row();
                    ui.label(tr("Beats per measure:"));
                    let mut per = tempo.beats_per_measure as f64;
                    widgets::spin_box(ui, &mut per, 1.0..=99.0, 1.0, 0, SMALL_LIST);
                    tempo.beats_per_measure = per.round().clamp(1.0, 99.0) as u32;
                    ui.end_row();
                    ui.label(tr("Note that gets one beat:"));
                    let notes: Vec<(u32, String, bool)> = [
                        (1, tr("Whole")),
                        (2, tr("Half")),
                        (4, tr("Quarter")),
                        (8, tr("8th")),
                        (16, tr("16th")),
                        (32, tr("32nd")),
                    ]
                    .iter()
                    .map(|&(n, label)| (n, label.to_string(), true))
                    .collect();
                    choice(ui, "pp-beat-note", SMALL_LIST, &mut tempo.beat_note, &notes);
                    ui.end_row();
                });
        });
    }

    /// The Summary tab: title, artist, engineer, copyright and comments
    /// (rendered WAV files carry them).
    fn summary(&mut self, ui: &mut Ui) {
        let sum = &mut self.draft.summary;
        egui::Grid::new("pp-summary")
            .num_columns(2)
            .spacing(vec2(10.0, 6.0))
            .show(ui, |ui| {
                let h = ui.spacing().interact_size.y;
                for (label, text) in [
                    (tr("Title:"), &mut sum.title),
                    (tr("Artist:"), &mut sum.artist),
                    (tr("Engineer:"), &mut sum.engineer),
                    (tr("Copyright:"), &mut sum.copyright),
                ] {
                    ui.label(label);
                    ui.add_sized(
                        vec2(ui.available_width(), h),
                        egui::TextEdit::singleline(text),
                    );
                    ui.end_row();
                }
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.label(tr("Comments:"));
                });
                ui.add_sized(
                    vec2(ui.available_width(), 170.0),
                    egui::TextEdit::multiline(&mut sum.comments),
                );
                ui.end_row();
            });
    }
}

impl ProjectPropertiesDialog {
    /// The Audio CD tab: the disc's catalog number and first track number.
    fn audio_cd(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let cd = &mut self.draft.audio_cd;
        audio_row(
            ui,
            tr("Universal Product Code / Media Catalog Number:"),
            0.0,
            true,
            |ui| {
                let h = ui.spacing().interact_size.y;
                ui.add_sized(
                    vec2(120.0, h),
                    egui::TextEdit::singleline(&mut cd.catalog_number).char_limit(13),
                );
            },
        );
        // Digits only.
        cd.catalog_number.retain(|c| c.is_ascii_digit());
        if (1..13).contains(&cd.catalog_number.len()) {
            crate::tabs::hint(ui, tr("A catalog number has 13 digits."));
        }
        audio_row(ui, tr("First track number on disc:"), 0.0, true, |ui| {
            let mut first = cd.first_track as f64;
            widgets::spin_box(ui, &mut first, 1.0..=99.0, 1.0, 0, 66.0);
            cd.first_track = first.round().clamp(1.0, 99.0) as u32;
        });
        if cd.first_track != 1 {
            crate::tabs::hint(
                ui,
                tr("Some CD players may not play a disc that does not start at track 1."),
            );
        }
    }
}

/// The Advanced tab: the master display of HDR projects (still to come)
/// and 360 Output: the frames are the whole sphere around the camera,
/// equirectangular (the Video Preview's 360° View looks around in them,
/// renders are marked as 360° video).
fn advanced(ui: &mut Ui, s: &mut ProjectSettings) {
    let soon = tr("Coming soon to BananaV");
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.label(tr("Master Display:"));
    ui.horizontal(|ui| {
        // As wide as its text needs (longer in Portuguese).
        let font = egui::TextStyle::Button.resolve(ui.style());
        let text_w = ui
            .painter()
            .layout_no_wrap(tr("Customize…").to_string(), font, theme::text())
            .size()
            .x;
        let button_w = (text_w + 2.0 * ui.spacing().button_padding.x).max(74.0);
        let width = ui.available_width() - button_w - 8.0;
        fixed_choice(
            ui,
            width,
            "pp-master-display",
            "Rec.2020,1000 Nits,D65,ST.2084,Full",
            soon,
        );
        ui.add_enabled(
            false,
            egui::Button::new(tr("Customize…")).min_size(vec2(button_w, 0.0)),
        )
        .on_disabled_hover_text(soon);
    });
    ui.add_space(18.0);
    ui.separator();
    widgets::checkbox(ui, &mut s.output_360, tr("360 Output"));
}

/// A group title with a line running to the right edge, like VEGAS's
/// group boxes ("Measures & Beats").
fn group_title(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        let r = ui.label(text);
        let y = r.rect.center().y;
        let x0 = r.rect.right() + 8.0;
        let x1 = ui.max_rect().right();
        if x1 > x0 {
            ui.painter().line_segment(
                [egui::pos2(x0, y), egui::pos2(x1, y)],
                Stroke::new(1.0, theme::separator()),
            );
        }
    });
}

/// One row of the Audio tab: the label on the left (indented by `indent`),
/// the control on the right edge; both greyed out when not `enabled`.
fn audio_row(ui: &mut Ui, label: &str, indent: f32, enabled: bool, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.add_enabled(enabled, egui::Label::new(label));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(enabled, add);
        });
    });
}

fn audio_qualities() -> Vec<(AudioQuality, String, bool)> {
    AudioQuality::ALL
        .iter()
        .map(|&q| {
            let label = match q {
                AudioQuality::Preview => tr("Preview"),
                AudioQuality::Good => tr("Good"),
                AudioQuality::Best => tr("Best"),
            };
            (q, label.to_string(), true)
        })
        .collect()
}

/// A width or height typed in its box: even, from 16 to 8192.
fn parse_size(text: &str) -> Option<u32> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    let v: u32 = digits.parse().ok()?;
    Some(v.clamp(16, 8192) & !1)
}

fn rotation_label(r: OutputRotation) -> &'static str {
    match r {
        OutputRotation::None => tr("0° (original)"),
        OutputRotation::Clockwise90 => tr("90° clockwise"),
        OutputRotation::Inverted180 => tr("180° (inverted)"),
        OutputRotation::Counterclockwise90 => tr("90° counterclockwise"),
    }
}

/// A pixel format picked from the list: 32-bit floating point (full range)
/// composites in linear light, VEGAS's default for it ("1.000 (Linear)":
/// the 2.222 gamma would look as 8-bit does), and shows through ACES's
/// sRGB view when none was chosen, as VEGAS then does (its preview's
/// "sRGB (ACES)").
fn picked_pixel_format(s: &mut ProjectSettings) {
    if s.pixel_format == PixelFormat::Float32Full {
        s.compositing_gamma = CompositingGamma::Linear;
        if s.color.view.is_none() {
            s.color.view = Some(ViewTransform {
                display: AcesDisplay::Srgb,
                view: AcesView::Aces,
            });
        }
    }
}

fn pixel_format_label(f: PixelFormat) -> &'static str {
    match f {
        PixelFormat::Legacy8BitVideoLevels => tr("Legacy 8-bit (video levels)"),
        PixelFormat::Full8Bit => tr("8-bit (full range)"),
        PixelFormat::Float32VideoLevels => tr("32-bit floating point (video levels)"),
        PixelFormat::Float32Full => tr("32-bit floating point (full range)"),
    }
}

fn quality_label(q: RenderQuality) -> &'static str {
    match q {
        RenderQuality::Draft => tr("Draft"),
        RenderQuality::Preview => tr("Preview"),
        RenderQuality::Good => tr("Good"),
        RenderQuality::Best => tr("Best"),
    }
}

/// VEGAS's pixel aspect ratios, and `current` when it is another (taken
/// from a media file): the list and the entry standing for `current`.
pub(crate) fn pixel_aspects(
    current: PixelAspect,
) -> (Vec<(PixelAspect, String, bool)>, PixelAspect) {
    let mut ratios: Vec<(PixelAspect, String, bool)> = [
        (PixelAspect::SQUARE, tr("Square")),
        (PixelAspect::new(4, 3), "HDV 1080"),
        (PixelAspect::new(10, 11), "NTSC DV"),
        (PixelAspect::new(40, 33), tr("NTSC DV Widescreen")),
        (PixelAspect::new(59, 54), "PAL DV"),
        (PixelAspect::new(118, 81), tr("PAL DV Widescreen")),
    ]
    .iter()
    .map(|&(a, name)| {
        (
            a,
            format!("{} ({name})", crate::fmt::num(a.ratio() as f64, 4)),
            true,
        )
    })
    .collect();
    if !ratios.iter().any(|r| r.0.ratio() == current.ratio()) {
        ratios.push((current, crate::fmt::num(current.ratio() as f64, 4), true));
    }
    let par = ratios
        .iter()
        .find(|r| r.0.ratio() == current.ratio())
        .map_or(current, |r| r.0);
    (ratios, par)
}

/// The field orders' list (Project Properties, Media Properties).
pub(crate) fn field_orders() -> [(bv_core::FieldOrder, String, bool); 3] {
    [
        (
            bv_core::FieldOrder::Progressive,
            tr("None (progressive scan)").into(),
            true,
        ),
        (
            bv_core::FieldOrder::UpperFirst,
            tr("Upper field first").into(),
            true,
        ),
        (
            bv_core::FieldOrder::LowerFirst,
            tr("Lower field first").into(),
            true,
        ),
    ]
}

/// A drop-down list of `options` (value, text, available); the ones BananaV
/// cannot do yet are greyed out with the "Coming soon" tip.
pub(crate) fn choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    width: f32,
    value: &mut T,
    options: &[(T, String, bool)],
) {
    // The value's own entry, else a greyed one standing for it.
    let selected = options
        .iter()
        .find(|o| o.2 && o.0 == *value)
        .or_else(|| options.iter().find(|o| o.0 == *value))
        .map_or(String::new(), |o| o.1.clone());
    egui::ComboBox::from_id_salt(id)
        .width(width)
        // Long lists (the view transforms) open whole where they fit.
        .height(640.0)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            // As wide as its longest entry: VEGAS's lists widen past their
            // box (ACES 1.3's view names).
            let font = egui::TextStyle::Button.resolve(ui.style());
            let widest = options
                .iter()
                .map(|o| {
                    ui.painter()
                        .layout_no_wrap(o.1.clone(), font.clone(), theme::text())
                        .size()
                        .x
                })
                .fold(0.0, f32::max);
            ui.set_min_width(widest + 2.0 * ui.spacing().button_padding.x);
            for (v, text, ready) in options {
                if *ready {
                    ui.selectable_value(value, *v, text);
                } else {
                    widgets::soon_item(ui, text);
                }
            }
        });
}

/// The name of an ACES working space in the list.
fn aces_space_label(space: AcesSpace) -> String {
    match space {
        AcesSpace::Aces2065_1 => tr("Default (ACES2065-1)").to_string(),
        AcesSpace::AcesCg => "ACEScg".to_string(),
    }
}

/// ACES 1.2's views as VEGAS lists them (its configuration's, but Raw),
/// with the display and view of BananaV's configuration that draws each
/// (`None`: still to come — the HDR outputs, the gamut-limited and D60
/// simulation ones, DCDM, P3-D60 and Log).
const VIEWS_1_2: [(&str, Option<(AcesDisplay, AcesView)>); 25] = [
    ("sRGB", Some((AcesDisplay::Srgb, AcesView::Aces))),
    ("DCDM", None),
    ("DCDM P3D60 Limited", None),
    ("DCDM P3D65 Limited", None),
    ("P3-D60", None),
    ("P3-D65 ST2084 1000 nits", None),
    ("P3-D65 ST2084 2000 nits", None),
    ("P3-D65 ST2084 4000 nits", None),
    ("P3-DCI D60 simulation", None),
    ("P3-DCI D65 simulation", None),
    ("P3D65", Some((AcesDisplay::P3D65, AcesView::Aces))),
    ("P3D65 D60 simulation", None),
    ("P3D65 Rec.709 Limited", None),
    ("P3D65 ST2084 108 nits", None),
    ("Rec.2020", Some((AcesDisplay::Rec2020, AcesView::Aces))),
    ("Rec.2020 P3D65 Limited", None),
    ("Rec.2020 Rec.709 Limited", None),
    ("Rec.2020 HLG 1000 nits", None),
    ("Rec.2020 ST2084 1000 nits", None),
    ("Rec.2020 ST2084 2000 nits", None),
    ("Rec.2020 ST2084 4000 nits", None),
    ("Rec.709", Some((AcesDisplay::Rec709, AcesView::Aces))),
    ("Rec.709 D60 sim.", None),
    ("sRGB D60 sim.", None),
    ("Log", None),
];

/// ACES 1.3's views as VEGAS lists them (OpenColorIO's ACES studio
/// configuration's displays and views, "ACES 1.x - " left out), with the
/// display and view of BananaV's configuration that draws each (`None`:
/// still to come — the HDR outputs, the gamut-limited and the D60 or D65
/// simulation ones, P3-D60's).
const VIEWS_1_3: [(&str, Option<(AcesDisplay, AcesView)>); 43] = {
    use AcesDisplay::*;
    use AcesView::*;
    [
        ("sRGB: SDR Video", Some((Srgb, Aces))),
        ("sRGB: SDR Video (D60 sim on D65)", None),
        ("sRGB: Un-tone-mapped", Some((Srgb, UnToneMapped))),
        ("sRGB: Raw", Some((Srgb, Raw))),
        ("Display P3: SDR Video", Some((DisplayP3, Aces))),
        ("Display P3: SDR Video (D60 sim on D65)", None),
        (
            "Display P3: Un-tone-mapped",
            Some((DisplayP3, UnToneMapped)),
        ),
        ("Display P3: Raw", Some((DisplayP3, Raw))),
        ("Rec.1886 Rec.709: SDR Video", Some((Rec709, Aces))),
        ("Rec.1886 Rec.709: SDR Video (D60 sim on D65)", None),
        (
            "Rec.1886 Rec.709: Un-tone-mapped",
            Some((Rec709, UnToneMapped)),
        ),
        ("Rec.1886 Rec.709: Raw", Some((Rec709, Raw))),
        ("Rec.1886 Rec.2020: SDR Video", Some((Rec2020, Aces))),
        ("Rec.1886 Rec.2020: SDR Video (P3 lim)", None),
        ("Rec.1886 Rec.2020: SDR Video (Rec.709 lim)", None),
        (
            "Rec.1886 Rec.2020: Un-tone-mapped",
            Some((Rec2020, UnToneMapped)),
        ),
        ("Rec.1886 Rec.2020: Raw", Some((Rec2020, Raw))),
        ("Rec.2100-HLG: HDR Video (1000 nits & Rec.2020 lim)", None),
        ("Rec.2100-HLG: Un-tone-mapped", None),
        ("Rec.2100-HLG: Raw", None),
        ("Rec.2100-PQ: HDR Video (1000 nits & Rec.2020 lim)", None),
        ("Rec.2100-PQ: HDR Video (2000 nits & Rec.2020 lim)", None),
        ("Rec.2100-PQ: HDR Video (4000 nits & Rec.2020 lim)", None),
        ("Rec.2100-PQ: Un-tone-mapped", None),
        ("Rec.2100-PQ: Raw", None),
        ("ST2084-P3-D65: HDR Video (1000 nits & P3 lim)", None),
        ("ST2084-P3-D65: HDR Video (2000 nits & P3 lim)", None),
        ("ST2084-P3-D65: HDR Video (4000 nits & P3 lim)", None),
        ("ST2084-P3-D65: HDR Cinema (108 nits & P3 lim)", None),
        ("ST2084-P3-D65: Un-tone-mapped", None),
        ("ST2084-P3-D65: Raw", None),
        ("P3-D60: SDR Cinema", None),
        ("P3-D60: Un-tone-mapped", None),
        ("P3-D60: Raw", None),
        ("P3-D65: SDR Cinema", Some((P3D65, Aces))),
        ("P3-D65: SDR Cinema (Rec.709 lim)", None),
        ("P3-D65: SDR Cinema (D60 sim on D65)", None),
        ("P3-D65: Un-tone-mapped", Some((P3D65, UnToneMapped))),
        ("P3-D65: Raw", Some((P3D65, Raw))),
        ("P3-DCI: SDR Cinema (D60 sim on DCI)", None),
        ("P3-DCI: SDR Cinema (D65 sim on DCI)", None),
        ("P3-DCI: Un-tone-mapped", Some((P3Dci, UnToneMapped))),
        ("P3-DCI: Raw", Some((P3Dci, Raw))),
    ]
};

/// The version's list of views, as VEGAS lists them.
fn views_of(version: AcesVersion) -> &'static [(&'static str, Option<(AcesDisplay, AcesView)>)] {
    match version {
        AcesVersion::V1_2 => &VIEWS_1_2,
        AcesVersion::V1_3 => &VIEWS_1_3,
    }
}

/// The View transform list: Off, the monitor's own (to come), then the
/// version's views as VEGAS lists them; a view the project has that the
/// list lacks (chosen under the other version) last.
fn view_list(
    version: AcesVersion,
    current: Option<ViewTransform>,
) -> Vec<(Option<ViewTransform>, String, bool)> {
    let mut views = vec![
        (None, view_label(version, None), true),
        (None, tr("Use Windows monitor data").to_string(), false),
    ];
    for &(name, drawn) in views_of(version) {
        let v = drawn.map(|(display, view)| ViewTransform { display, view });
        views.push((v, name.to_string(), v.is_some()));
    }
    if !views.iter().any(|(v, _, ready)| *ready && *v == current) {
        views.push((current, view_label(version, current), true));
    }
    views
}

/// A view transform as the version's list names it, else as 1.3's does;
/// BananaV's own P3-DCI view, which neither has, "P3-DCI: ACES".
pub(crate) fn view_label(version: AcesVersion, v: Option<ViewTransform>) -> String {
    let Some(v) = v else {
        return tr("Off").to_string();
    };
    let named = |list: &[(&str, Option<(AcesDisplay, AcesView)>)]| {
        list.iter()
            .find(|(_, drawn)| *drawn == Some((v.display, v.view)))
            .map(|(name, _)| name.to_string())
    };
    named(views_of(version))
        .or_else(|| named(&VIEWS_1_3))
        .unwrap_or_else(|| format!("{}: {}", v.display.name(), v.view.name()))
}

fn look_label(look: AcesLook) -> &'static str {
    match look {
        AcesLook::None => tr("None"),
        AcesLook::BlueLightFix => tr("Apply Blue Light Artifact Fix"),
        AcesLook::GamutCompression => "ACES 1.3 Reference Gamut Compression",
    }
}

/// The version's looks, as VEGAS lists them (1.3's seen on VEGAS 23), and
/// `current` last when the list lacks it (chosen under the other version).
fn look_list(version: AcesVersion, current: AcesLook) -> Vec<(AcesLook, String, bool)> {
    let looks: &[AcesLook] = match version {
        AcesVersion::V1_2 => &[AcesLook::None, AcesLook::BlueLightFix],
        AcesVersion::V1_3 => &[AcesLook::None, AcesLook::GamutCompression],
    };
    let mut list: Vec<(AcesLook, String, bool)> = looks
        .iter()
        .map(|&l| (l, look_label(l).to_string(), true))
        .collect();
    if !looks.contains(&current) {
        list.push((current, look_label(current).to_string(), true));
    }
    list
}

/// ACES's version, color space, view transform and look: with 32-bit
/// floating point (full range) only, the look with a view transform in
/// linear light, as VEGAS has them.
fn aces_rows(ui: &mut Ui, s: &mut ProjectSettings, list_w: f32) {
    let ready = s.pixel_format == PixelFormat::Float32Full;
    let needs = tr("ACES needs the 32-bit floating point (full range) pixel format.");
    ui.label(tr("ACES version:"));
    if ready {
        let versions: Vec<(AcesVersion, String, bool)> = AcesVersion::ALL
            .iter()
            .map(|&v| (v, v.name().to_string(), true))
            .collect();
        choice(
            ui,
            "pp-aces-version",
            list_w,
            &mut s.color.version,
            &versions,
        );
    } else {
        fixed_choice(ui, list_w, "pp-aces-version", s.color.version.name(), needs);
    }
    ui.end_row();

    ui.label(tr("ACES color space:"));
    if ready {
        // VEGAS's list; the log spaces are still to come.
        let spaces = vec![
            (
                AcesSpace::Aces2065_1,
                aces_space_label(AcesSpace::Aces2065_1),
                true,
            ),
            (AcesSpace::Aces2065_1, "ACEScc".to_string(), false),
            (AcesSpace::Aces2065_1, "ACEScct".to_string(), false),
            (AcesSpace::Aces2065_1, "ACESproxy".to_string(), false),
            (AcesSpace::AcesCg, aces_space_label(AcesSpace::AcesCg), true),
        ];
        choice(ui, "pp-aces-space", list_w, &mut s.color.space, &spaces);
    } else {
        fixed_choice(
            ui,
            list_w,
            "pp-aces-space",
            &aces_space_label(s.color.space),
            needs,
        );
    }
    ui.end_row();

    ui.label(tr("View transform:"));
    if ready {
        let views = view_list(s.color.version, s.color.view);
        choice(ui, "pp-view-transform", list_w, &mut s.color.view, &views);
    } else {
        fixed_choice(
            ui,
            list_w,
            "pp-view-transform",
            &view_label(s.color.version, s.color.view),
            needs,
        );
    }
    ui.end_row();

    ui.label(tr("Look modification transform:"));
    if s.look_applies() {
        let looks = look_list(s.color.version, s.color.look);
        choice(ui, "pp-lmt", list_w, &mut s.color.look, &looks);
    } else {
        fixed_choice(
            ui,
            list_w,
            "pp-lmt",
            look_label(s.color.look),
            tr(
                "The look applies with a view transform, in 32-bit floating point (full range) compositing in linear light.",
            ),
        );
    }
    ui.end_row();
}

/// A greyed-out drop-down list showing `text`, with `tip` on hover.
pub(crate) fn fixed_choice(ui: &mut Ui, width: f32, id: &str, text: &str, tip: &str) {
    ui.add_enabled_ui(false, |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(text)
            .show_ui(ui, |_| {})
            .response
    })
    .inner
    .on_disabled_hover_text(tip);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_frame_rates() {
        assert_eq!(parse_rate("29,970 (NTSC)"), Some(FrameRate::FPS_29_97));
        assert_eq!(parse_rate("29.97"), Some(FrameRate::FPS_29_97));
        assert_eq!(parse_rate("23,976"), Some(FrameRate::FPS_23_976));
        assert_eq!(parse_rate("59.94"), Some(FrameRate::FPS_59_94));
        assert_eq!(parse_rate("30"), Some(FrameRate::FPS_30));
        assert_eq!(parse_rate("12,5"), Some(FrameRate::new(25, 2)));
        assert_eq!(parse_rate("14.985"), Some(FrameRate::new(15000, 1001)));
        assert_eq!(parse_rate("abc"), None);
        assert_eq!(parse_rate("0"), None);
    }

    #[test]
    fn rate_labels_round_trip() {
        for (r, _) in RATES {
            assert_eq!(parse_rate(&rate_label(r)), Some(r), "{}", rate_label(r));
        }
    }

    #[test]
    fn typed_audio_values() {
        assert_eq!(parse_sample_rate("48.000"), Some(SampleRate(48_000)));
        assert_eq!(parse_sample_rate("44,100"), Some(SampleRate(44_100)));
        assert_eq!(parse_sample_rate("96000"), Some(SampleRate(96_000)));
        assert_eq!(parse_sample_rate("44,1"), None);
        assert_eq!(parse_sample_rate("999999"), None);
        assert_eq!(parse_cutoff("120 (pro/film)"), Some(120));
        assert_eq!(parse_cutoff("95"), Some(95));
        assert_eq!(parse_cutoff("x"), None);
        for (hz, _) in LFE_CUTOFFS {
            assert_eq!(parse_cutoff(&cutoff_label(hz)), Some(hz));
        }
        for hz in SAMPLE_RATES {
            let text = crate::fmt::grouped(hz as u64);
            assert_eq!(parse_sample_rate(&text), Some(SampleRate(hz)), "{text}");
        }
    }

    #[test]
    fn typed_sizes_are_even_and_in_range() {
        assert_eq!(parse_size("1.920"), Some(1920));
        assert_eq!(parse_size("1081"), Some(1080));
        assert_eq!(parse_size("4"), Some(16));
        assert_eq!(parse_size("99999"), Some(8192));
        assert_eq!(parse_size(""), None);
    }

    #[test]
    fn picking_32_bit_full_range_composites_in_linear_light() {
        let mut s = ProjectSettings {
            pixel_format: PixelFormat::Float32Full,
            compositing_gamma: CompositingGamma::Video,
            ..ProjectSettings::default()
        };
        picked_pixel_format(&mut s);
        assert_eq!(s.compositing_gamma, CompositingGamma::Linear);
        // Through ACES's sRGB view, as VEGAS's then, unless one was chosen.
        let srgb = ViewTransform {
            display: AcesDisplay::Srgb,
            view: AcesView::Aces,
        };
        assert_eq!(s.color.view, Some(srgb));
        let rec709 = ViewTransform {
            display: AcesDisplay::Rec709,
            view: AcesView::Aces,
        };
        s.color.view = Some(rec709);
        picked_pixel_format(&mut s);
        assert_eq!(s.color.view, Some(rec709), "the one chosen stays");
        // The others leave it (they composite in video gamma anyway).
        s.pixel_format = PixelFormat::Full8Bit;
        s.compositing_gamma = CompositingGamma::Video;
        picked_pixel_format(&mut s);
        assert_eq!(s.compositing_gamma, CompositingGamma::Video);
    }

    #[test]
    fn apply_edits_the_project_and_can_start_new_projects() {
        let mut app = BananaApp::new(egui::Context::default());
        let mut d = ProjectPropertiesDialog::new(&app);
        assert!(!d.changed(&app));
        d.width = "1281".into();
        d.rate = "25".into();
        d.draft.settings.rotation = OutputRotation::Clockwise90;
        d.draft.settings.pixel_format = PixelFormat::Full8Bit;
        d.folder = "D:/Prerender".into();
        assert!(d.changed(&app));
        d.apply(&mut app);
        let s = app.project().settings;
        assert_eq!((s.width, s.frame_rate), (1280, FrameRate::FPS_25));
        assert_eq!(s.rotation, OutputRotation::Clockwise90);
        assert_eq!(s.pixel_format, PixelFormat::Full8Bit);
        assert_eq!(app.project().prerender_folder, "D:/Prerender");
        assert!(!d.changed(&app), "applied: nothing left to apply");
        assert_eq!(d.template, tr("(Custom)"));
        // One undoable edit.
        app.undo();
        assert_eq!(app.project().settings, ProjectSettings::default());
        // The default folder is stored as empty.
        let d = ProjectPropertiesDialog::new(&app);
        assert_eq!(PathBuf::from(&d.folder), default_prerender_folder());
        assert_eq!(d.edited().prerender_folder, "");
        // The Audio tab.
        let mut d = ProjectPropertiesDialog::new(&app);
        d.sample_rate = "96.000".into();
        d.draft.settings.bit_depth = 24;
        d.draft.settings.resample_quality = AudioQuality::Best;
        d.recorded = "D:/Recordings".into();
        d.apply(&mut app);
        let s = app.project().settings;
        assert_eq!((s.sample_rate, s.bit_depth), (SampleRate(96_000), 24));
        assert_eq!(s.resample_quality, AudioQuality::Best);
        assert_eq!(app.project().recorded_folder, "D:/Recordings");
        app.undo();
        // The Audio CD tab (not part of "Start all new projects").
        let mut d = ProjectPropertiesDialog::new(&app);
        d.draft.audio_cd.catalog_number = "0123456789012".into();
        d.draft.audio_cd.first_track = 3;
        d.apply(&mut app);
        assert_eq!(app.project().audio_cd.catalog_number, "0123456789012");
        assert_eq!(app.project().audio_cd.first_track, 3);
        app.undo();
        // The Ruler and Summary tabs.
        let mut d = ProjectPropertiesDialog::new(&app);
        d.draft.settings.ruler_format = Some(RulerFormat::MeasuresBeats);
        d.draft.settings.tempo.beats_per_minute = 90.0;
        d.ruler_start = "01:00:00;00".into();
        d.draft.summary.title = "Demo".into();
        d.apply(&mut app);
        let p = app.project();
        assert_eq!(
            p.settings.effective_ruler_format(),
            RulerFormat::MeasuresBeats
        );
        assert_eq!(p.settings.tempo.beats_per_minute, 90.0);
        assert!((p.ruler_offset.as_seconds_f64() - 3600.0).abs() < 0.1);
        assert_eq!(p.summary.title, "Demo");
        app.undo();
        // "Start all new projects with these settings".
        let mut d = ProjectPropertiesDialog::new(&app);
        d.width = "1280".into();
        d.height = "720".into();
        d.start_new_projects = true;
        d.apply(&mut app);
        app.new_project();
        assert_eq!(
            (app.project().settings.width, app.project().settings.height),
            (1280, 720)
        );
    }

    #[test]
    fn template_names() {
        let s = ProjectSettings::default();
        assert_eq!(
            template_name(&s, &[]),
            template_label("HD 1080-30p", 1920, 1080, FrameRate::FPS_29_97)
        );
        let saved = vec![VideoTemplate {
            name: "Mine".into(),
            settings: s,
        }];
        assert_eq!(template_name(&s, &saved), "Mine");
        let odd = ProjectSettings { width: 1000, ..s };
        assert_eq!(template_name(&odd, &saved), tr("(Custom)"));
    }
}
