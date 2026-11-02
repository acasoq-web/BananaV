//! Tools › Burn Disc › DVD (VEGAS's "Burning a DVD from the timeline"):
//! the Burn DVD dialog — the video format, the audio format it renders,
//! the drive, its speed, Eject when done — and the job its OK starts
//! (`burn_job.rs`): the project rendered into a DVD-Video folder
//! (`RenderOptions::dvd`, the engine's `render/dvd.rs`), the folder made a
//! disc image (`bv_engine::write_dvd_image`) and the image burned onto the
//! blank disc (`bv_platform::burn_image`).

use std::time::Duration;

use bv_core::{FrameRate, Tick, TimeRange};
use bv_engine::{AudioParams, FileFormat, RenderOptions, RenderSettings, VideoParams};
use bv_platform::Burner;
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::burn_job::{BurnJob, Drives, Step, Task, drive_box, no_drive_note};
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;
use crate::theme;

/// The dialog's video formats: DVD's frames, rates and shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DvdFormat {
    Ntsc,
    NtscWide,
    Pal,
    PalWide,
}

impl DvdFormat {
    const ALL: [DvdFormat; 4] = [
        DvdFormat::Ntsc,
        DvdFormat::NtscWide,
        DvdFormat::Pal,
        DvdFormat::PalWide,
    ];

    fn name(self) -> &'static str {
        match self {
            DvdFormat::Ntsc => tr("DVD NTSC video stream"),
            DvdFormat::NtscWide => tr("DVD NTSC Widescreen video stream"),
            DvdFormat::Pal => tr("DVD PAL video stream"),
            DvdFormat::PalWide => tr("DVD PAL Widescreen video stream"),
        }
    }

    fn pal(self) -> bool {
        matches!(self, DvdFormat::Pal | DvdFormat::PalWide)
    }

    fn wide(self) -> bool {
        matches!(self, DvdFormat::NtscWide | DvdFormat::PalWide)
    }

    /// The project's nearest: PAL at 25 and 50 frames a second, wide for
    /// pictures nearer 16:9 than 4:3.
    fn for_project(s: &bv_timeline::ProjectSettings) -> DvdFormat {
        let fps = s.frame_rate.as_f64();
        let pal = (fps - 25.0).abs() < 0.1 || (fps - 50.0).abs() < 0.1;
        let (w, h) = s.output_size();
        let par = s.pixel_aspect;
        let shape = w as f64 * par.num as f64 / par.den.max(1) as f64 / h.max(1) as f64;
        let wide = (shape - 16.0 / 9.0).abs() < (shape - 4.0 / 3.0).abs();
        match (pal, wide) {
            (false, false) => DvdFormat::Ntsc,
            (false, true) => DvdFormat::NtscWide,
            (true, false) => DvdFormat::Pal,
            (true, true) => DvdFormat::PalWide,
        }
    }

    /// The render's video: DVD's frame at 6 Mbit/s, 8 at most.
    fn video(self) -> VideoParams {
        VideoParams {
            width: 720,
            height: if self.pal() { 576 } else { 480 },
            frame_rate: if self.pal() {
                FrameRate::FPS_25
            } else {
                FrameRate::FPS_29_97
            },
            bitrate: 6_000_000,
            max_bitrate: 8_000_000,
            ..VideoParams::default()
        }
    }
}

/// DVD's sound as BananaV writes it: 48 kHz 16-bit stereo linear PCM.
const AUDIO: AudioParams = AudioParams {
    sample_rate: 48_000,
    channels: 2,
    bit_depth: 16,
    float: false,
    bitrate: 0,
};

/// The Burn speed list: times a DVD's speed, Max first (0).
const SPEEDS: [u32; 6] = [0, 16, 8, 4, 2, 1];

pub(crate) fn speed_name(x: u32) -> String {
    if x == 0 {
        tr("Max").to_string()
    } else {
        format!("{x}x")
    }
}

/// What the Audio format box shows.
fn audio_text() -> String {
    trf!(
        "{} Hz, {}-bit, Stereo, PCM",
        crate::fmt::grouped(AUDIO.sample_rate as u64),
        AUDIO.bit_depth
    )
}

/// The Burn DVD dialog's choices, kept between uses
/// (`UiSettings::burn_dvd`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BurnDvdSettings {
    /// `None`: the project's nearest.
    pub format: Option<DvdFormat>,
    /// The drive picked last (the system's id of it).
    pub drive: String,
    /// Times a DVD's speed; 0: Max.
    pub speed: u32,
    pub eject: bool,
}

