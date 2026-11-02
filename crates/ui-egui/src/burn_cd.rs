//! Tools › Burn Disc's audio CDs (VEGAS's "Track-at-once (TAO) CD burning"
//! and "Disc-at-once (DAO or Red Book) CD Burning"): the Burn
//! Track-at-Once Audio CD dialog — the file's length and the time left on
//! the disc, the Action (Burn audio, Close disc, Erase RW disc; the tests
//! greyed out), the burning options, the drive and its speed — and the
//! Burn Disc-at-Once Audio CD dialog — the drive, its speed, Buffer
//! underrun protection, the Burn mode (Burn CDs; the tests greyed out),
//! Automatically erase rewritable discs and Eject when done. The jobs they
//! start (`burn_job.rs`) render the project's sound as CD audio (a 44.1 kHz
//! 16-bit stereo Wave file), then burn it as a track or as the CD layout's
//! tracks with their indices, ISRCs and flags
//! (`bv_platform::burn_audio_track`, `burn_audio_disc`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bv_core::{Tick, TimeRange};
use bv_engine::{AudioParams, FileFormat, RenderOptions, RenderSettings};
use bv_platform::{Burner, CdOptions};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::burn_dvd::speed_name;
use crate::burn_job::{BurnJob, CdPart, Drives, Step, Task, drive_box, no_drive_note};
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;
use crate::theme;

/// CD audio: 44.1 kHz 16-bit stereo.
const AUDIO: AudioParams = AudioParams {
    sample_rate: 44_100,
    channels: 2,
    bit_depth: 16,
    float: false,
    bitrate: 0,
};

/// The Speed list: times a CD's speed, Max first (0).
const SPEEDS: [u32; 11] = [0, 48, 40, 32, 24, 16, 12, 8, 4, 2, 1];

/// The audio CD dialogs' choices, kept between uses
/// (`UiSettings::burn_cd`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BurnCdSettings {
    /// The drive picked last (the system's id of it).
    pub drive: String,
    /// Times a CD's speed; 0: Max.
    pub speed: u32,
    pub underrun_protection: bool,
    /// Track-at-Once: Erase RW disc before burning, Close disc when done
    /// burning.
    pub erase_first: bool,
    pub close: bool,
    pub eject: bool,
    /// Disc-at-Once: Automatically erase rewritable discs.
    pub auto_erase: bool,
}

impl Default for BurnCdSettings {
    fn default() -> Self {
        BurnCdSettings {
            drive: String::new(),
            speed: 0,
            underrun_protection: true,
            erase_first: false,
            close: true,
            eject: true,
            auto_erase: false,
        }
    }
}

impl BurnCdSettings {
    fn options(&self, erase: bool) -> CdOptions {
        CdOptions {
            speed: (self.speed > 0).then_some(self.speed),
            eject: self.eject,
            underrun_protection: self.underrun_protection,
            erase,
        }
    }
}

/// Track-at-Once's Action list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Burn,
    TestThenBurn,
    Test,
    Close,
    Erase,
}

impl Action {
    const ALL: [Action; 5] = [
        Action::Burn,
        Action::TestThenBurn,
        Action::Test,
        Action::Close,
        Action::Erase,
    ];

    fn name(self) -> &'static str {
        match self {
            Action::Burn => tr("Burn audio"),
            Action::TestThenBurn => tr("Test, then burn audio"),
            Action::Test => tr("Test only"),
            Action::Close => tr("Close disc"),
            Action::Erase => tr("Erase RW disc"),
        }
    }

    /// The system's recording service writes no test runs.
    fn ready(self) -> bool {
        !matches!(self, Action::TestThenBurn | Action::Test)
    }
}

/// The free sectors on the picked drive's disc, looked up on a thread
/// whenever another drive is picked: the drive's id, then what it said
/// (`None`: still asking; `Some(None)`: no disc to write).
#[derive(Clone, Debug, Default)]
struct Free(Arc<Mutex<(String, Option<Option<u64>>)>>);

