//! Windows of the Insert, Tools, Options and Help menus: Insert Time,
//! Slideshow, ruler time, the loudness log, Preferences and Help.

use crate::i18n::tr;
use std::path::PathBuf;

use bv_core::{Tick, timecode};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::theme;

#[derive(Debug, Default)]
pub struct MoreDialogs {
    insert_time: Option<(String, bool)>,
    slideshow: Option<Slideshow>,
    /// "Set Time at Cursor" (`false`) or "Set Project Time" (`true`).
    ruler_time: Option<(String, bool, bool)>,
    pub loudness: bool,
    /// The loudness log written when the measure finished.
    loudness_written: bool,
    /// Options › Preferences.
    pub preferences: Option<crate::preferences::PreferencesDialog>,
    /// Options › Internal… (the menu opened with Shift held).
    pub(crate) internal: Option<crate::internal_prefs::InternalDialog>,
    /// Help window: 0 = contents, 1 = keyboard shortcuts.
    pub help: Option<usize>,
    /// Options › Customize Keyboard.
    pub keyboard: Option<crate::keyboard_dialog::KeyboardDialog>,
    /// Options › Customize Timeline Toolbar.
    pub toolbar: Option<crate::customize_toolbar::ToolbarDialog>,
    /// Video Scope Settings (the Video Scopes' gear).
    pub scope_settings: Option<crate::tabs::scopes::SettingsDialog>,
    /// Insert › Insert Subtitles from File / from Regions.
    pub subtitles: Option<crate::subtitles::SubtitlesDialog>,
    /// Insert › Command…'s Command Properties (and a command marker's).
    pub command: Option<crate::command_ui::CommandDialog>,
    /// Create Subclip.
    pub subclip: Option<crate::subclip_ui::SubclipDialog>,
    /// Options › Envelope Appearance… (and an envelope's shortcut menu).
    pub envelope_look: Option<crate::envelope_look::EnvelopeLookDialog>,
    /// Help › Check for Software Updates…, and the check as BananaV starts.
    pub updates: Option<crate::updates::UpdateDialog>,
    /// Project Media's Rename: the media, its name being typed and where
    /// the box shows.
    pub media_rename: Option<(bv_timeline::MediaId, String, egui::Pos2)>,
    /// Detect Scenes looking through a media.
    pub media_scenes: Option<crate::media_menu::MediaScenes>,
    /// Tools › Burn Disc › DVD, and the job it started.
    pub burn_dvd: Option<crate::burn_dvd::BurnDvdDialog>,
    /// Tools › Burn Disc › Track-at-Once and Disc-at-Once Audio CD.
    pub burn_tao: Option<crate::burn_cd::BurnTaoDialog>,
    pub burn_dao: Option<crate::burn_cd::BurnDaoDialog>,
    pub burn_job: Option<crate::burn_job::BurnJob>,
}

#[derive(Debug)]
struct Slideshow {
    paths: Vec<PathBuf>,
    seconds: f32,
    overlap: f32,
    by_name: bool,
}

/// What File-independent options are exported by Options › Export
/// Preferences.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preferences {
    pub snapping: bool,
    pub quantize: bool,
    pub snap_grid: bool,
    pub snap_markers: bool,
    pub snap_events: bool,
    pub auto_ripple: bool,
    pub loop_enabled: bool,
    pub show_welcome: bool,
    pub layouts: Vec<Option<crate::settings::SavedLayout>>,
    /// Missing in files exported before the Language tab existed.
    #[serde(default)]
    pub language: Option<crate::i18n::Language>,
}

impl Preferences {
    pub fn of(app: &BananaApp) -> Preferences {
        Preferences {
            snapping: app.snapping,
            quantize: app.quantize,
            snap_grid: app.snap_grid,
            snap_markers: app.snap_markers,
            snap_events: app.snap_events,
            auto_ripple: app.auto_ripple,
            loop_enabled: app.loop_enabled,
            show_welcome: app.settings.show_welcome,
            layouts: app.settings.layouts.clone(),
            language: Some(app.settings.language),
        }
    }

    pub fn apply(self, app: &mut BananaApp) {
        app.snapping = self.snapping;
        app.quantize = self.quantize;
        app.snap_grid = self.snap_grid;
        app.snap_markers = self.snap_markers;
        app.snap_events = self.snap_events;
        app.auto_ripple = self.auto_ripple;
        app.loop_enabled = self.loop_enabled;
        app.settings.show_welcome = self.show_welcome;
        app.settings.layouts = self.layouts;
        app.settings.layouts.resize(10, None);
        if let Some(lang) = self.language {
            app.set_language(lang);
        }
        let _ = app.settings.save();
    }
}