impl Default for BurnDvdSettings {
    fn default() -> Self {
        BurnDvdSettings {
            format: None,
            drive: String::new(),
            speed: 0,
            eject: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct BurnDvdDialog {
    format: DvdFormat,
    drive: String,
    speed: u32,
    eject: bool,
    drives: Drives,
}

impl BurnDvdDialog {
    fn new(app: &BananaApp, drives: Drives) -> BurnDvdDialog {
        let s = &app.settings.burn_dvd;
        BurnDvdDialog {
            format: s
                .format
                .unwrap_or_else(|| DvdFormat::for_project(&app.project().settings)),
            drive: s.drive.clone(),
            speed: if SPEEDS.contains(&s.speed) {
                s.speed
            } else {
                0
            },
            eject: s.eject,
            drives,
        }
    }

    /// Draws the dialog; `false` once it is closed.
    pub(crate) fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let drives = self.drives.found();
        if drives.is_none() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let mut picked = None;
        let (mut open, mut ok, mut cancel) = (true, false, false);
        let width = 280.0;
        crate::floating::Window::new(tr("Burn DVD"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("burn-dvd")
                    .num_columns(2)
                    .spacing(vec2(8.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(tr("Video format:"));
                        egui::ComboBox::from_id_salt("burn-dvd-format")
                            .width(width)
                            .selected_text(self.format.name())
                            .show_ui(ui, |ui| {
                                for f in DvdFormat::ALL {
                                    ui.selectable_value(&mut self.format, f, f.name());
                                }
                            });
                        ui.end_row();
                        ui.label(tr("Audio format:"));
                        let mut text = audio_text();
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut text).desired_width(width),
                        );
                        ui.end_row();
                        // One grid, so the boxes line up.
                        ui.label(
                            RichText::new(tr("Recording options")).color(theme::text_strong()),
                        );
                        ui.end_row();
                        ui.label(tr("Burn drive:"));
                        picked = drive_box(ui, "burn-dvd-drive", width, &drives, &mut self.drive);
                        ui.end_row();
                        ui.label(tr("Burn speed:"));
                        egui::ComboBox::from_id_salt("burn-dvd-speed")
                            .width(width)
                            .selected_text(speed_name(self.speed))
                            .show_ui(ui, |ui| {
                                for s in SPEEDS {
                                    ui.selectable_value(&mut self.speed, s, speed_name(s));
                                }
                            });
                        ui.end_row();
                    });
                ui.checkbox(&mut self.eject, tr("Eject when done"));
                no_drive_note(ui, &drives);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ok |= ui
                        .add_enabled(
                            picked.is_some(),
                            egui::Button::new("OK").min_size(vec2(72.0, 0.0)),
                        )
                        .clicked();
                    cancel = ui
                        .add(egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)))
                        .clicked();
                });
            });
        if ok && let Some(drive) = picked {
            app.settings.burn_dvd = BurnDvdSettings {
                format: Some(self.format),
                drive: drive.id.clone(),
                speed: self.speed,
                eject: self.eject,
            };
            let _ = app.settings.save();
            app.start_dvd_burn(self.format, drive, self.speed, self.eject);
            return false;
        }
        open && !cancel
    }
}

impl BananaApp {
    /// Tools › Burn Disc › DVD….
    pub(crate) fn open_burn_dvd(&mut self) {
        if self.project().duration() == Tick::ZERO {
            self.notify(tr("The timeline is empty."));
            return;
        }
        self.menu_dialogs.more.burn_dvd = Some(BurnDvdDialog::new(self, Drives::look()));
    }

    /// Renders the whole project for the DVD; the job's window follows it
    /// to the image and the burn.
    fn start_dvd_burn(&mut self, format: DvdFormat, drive: Burner, speed: u32, eject: bool) {
        let work = crate::burn_job::work_folder();
        let label = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        self.stop();
        self.engine.set_project(self.history.current());
        self.engine.start_render(RenderSettings {
            format: FileFormat::Mpeg2,
            path: work.join("DVD"),
            range: TimeRange::new(Tick::ZERO, self.project().duration()),
            video: Some(format.video()),
            audio: Some(AUDIO),
            options: RenderOptions {
                dvd: true,
                dvd_wide: Some(format.wide()),
                ..RenderOptions::default()
            },
            project_file: None,
        });
        crate::logs::log(
            crate::logs::LogKind::Renders,
            &[format!(
                "DVD burn started: {} ({})",
                format.name(),
                drive.name
            )],
        );
        let task = Task::Dvd {
            label,
            speed,
            eject,
        };
        self.menu_dialogs.more.burn_job = Some(BurnJob::new(work, task, drive, true));
    }

    /// The Burn DVD dialog with a drive of the tests' (screen tests).
    pub fn burn_dvd_for_review(&mut self) {
        let drives = Drives::given(vec![crate::burn_job::review_drive()]);
        self.menu_dialogs.more.burn_dvd = Some(BurnDvdDialog::new(self, drives));
    }

    /// The job's window burning, halfway (screen tests: nothing renders).
    pub fn burn_job_for_review(&mut self) {
        let task = Task::Dvd {
            label: String::new(),
            speed: 0,
            eject: true,
        };
        let drive = crate::burn_job::review_drive();
        self.menu_dialogs.more.burn_job = Some(BurnJob::at_step(task, drive, Step::Burning, 0.5));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn the_format_follows_the_project() {
        let mut s = bv_timeline::ProjectSettings {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::FPS_29_97,
            ..bv_timeline::ProjectSettings::default()
        };
        assert_eq!(DvdFormat::for_project(&s), DvdFormat::NtscWide);
        s.width = 720;
        s.height = 576;
        s.frame_rate = FrameRate::FPS_25;
        s.pixel_aspect = bv_timeline::PixelAspect { num: 16, den: 15 };
        assert_eq!(DvdFormat::for_project(&s), DvdFormat::Pal);
        let v = DvdFormat::PalWide.video();
        assert_eq!(
            (v.width, v.height, v.frame_rate),
            (720, 576, FrameRate::FPS_25)
        );
    }

    #[test]
    fn the_dialog_starts_a_render_into_a_dvd_folder() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        app.start_dvd_burn(DvdFormat::Ntsc, crate::burn_job::review_drive(), 0, false);
        let job = app.menu_dialogs.more.burn_job.as_ref().unwrap();
        assert_eq!(job.step().0, Step::Rendering);
        assert!(app.engine.render_progress().is_some());
        // Canceled while rendering: the render stops, nothing is burned.
        let mut job = app.menu_dialogs.more.burn_job.take().unwrap();
        job.cancel(&app);
        let start = Instant::now();
        while !app.engine.render_progress().is_some_and(|p| p.finished) {
            assert!(start.elapsed() < Duration::from_secs(60));
            std::thread::sleep(Duration::from_millis(10));
        }
        job.follow(&mut app);
        assert_eq!(job.step().0, Step::Canceled);
        let _ = std::fs::remove_dir_all(&job.work);
    }
}