impl Free {
    /// What the disc in `drive` has free, asking it the first time.
    fn of(&self, drive: &str) -> Option<Option<u64>> {
        let mut f = self.0.lock().expect("free");
        if f.0 != drive {
            *f = (drive.to_string(), None);
            let (into, id) = (self.0.clone(), drive.to_string());
            std::thread::spawn(move || {
                let free = bv_platform::free_sectors(&id);
                let mut f = into.lock().expect("free");
                if f.0 == id {
                    f.1 = Some(free);
                }
            });
        }
        f.1
    }
}

/// A time as the dialog shows it ("m:ss").
fn minutes(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The two-column grid the dialogs lay their rows in.
fn grid(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(vec2(8.0, 6.0))
        .show(ui, rows);
}

/// The Speed box.
fn speed_box(ui: &mut egui::Ui, salt: &str, width: f32, speed: &mut u32) {
    egui::ComboBox::from_id_salt(salt)
        .width(width)
        .selected_text(speed_name(*speed))
        .show_ui(ui, |ui| {
            for s in SPEEDS {
                ui.selectable_value(speed, s, speed_name(s));
            }
        });
}

/// OK (or Start) and Cancel: which was clicked.
fn buttons(ui: &mut egui::Ui, ok_label: &str, ready: bool) -> (bool, bool) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let ok = ui
            .add_enabled(ready, egui::Button::new(ok_label).min_size(vec2(72.0, 0.0)))
            .clicked();
        let cancel = ui
            .add(egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)))
            .clicked();
        (ok, cancel)
    })
    .inner
}

#[derive(Debug)]
pub(crate) struct BurnTaoDialog {
    action: Action,
    s: BurnCdSettings,
    selection_only: bool,
    drives: Drives,
    free: Free,
}

impl BurnTaoDialog {
    fn new(app: &BananaApp, drives: Drives) -> BurnTaoDialog {
        BurnTaoDialog {
            action: Action::Burn,
            s: app.settings.burn_cd.clone(),
            selection_only: false,
            drives,
            free: Free::default(),
        }
    }

    /// What is burned: the loop region with Burn selection only, else the
    /// whole project.
    fn range(&self, app: &BananaApp) -> TimeRange {
        let p = app.project();
        match (self.selection_only, p.loop_region) {
            (true, Some(r)) => r,
            _ => TimeRange::new(Tick::ZERO, p.duration()),
        }
    }

