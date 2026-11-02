//! The render progress window, like VEGAS's: the file, a thin progress bar,
//! the share of the estimated size and the time left, then Estimated
//! Properties, Performance and Options (each can be folded), and Open
//! Folder, Open and Cancel. Cancel asks first ("Cancel Warning"): a canceled
//! render leaves nothing behind. The main window's title, the status bar
//! and the taskbar button show the progress too.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::app::BananaApp;
use crate::egui::{
    self, Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use crate::i18n::{Language, tr};
use crate::theme;

/// "00:17:30.00": hours, minutes, seconds and hundredths.
pub(crate) fn clock(seconds: f64) -> String {
    let cs = (seconds.max(0.0) * 100.0).round() as u64;
    let s = cs / 100;
    format!(
        "{:02}:{:02}:{:02}{}{:02}",
        s / 3600,
        s / 60 % 60,
        s % 60,
        crate::i18n::language().decimal_separator(),
        cs % 100
    )
}

/// "4:27:44 AM" in English, "04:27:44" in Portuguese.
fn time_of_day(seconds: u64, lang: Language) -> String {
    let s = seconds % 86_400;
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    match lang {
        Language::English => {
            let (h12, half) = match h {
                0 => (12, "AM"),
                1..=11 => (h, "AM"),
                12 => (12, "PM"),
                _ => (h - 12, "PM"),
            };
            format!("{h12}:{m:02}:{sec:02} {half}")
        }
        _ => format!("{h:02}:{m:02}:{sec:02}"),
    }
}

/// "92 GB": the free space, whole.
fn size0(bytes: f64) -> String {
    let gb = bytes / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 {
        format!("{} GB", crate::fmt::num(gb.floor(), 0))
    } else {
        crate::fmt::bytes(bytes)
    }
}

fn open_with_system(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

fn show_in_folder(path: &Path) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
}

/// What the file is, for the window's title ("Writing MP4 File").
fn writing_title(file: &Path) -> String {
    let ext = file
        .extension()
        .map_or(String::new(), |e| e.to_string_lossy().to_uppercase());
    trf!("Writing {} File", ext)
}

#[derive(Debug)]
pub struct ProgressWindow {
    file: PathBuf,
    started: Instant,
    /// The folded sections: Estimated Properties, Performance, Options.
    folded: [bool; 3],
    usage: bv_platform::Usage,
    /// Processor share, memory and disk share, refreshed every second.
    measured: (Option<f64>, Option<u64>, Option<f64>),
    last_measure: Option<Instant>,
    /// Shut the computer down when the render ends (this render only).
    shutdown: bool,
    /// "Cancel Warning" is up.
    asking_cancel: bool,
    /// The automatic actions of the end were done.
    ended: bool,
    /// The taskbar shows this many thousandths.
    taskbar: Option<u32>,
    /// Controlled folder access refusing a failed render's folder (looked
    /// up once; other errors say enough by themselves).
    problem: Option<Option<super::folder::FolderProblem>>,
    /// The window's title as it was shown last.
    title: String,
    /// The project's CC1 and CC2 captions as a Scenarist file, written
    /// beside the rendered file when it is done (VEGAS's Help: formats
    /// that do not carry captions get an `.scc` file of the same name).
    scc: Option<String>,
}

/// `project`'s CC1 and CC2 captions from `from` on (the render's start) as
/// a Scenarist file; `None` when it has none.
pub(crate) fn captions_file(project: &bv_timeline::Project, from: bv_core::Tick) -> Option<String> {
    let captions: Vec<bv_timeline::captions::Caption> = project
        .captions()
        .into_iter()
        .filter(|c| c.channel < 2 && c.time >= from)
        .collect();
    (!captions.is_empty()).then(|| bv_timeline::caption_files::write_scc(&captions, from))
}

/// "Cancel Warning", the question's window title.
pub(crate) fn cancel_warning_title() -> &'static str {
    tr("Cancel Warning")
}

