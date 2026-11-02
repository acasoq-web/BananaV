//! Tools › Burn Disc's jobs: the project rendered into a temporary folder
//! (a DVD-Video folder, or the CD's sound as a Wave file), then the disc
//! made from it on a thread — a DVD's image burned, a CD's track or
//! tracks — or a CD closed or erased; each step shown in the job's
//! window, modal as the render's, with Cancel. The folder goes when the
//! window closes.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bv_platform::{BurnError, Burner, CdOptions, CdTrackData};

use crate::app::BananaApp;
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;
use crate::theme;

/// The job's window, modal while it is up.
pub(crate) const JOB_ID: &str = "burn-job";

/// The computer's drives that write discs, looked for on a thread (the
/// service can take a while), or given (screen tests).
#[derive(Clone, Debug)]
pub(crate) struct Drives(Arc<Mutex<Option<Vec<Burner>>>>);

impl Drives {
    pub(crate) fn look() -> Drives {
        let found = Arc::new(Mutex::new(None));
        let into = found.clone();
        std::thread::spawn(move || {
            let list = bv_platform::burners();
            *into.lock().expect("drives") = Some(list);
        });
        Drives(found)
    }

    pub(crate) fn given(list: Vec<Burner>) -> Drives {
        Drives(Arc::new(Mutex::new(Some(list))))
    }

    /// The drives found (`None`: still looking).
    pub(crate) fn found(&self) -> Option<Vec<Burner>> {
        self.0.lock().expect("drives").clone()
    }
}

/// A drive of the screen tests'.
pub(crate) fn review_drive() -> Burner {
    Burner {
        id: "review".into(),
        name: "E: DVD-RW".into(),
    }
}

/// The Burn drive box: the drives found, `drive` the one picked (the
/// first when the one picked last is gone); the drive picked.
pub(crate) fn drive_box(
    ui: &mut egui::Ui,
    salt: &str,
    width: f32,
    drives: &Option<Vec<Burner>>,
    drive: &mut String,
) -> Option<Burner> {
    if let Some(list) = drives
        && !list.iter().any(|b| b.id == *drive)
    {
        *drive = list.first().map(|b| b.id.clone()).unwrap_or_default();
    }
    let picked = drives.iter().flatten().find(|b| b.id == *drive).cloned();
    let shown = match (drives, &picked) {
        (None, _) => tr("Looking for drives…").to_string(),
        (_, Some(b)) => b.name.clone(),
        (Some(_), None) => tr("(none found)").to_string(),
    };
    egui::ComboBox::from_id_salt(salt)
        .width(width)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for b in drives.iter().flatten() {
                ui.selectable_value(drive, b.id.clone(), &b.name);
            }
        });
    picked
}

/// The note under a dialog that found no drive.
pub(crate) fn no_drive_note(ui: &mut egui::Ui, drives: &Option<Vec<Burner>>) {
    if drives.as_ref().is_some_and(Vec::is_empty) {
        ui.add(
            egui::Label::new(
                RichText::new(tr("No drive that writes discs was found on this computer."))
                    .color(theme::text_dim()),
            )
            .wrap(),
        );
    }
}

/// A job's temporary folder, emptied (a canceled job's leftovers: the
/// jobs' own folder).
pub(crate) fn work_folder() -> PathBuf {
    let work = std::env::temp_dir().join(format!("BananaV burn {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    work
}

/// What a job makes once its render is done.
#[derive(Clone, Debug)]
pub(crate) enum Task {
    /// The DVD-Video folder made an image named `label`, and burned.
    Dvd {
        label: String,
        speed: u32,
        eject: bool,
    },
    /// The sound burned as one track (Track-at-Once), the disc closed
    /// after it when `close`.
    CdTrack { close: bool, options: CdOptions },
    /// The sound burned as the CD layout's tracks (Disc-at-Once).
    CdDisc {
        tracks: Vec<CdPart>,
        first_track: u32,
        catalog: String,
        options: CdOptions,
    },
    /// The CD closed (no render).
    CloseCd { eject: bool },
    /// The rewritable disc erased (no render).
    EraseCd,
}

/// A Disc-at-Once track: where its sound is in the rendered file (CD
/// sectors from the file's sound's start) and what its subcode says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CdPart {
    pub start: u64,
    pub sectors: u64,
    pub indices: Vec<u32>,
    pub isrc: String,
    pub protect: bool,
    pub emphasis: bool,
}

impl Task {
    fn dvd(&self) -> bool {
        matches!(self, Task::Dvd { .. })
    }

    /// The job window's title.
    fn title(&self) -> &'static str {
        match self {
            Task::Dvd { .. } => tr("Burn DVD"),
            Task::CdDisc { .. } => tr("Burn Disc-at-Once Audio CD"),
            _ => tr("Burn Track-at-Once Audio CD"),
        }
    }
}