    /// Draws the dialog; `false` once it is closed.
    pub(crate) fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let drives = self.drives.found();
        let mut picked = None;
        let free = self.free.of(&self.s.drive);
        if drives.is_none() || free.is_none() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let length = self.range(app).duration.as_seconds_f64();
        let has_selection = app.project().loop_region.is_some();
        let (mut open, mut ok, mut cancel) = (true, false, false);
        let width = 240.0;
        crate::floating::Window::new(tr("Burn Track-at-Once Audio CD"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                grid(ui, "burn-tao", |ui| {
                    ui.label(tr("File length:"));
                    ui.label(minutes(length));
                    ui.end_row();
                    ui.label(tr("Time left on disc:"));
                    let left = match free {
                        Some(Some(sectors)) => minutes(sectors as f64 / 75.0),
                        _ => "—".to_string(),
                    };
                    ui.label(left);
                    ui.end_row();
                    ui.label(tr("Action:"));
                    egui::ComboBox::from_id_salt("burn-tao-action")
                        .width(width)
                        .selected_text(self.action.name())
                        .show_ui(ui, |ui| {
                            for a in Action::ALL {
                                ui.add_enabled_ui(a.ready(), |ui| {
                                    ui.selectable_value(&mut self.action, a, a.name())
                                })
                                .response
                                .on_disabled_hover_text(tr("Coming soon to BananaV"));
                            }
                        });
                    ui.end_row();
                    ui.label(tr("Drive:"));
                    picked = drive_box(ui, "burn-tao-drive", width, &drives, &mut self.s.drive);
                    ui.end_row();
                    ui.label(tr("Speed:"));
                    speed_box(ui, "burn-tao-speed", width, &mut self.s.speed);
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Burning options")).color(theme::text_strong()));
                let s = &mut self.s;
                ui.checkbox(&mut s.underrun_protection, tr("Buffer underrun protection"));
                ui.checkbox(&mut s.erase_first, tr("Erase RW disc before burning"));
                ui.checkbox(&mut s.close, tr("Close disc when done burning"));
                ui.checkbox(&mut s.eject, tr("Eject disc when done"));
                ui.add_enabled(
                    has_selection,
                    egui::Checkbox::new(&mut self.selection_only, tr("Burn selection only")),
                );
                no_drive_note(ui, &drives);
                let ready = picked.is_some() && self.action.ready();
                (ok, cancel) = buttons(ui, tr("Start"), ready);
            });
        if ok && let Some(drive) = picked {
            app.settings.burn_cd = self.s.clone();
            let _ = app.settings.save();
            let task = match self.action {
                Action::Close => Task::CloseCd {
                    eject: self.s.eject,
                },
                Action::Erase => Task::EraseCd,
                _ => Task::CdTrack {
                    close: self.s.close,
                    options: self.s.options(self.s.erase_first),
                },
            };
            let range = self.range(app);
            app.start_cd_burn(task, drive, range);
            return false;
        }
        open && !cancel
    }
}

#[derive(Debug)]
pub(crate) struct BurnDaoDialog {
    s: BurnCdSettings,
    drives: Drives,
}

impl BurnDaoDialog {
    fn new(app: &BananaApp, drives: Drives) -> BurnDaoDialog {
        BurnDaoDialog {
            s: app.settings.burn_cd.clone(),
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
        let width = 240.0;
        crate::floating::Window::new(tr("Burn Disc-at-Once Audio CD"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                grid(ui, "burn-dao", |ui| {
                    ui.label(tr("Drive:"));
                    picked = drive_box(ui, "burn-dao-drive", width, &drives, &mut self.s.drive);
                    ui.end_row();
                    ui.label(tr("Speed:"));
                    speed_box(ui, "burn-dao-speed", width, &mut self.s.speed);
                    ui.end_row();
                });
                let s = &mut self.s;
                ui.checkbox(&mut s.underrun_protection, tr("Buffer underrun protection"));
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Burn mode")).color(theme::text_strong()));
                let _ = ui.radio(true, tr("Burn CDs"));
                for test in [
                    tr("Test first, then burn CDs"),
                    tr("Test only (do not burn CDs)"),
                ] {
                    ui.add_enabled(false, egui::RadioButton::new(false, test))
                        .on_disabled_hover_text(tr("Coming soon to BananaV"));
                }
                ui.add_space(6.0);
                // BananaV always renders the sound first.
                let mut prerender = true;
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(
                        &mut prerender,
                        tr("Render temporary image before burning"),
                    ),
                )
                .on_disabled_hover_text(tr("BananaV always renders the sound before burning."));
                ui.checkbox(
                    &mut s.auto_erase,
                    tr("Automatically erase rewritable discs"),
                );
                ui.checkbox(&mut s.eject, tr("Eject when done"));
                no_drive_note(ui, &drives);
                (ok, cancel) = buttons(ui, "OK", picked.is_some());
            });
        if ok && let Some(drive) = picked {
            app.settings.burn_cd = self.s.clone();
            let _ = app.settings.save();
            let (range, tracks) = cd_layout(app.project());
            let task = Task::CdDisc {
                tracks,
                first_track: app.project().audio_cd.first_track,
                catalog: app.project().audio_cd.catalog_number.clone(),
                options: self.s.options(self.s.auto_erase),
            };
            app.start_cd_burn(task, drive, range);
            return false;
        }
        open && !cancel
    }
}

/// The CD layout as Disc-at-Once tracks: the time from the first track's
/// start to the last one's end, and each track from its start to the
/// next one's (the pause between them its tail), with its indices after
/// the first, in CD sectors.
fn cd_layout(p: &bv_timeline::Project) -> (TimeRange, Vec<CdPart>) {
    let mut tracks: Vec<&bv_timeline::CdTrack> = p.cd_tracks.iter().collect();
    tracks.sort_by_key(|t| t.range.start);
    let (Some(first), Some(last)) = (tracks.first(), tracks.last()) else {
        return (TimeRange::new(Tick::ZERO, Tick::ZERO), Vec::new());
    };
    let begin = first.range.start;
    let end = last.range.end();
    let sector = |t: Tick| ((t - begin).as_seconds_f64() * 75.0).round().max(0.0) as u64;
    let parts = tracks
        .iter()
        .enumerate()
        .map(|(k, t)| {
            let start = sector(t.range.start);
            let stop = tracks
                .get(k + 1)
                .map_or(sector(end), |n| sector(n.range.start));
            let indices = p
                .cd_indices
                .iter()
                .filter(|i| i.time > t.range.start && i.time < t.range.end())
                .map(|i| sector(i.time).saturating_sub(start) as u32)
                .collect();
            CdPart {
                start,
                sectors: stop.saturating_sub(start),
                indices,
                isrc: t.isrc.clone(),
                protect: t.protect,
                emphasis: t.emphasis,
            }
        })
        .collect();
    (TimeRange::new(begin, end - begin), parts)
}

impl BananaApp {
    /// Tools › Burn Disc › Track-at-Once Audio CD….
    pub(crate) fn open_burn_tao(&mut self) {
        self.menu_dialogs.more.burn_tao = Some(BurnTaoDialog::new(self, Drives::look()));
    }