pub fn export_preferences(app: &mut BananaApp) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Export Preferences"))
        .add_filter(tr("BananaV Preferences"), &["json"])
        .set_file_name(tr("BananaV Preferences.json"))
        .save_file()
    else {
        return;
    };
    let json = serde_json::to_string_pretty(&Preferences::of(app)).unwrap_or_default();
    match std::fs::write(&path, json) {
        Ok(()) => app.notify(trf!("Preferences exported to {}", path.display())),
        Err(e) => app.report_error(trf!("Could not export: {}", e)),
    }
}

pub fn import_preferences(app: &mut BananaApp) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Import Preferences"))
        .add_filter(tr("BananaV Preferences"), &["json"])
        .pick_file()
    else {
        return;
    };
    match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<Preferences>(&s).map_err(|e| e.to_string()))
    {
        Ok(p) => {
            p.apply(app);
            app.notify(tr("Preferences imported."));
        }
        Err(e) => app.report_error(trf!("Invalid preferences file: {}", e)),
    }
}

/// The loudness log's text: the measures, then the short-term loudness
/// along the time.
fn loudness_text(
    app: &BananaApp,
    r: &bv_engine::LoudnessReport,
    timeline: &[(bv_core::Tick, f64)],
) -> String {
    let rate = app.frame_rate();
    let mut text = trf!(
        "Loudness log (EBU R 128)\nIntegrated loudness: {:.1} LUFS\nLRA: {:.1} LU\nMax momentary: {:.1} LUFS\nMax short-term: {:.1} LUFS\nSample peak: {:.1} dBFS\n\nTime\tShort-term (LUFS)\n",
        r.integrated,
        r.range,
        r.max_momentary,
        r.max_short_term,
        r.sample_peak
    );
    for (t, l) in timeline {
        text.push_str(&format!("{}\t{:.1}\n", timecode::format(*t, rate), l));
    }
    text
}

impl MoreDialogs {
    pub fn open_insert_time(&mut self) {
        self.insert_time = Some(("00:00:05;00".into(), false));
    }

    pub fn open_slideshow(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title(tr("Slideshow: choose the images"))
            .add_filter(tr("Images"), &["png", "jpg", "jpeg", "bmp"])
            .pick_files()
        {
            self.slideshow = Some(Slideshow {
                paths,
                seconds: 3.0,
                overlap: 0.5,
                by_name: true,
            });
        }
    }