impl ProgressWindow {
    pub fn new(file: PathBuf) -> ProgressWindow {
        ProgressWindow {
            file,
            started: Instant::now(),
            folded: [false; 3],
            usage: bv_platform::Usage::new(),
            measured: (None, None, None),
            last_measure: None,
            shutdown: false,
            asking_cancel: false,
            ended: false,
            taskbar: None,
            problem: None,
            title: String::new(),
            scc: None,
        }
    }

    /// With the captions file to write beside the rendered one.
    pub(crate) fn with_captions(mut self, scc: Option<String>) -> ProgressWindow {
        self.scc = scc;
        self
    }

    /// The window and its Cancel Warning: modal while it is up, as VEGAS's
    /// render window is.
    pub(crate) fn ids() -> [egui::Id; 2] {
        [
            egui::Id::new("render-progress"),
            egui::Id::new("render-cancel-warning"),
        ]
    }

    /// The window's title as it was shown last ("Writing MP4 File: 37%").
    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    /// The file being rendered.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Asks "Cancel Warning" (the status bar's Cancel).
    pub fn ask_cancel(&mut self) {
        self.asking_cancel = true;
    }

    fn measure(&mut self) {
        if self
            .last_measure
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.last_measure = Some(Instant::now());
        let cpu = self.usage.cpu().or(self.measured.0);
        let disk = self.usage.disk().or(self.measured.2);
        self.measured = (cpu, self.usage.memory(), disk);
    }

    /// The progress on the taskbar button (only when it changes).
    fn taskbar(&mut self, progress: Option<f32>) {
        let now = progress.map(|p| (p * 1000.0) as u32);
        if now != self.taskbar {
            self.taskbar = now;
            bv_platform::taskbar_progress(&crate::floating::main_title(), progress);
        }
    }

    /// A folding section header ("▾ Performance"); true when open.
    fn section(&mut self, ui: &mut Ui, i: usize, title: &str) -> bool {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
        if resp.clicked() {
            self.folded[i] = !self.folded[i];
        }
        let p = ui.painter();
        let c = pos2(rect.left() + 8.0, rect.center().y);
        let pts = if self.folded[i] {
            vec![
                pos2(c.x - 2.0, c.y - 4.0),
                pos2(c.x + 3.0, c.y),
                pos2(c.x - 2.0, c.y + 4.0),
            ]
        } else {
            vec![
                pos2(c.x - 4.0, c.y - 2.0),
                pos2(c.x + 4.0, c.y - 2.0),
                pos2(c.x, c.y + 3.0),
            ]
        };
        p.add(egui::Shape::convex_polygon(
            pts,
            theme::text(),
            Stroke::NONE,
        ));
        p.text(
            pos2(rect.left() + 20.0, rect.center().y),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(13.0),
            theme::text_strong(),
        );
        !self.folded[i]
    }