/// The job's steps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Rendering,
    Imaging,
    Burning,
    Done,
    Canceled,
    Failed(BurnError),
}

/// Where the job is: its step and how far through it (0–1).
#[derive(Debug)]
pub(crate) struct Shared {
    pub step: Step,
    pub fraction: f32,
}

#[derive(Debug)]
pub(crate) struct BurnJob {
    /// The temporary folder: the DVD-Video folder and its image, or the
    /// CD's sound.
    pub(crate) work: PathBuf,
    task: Task,
    drive: Burner,
    started: Instant,
    cancel: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
}

impl BurnJob {
    /// A job whose render has started (or, for a task that needs none,
    /// burning on a thread now).
    pub(crate) fn new(work: PathBuf, task: Task, drive: Burner, rendering: bool) -> BurnJob {
        let job = BurnJob {
            work,
            task,
            drive,
            started: Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
            shared: Arc::new(Mutex::new(Shared {
                step: Step::Rendering,
                fraction: 0.0,
            })),
        };
        if !rendering {
            job.go_on();
        }
        job
    }

    /// A job at `step` (screen tests: nothing runs).
    pub(crate) fn at_step(task: Task, drive: Burner, step: Step, fraction: f32) -> BurnJob {
        BurnJob {
            work: std::env::temp_dir().join("BananaV burn review"),
            task,
            drive,
            started: Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
            shared: Arc::new(Mutex::new(Shared { step, fraction })),
        }
    }

    pub(crate) fn step(&self) -> (Step, f32) {
        let s = self.shared.lock().expect("burn");
        (s.step.clone(), s.fraction)
    }

    fn set(&self, step: Step, fraction: f32) {
        *self.shared.lock().expect("burn") = Shared { step, fraction };
    }

    /// Follows the render; once it is done, the rest goes on a thread.
    pub(crate) fn follow(&mut self, app: &mut BananaApp) {
        if self.step().0 != Step::Rendering {
            return;
        }
        let Some(p) = app.engine.render_progress() else {
            self.set(Step::Canceled, 0.0);
            return;
        };
        app.engine.show_render_video(
            self.task.dvd() && !app.settings.render.hide_render_video && !p.finished,
        );
        if !p.finished {
            self.set(Step::Rendering, p.fraction());
            return;
        }
        app.engine.clear_render();
        if p.cancelled || self.cancel.load(Ordering::Relaxed) {
            self.set(Step::Canceled, 0.0);
            return;
        }
        if let Some(e) = p.error {
            self.set(Step::Failed(BurnError::Failed(e)), 0.0);
            return;
        }
        self.go_on();
    }

    /// The disc made on a thread.
    fn go_on(&self) {
        let first = if self.task.dvd() {
            Step::Imaging
        } else {
            Step::Burning
        };
        self.set(first, 0.0);
        let (work, task, drive) = (self.work.clone(), self.task.clone(), self.drive.id.clone());
        let (cancel, shared) = (self.cancel.clone(), self.shared.clone());
        std::thread::spawn(move || {
            let result = make(&work, &task, &drive, &cancel, &shared);
            let mut s = shared.lock().expect("burn");
            s.step = match result {
                Ok(()) => Step::Done,
                Err(_) if cancel.load(Ordering::Relaxed) => Step::Canceled,
                Err(e) => Step::Failed(e),
            };
        });
    }

    pub(crate) fn cancel(&self, app: &BananaApp) {
        self.cancel.store(true, Ordering::Relaxed);
        if self.step().0 == Step::Rendering {
            app.engine.cancel_render();
        }
    }