    /// Tools › Burn Disc › Disc-at-Once Audio CD….
    pub(crate) fn open_burn_dao(&mut self) {
        self.menu_dialogs.more.burn_dao = Some(BurnDaoDialog::new(self, Drives::look()));
    }

    /// The job of `task`: the sound of `range` rendered as CD audio first
    /// when the task burns it.
    fn start_cd_burn(&mut self, task: Task, drive: Burner, range: TimeRange) {
        let work = crate::burn_job::work_folder();
        let render = matches!(task, Task::CdTrack { .. } | Task::CdDisc { .. });
        if render {
            let _ = std::fs::create_dir_all(&work);
            self.stop();
            self.engine.set_project(self.history.current());
            self.engine.start_render(RenderSettings {
                format: FileFormat::Wav,
                path: work.join("CD.wav"),
                range,
                video: None,
                audio: Some(AUDIO),
                options: RenderOptions::default(),
                project_file: None,
            });
        }
        crate::logs::log(
            crate::logs::LogKind::Renders,
            &[format!("Audio CD job started: {task:?} ({})", drive.name)],
        );
        self.menu_dialogs.more.burn_job = Some(BurnJob::new(work, task, drive, render));
    }

    /// The audio CD dialogs with a drive of the tests' (screen tests):
    /// Track-at-Once, or Disc-at-Once.
    pub fn burn_cd_for_review(&mut self, disc_at_once: bool) {
        let drives = Drives::given(vec![crate::burn_job::review_drive()]);
        if disc_at_once {
            self.menu_dialogs.more.burn_dao = Some(BurnDaoDialog::new(self, drives));
        } else {
            self.menu_dialogs.more.burn_tao = Some(BurnTaoDialog::new(self, drives));
        }
    }

    /// An audio CD job's window burning, a third of the way (screen tests:
    /// nothing renders).
    pub fn burn_cd_job_for_review(&mut self) {
        let task = Task::CdTrack {
            close: true,
            options: BurnCdSettings::default().options(false),
        };
        let drive = crate::burn_job::review_drive();
        self.menu_dialogs.more.burn_job = Some(BurnJob::at_step(task, drive, Step::Burning, 0.33));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_becomes_tracks_in_sectors() {
        let s = Tick::from_seconds;
        let mut p = bv_timeline::Project::new(bv_timeline::ProjectSettings::default());
        let a = p.add_cd_track(TimeRange::new(s(2), s(4))).unwrap();
        p.add_cd_track(TimeRange::new(s(8), s(5))).unwrap();
        p.add_cd_index(s(10)).unwrap();
        p.set_cd_track(a, |t| {
            t.isrc = "USABC2600001".into();
            t.protect = true;
        });
        let (range, parts) = cd_layout(&p);
        assert_eq!((range.start, range.duration), (s(2), s(11)));
        // The first runs to the second's start (its pause included).
        assert_eq!((parts[0].start, parts[0].sectors), (0, 6 * 75));
        assert_eq!(parts[0].isrc, "USABC2600001");
        assert!(parts[0].protect);
        assert_eq!((parts[1].start, parts[1].sectors), (6 * 75, 5 * 75));
        assert_eq!(parts[1].indices, [2 * 75]);
    }
}
