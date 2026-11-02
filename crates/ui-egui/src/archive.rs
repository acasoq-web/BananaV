//! Project archives and copies of a project's media, as VEGAS's Help
//! describes them ("Archiving projects", "Saving and renaming a project
//! (Save As)"): File › Export › Project Archive writes the project and
//! copies its media files (and their proxies) into the archive's folder,
//! the copy of the project pointing at the copies (with Include nested
//! projects, each nested project's file too, written pointing at its own
//! media's copies); with File › Copy Media
//! with Project on, Save As copies the project's media from outside its
//! new folder into it, the project then pointing at the copies. The files
//! are copied on a thread of their own, the status bar showing how far,
//! with Cancel; what a canceled job copied goes. Save As asks how first
//! (VEGAS's dialog): Copy source media, or Create trimmed copies of source
//! media — the sound files only the parts events use (with an extra head
//! and tail) rendered to WAV files, their events then pointing into them
//! and the events' inactive takes removed; video files, which VEGAS too
//! leaves whole when they are compressed, copied as they are.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bv_core::{Tick, TimeRange};
use bv_timeline::{ImageSequence, MediaId, MediaInfo, MediaSource, Project};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;

/// Files are copied a mebibyte at a time.
const CHUNK: usize = 1 << 20;

/// The Project Archive dialog's choices.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ArchiveOptions {
    /// Include nested projects: each one's file archived too, pointing at
    /// its media's copies.
    pub nested: bool,
    /// Copy the media files with the archive.
    pub media: bool,
    /// Their proxies too.
    pub proxies: bool,
    /// Leave out the media no event uses.
    pub exclude_unused: bool,
}

impl Default for ArchiveOptions {
    fn default() -> ArchiveOptions {
        ArchiveOptions {
            nested: true,
            media: true,
            proxies: false,
            exclude_unused: true,
        }
    }
}

/// What a copy does: files from where to where, and the media paths that
/// change.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CopyPlan {
    pub files: Vec<(PathBuf, PathBuf)>,
    /// Each media whose file is copied, and its new path.
    pub moves: Vec<(MediaId, String)>,
    /// Sound files of which only a part is rendered.
    pub trims: Vec<TrimCopy>,
    /// Media files that were not found (left where they were).
    pub missing: usize,
    pub bytes: u64,
    /// Nested projects archived: where each goes and its text, pointing
    /// at its media's copies.
    pub projects: Vec<(PathBuf, String)>,
}

/// A trimmed copy: the part of a media's sound (media time) written to a
/// WAV file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrimCopy {
    pub info: MediaInfo,
    pub part: TimeRange,
    pub to: PathBuf,
}

impl TrimCopy {
    /// About how many bytes it writes (24-bit samples).
    fn bytes(&self) -> u64 {
        let a = self.info.audio.unwrap_or(bv_timeline::AudioStreamInfo {
            sample_rate: bv_core::SampleRate(48_000),
            channels: 2,
        });
        let per_second = u64::from(a.sample_rate.0) * u64::from(a.channels) * 3;
        (self.part.duration.as_seconds_f64() * per_second as f64) as u64
    }
}

/// The media times events play of media `m` (`len` long; its takes'
/// too), from the first to the last; `None` when none does or one goes
/// round it with Loop (all of it is needed).
fn used_span(p: &Project, m: MediaId, len: Tick) -> Option<(Tick, Tick)> {
    let mut span: Option<(Tick, Tick)> = None;
    for (_, c) in p.clips() {
        for (k, take) in c.take_list().iter().enumerate() {
            if take.media != m {
                continue;
            }
            // Another take: its offset from the active one's start.
            let shift = if k == c.active_take() {
                Tick::ZERO
            } else {
                take.offset
            };
            // The media times along the event (its speeds and velocity
            // envelope may go either way).
            let steps = 64;
            for i in 0..=steps {
                let t = c.start + Tick(c.duration.0 * i / steps);
                let s = c.source_time(t) + shift;
                if c.looped && (s < Tick::ZERO || s > len) {
                    return None;
                }
                span = Some(match span {
                    Some((a, b)) => (a.min(s), b.max(s)),
                    None => (s, s),
                });
            }
        }
    }
    span
}

/// The media no event uses (nor a take of one).
fn used_media(p: &Project) -> HashSet<MediaId> {
    let mut used = HashSet::new();
    for (_, c) in p.clips() {
        used.insert(c.media);
        for t in &c.takes {
            used.insert(t.media);
        }
    }
    // A subclip used stands for its parent.
    for s in &p.subclips {
        if used.contains(&s.id) {
            used.insert(s.parent);
        }
    }
    used
}