    pub fn open_ruler_time(&mut self, current: String, project: bool) {
        self.ruler_time = Some((current, project, false));
    }

    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        self.insert_time_window(ctx, app);
        self.slideshow_window(ctx, app);
        self.ruler_time_window(ctx, app);
        self.loudness_window(ctx, app);
        if let Some(mut p) = self.preferences.take()
            && p.show(ctx, app)
        {
            self.preferences = Some(p);
        }
        if let Some(mut d) = self.internal.take()
            && d.show(ctx, app)
        {
            self.internal = Some(d);
        }
        self.help_window(ctx, app);
        if let Some(mut k) = self.keyboard.take()
            && k.show(ctx, app)
        {
            self.keyboard = Some(k);
        }
        if let Some(mut t) = self.toolbar.take()
            && t.show(ctx, app)
        {
            self.toolbar = Some(t);
        }
        if let Some(mut s) = self.scope_settings.take()
            && s.show(ctx, app)
        {
            self.scope_settings = Some(s);
        }
        if let Some(mut s) = self.subtitles.take()
            && s.show(ctx, app)
        {
            self.subtitles = Some(s);
        }
        if let Some(mut c) = self.command.take()
            && c.show(ctx, app)
        {
            self.command = Some(c);
        }
        if let Some(mut d) = self.subclip.take()
            && d.show(ctx, app)
        {
            self.subclip = Some(d);
        }
        if let Some(mut d) = self.envelope_look.take()
            && d.show(ctx, app)
        {
            self.envelope_look = Some(d);
        }
        if let Some(mut d) = self.updates.take()
            && d.show(ctx, app)
        {
            self.updates = Some(d);
        }
        if let Some(mut r) = self.media_rename.take()
            && crate::media_menu::rename_box(ctx, app, &mut r)
        {
            self.media_rename = Some(r);
        }
        if let Some(mut s) = self.media_scenes.take()
            && s.show(ctx, app)
        {
            self.media_scenes = Some(s);
        }
        if let Some(mut d) = self.burn_dvd.take()
            && d.show(ctx, app)
        {
            self.burn_dvd = Some(d);
        }
        if let Some(mut d) = self.burn_tao.take()
            && d.show(ctx, app)
        {
            self.burn_tao = Some(d);
        }
        if let Some(mut d) = self.burn_dao.take()
            && d.show(ctx, app)
        {
            self.burn_dao = Some(d);
        }
        if let Some(mut j) = self.burn_job.take()
            && j.show(ctx, app)
        {
            self.burn_job = Some(j);
        }
    }

    fn insert_time_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((mut text, mut error)) = self.insert_time.take() else {
            return;
        };
        let (mut open, mut ok, mut cancel) = (true, false, false);
        crate::floating::Window::new(tr("Insert Time"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("Amount of time to insert at the cursor:"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(140.0),
                );
                ok |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if error {
                    ui.colored_label(theme::error(), tr("Use HH:MM:SS;FF or seconds."));
                }
                crate::tabs::hint(
                    ui,
                    tr("All tracks, markers and regions after the cursor move to the right."),
                );
                ui.horizontal(|ui| {
                    ok |= ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            match timecode::parse(&text, app.frame_rate()) {
                Some(t) if t > Tick::ZERO => {
                    app.insert_time(t);
                    return;
                }
                _ => error = true,
            }
        }
        if open && !cancel {
            self.insert_time = Some((text, error));
        }
    }

    fn slideshow_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some(mut s) = self.slideshow.take() else {
            return;
        };
        let (mut open, mut ok, mut cancel) = (true, false, false);
        crate::floating::Window::new(tr("Slideshow"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(trn!(
                    s.paths.len(),
                    "{} image selected.",
                    "{} images selected.",
                    s.paths.len()
                ));
                egui::Grid::new("slides").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Length of each image"));
                    ui.add(
                        crate::fmt::drag(&mut s.seconds, 1, " s")
                            .speed(0.1)
                            .range(0.2..=600.0),
                    );
                    ui.end_row();
                    ui.label(tr("Overlap (transition)"));
                    ui.add(
                        crate::fmt::drag(&mut s.overlap, 2, " s")
                            .speed(0.05)
                            .range(0.0..=10.0),
                    );
                    ui.end_row();
                });
                crate::widgets::checkbox(ui, &mut s.by_name, tr("Sort by file name"));
                ui.horizontal(|ui| {
                    ok = ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            let mut paths = s.paths.clone();
            if s.by_name {
                paths.sort();
            }
            let per = Tick::from_seconds_f64(s.seconds as f64);
            let overlap = Tick::from_seconds_f64(s.overlap.min(s.seconds * 0.9) as f64);
            app.insert_slideshow(&paths, per, overlap);
            return;
        }
        if open && !cancel {
            self.slideshow = Some(s);
        }
    }

    fn ruler_time_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((mut text, project, mut error)) = self.ruler_time.take() else {
            return;
        };
        let (mut open, mut ok, mut cancel) = (true, false, false);
        let title = if project {
            tr("Set Project Time")
        } else {
            tr("Set Time at Cursor")
        };
        crate::floating::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(if project {
                    tr("Ruler time at the project start:")
                } else {
                    tr("Time the ruler should show at the cursor:")
                });
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(140.0),
                );
                ok |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if error {
                    ui.colored_label(
                        theme::error(),
                        tr("Use the ruler's format, HH:MM:SS;FF or seconds."),
                    );
                }
                ui.horizontal(|ui| {
                    ok |= ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                    if ui.button(tr("Set to Zero")).clicked() {
                        text = "0".into();
                        ok = true;
                    }
                });
            });
        if ok {
            use crate::timefmt::RulerFormatExt;
            let typed = app
                .effective_ruler_format()
                .parse(&text, &app.project().settings)
                .or_else(|| timecode::parse(&text, app.frame_rate()));
            match typed {
                Some(t) => {
                    if project {
                        app.modify(|p| p.ruler_offset = t);
                    } else {
                        app.set_time_at_cursor(t);
                    }
                    return;
                }
                None => error = true,
            }
        }
        if open && !cancel {
            self.ruler_time = Some((text, project, error));
        }
    }

    fn loudness_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        if !self.loudness {
            return;
        }
        let progress = app.engine.loudness_progress();
        // Finished: the log goes to a file and, with Options › Preferences ›
        // Audio's "Open editor when new loudness log is generated", opens in
        // the text editor.
        if let Some(p) = progress.as_ref()
            && p.finished
            && let Some(r) = p.report
            && !self.loudness_written
        {
            self.loudness_written = true;
            let dir = app.settings.prefs.temp_dir().join("Loudness");
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let path = dir.join(format!("Loudness log {stamp}.txt"));
            let text = loudness_text(app, &r, &p.timeline);
            match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text)) {
                Ok(()) if app.settings.prefs.open_loudness_log => {
                    if let Err(e) = crate::app::more_actions::open_externally(&path) {
                        app.report_error(trf!("Could not open the editor: {}", e));
                    }
                }
                Ok(()) => {}
                Err(e) => app.report_save_error(&path, &e),
            }
        }
        let mut open = true;
        let mut save = false;
        crate::floating::Window::new(tr("Loudness Log"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let Some(p) = &progress else {
                    crate::tabs::hint(ui, tr("Nothing to measure."));
                    return;
                };
                if !p.finished {
                    ui.label(tr("Measuring the mix (EBU R 128)…"));
                    let f = p.done as f32 / p.total.max(1) as f32;
                    ui.add(
                        egui::ProgressBar::new(f)
                            .show_percentage()
                            .desired_width(320.0),
                    );
                    return;
                }
                let Some(r) = p.report else { return };
                let db = |v: f64| {
                    if v <= -199.0 {
                        "−∞".to_string()
                    } else {
                        crate::fmt::num(v, 1)
                    }
                };
                egui::Grid::new("sonoridade")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for (label, value) in [
                            (
                                tr("Integrated loudness"),
                                format!("{} LUFS", db(r.integrated)),
                            ),
                            (
                                tr("Loudness range (LRA)"),
                                format!("{} LU", crate::fmt::num(r.range, 1)),
                            ),
                            (tr("Max momentary"), format!("{} LUFS", db(r.max_momentary))),
                            (
                                tr("Max short-term"),
                                format!("{} LUFS", db(r.max_short_term)),
                            ),
                            (tr("Sample peak"), format!("{} dBFS", db(r.sample_peak))),
                            (
                                tr("Measured length"),
                                format!("{} s", crate::fmt::num(r.duration, 1)),
                            ),
                        ] {
                            ui.label(label);
                            ui.label(RichText::new(value).monospace().strong());
                            ui.end_row();
                        }
                    });
                let target = -23.0;
                let delta = r.integrated - target;
                crate::tabs::hint(
                    ui,
                    &trf!(
                        "EBU R 128 reference: −23 LUFS (difference {} LU).",
                        crate::fmt::signed(delta, 1)
                    ),
                );
                save = ui.button(tr("Save Log…")).clicked();
            });
        if save
            && let Some(p) = progress.as_ref()
            && let Some(r) = p.report
            && let Some(path) = rfd::FileDialog::new()
                .set_title(tr("Save Loudness Log"))
                .add_filter(tr("Text"), &["txt"])
                .set_file_name(tr("Loudness log.txt"))
                .save_file()
        {
            let text = loudness_text(app, &r, &p.timeline);
            match std::fs::write(&path, text) {
                Ok(()) => app.notify(trf!("Log saved to {}", path.display())),
                Err(e) => app.report_save_error(&path, &e),
            }
        }
        if !open {
            app.engine.cancel_loudness_scan();
            self.loudness_written = false;
        }
        self.loudness = open;
    }

    fn help_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some(mut page) = self.help else {
            return;
        };
        let mut open = true;
        crate::floating::Window::new(tr("BananaV Help"))
            .default_size([620.0, 520.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut page, 0, tr("Contents"));
                    ui.selectable_value(&mut page, 1, tr("Keyboard Shortcuts"));
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if page == 0 {
                            for (title, text) in HELP {
                                ui.label(RichText::new(tr(title)).strong().color(theme::banana()));
                                ui.label(tr(text));
                                ui.add_space(6.0);
                            }
                        } else {
                            egui::Grid::new("atalhos")
                                .striped(true)
                                .num_columns(2)
                                .show(ui, |ui| {
                                    for (label, keys) in crate::commands::shortcut_table(app) {
                                        ui.label(label);
                                        ui.label(RichText::new(keys).monospace());
                                        ui.end_row();
                                    }
                                });
                        }
                    });
            });
        self.help = open.then_some(page);
    }
}

const HELP: &[(&str, &str)] = &[
    (
        "Getting Started",
        "Import media (File › Import › Media, or drag from the Explorer) and drop it on the timeline. The first video matches the project properties to the media.",
    ),
    (
        "Editing Events",
        "Drag events to move them; drag the edges to trim; S splits at the cursor. The top corners adjust fades and the horizontal line adjusts the level. Ctrl+Z undoes.",
    ),
    (
        "Pan/Crop and Effects",
        "The buttons in the event's corner open Pan/Crop, the effects chain and the extras (⋯).",
    ),
    (
        "Envelopes",
        "Insert › Audio Envelopes creates volume (Shift+V), pan (Shift+P) and mute envelopes on the track. Double-click the line to add a point; drag the points; right-click changes the curve or deletes it.",
    ),
    (
        "Time Selection",
        "Drag on the bar above the ruler or use I and O. Q turns looping on. Ctrl+T trims the selected events to the selection.",
    ),
    (
        "Rendering",
        "File › Render exports MP4 (H.264/HEVC with AAC), WAV or a PNG sequence. Tools › Generate Loudness Log measures the loudness (EBU R 128).",
    ),
];