    /// A label and its value, right-aligned.
    fn row(ui: &mut Ui, label: &str, value: &str) {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label(label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(value);
            });
        });
    }

    /// Shows the window; false when it closes.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let Some(p) = app.engine.render_progress() else {
            self.taskbar(None);
            return false;
        };
        let running = !p.finished;
        let ok = p.finished && p.error.is_none() && !p.cancelled;
        let show_video = !app.settings.render.hide_render_video;
        app.engine.show_render_video(show_video && running);
        self.taskbar(running.then(|| p.fraction()));
        self.measure();
        // A few times a second: drawn more often, the interface took from
        // the render the processor and graphics card it needs.
        if running {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let f = p.fraction() as f64;
        let elapsed = if running {
            self.started.elapsed().as_secs_f64()
        } else {
            p.elapsed_secs
        };
        let remaining = if running && f > 0.001 {
            elapsed * (1.0 - f) / f
        } else {
            0.0
        };
        let estimated = if f > 0.001 { p.bytes as f64 / f } else { 0.0 };
        let pct = (f * 100.0).floor() as u32;
        let title = if running {
            format!("{}: {pct}%", writing_title(&self.file))
        } else if p.cancelled {
            tr("Render Canceled").to_string()
        } else if p.error.is_some() {
            tr("Render Failed").to_string()
        } else {
            tr("Render Complete").to_string()
        };
        // The first file written (an image sequence's first picture).
        let first = p.files.first().cloned().unwrap_or(self.file.clone());
        let mut close = false;
        title.clone_into(&mut self.title);
        crate::floating::Window::new(title)
            .id(Self::ids()[0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(300.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(
                    RichText::new(self.file.display().to_string()).color(theme::text_strong()),
                );
                // The thin bar under the file.
                let (bar, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
                ui.painter().rect_filled(bar, 0.0, theme::panel_dark());
                let done = Rect::from_min_size(bar.min, vec2(bar.width() * f as f32, bar.height()));
                ui.painter().rect_filled(done, 0.0, super::paint::BLUE);
                let dim = |t: String| RichText::new(t).color(theme::text_dim());
                ui.horizontal(|ui| {
                    ui.label(dim(trf!("{}% of {}", pct, crate::fmt::bytes(estimated))));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(dim(trf!("Remaining: -{}", clock(remaining))));
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(dim(trf!("Rendered: {}", crate::fmt::bytes(p.bytes as f64))));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(dim(trf!("Elapsed: {}", clock(elapsed))));
                    });
                });
                if let Some(e) = &p.error {
                    ui.add(
                        egui::Label::new(RichText::new(trf!("Error: {}", e)).color(theme::error()))
                            .wrap(),
                    );
                    let file = &self.file;
                    let problem = self.problem.get_or_insert_with(|| {
                        file.parent()
                            .and_then(super::folder::FolderProblem::of)
                            .filter(|p| p.protected())
                    });
                    if let Some(problem) = problem {
                        ui.add_space(4.0);
                        problem.explain(ui);
                        ui.horizontal(|ui| problem.security_button(ui));
                    }
                }
                ui.add_space(4.0);
                if self.section(ui, 0, tr("Estimated Properties")) {
                    let free = bv_platform::free_space(&self.file);
                    Self::row(
                        ui,
                        tr("Free Disk Space:"),
                        &free.map_or("—".into(), |b| size0(b as f64)),
                    );
                    Self::row(ui, tr("Rendering Duration:"), &clock(elapsed + remaining));
                    let finish = bv_platform::local_time().map(|(h, m, s)| {
                        let now = (h * 3600 + m * 60 + s) as f64;
                        time_of_day((now + remaining).round() as u64, crate::i18n::language())
                    });
                    Self::row(
                        ui,
                        tr("Time of Completion:"),
                        &finish.unwrap_or_else(|| "—".into()),
                    );
                }
                if self.section(ui, 1, tr("Performance")) {
                    let fps = |v: f64| {
                        if p.video {
                            format!("{} fps", crate::fmt::num(v, 2))
                        } else {
                            "—".to_string()
                        }
                    };
                    Self::row(ui, tr("Average Speed:"), &fps(p.avg_fps));
                    Self::row(
                        ui,
                        tr("Current Speed:"),
                        &fps(if running { p.fps } else { 0.0 }),
                    );
                    if p.passed > 0 {
                        Self::row(ui, tr("Frames not recompressed:"), &p.passed.to_string());
                    }
                    let share = |v: Option<f64>| {
                        v.map_or("—".into(), |v| format!("{} %", crate::fmt::num(v, 2)))
                    };
                    Self::row(ui, tr("CPU Usage:"), &share(self.measured.0));
                    Self::row(
                        ui,
                        tr("Memory Usage:"),
                        &self
                            .measured
                            .1
                            .map_or("—".into(), |b| crate::fmt::bytes(b as f64)),
                    );
                    Self::row(ui, tr("Disk usage:"), &share(self.measured.2));
                }
                if self.section(ui, 2, tr("Options")) {
                    let prefs = &mut app.settings.render;
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        let mut show = !prefs.hide_render_video;
                        if crate::widgets::checkbox(
                            ui,
                            &mut show,
                            tr("Show Video in Preview Window"),
                        )
                        .on_hover_text(tr("Shows the frames being rendered (may slow rendering)."))
                        .changed()
                        {
                            prefs.hide_render_video = !show;
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        changed |= crate::widgets::checkbox(
                            ui,
                            &mut prefs.open_folder_when_done,
                            tr("Open folder when rendering is complete"),
                        )
                        .changed();
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        changed |= crate::widgets::checkbox(
                            ui,
                            &mut prefs.close_when_done,
                            tr("Close dialog when rendering is complete"),
                        )
                        .changed();
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.add_enabled_ui(running, |ui| {
                            crate::widgets::checkbox(
                                ui,
                                &mut self.shutdown,
                                tr("Shut down computer after render completes"),
                            )
                        });
                    });
                    if changed {
                        let _ = app.settings.save();
                    }
                }
                ui.add_space(8.0);
                // Open Folder, Open and Cancel (Close once it ended).
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let w = (ui.available_width() - 8.0) / 3.0;
                    let size = vec2(w, 26.0);
                    if ui
                        .add_enabled(ok, egui::Button::new(tr("Open Folder")).min_size(size))
                        .clicked()
                    {
                        show_in_folder(&first);
                    }
                    if ui
                        .add_enabled(ok, egui::Button::new(tr("Open")).min_size(size))
                        .clicked()
                    {
                        open_with_system(&first);
                    }
                    let label = if running { tr("Cancel") } else { tr("Close") };
                    if ui.add(egui::Button::new(label).min_size(size)).clicked() {
                        if running {
                            self.asking_cancel = true;
                        } else {
                            close = true;
                        }
                    }
                });
            });
        if self.asking_cancel && running {
            self.cancel_warning(ctx, app);
        } else {
            self.asking_cancel = false;
        }
        if p.finished && !self.ended {
            self.ended = true;
            let shown = self.file.display();
            let line = if p.cancelled {
                format!("Render canceled: {shown}")
            } else if let Some(e) = &p.error {
                format!("Render failed: {shown}: {e}")
            } else {
                format!("Rendered {shown} in {:.1} s", p.elapsed_secs)
            };
            crate::logs::log(crate::logs::LogKind::Renders, &[line]);
            self.taskbar(None);
            app.engine.show_render_video(false);
            let file_name = self
                .file
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().to_string());
            if p.cancelled {
                app.notify(tr("Render canceled.").to_string());
                close = true;
            } else if ok {
                if p.elapsed_secs > 60.0 {
                    bv_platform::notify_sound();
                }
                if app.settings.render.open_folder_when_done {
                    show_in_folder(&first);
                }
                if app.settings.render.close_when_done {
                    close = true;
                }
                app.notify(trf!("Rendered {} in {}.", file_name, clock(p.elapsed_secs)));
                if let Some(scc) = self.scc.take() {
                    let path = self.file.with_extension("scc");
                    if let Err(e) = std::fs::write(&path, scc) {
                        app.report_error(trf!("Could not write the captions file: {}", e));
                    }
                }
                if self.shutdown {
                    bv_platform::shutdown_in(
                        60,
                        tr("BananaV finished rendering: the computer shuts down in one minute."),
                    );
                }
            }
        }
        if close {
            app.engine.clear_render();
            return false;
        }
        true
    }

    /// "Cancel Warning": Yes cancels the render (and loses what it made).
    fn cancel_warning(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let mut answer = None;
        crate::floating::Window::new(cancel_warning_title())
            .id(Self::ids()[1])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.add(
                    egui::Label::new(tr(
                        "You are attempting to cancel the render process. Everything rendered up to this point will be lost. Are you sure you want to cancel rendering?",
                    ))
                    .wrap(),
                );
                ui.add_space(28.0);
                // One row (a bare right-to-left layout would take all the
                // height there is).
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(tr("No")).min_size(vec2(96.0, 24.0)))
                            .clicked()
                        {
                            answer = Some(false);
                        }
                        if ui
                            .add(egui::Button::new(tr("Yes")).min_size(vec2(96.0, 24.0)))
                            .clicked()
                        {
                            answer = Some(true);
                        }
                    });
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(false);
                }
            });
        match answer {
            Some(true) => {
                app.engine.cancel_render();
                self.asking_cancel = false;
            }
            Some(false) => self.asking_cancel = false,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project's CC1 captions go beside its render as Scenarist's, from
    /// the render's start; a project without captions writes none.
    #[test]
    fn captions_go_beside_the_render() {
        use bv_core::Tick;
        use bv_timeline::captions::{Caption, pop_on};
        let mut p = bv_timeline::Project::new(bv_timeline::ProjectSettings::default());
        assert!(captions_file(&p, Tick::ZERO).is_none());
        p.add_captions(&[Caption {
            time: Tick::from_seconds(5),
            channel: 0,
            markup: pop_on("Hello"),
        }]);
        let scc = captions_file(&p, Tick::from_seconds(2)).expect("a captions file");
        let back = bv_timeline::caption_files::read("scc", &scc, bv_core::FrameRate::FPS_29_97);
        assert_eq!(back.len(), 1);
        let secs = back[0].time.as_seconds_f64();
        assert!((secs - 3.0).abs() < 0.05, "{secs}");
        assert!(
            captions_file(&p, Tick::from_seconds(6)).is_none(),
            "after them"
        );
    }

    #[test]
    fn clocks_and_times() {
        assert_eq!(clock(0.0), "00:00:00.00");
        assert_eq!(clock(1050.0), "00:17:30.00");
        assert_eq!(clock(43.88), "00:00:43.88");
        assert_eq!(
            time_of_day(4 * 3600 + 27 * 60 + 44, Language::English),
            "4:27:44 AM"
        );
        assert_eq!(time_of_day(16 * 3600 + 5, Language::English), "4:00:05 PM");
        assert_eq!(
            time_of_day(16 * 3600 + 5, Language::PortugueseBrazil),
            "16:00:05"
        );
        assert_eq!(time_of_day(0, Language::English), "12:00:00 AM");
    }

    #[test]
    fn sizes() {
        assert_eq!(crate::fmt::bytes(237.06 * 1024.0 * 1024.0), "237.06 MB");
        assert_eq!(size0(92.7 * 1024.0 * 1024.0 * 1024.0), "92 GB");
    }

    /// Playing when a render starts, playback stops, as VEGAS's: the
    /// processor and the graphics card go to the render.
    #[test]
    fn a_render_stops_playback() {
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        app.toggle_play();
        assert!(app.is_playing(), "playing");
        let dir = std::env::temp_dir().join(format!("bv-stops-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        app.engine.set_project(app.history.current());
        app.engine.start_render(bv_engine::RenderSettings {
            format: bv_engine::FileFormat::Wav,
            path: dir.join("Mix.wav"),
            range: bv_core::TimeRange::new(bv_core::Tick::ZERO, bv_core::Tick::from_seconds(8)),
            video: None,
            audio: Some(bv_engine::AudioParams::default()),
            options: bv_engine::RenderOptions::default(),
            project_file: None,
        });
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.frame(ui));
        // No graphics card: the textures go nowhere.
        out.textures_delta.clear();
        assert!(!app.is_playing(), "stopped by the render");
        app.engine.cancel_render();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn titles_name_the_file_kind() {
        assert_eq!(
            writing_title(Path::new("C:/a/Untitled.mp4")),
            "Writing MP4 File"
        );
    }
}