/// `name` in `folder`, or with " (2)", … when that is taken by another
/// file (or by a copy planned already).
fn free_in(folder: &Path, name: &str, taken: &mut HashSet<PathBuf>) -> PathBuf {
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map_or_else(|| "media".into(), |s| s.to_string_lossy().to_string());
    let ext = p
        .extension()
        .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
    let path = (1..)
        .map(|n| {
            if n == 1 {
                folder.join(name)
            } else {
                folder.join(format!("{stem} ({n}){ext}"))
            }
        })
        .find(|f| !f.exists() && !taken.contains(f))
        .expect("a free name");
    taken.insert(path.clone());
    path
}

/// The copies that put `p`'s media files in `folder`: those already in
/// it (or in a folder below it) stay; `all`: every media, else only those
/// events use; `proxies`: their proxies follow them; `trim`: the sound
/// files' parts events use, with that much more before and after, are
/// rendered instead (Save As's trimmed copies).
pub(crate) fn plan(
    p: &Project,
    folder: &Path,
    all: bool,
    proxies: bool,
    trim: Option<Tick>,
) -> CopyPlan {
    plan_into(p, folder, all, proxies, trim, false, &mut HashSet::new())
}

/// As [`plan`], the names given out in `taken` (a plan of several
/// projects into one folder); `skip_projects` leaves nested projects'
/// files out (the archive's own care).
fn plan_into(
    p: &Project,
    folder: &Path,
    all: bool,
    proxies: bool,
    trim: Option<Tick>,
    skip_projects: bool,
    taken: &mut HashSet<PathBuf>,
) -> CopyPlan {
    let used = used_media(p);
    let mut out = CopyPlan::default();
    for m in &p.media {
        let MediaSource::File { path } = &m.source else {
            continue;
        };
        if !all && !used.contains(&m.id) {
            continue;
        }
        if skip_projects && bv_engine::is_project(path) {
            continue;
        }
        let from = PathBuf::from(path);
        if from.starts_with(folder) {
            continue;
        }
        // An image sequence: each of its pictures.
        let pictures: Vec<PathBuf> = match &m.sequence {
            Some(s) => (0..s.count)
                .filter_map(|i| ImageSequence::path(path, i))
                .map(PathBuf::from)
                .collect(),
            None => vec![from.clone()],
        };
        if !pictures.iter().all(|f| f.is_file()) {
            out.missing += 1;
            continue;
        }
        // A sound file (one stream) events use a part of: that part.
        if let Some(extra) = trim
            && m.video.is_none()
            && m.audio.is_some()
            && m.extra_audio.is_empty()
            && let Some((a, b)) = used_span(p, m.id, m.duration)
        {
            let start = (a - extra).max(Tick::ZERO);
            let end = (b + extra).min(m.duration).max(start);
            let stem = from
                .file_stem()
                .map_or_else(|| "media".into(), |s| s.to_string_lossy().to_string());
            let to = free_in(folder, &format!("{stem}.wav"), taken);
            let t = TrimCopy {
                info: m.clone(),
                part: TimeRange::from_bounds(start, end),
                to,
            };
            out.bytes += t.bytes();
            out.trims.push(t);
            continue;
        }
        let mut first = None;
        for f in &pictures {
            let name = f
                .file_name()
                .map_or_else(|| "media".into(), |n| n.to_string_lossy().to_string());
            let to = free_in(folder, &name, taken);
            out.bytes += std::fs::metadata(f).map_or(0, |m| m.len());
            first.get_or_insert_with(|| to.clone());
            out.files.push((f.clone(), to));
        }
        let Some(first) = first else {
            continue;
        };
        let new = first.to_string_lossy().to_string();
        if proxies {
            let proxy = bv_engine::proxy_path(path);
            if proxy.is_file() {
                out.bytes += std::fs::metadata(&proxy).map_or(0, |m| m.len());
                out.files.push((proxy, bv_engine::proxy_path(&new)));
            }
        }
        out.moves.push((m.id, new));
    }
    out
}

/// The Project Archive's plan of `p` into `folder`: its media as [`plan`]
/// copies them (with Include media), and with Include nested projects
/// each nested project archived beside them — its media copied the same
/// way and its file written pointing at the copies (theirs too, nested
/// deeper); without it, nested projects stay where they are.
pub(crate) fn plan_archive(p: &Project, folder: &Path, o: &ArchiveOptions) -> CopyPlan {
    archive_into(p, folder, o, &mut HashSet::new(), &mut HashMap::new())
}