    /// Draws the job's window; `false` once it is closed.
    pub(crate) fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        self.follow(app);
        let (step, fraction) = self.step();
        let running = matches!(step, Step::Rendering | Step::Imaging | Step::Burning);
        if running {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let mut close = false;
        crate::floating::Window::new(self.task.title())
            .id(egui::Id::new(JOB_ID))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new(self.what(&step)).color(theme::text_strong()));
                if running {
                    ui.add(egui::ProgressBar::new(fraction).show_percentage());
                    let elapsed =
                        crate::render::progress::clock(self.started.elapsed().as_secs_f64());
                    ui.label(RichText::new(trf!("Elapsed: {}", elapsed)).color(theme::text_dim()));
                }
                if let Step::Failed(e) = &step {
                    ui.add(
                        egui::Label::new(RichText::new(failure_text(e)).color(theme::error()))
                            .wrap(),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if running {
                        let asked = self.cancel.load(Ordering::Relaxed);
                        if ui
                            .add_enabled(
                                !asked,
                                egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)),
                            )
                            .clicked()
                        {
                            self.cancel(app);
                        }
                    } else {
                        close = ui
                            .add(egui::Button::new("OK").min_size(vec2(72.0, 0.0)))
                            .clicked();
                    }
                });
            });
        if close {
            self.log(&step);
            // Only the job's own temporary folder.
            let _ = std::fs::remove_dir_all(&self.work);
        }
        !close
    }

    /// The window's line for `step`.
    fn what(&self, step: &Step) -> String {
        let dvd = self.task.dvd();
        match (step, &self.task) {
            (Step::Rendering, _) if dvd => tr("Rendering the project for the DVD…").to_string(),
            (Step::Rendering, _) => tr("Rendering the project's sound for the CD…").to_string(),
            (Step::Imaging, _) => tr("Making the disc image…").to_string(),
            (Step::Burning, Task::CloseCd { .. }) => {
                trf!("Closing the disc in {}…", self.drive.name)
            }
            (Step::Burning, Task::EraseCd) => trf!("Erasing the disc in {}…", self.drive.name),
            (Step::Burning, _) => trf!("Burning the disc in {}…", self.drive.name),
            (Step::Done, Task::CloseCd { .. }) => tr("The disc was closed.").to_string(),
            (Step::Done, Task::EraseCd) => tr("The disc was erased.").to_string(),
            (Step::Done, _) if dvd => tr("The DVD was burned.").to_string(),
            (Step::Done, _) => tr("The CD was burned.").to_string(),
            (Step::Canceled, _) => tr("Burning was canceled.").to_string(),
            (Step::Failed(_), _) if dvd => tr("The DVD could not be burned.").to_string(),
            (Step::Failed(_), _) => tr("The CD could not be burned.").to_string(),
        }
    }

    fn log(&self, step: &Step) {
        let how = match step {
            Step::Done => "done".to_string(),
            Step::Canceled => "canceled".to_string(),
            Step::Failed(e) => format!("failed: {e:?}"),
            _ => return,
        };
        crate::logs::log(
            crate::logs::LogKind::Renders,
            &[format!("{} {how} ({})", self.task.title(), self.drive.name)],
        );
    }
}

/// Why the disc was not burned, in the interface's language.
fn failure_text(e: &BurnError) -> String {
    match e {
        BurnError::Drive => tr("The drive is gone, or it cannot write discs.").to_string(),
        BurnError::NoDisc => tr("There is no disc the drive can write in it.").to_string(),
        BurnError::NotBlank => tr("The disc in the drive is not blank.").to_string(),
        BurnError::TooBig { needed, free } => trf!(
            "The project needs {} sectors on the disc; the disc has {}.",
            crate::fmt::grouped(*needed),
            crate::fmt::grouped(*free)
        ),
        BurnError::Canceled => tr("Burning was canceled.").to_string(),
        BurnError::Failed(text) => trf!("Error: {}", text),
    }
}