/// [`plan_archive`] of one project; `done`: the nested projects archived
/// already, and where (one nested twice, or nesting another in a circle,
/// is archived once).
fn archive_into(
    p: &Project,
    folder: &Path,
    o: &ArchiveOptions,
    taken: &mut HashSet<PathBuf>,
    done: &mut HashMap<PathBuf, String>,
) -> CopyPlan {
    let mut out = if o.media {
        plan_into(p, folder, !o.exclude_unused, o.proxies, None, true, taken)
    } else {
        CopyPlan::default()
    };
    if !o.nested {
        return out;
    }
    let used = used_media(p);
    for m in &p.media {
        let MediaSource::File { path } = &m.source else {
            continue;
        };
        if !bv_engine::is_project(path) || (o.exclude_unused && !used.contains(&m.id)) {
            continue;
        }
        let from = PathBuf::from(path);
        if from.starts_with(folder) {
            continue;
        }
        let key = std::fs::canonicalize(&from).unwrap_or_else(|_| from.clone());
        if let Some(to) = done.get(&key) {
            out.moves.push((m.id, to.clone()));
            continue;
        }
        let Ok(nested) = bv_engine::read_project(path) else {
            out.missing += 1;
            continue;
        };
        let name = from.file_name().map_or_else(
            || "nested.bvproj".into(),
            |n| n.to_string_lossy().to_string(),
        );
        let to = free_in(folder, &name, taken);
        let target = to.to_string_lossy().to_string();
        done.insert(key, target.clone());
        let inner = archive_into(&nested, folder, o, taken, done);
        let text = moved(&nested, &inner.moves).to_json();
        out.files.extend(inner.files);
        out.missing += inner.missing;
        out.bytes += inner.bytes;
        out.projects.extend(inner.projects);
        out.projects.push((to, text));
        out.moves.push((m.id, target));
    }
    out
}

/// Writes the nested projects archived.
fn write_projects(projects: &[(PathBuf, String)]) -> io::Result<()> {
    for (path, text) in projects {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)?;
    }
    Ok(())
}

/// `p` with its media at their new paths.
pub(crate) fn moved(p: &Project, moves: &[(MediaId, String)]) -> Project {
    let mut p = p.clone();
    for (id, path) in moves {
        if let Some(m) = p.media.iter_mut().find(|m| m.id == *id) {
            m.source = MediaSource::File { path: path.clone() };
        }
    }
    p
}

/// How far a copy is, and how it ended.
#[derive(Debug, Default)]
struct Progress {
    done: u64,
    total: u64,
    finished: Option<Result<Done, String>>,
}

/// What a finished job leaves to do.
#[derive(Debug)]
enum Done {
    /// An archive written at this path.
    Archived(PathBuf, usize),
    /// Save As's copies made: the media's new paths (and the trimmed
    /// ones' parts), then the project saved again.
    Copied {
        moves: Vec<(MediaId, String)>,
        trims: Vec<(MediaId, String, TimeRange)>,
        missing: usize,
    },
    Canceled,
}

/// Files being copied on a thread of their own.
#[derive(Debug)]
pub(crate) struct CopyJob {
    label: String,
    state: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for CopyJob {
    /// BananaV closing mid-way: the copying stops (and takes back what it
    /// wrote).
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Copies `files`, telling `state` how far and stopping at `cancel`; what
/// it copied goes again when it stops before the end.
fn copy_files(
    files: &[(PathBuf, PathBuf)],
    state: &Mutex<Progress>,
    cancel: &AtomicBool,
) -> io::Result<()> {
    let mut made: Vec<PathBuf> = Vec::new();
    let result = (|| {
        for (from, to) in files {
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut r = std::fs::File::open(from)?;
            made.push(to.clone());
            let mut w = std::fs::File::create(to)?;
            let mut buf = vec![0u8; CHUNK];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(io::Error::other("canceled"));
                }
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                if let Ok(mut s) = state.lock() {
                    s.done += n as u64;
                }
            }
            w.flush()?;
        }
        Ok(())
    })();
    if result.is_err() {
        for f in made {
            let _ = std::fs::remove_file(f);
        }
    }
    result
}

/// Renders the trimmed copies, telling `state` how far and stopping at
/// `cancel`; what it wrote goes again when it stops before the end.
fn trim_files(trims: &[TrimCopy], state: &Mutex<Progress>, cancel: &AtomicBool) -> io::Result<()> {
    let mut made: Vec<&Path> = Vec::new();
    for t in trims {
        if let Some(dir) = t.to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let per_second = t.bytes() as f64 / t.part.duration.as_seconds_f64().max(1e-6);
        let mut tick = |seconds: f64| {
            if let Ok(mut s) = state.lock() {
                s.done += (seconds * per_second) as u64;
            }
            !cancel.load(Ordering::Relaxed)
        };
        let written = bv_engine::trim_sound(&t.info, t.part.start, t.part.end(), &t.to, &mut tick);
        if let Err(e) = written {
            for f in made {
                let _ = std::fs::remove_file(f);
            }
            return Err(io::Error::other(e));
        }
        made.push(&t.to);
    }
    Ok(())
}