/// A writer that tells how far the image is and stops once canceled.
struct Counting<W: Write> {
    inner: W,
    written: u64,
    total: u64,
    cancel: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("canceled"));
        }
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        if let Ok(mut s) = self.shared.lock() {
            s.fraction = (self.written as f64 / self.total.max(1) as f64).min(1.0) as f32;
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Where a Wave file's sound is: its data chunk's offset and length.
pub(crate) fn wave_data(path: &Path) -> io::Result<(u64, u64)> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head)?;
    if &head[..4] != b"RIFF" || &head[8..] != b"WAVE" {
        return Err(io::Error::other("not a Wave file"));
    }
    let mut at = 12u64;
    loop {
        let mut chunk = [0u8; 8];
        f.read_exact(&mut chunk)?;
        let len = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
        if &chunk[..4] == b"data" {
            return Ok((at + 8, len));
        }
        at += 8 + len + (len & 1);
        io::Seek::seek(&mut f, io::SeekFrom::Start(at))?;
    }
}

/// The disc made from what the render left in `work`.
fn make(
    work: &Path,
    task: &Task,
    drive: &str,
    cancel: &Arc<AtomicBool>,
    shared: &Arc<Mutex<Shared>>,
) -> Result<(), BurnError> {
    let failed = |e: io::Error| BurnError::Failed(e.to_string());
    let to = shared.clone();
    let progress: Arc<dyn Fn(f32) + Send + Sync> = Arc::new(move |f| {
        if let Ok(mut s) = to.lock() {
            s.fraction = f;
        }
    });
    let sound = work.join("CD.wav");
    match task {
        Task::Dvd {
            label,
            speed,
            eject,
        } => {
            let folder = work.join("DVD");
            let image = work.join("DVD.iso");
            let total = std::fs::read_dir(folder.join("VIDEO_TS"))
                .map_err(failed)?
                .flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum();
            let file = File::create(&image).map_err(failed)?;
            let mut out = Counting {
                inner: BufWriter::with_capacity(1 << 20, file),
                written: 0,
                total,
                cancel: cancel.clone(),
                shared: shared.clone(),
            };
            bv_engine::write_dvd_image(&folder, label, &mut out).map_err(failed)?;
            drop(out);
            *shared.lock().expect("burn") = Shared {
                step: Step::Burning,
                fraction: 0.0,
            };
            bv_platform::burn_image(
                drive,
                &image,
                (*speed > 0).then_some(*speed),
                *eject,
                progress,
                cancel.clone(),
            )
        }
        Task::CdTrack { close, options } => {
            let (offset, len) = wave_data(&sound).map_err(failed)?;
            bv_platform::burn_audio_track(
                drive,
                &sound,
                offset,
                len,
                *close,
                *options,
                progress,
                cancel.clone(),
            )
        }
        Task::CdDisc {
            tracks,
            first_track,
            catalog,
            options,
        } => {
            let (offset, len) = wave_data(&sound).map_err(failed)?;
            let sector = bv_platform::CD_SECTOR;
            let data: Vec<CdTrackData> = tracks
                .iter()
                .map(|t| {
                    let start = (t.start * sector).min(len);
                    CdTrackData {
                        file: sound.clone(),
                        offset: offset + start,
                        len: (t.sectors * sector).min(len - start),
                        indices: t.indices.clone(),
                        isrc: t.isrc.clone(),
                        protect: t.protect,
                        emphasis: t.emphasis,
                    }
                })
                .collect();
            bv_platform::burn_audio_disc(
                drive,
                &data,
                *first_track,
                catalog,
                *options,
                progress,
                cancel.clone(),
            )
        }
        Task::CloseCd { eject } => bv_platform::close_disc(drive, *eject),
        Task::EraseCd => bv_platform::erase_disc(drive),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wave_file_s_sound_is_found_past_its_chunks() {
        let path = std::env::temp_dir().join(format!("bv-wave-data-{}.wav", std::process::id()));
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF\0\0\0\0WAVE");
        b.extend_from_slice(b"fmt \x10\0\0\0");
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        b.extend_from_slice(b"data\x08\0\0\0");
        b.extend_from_slice(&[1; 8]);
        std::fs::write(&path, &b).unwrap();
        assert_eq!(wave_data(&path).unwrap(), (56, 8));
        let _ = std::fs::remove_file(&path);
    }
}