/// `p` with the trimmed copies in place: each media `id` at its new
/// `path` and as long as its `part`, its events (and markers) moved
/// back by the part's start; the events' inactive takes removed.
pub(crate) fn retarget(p: &mut Project, trims: &[(MediaId, String, TimeRange)]) {
    for (id, path, part) in trims {
        if let Some(m) = p.media.iter_mut().find(|m| m.id == *id) {
            m.source = MediaSource::File { path: path.clone() };
            m.duration = part.duration;
            m.markers
                .retain(|k| k.time >= part.start && k.time <= part.end());
            for k in &mut m.markers {
                k.time -= part.start;
            }
        }
    }
    for t in &mut p.tracks {
        for c in &mut t.clips {
            c.takes.clear();
            c.active_take = 0;
            let Some((_, _, part)) = trims.iter().find(|(id, _, _)| *id == c.media) else {
                continue;
            };
            // A reversed event mirrors its media times around `reverse`.
            match &mut c.reverse {
                Some(mirror) => *mirror -= part.start,
                None => c.source_in -= part.start,
            }
            if let Some(f) = &mut c.freeze {
                *f -= part.start;
            }
        }
    }
}

impl CopyJob {
    fn start(
        label: String,
        plan: CopyPlan,
        then: impl FnOnce(&CopyPlan) -> io::Result<Done> + Send + 'static,
    ) -> CopyJob {
        let state = Arc::new(Mutex::new(Progress {
            total: plan.bytes,
            ..Progress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (s, c) = (state.clone(), cancel.clone());
        let thread = std::thread::spawn(move || {
            let result = copy_files(&plan.files, &s, &c)
                .and_then(|()| trim_files(&plan.trims, &s, &c))
                .and_then(|()| write_projects(&plan.projects))
                .and_then(|()| then(&plan));
            let result = match result {
                Err(_) if c.load(Ordering::Relaxed) => Ok(Done::Canceled),
                r => r.map_err(|e| e.to_string()),
            };
            if let Ok(mut st) = s.lock() {
                st.finished = Some(result);
            }
        });
        CopyJob {
            label,
            state,
            cancel,
            thread: Some(thread),
        }
    }
}

/// The Project Archive dialog: its two pages' state.
#[derive(Debug)]
pub(crate) struct ArchiveDialog {
    path: String,
    options: ArchiveOptions,
    /// On its second page: what will be copied.
    summary: Option<CopyPlan>,
}

impl BananaApp {
    /// File › Export › Project Archive: its dialog, the archive in a folder
    /// of its own beside the project (or in the documents).
    pub(crate) fn open_archive_dialog(&mut self) {
        let stem = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or_else(
                || tr("Untitled").to_string(),
                |s| s.to_string_lossy().to_string(),
            );
        let base = self
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| bv_platform::user_dir(bv_platform::UserDir::Documents))
            .unwrap_or_else(|| PathBuf::from("."));
        let path = base
            .join(trf!("{} Archive", stem))
            .join(format!("{stem}.bvproj"));
        self.archive_dialog = Some(ArchiveDialog {
            path: path.display().to_string(),
            options: ArchiveOptions::default(),
            summary: None,
        });
    }
}

impl BananaApp {
    /// The Project Archive dialog open on an archive in the system's temp
    /// folder (screen tests: no user's path in the pictures).
    pub fn archive_dialog_for_review(&mut self) {
        self.open_archive_dialog();
        let path = std::env::temp_dir()
            .join("Demo Archive")
            .join("Demo.bvproj");
        if let Some(d) = self.archive_dialog.as_mut() {
            d.path = path.display().to_string();
        }
    }

    /// Save As's Copy Media dialog, trimmed copies chosen with a second
    /// of extra head and tail (screen tests: OK is never pressed).
    pub fn copy_media_dialog_for_review(&mut self) {
        self.archive_dialog = None;
        self.copy_media_dialog = Some(CopyMediaDialog {
            trimmed: true,
            extra: 1.0,
        });
    }
}

/// Whether file `path` keeps a path to the project it was rendered from
/// (read once per file and change, as menus ask each frame).
pub(crate) fn keeps_reference(path: &Path) -> bool {
    use std::collections::HashMap;
    use std::time::SystemTime;
    type Seen = HashMap<PathBuf, (Option<SystemTime>, bool)>;
    static SEEN: std::sync::OnceLock<Mutex<Seen>> = std::sync::OnceLock::new();
    let changed = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let seen = SEEN.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = seen.lock()
        && let Some(&(when, keeps)) = map.get(path)
        && when == changed
    {
        return keeps;
    }
    let keeps = bv_engine::Engine::project_reference(path).is_some();
    if let Ok(mut map) = seen.lock() {
        map.insert(path.to_path_buf(), (changed, keeps));
    }
    keeps
}

/// A size for people.
fn size_text(bytes: u64) -> String {
    crate::fmt::bytes(bytes as f64)
}

/// The Project Archive dialog, while open (VEGAS's, a wizard of two
/// pages): the archive's project file and what goes with it; then what
/// will be copied, and Finish.
pub(crate) fn archive_dialog(ctx: &egui::Context, app: &mut BananaApp) {
    let Some(mut d) = app.archive_dialog.take() else {
        return;
    };
    let (mut open, mut next, mut back, mut finish, mut cancel) = (true, false, false, false, false);
    crate::floating::Window::new(tr("Archive"))
        .id(egui::Id::new("project-archive"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(480.0);
            match &d.summary {
                None => {
                    ui.horizontal(|ui| {
                        ui.label(tr("Project file:"));
                        ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(300.0));
                        if ui.button(tr("Browse…")).clicked() {
                            let name = Path::new(&d.path)
                                .file_name()
                                .map_or_else(String::new, |n| n.to_string_lossy().to_string());
                            if let Some(p) = rfd::FileDialog::new()
                                .set_title(tr("Archive"))
                                .add_filter(tr("BananaV Project"), &["bvproj"])
                                .set_file_name(name)
                                .save_file()
                            {
                                d.path = p.display().to_string();
                            }
                        }
                    });
                    let o = &mut d.options;
                    crate::widgets::checkbox(ui, &mut o.nested, tr("Include nested projects"));
                    crate::widgets::checkbox(ui, &mut o.media, tr("Include media"));
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        ui.add_enabled(
                            o.media,
                            egui::Checkbox::new(
                                &mut o.proxies,
                                tr("Include video proxy/audio peak files"),
                            ),
                        );
                    });
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut false, tr("Include RAW companion files")),
                    )
                    .on_disabled_hover_text(tr("Coming soon to BananaV"));
                    ui.add_enabled(
                        o.media,
                        egui::Checkbox::new(&mut o.exclude_unused, tr("Exclude unused media")),
                    );
                }
                Some(plan) => {
                    let folder = Path::new(&d.path)
                        .parent()
                        .map_or_else(String::new, |p| p.display().to_string());
                    ui.label(trf!("The project is saved as {}.", d.path));
                    if d.options.media {
                        ui.label(trn!(
                            plan.files.len(),
                            "{} file ({}) is copied into {}.",
                            "{} files ({}) are copied into {}.",
                            plan.files.len(),
                            size_text(plan.bytes),
                            folder
                        ));
                    }
                    if !plan.projects.is_empty() {
                        ui.label(trf!("Nested projects archived with it: {}", plan.projects.len()));
                    }
                    if plan.missing > 0 {
                        ui.label(
                            RichText::new(trn!(
                                plan.missing,
                                "{} media file was not found: the archive points where it was.",
                                "{} media files were not found: the archive points where they were.",
                                plan.missing
                            ))
                            .color(crate::theme::error()),
                        );
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(d.summary.is_some(), egui::Button::new(tr("< Back")))
                    .clicked()
                {
                    back = true;
                }
                if d.summary.is_none() {
                    let can = !d.path.trim().is_empty();
                    if ui
                        .add_enabled(can, egui::Button::new(tr("Next >")))
                        .clicked()
                    {
                        next = true;
                    }
                } else if ui.button(RichText::new(tr("Finish")).strong()).clicked() {
                    finish = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if next {
        let mut path = PathBuf::from(d.path.trim());
        if path.extension().is_none() {
            path.set_extension("bvproj");
        }
        d.path = path.display().to_string();
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        d.summary = Some(plan_archive(app.project(), &folder, &d.options));
    }
    if back {
        d.summary = None;
    }
    if finish && let Some(plan) = d.summary.take() {
        start_archive(app, PathBuf::from(&d.path), plan);
        return;
    }
    if open && !cancel {
        app.archive_dialog = Some(d);
    }
}

/// Copies the media of `plan`, then writes the project pointing at them as
/// `path`.
fn start_archive(app: &mut BananaApp, path: PathBuf, plan: CopyPlan) {
    if app.copy_job.is_some() {
        app.notify(tr("Wait for the files being copied, or cancel them."));
        return;
    }
    let text = moved(app.project(), &plan.moves).to_json();
    let missing = plan.missing;
    let label = trf!("Archiving to {}", path.display());
    app.copy_job = Some(CopyJob::start(label, plan, move |_| {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, text)?;
        Ok(Done::Archived(path, missing))
    }));
}

impl BananaApp {
    /// Save As with File › Copy Media with Project on: the project's media
    /// from outside its folder copied into it; the project then points at
    /// the copies and is saved again.
    pub(crate) fn copy_media_with_project(&mut self, trim: Option<Tick>) {
        let Some(folder) = self
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        else {
            return;
        };
        if self.copy_job.is_some() {
            self.notify(tr("Wait for the files being copied, or cancel them."));
            return;
        }
        let plan = plan(self.project(), &folder, true, false, trim);
        if plan.files.is_empty() && plan.trims.is_empty() {
            return;
        }
        let missing = plan.missing;
        let moves = plan.moves.clone();
        let trims = plan
            .trims
            .iter()
            .map(|t| (t.info.id, t.to.display().to_string(), t.part))
            .collect();
        let label = trf!("Copying the media to {}", folder.display());
        self.copy_job = Some(CopyJob::start(label, plan, move |_| {
            Ok(Done::Copied {
                moves,
                trims,
                missing,
            })
        }));
    }
}

/// Save As's Copy Media dialog: copy the media whole, or trimmed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CopyMediaDialog {
    pub trimmed: bool,
    /// Extra head and tail, in seconds.
    pub extra: f64,
}

/// Save As's Copy Media dialog, while open (VEGAS's): Copy source media,
/// or Create trimmed copies of source media with their Extra head and
/// tail; OK copies, Cancel leaves the media where they are.
pub(crate) fn copy_media_dialog(ctx: &egui::Context, app: &mut BananaApp) {
    let Some(mut d) = app.copy_media_dialog.take() else {
        return;
    };
    let (mut open, mut ok, mut cancel) = (true, false, false);
    crate::floating::Window::new(tr("Copy Media"))
        .id(egui::Id::new("copy-media-with-project"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            if crate::widgets::radio(ui, !d.trimmed, tr("Copy source media")).clicked() {
                d.trimmed = false;
            }
            if crate::widgets::radio(ui, d.trimmed, tr("Create trimmed copies of source media"))
                .clicked()
            {
                d.trimmed = true;
            }
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                ui.add_enabled_ui(d.trimmed, |ui| {
                    ui.label(tr("Extra head and tail:"));
                    ui.add(
                        egui::DragValue::new(&mut d.extra)
                            .range(0.0..=600.0)
                            .speed(0.1)
                            .max_decimals(3)
                            .suffix(" s"),
                    );
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("OK").strong()).clicked() {
                    ok = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if ok {
        app.settings.copy_media_trimmed = d.trimmed;
        app.settings.copy_media_extra = d.extra;
        let _ = app.settings.save();
        let trim = d.trimmed.then(|| Tick::from_seconds_f64(d.extra.max(0.0)));
        app.copy_media_with_project(trim);
        return;
    }
    if open && !cancel {
        app.copy_media_dialog = Some(d);
    }
}

/// Follows the copying: when it ends, says so (Save As's copies: the
/// project points at them and is saved again).
pub(crate) fn follow(app: &mut BananaApp, ctx: &egui::Context) {
    let Some(job) = &app.copy_job else {
        return;
    };
    let finished = job.state.lock().ok().and_then(|mut s| s.finished.take());
    let Some(result) = finished else {
        ctx.request_repaint_after(Duration::from_millis(200));
        return;
    };
    app.copy_job = None;
    let missing_note = |n: usize| {
        trn!(
            n,
            "{} media file was not found.",
            "{} media files were not found.",
            n
        )
    };
    match result {
        Ok(Done::Archived(path, missing)) => {
            let msg = trf!("Project archived to {}", path.display());
            if missing > 0 {
                app.report_error(format!("{msg} {}", missing_note(missing)));
            } else {
                app.notify(msg);
            }
        }
        Ok(Done::Copied {
            moves,
            trims,
            missing,
        }) => {
            app.modify(|p| {
                for (id, path) in &moves {
                    if let Some(m) = p.media.iter_mut().find(|m| m.id == *id) {
                        m.source = MediaSource::File { path: path.clone() };
                    }
                }
                if !trims.is_empty() {
                    retarget(p, &trims);
                }
            });
            app.save(false);
            if missing > 0 {
                app.report_error(missing_note(missing));
            }
        }
        Ok(Done::Canceled) => app.notify(tr("Canceled.")),
        Err(e) => app.report_error(trf!("Could not copy the media: {}", e)),
    }
}

/// The status bar's part while files are copied: Cancel, how far, what.
pub(crate) fn status(app: &BananaApp, ui: &mut egui::Ui) {
    let Some(job) = &app.copy_job else {
        return;
    };
    if ui.button(tr("Cancel")).clicked() {
        job.cancel.store(true, Ordering::Relaxed);
    }
    let (done, total) = job.state.lock().map_or((0, 0), |s| (s.done, s.total));
    let f = if total > 0 {
        (done as f32 / total as f32).min(1.0)
    } else {
        0.0
    };
    ui.add(
        egui::ProgressBar::new(f)
            .desired_width(200.0)
            .text(format!("{} %", (f * 100.0).floor())),
    );
    ui.label(&job.label);
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::Tick;
    use bv_timeline::{MediaInfo, ProjectSettings};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bv-archive-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A project of three media files in `src`: two used by events, one
    /// not; and a file already in the archive's folder.
    fn project(src: &Path, archive: &Path) -> (Project, [MediaId; 4]) {
        let mut p = Project::new(ProjectSettings::default());
        let mut ids = Vec::new();
        for (dir, name, bytes) in [
            (src, "a.wav", 10usize),
            (src, "b.wav", 20),
            (src, "unused.wav", 30),
            (archive, "inside.wav", 40),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, vec![7u8; bytes]).unwrap();
            let mut info = MediaInfo::generated(
                bv_timeline::Generator::TestTone {
                    frequency_hz: 440.0,
                    level_db: -6.0,
                },
                &p.settings,
            );
            info.source = MediaSource::File {
                path: path.display().to_string(),
            };
            info.name = name.into();
            ids.push(p.add_media(info));
        }
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Audio)
            .map(|t| t.id);
        for (k, &m) in [ids[0], ids[1], ids[3]].iter().enumerate() {
            p.insert_media(m, Tick::from_seconds(k as i64 * 10), None, track)
                .unwrap();
        }
        (p, [ids[0], ids[1], ids[2], ids[3]])
    }

    #[test]
    fn a_plan_copies_what_is_outside_the_folder_and_used() {
        let (src, dst) = (temp("plan-src"), temp("plan-dst"));
        // A file of that name already there: the copy gets a number.
        std::fs::write(dst.join("a.wav"), b"other").unwrap();
        let (p, [a, b, unused, inside]) = project(&src, &dst);
        let used = plan(&p, &dst, false, false, None);
        let names: Vec<String> = used
            .files
            .iter()
            .map(|(_, to)| to.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, ["a (2).wav", "b.wav"]);
        assert_eq!(used.bytes, 30);
        assert_eq!(used.moves.iter().map(|m| m.0).collect::<Vec<_>>(), [a, b]);
        // All the media: the unused one too; the one inside stays.
        let all = plan(&p, &dst, true, false, None);
        assert!(all.moves.iter().any(|m| m.0 == unused));
        assert!(!all.moves.iter().any(|m| m.0 == inside));
        // A file that is gone is counted, not copied.
        std::fs::remove_file(src.join("b.wav")).unwrap();
        assert_eq!(plan(&p, &dst, false, false, None).missing, 1);
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    /// Save As's trimmed copies: the part of a sound file an event plays,
    /// with the extra head and tail, rendered to a WAV file; the event
    /// then points into it at the same sound.
    #[test]
    fn trimmed_copies_hold_the_parts_events_use() {
        let (src, dst) = (temp("trim-src"), temp("trim-dst"));
        // Ten seconds of a ramp at 8 kHz: each sample says where it is.
        let wav = src.join("voice.wav");
        {
            let f = std::fs::File::create(&wav).unwrap();
            let mut w = bv_wav::WavWriter::new(
                std::io::BufWriter::new(f),
                8000,
                1,
                bv_wav::SampleFormat::Float(32),
            )
            .unwrap();
            let ramp: Vec<f32> = (0..80_000).map(|i| i as f32 / 80_000.0).collect();
            w.write_planar(&[ramp]).unwrap();
            w.finish().unwrap();
        }
        let info = bv_engine::Engine::probe_file(&wav.display().to_string()).unwrap();
        let mut p = Project::new(ProjectSettings::default());
        let m = p.add_media(info);
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Audio)
            .map(|t| t.id);
        p.insert_media(m, Tick::from_seconds(5), None, track)
            .unwrap();
        // The event plays 3 s to 5 s of the file.
        for t in &mut p.tracks {
            for c in &mut t.clips {
                c.source_in = Tick::from_seconds(3);
                c.duration = Tick::from_seconds(2);
            }
        }
        let plan = plan(&p, &dst, false, false, Some(Tick::from_seconds(1)));
        assert!(plan.files.is_empty(), "not copied whole");
        assert_eq!(plan.trims.len(), 1);
        let part = plan.trims[0].part;
        let secs = |t: Tick| t.as_seconds_f64();
        assert!((secs(part.start) - 2.0).abs() < 1e-3 && (secs(part.end()) - 6.0).abs() < 0.01);
        let state = Mutex::new(Progress::default());
        trim_files(&plan.trims, &state, &AtomicBool::new(false)).unwrap();
        let to = &plan.trims[0].to;
        let mut r = bv_wav::WavReader::new(std::fs::File::open(to).unwrap()).unwrap();
        let first = r.read_planar(0, 1).unwrap()[0][0];
        assert!((first - 0.2).abs() < 1e-4, "it starts at 2 s: {first}");
        // The event points into the copy, at the same sound.
        let path = to.display().to_string();
        retarget(&mut p, &[(m, path.clone(), part)]);
        let (_, c) = p.clips().next().unwrap();
        assert!((secs(c.source_in) - 1.0).abs() < 1e-3);
        assert!(matches!(&p.media(m).unwrap().source, MediaSource::File { path: q } if *q == path));
        assert_eq!(p.media(m).unwrap().duration, part.duration);
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn copies_are_made_and_taken_back_when_canceled() {
        let (src, dst) = (temp("copy-src"), temp("copy-dst"));
        let (p, _) = project(&src, &dst);
        let plan = plan(&p, &dst, false, false, None);
        let state = Mutex::new(Progress::default());
        copy_files(&plan.files, &state, &AtomicBool::new(false)).unwrap();
        for (from, to) in &plan.files {
            assert_eq!(std::fs::read(from).unwrap(), std::fs::read(to).unwrap());
        }
        assert_eq!(state.lock().unwrap().done, plan.bytes);
        // The project points at the copies.
        let copy = moved(&p, &plan.moves);
        let MediaSource::File { path } = &copy.media[0].source else {
            panic!("a file")
        };
        assert!(Path::new(path).starts_with(&dst));
        // Canceled: what it copied goes.
        for (_, to) in &plan.files {
            std::fs::remove_file(to).unwrap();
        }
        assert!(copy_files(&plan.files, &state, &AtomicBool::new(true)).is_err());
        assert!(plan.files.iter().all(|(_, to)| !to.exists()));
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn nested_projects_are_archived_with_their_media() {
        let (src, dst) = (temp("nest-src"), temp("nest-dst"));
        // A nested project of the three media, used by the one archived.
        let (inner, _) = project(&src, &dst);
        let nested = src.join("Inner.bvproj");
        std::fs::write(&nested, inner.to_json()).unwrap();
        let mut p = Project::new(ProjectSettings::default());
        let mut info = MediaInfo::generated(
            bv_timeline::Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &p.settings,
        );
        info.source = MediaSource::File {
            path: nested.display().to_string(),
        };
        let id = p.add_media(info);
        let track = p
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Audio)
            .map(|t| t.id);
        p.insert_media(id, Tick::ZERO, None, track).unwrap();
        let o = ArchiveOptions::default();
        let plan = plan_archive(&p, &dst, &o);
        // The nested project's file written beside the copies of the
        // media its events use, pointing at them; never copied as it was.
        let to = dst.join("Inner.bvproj");
        assert_eq!(plan.moves, [(id, to.display().to_string())]);
        assert_eq!(plan.projects.len(), 1);
        assert_eq!(plan.projects[0].0, to);
        let copy = Project::from_json(&plan.projects[0].1).unwrap();
        let in_folder = copy
            .media
            .iter()
            .filter(|m| matches!(&m.source, MediaSource::File { path } if Path::new(path).starts_with(&dst)))
            .count();
        assert_eq!(in_folder, 3, "a, b and inside");
        let copied: Vec<&PathBuf> = plan.files.iter().map(|(from, _)| from).collect();
        assert_eq!(copied, [&src.join("a.wav"), &src.join("b.wav")]);
        // Without Include nested projects it stays where it is.
        let alone = plan_archive(&p, &dst, &ArchiveOptions { nested: false, ..o });
        assert!(alone.projects.is_empty() && alone.moves.is_empty());
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn an_archive_holds_the_project_and_copies_of_its_media() {
        use crate::egui::vec2;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        crate::i18n::set_language(crate::i18n::Language::English);
        let (src, dst) = (temp("ui-src"), temp("ui-dst"));
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        let (p, _) = project(&src, &dst);
        app.modify(|q| *q = p);
        app.open_archive_dialog();
        let archive = dst.join("Archive").join("Show.bvproj");
        if let Some(d) = app.archive_dialog.as_mut() {
            d.path = archive.display().to_string();
        }
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
        h.run_steps(6);
        h.get_by_label("Next >").click();
        h.run_steps(6);
        h.get_by_label("Finish").click();
        h.run_steps(2);
        for _ in 0..200 {
            if h.state().copy_job.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            h.step();
        }
        assert!(h.state().copy_job.is_none(), "the copies finished");
        let folder = archive.parent().unwrap();
        // The three media events use, copied (the archive's own folder is
        // new); the unused one left out.
        for name in ["a.wav", "b.wav", "inside.wav"] {
            assert!(folder.join(name).is_file(), "{name}");
        }
        assert!(!folder.join("unused.wav").exists());
        let saved = Project::from_json(&std::fs::read_to_string(&archive).unwrap()).unwrap();
        let in_folder = saved
            .media
            .iter()
            .filter(|m| {
                matches!(&m.source, MediaSource::File { path } if Path::new(path).starts_with(folder))
            })
            .count();
        assert_eq!(in_folder, 3, "the archive points at its copies");
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }
}
