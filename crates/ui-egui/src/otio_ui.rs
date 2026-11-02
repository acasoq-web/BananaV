//! File › Import and File › Export › OpenTimelineIO, and File › Open of
//! its files: `.otio` documents and OpenTimelineIO's file bundles —
//! `.otioz`, a zip, and `.otiod`, a folder, of the document
//! (`content.otio`), the bundle's version (`version.txt`) and the media its
//! clips use (`media/`), where the document's references point. A
//! bundle's media are copied, or its zip unpacked (into a folder beside
//! it), on a thread of their own; the status bar shows how far, with
//! Cancel.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bv_core::FrameRate;
use bv_engine::Engine;
use bv_timeline::otio::{self, MediaRef, MediaRefSource};
use bv_timeline::{ImageSequence, MediaId, MediaInfo, MediaSource, Project};

use crate::app::{BananaApp, Pending};
use crate::egui;
use crate::i18n::tr;

/// A bundle's document, version file (`.otioz`'s) and media folder, and
/// its version, as OpenTimelineIO's bundles have them.
const CONTENT: &str = "content.otio";
const VERSION_FILE: &str = "version.txt";
const VERSION: &str = "1.0.0";
const MEDIA: &str = "media";
/// Files are copied a mebibyte at a time.
const CHUNK: usize = 1 << 20;

/// How far a bundle's work is, and how it ended.
#[derive(Debug, Default)]
struct Progress {
    done: u64,
    total: u64,
    finished: Option<Result<Finished, String>>,
}

#[derive(Debug)]
enum Finished {
    /// A bundle written; how many media files were not found (their
    /// references stay where they were).
    Packed {
        path: PathBuf,
        missing: usize,
    },
    /// A bundle unpacked: its document, to open.
    Unpacked(PathBuf),
    Canceled,
}

/// A bundle being packed or unpacked, on a thread of its own.
#[derive(Debug)]
pub(crate) struct Job {
    /// What the status bar says it does.
    label: String,
    state: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// The thread's side of a job: the progress it tells, the Cancel it hears.
struct Work {
    state: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
}

impl Work {
    fn total(&self, n: u64) {
        if let Ok(mut s) = self.state.lock() {
            s.total = n;
        }
    }

    fn advance(&self, n: u64) {
        if let Ok(mut s) = self.state.lock() {
            s.done += n;
        }
    }

    /// An error once Cancel was pressed (not `Interrupted`: reading and
    /// writing whole buffers retry those).
    fn go_on(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(io::Error::other("canceled"))
        } else {
            Ok(())
        }
    }
}

impl Job {
    fn start(
        label: String,
        job: impl FnOnce(&Work) -> io::Result<Finished> + Send + 'static,
    ) -> Job {
        let state = Arc::new(Mutex::new(Progress::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let work = Work {
            state: state.clone(),
            cancel: cancel.clone(),
        };
        let thread = std::thread::spawn(move || {
            let result = match job(&work) {
                Err(_) if work.cancel.load(Ordering::Relaxed) => Ok(Finished::Canceled),
                r => r.map_err(|e| e.to_string()),
            };
            if let Ok(mut s) = work.state.lock() {
                s.finished = Some(result);
            }
        });
        Job {
            label,
            state,
            cancel,
            thread: Some(thread),
        }
    }
}

impl Drop for Job {
    /// BananaV closing mid-way: the work stops and takes back what it
    /// wrote.
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A reader that stops at Cancel, telling the progress.
struct Watched<'a, R: Read> {
    inner: R,
    work: &'a Work,
}

impl<R: Read> Read for Watched<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.work.go_on()?;
        let n = self.inner.read(buf)?;
        self.work.advance(n as u64);
        Ok(n)
    }
}

/// A writer that stops at Cancel, telling the progress.
struct Counted<'a, W: Write> {
    inner: W,
    work: &'a Work,
}

impl<W: Write> Write for Counted<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.work.go_on()?;
        let n = self.inner.write(buf)?;
        self.work.advance(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The files and folders a job made (only those), to take back when it
/// stops.
#[derive(Default)]
struct Made {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl Made {
    /// Makes `dir` and the folders above it that are missing.
    fn dir(&mut self, dir: &Path) -> io::Result<()> {
        let mut missing = Vec::new();
        let mut d = Some(dir);
        while let Some(x) = d {
            if x.as_os_str().is_empty() || x.exists() {
                break;
            }
            missing.push(x.to_path_buf());
            d = x.parent();
        }
        for x in missing.into_iter().rev() {
            fs::create_dir(&x)?;
            self.dirs.push(x);
        }
        Ok(())
    }

    /// Makes file `path` — never one already there — and writes it.
    fn file(
        &mut self,
        path: &Path,
        write: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
    ) -> io::Result<()> {
        let f = File::options().write(true).create_new(true).open(path)?;
        self.files.push(path.to_path_buf());
        let mut out = BufWriter::with_capacity(CHUNK, f);
        write(&mut out)?;
        out.flush()
    }

    fn take_back(&self) {
        for f in self.files.iter().rev() {
            let _ = fs::remove_file(f);
        }
        // Only emptied ones go.
        for d in self.dirs.iter().rev() {
            let _ = fs::remove_dir(d);
        }
    }
}

fn copy(from: &mut impl Read, to: &mut impl Write, work: &Work) -> io::Result<()> {
    let mut buf = vec![0; CHUNK];
    loop {
        work.go_on()?;
        let n = match from.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buf[..n])?;
        work.advance(n as u64);
    }
}

fn extension(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Whether a bundle is being packed or unpacked (said, if so).
fn busy(app: &mut BananaApp) -> bool {
    let busy = app.otio_job.is_some();
    if busy {
        app.report_error(tr(
            "An OpenTimelineIO bundle is still being packed or unpacked.",
        ));
    }
    busy
}

// ---- export ---------------------------------------------------------------------

/// What a bundle holds besides its document: each media file's address in
/// it (`media/…`, by its path), the copies to make, how many files were
/// not found.
#[derive(Debug, Default)]
struct Plan {
    urls: HashMap<String, String>,
    copies: Vec<(PathBuf, String)>,
    missing: usize,
}

/// A name not taken yet (whatever the case, as Windows sees names):
/// `clip.mov`, `clip (2).mov`…
fn unique(taken: &mut HashSet<String>, name: &str) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| match n {
            1 => name.to_string(),
            n => format!("{stem} ({n}){ext}"),
        })
        .find(|c| taken.insert(c.to_lowercase()))
        .expect("a name is free")
}

/// The media files the project's events use, in a bundle: each in
/// `media/` (an image sequence's pictures in a folder of their own there).
fn plan(p: &Project) -> Plan {
    let used: HashSet<MediaId> = p.clips().map(|(_, c)| c.media).collect();
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut plan = Plan::default();
    let mut taken = HashSet::new();
    for m in p.media.iter().filter(|m| used.contains(&m.id)) {
        let MediaSource::File { path } = &m.source else {
            continue;
        };
        if plan.urls.contains_key(path) {
            continue;
        }
        match m.sequence {
            Some(seq) => {
                let frames: Option<Vec<String>> = (0..seq.count)
                    .map(|i| ImageSequence::path(path, i))
                    .collect();
                let Some(frames) = frames.filter(|f| f.iter().all(|f| Path::new(f).is_file()))
                else {
                    plan.missing += 1;
                    continue;
                };
                let stem = Path::new(path)
                    .file_stem()
                    .map_or_else(|| "sequence".into(), |s| s.to_string_lossy().into_owned());
                let folder = unique(&mut taken, &stem);
                for f in &frames {
                    let inside = format!("{MEDIA}/{folder}/{}", name(f));
                    plan.copies.push((PathBuf::from(f), inside));
                }
                let first = format!("{MEDIA}/{folder}/{}", name(path));
                plan.urls.insert(path.clone(), otio::relative_url(&first));
            }
            None if Path::new(path).is_file() => {
                let inside = format!("{MEDIA}/{}", unique(&mut taken, &name(path)));
                plan.urls.insert(path.clone(), otio::relative_url(&inside));
                plan.copies.push((PathBuf::from(path), inside));
            }
            None => plan.missing += 1,
        }
    }
    plan
}

/// File › Export › OpenTimelineIO: a document, or a bundle with the media
/// its events use, as the name's extension says.
pub(crate) fn export(app: &mut BananaApp) {
    if busy(app) {
        return;
    }
    let name = crate::interchange_ui::project_name(app);
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Export OpenTimelineIO"))
        .add_filter("OpenTimelineIO (*.otio)", &["otio"])
        .add_filter(tr("OpenTimelineIO Bundle (*.otioz)"), &["otioz"])
        .add_filter(tr("OpenTimelineIO Folder Bundle (*.otiod)"), &["otiod"])
        .set_file_name(format!("{name}.otio"))
        .save_file()
    else {
        return;
    };
    match extension(&path).as_str() {
        "otioz" => pack(app, path, &name, true),
        "otiod" => pack(app, path, &name, false),
        _ => {
            let text = otio::export(app.project(), &name);
            match fs::write(&path, text) {
                Ok(()) => app.notify(trf!("Exported to {}", path.display())),
                Err(e) => app.report_save_error(&path, &e),
            }
        }
    }
}

/// Starts writing a bundle (`zipped`: `.otioz`; else the folder `.otiod`).
fn pack(app: &mut BananaApp, path: PathBuf, name: &str, zipped: bool) {
    if !zipped && path.exists() {
        app.report_error(trf!(
            "{} already exists: choose another name.",
            path.display()
        ));
        return;
    }
    let plan = plan(app.project());
    let url = |p: &str| plan.urls.get(p).cloned();
    let text = otio::export_with(app.project(), name, &url);
    let Plan {
        copies, missing, ..
    } = plan;
    let label = trf!("Packing {}", file_name(&path));
    app.otio_job = Some(Job::start(label, move |work| {
        let total = copies
            .iter()
            .filter_map(|(f, _)| fs::metadata(f).ok())
            .map(|m| m.len())
            .sum();
        work.total(total);
        if zipped {
            write_zip(&path, &text, &copies, work)?;
        } else {
            write_folder(&path, &text, &copies, work)?;
        }
        Ok(Finished::Packed { path, missing })
    }));
}

/// `path` with `.part` added: where a file is written before it takes its
/// name.
fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// The bundle as a zip, as OpenTimelineIO's specification draws it (the
/// document and the version compressed, the media stored). It is written
/// beside its place first: a bundle already there stays whole until the
/// new one is.
fn write_zip(path: &Path, text: &str, copies: &[(PathBuf, String)], work: &Work) -> io::Result<()> {
    let part = part_path(path);
    let written = (|| -> io::Result<()> {
        let out = BufWriter::with_capacity(CHUNK, File::create(&part)?);
        let mut zip = bv_zip::ZipWriter::new(out);
        zip.add_deflated(CONTENT, text.as_bytes())?;
        zip.add_deflated(VERSION_FILE, VERSION.as_bytes())?;
        for (from, inside) in copies {
            let file = File::open(from)?;
            let size = file.metadata()?.len();
            let mut input = Watched {
                inner: BufReader::with_capacity(CHUNK, file),
                work,
            };
            zip.add_stored(inside, &mut input, size, &mut |_| {})?;
        }
        let out = zip.finish()?;
        out.into_inner().map_err(|e| e.into_error())?.sync_all()
    })();
    match written {
        Ok(()) => fs::rename(&part, path),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

/// The bundle as a folder (its document and media: no version file); what
/// it wrote goes if it stops.
fn write_folder(
    path: &Path,
    text: &str,
    copies: &[(PathBuf, String)],
    work: &Work,
) -> io::Result<()> {
    let mut made = Made::default();
    let written = (|| -> io::Result<()> {
        made.dir(path)?;
        made.file(&path.join(CONTENT), |f| f.write_all(text.as_bytes()))?;
        for (from, inside) in copies {
            let to = inside
                .split('/')
                .fold(path.to_path_buf(), |p, part| p.join(part));
            if let Some(parent) = to.parent() {
                made.dir(parent)?;
            }
            made.file(&to, |out| {
                let mut input = BufReader::with_capacity(CHUNK, File::open(from)?);
                copy(&mut input, out, work)
            })?;
        }
        Ok(())
    })();
    if written.is_err() {
        made.take_back();
    }
    written
}

// ---- import ---------------------------------------------------------------------

/// File › Import › OpenTimelineIO (after the question about unsaved
/// changes).
pub(crate) fn import_dialog(app: &mut BananaApp) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Import OpenTimelineIO"))
        .add_filter("OpenTimelineIO (*.otio, *.otioz)", &["otio", "otioz"])
        .add_filter(tr("All Files"), &["*"])
        .pick_file()
    else {
        return;
    };
    open(app, path);
}

/// Opens a timeline as a new project: an OpenTimelineIO document or
/// bundle (a `.otioz` is unpacked into a folder beside it first; a
/// `.otiod`, a folder, opens its `content.otio`), Final Cut Pro 7's XML or
/// FCPXML.
pub(crate) fn open(app: &mut BananaApp, path: PathBuf) {
    if busy(app) {
        return;
    }
    let ext = extension(&path);
    if path.is_dir() {
        open_document(app, &path.join(CONTENT));
    } else if ext == "otioz" {
        unpack(app, path);
    } else if ext == "xml" || ext == "fcpxml" {
        import_fcp_file(app, &path, ext == "fcpxml");
    } else {
        open_document(app, &path);
    }
}

/// A folder for a zip's contents beside it, named after it: `Edit`, or
/// `Edit (2)`… when that is taken.
fn unpack_folder(zip: &Path) -> PathBuf {
    let dir = zip.parent().unwrap_or(Path::new("."));
    let stem = zip
        .file_stem()
        .map_or_else(|| "bundle".into(), |s| s.to_string_lossy().into_owned());
    (1..)
        .map(|n| match n {
            1 => dir.join(&stem),
            n => dir.join(format!("{stem} ({n})")),
        })
        .find(|p| !p.exists())
        .expect("a name is free")
}

fn unpack(app: &mut BananaApp, zip: PathBuf) {
    let folder = unpack_folder(&zip);
    let label = trf!("Unpacking {}", file_name(&zip));
    app.otio_job = Some(Job::start(label, move |work| {
        let mut made = Made::default();
        let result = extract(&zip, &folder, work, &mut made);
        if result.is_err() {
            made.take_back();
        }
        result.map(Finished::Unpacked)
    }));
}

/// Unpacks a bundle into `folder` (never outside it); its document.
fn extract(zip: &Path, folder: &Path, work: &Work, made: &mut Made) -> io::Result<PathBuf> {
    let input = BufReader::with_capacity(CHUNK, File::open(zip)?);
    let mut archive = bv_zip::ZipReader::new(input)?;
    let entries = archive.entries().to_vec();
    work.total(entries.iter().map(|e| e.size).sum());
    made.dir(folder)?;
    for (i, e) in entries.iter().enumerate() {
        let parts = bv_zip::safe_parts(&e.name).ok_or_else(|| {
            io::Error::other(trf!("The bundle has a file outside its folder: {}", e.name))
        })?;
        let to = parts
            .iter()
            .fold(folder.to_path_buf(), |p, part| p.join(part));
        if e.is_dir() {
            made.dir(&to)?;
            continue;
        }
        if let Some(parent) = to.parent() {
            made.dir(parent)?;
        }
        made.file(&to, |out| {
            archive.extract(i, &mut Counted { inner: out, work }, &mut |_| {})
        })?;
    }
    let content = folder.join(CONTENT);
    if content.is_file() {
        Ok(content)
    } else {
        Err(io::Error::other(tr("The bundle has no content.otio.")))
    }
}

/// Where a media file is: where the document says (a relative path from
/// its folder), else by its name beside the document or in its `media`
/// folder (a timeline moved with its media).
fn locate(dir: &Path, path: &str) -> PathBuf {
    let given = Path::new(path);
    let full: PathBuf = if given.is_absolute() || path.starts_with("//") || path.starts_with(r"\\")
    {
        given.components().collect()
    } else {
        dir.join(given).components().collect()
    };
    if full.exists() {
        return full;
    }
    if let Some(name) = given.file_name() {
        for near in [dir.join(name), dir.join(MEDIA).join(name)] {
            if near.is_file() {
                return near;
            }
        }
    }
    full
}

/// Finds and probes a media file of the document (`rate`: an image
/// sequence's, when the document gives none).
fn find(r: &mut MediaRef, dir: &Path, rate: FrameRate) -> Option<MediaInfo> {
    match &mut r.source {
        MediaRefSource::File(p) => {
            *p = locate(dir, p).to_string_lossy().into_owned();
            Engine::probe_file(p).ok()
        }
        MediaRefSource::Sequence {
            first,
            count,
            rate: own,
        } => {
            *first = locate(dir, first).to_string_lossy().into_owned();
            Engine::probe_sequence(first, *count, own.unwrap_or(rate)).ok()
        }
        // A file left out of a bundle: where BananaV saved it was, if it
        // is there.
        MediaRefSource::Missing => {
            let saved = r.saved.as_ref()?;
            let MediaSource::File { path } = &saved.source else {
                return None;
            };
            let at = locate(dir, path);
            if !at.exists() {
                return None;
            }
            let at = at.to_string_lossy().into_owned();
            let info = match saved.sequence {
                Some(seq) => {
                    let rate = saved.video.map_or(rate, |v| v.frame_rate);
                    Engine::probe_sequence(&at, seq.count, rate)
                }
                None => Engine::probe_file(&at),
            }
            .ok()?;
            r.source = match saved.sequence {
                Some(seq) => MediaRefSource::Sequence {
                    first: at,
                    count: seq.count,
                    rate: saved.video.map(|v| v.frame_rate),
                },
                None => MediaRefSource::File(at),
            };
            Some(info)
        }
        MediaRefSource::Generated(_) => None,
    }
}

/// Opens an OpenTimelineIO document as a new, unsaved project.
fn open_document(app: &mut BananaApp, path: &Path) {
    let read = fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| otio::read(&text));
    match read {
        Ok(doc) => open_doc(app, doc, path),
        Err(e) => app.report_error(trf!("Could not open the timeline: {}", e)),
    }
}

/// File › Import › Final Cut Pro 7/DaVinci Resolve (*.xml) or Final Cut
/// Pro X (*.fcpxml): the file's sequence as a new, unsaved project, what
/// could not come in reported.
pub(crate) fn import_fcp(app: &mut BananaApp, fcpxml: bool) {
    let (title, filter, ext) = if fcpxml {
        (tr("Import Final Cut Pro X"), "FCPXML (*.fcpxml)", "fcpxml")
    } else {
        (
            tr("Import XML (Final Cut Pro 7 / DaVinci Resolve)"),
            "XML (*.xml)",
            "xml",
        )
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter, &[ext])
        .add_filter(tr("All Files"), &["*"])
        .pick_file()
    else {
        return;
    };
    import_fcp_file(app, &path, fcpxml);
}

/// Imports Final Cut Pro 7 XML (or FCPXML) file `path`.
pub(crate) fn import_fcp_file(app: &mut BananaApp, path: &Path, fcpxml: bool) {
    let read = fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| {
            if fcpxml {
                bv_timeline::fcp_import::read_fcpxml(&text)
            } else {
                bv_timeline::xmeml::read(&text)
            }
        });
    match read {
        Ok(doc) => open_doc(app, doc, path),
        Err(e) => app.report_error(trf!("Could not import: {}", e)),
    }
}

/// Opens a timeline document read from `path` as a new, unsaved project
/// (its media found from its folder), saying what did not come in.
fn open_doc(app: &mut BananaApp, mut doc: otio::Document, path: &Path) {
    let dir = path.parent().unwrap_or(Path::new("."));
    let base = app.blank_project().settings;
    let found: Vec<Option<MediaInfo>> = doc
        .media
        .iter_mut()
        .map(|r| find(r, dir, base.frame_rate))
        .collect();
    let imported = match otio::build(&doc, &found, &base) {
        Ok(imported) => imported,
        Err(e) => {
            app.report_error(trf!("Could not open the timeline: {}", e));
            return;
        }
    };
    let events = imported.project.clips().count();
    app.open_imported(imported.project);
    app.settings.touch_recent(path);
    let mut message = trn!(
        events,
        "Timeline opened: {} event.",
        "Timeline opened: {} events.",
        events
    );
    let missing = imported.missing.len();
    if missing > 0 {
        let mut names = imported.missing[..missing.min(3)].join(", ");
        if missing > 3 {
            names.push_str(", …");
        }
        message.push_str(" · ");
        message.push_str(&trn!(
            missing,
            "{} media file was not found: {}.",
            "{} media files were not found: {}.",
            missing,
            names
        ));
    }
    if imported.skipped > 0 {
        message.push_str(" · ");
        message.push_str(&trn!(
            imported.skipped,
            "{} item could not be brought in.",
            "{} items could not be brought in.",
            imported.skipped
        ));
    }
    if missing > 0 || imported.skipped > 0 {
        app.report_error(message);
    } else {
        app.notify(message);
    }
}

// ---- the job, each frame -------------------------------------------------------

/// Follows a bundle's work: when it ends, says so (or opens the document
/// unpacked, after the question about unsaved changes).
pub(crate) fn follow(app: &mut BananaApp, ctx: &egui::Context) {
    let Some(job) = &app.otio_job else {
        return;
    };
    let finished = job.state.lock().ok().and_then(|mut s| s.finished.take());
    let Some(result) = finished else {
        ctx.request_repaint_after(Duration::from_millis(200));
        return;
    };
    app.otio_job = None;
    match result {
        Ok(Finished::Packed { path, missing: 0 }) => {
            app.notify(trf!("Exported to {}", path.display()))
        }
        Ok(Finished::Packed { path, missing }) => app.report_error(trn!(
            missing,
            "Exported to {} ({} media file was not found: it stays outside the bundle).",
            "Exported to {} ({} media files were not found: they stay outside the bundle).",
            path.display(),
            missing
        )),
        Ok(Finished::Unpacked(content)) => app.request(Pending::OpenPath(content)),
        Ok(Finished::Canceled) => app.notify(tr("Canceled.")),
        Err(e) => app.report_error(trf!("OpenTimelineIO: {}", e)),
    }
}

/// The status bar's part while a bundle is packed or unpacked: Cancel, how
/// far, what.
pub(crate) fn status(app: &BananaApp, ui: &mut egui::Ui) {
    let Some(job) = &app.otio_job else {
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

    /// Titles imported from Final Cut Pro's files are BananaV's Titles &
    /// Text.
    #[test]
    fn imported_titles_are_titles_and_text() {
        assert_eq!(
            bv_timeline::fcp_import::TITLES_PLUGIN,
            bv_engine::plugins::TITLES_ID
        );
    }

    fn work() -> Work {
        Work {
            state: Arc::new(Mutex::new(Progress::default())),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A WAV file of `samples` stereo samples, each byte `value`.
    fn wav(samples: u32, value: u8) -> Vec<u8> {
        let data = vec![value; samples as usize * 4];
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&48_000u32.to_le_bytes());
        v.extend_from_slice(&192_000u32.to_le_bytes());
        v.extend_from_slice(&4u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(&data);
        v
    }

    /// A project's bundle, written as a zip and as a folder, opens again
    /// with its media inside it; a file not found is left out, and found
    /// again where it was.
    #[test]
    fn bundles_carry_their_media() {
        let dir = std::env::temp_dir().join(format!("bv-otio-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        // Two files of the same name in two folders, and one missing.
        let one = dir.join("a").join("clip.wav");
        let two = dir.join("b").join("clip.wav");
        fs::write(&one, wav(750, 1)).unwrap();
        fs::write(&two, wav(1250, 2)).unwrap();
        let mut p = Project::default();
        let a = p.tracks[1].id;
        let add = |p: &mut Project, path: &Path| {
            let mut m = MediaInfo::generated(
                bv_timeline::Generator::TestTone {
                    frequency_hz: 440.0,
                    level_db: -6.0,
                },
                &p.settings,
            );
            m.source = MediaSource::File {
                path: path.to_string_lossy().into_owned(),
            };
            m.duration = bv_core::Tick::from_seconds(1);
            let id = p.add_media(m);
            p.insert_media(id, bv_core::Tick::ZERO, None, Some(a))
                .unwrap();
        };
        add(&mut p, &one);
        add(&mut p, &two);
        add(&mut p, &dir.join("gone.wav"));

        let plan = plan(&p);
        assert_eq!(plan.missing, 1);
        let inside: Vec<&str> = plan.copies.iter().map(|(_, i)| i.as_str()).collect();
        assert_eq!(inside, ["media/clip.wav", "media/clip (2).wav"]);
        let url = |path: &str| plan.urls.get(path).cloned();
        let text = otio::export_with(&p, "Bundle", &url);
        assert!(
            text.contains("\"target_url\": \"media/clip%20%282%29.wav\""),
            "{text}"
        );
        assert!(text.contains("MissingReference.1"), "the file not found");

        // The zip, unpacked beside it.
        let zip = dir.join("Bundle.otioz");
        let w = work();
        write_zip(&zip, &text, &plan.copies, &w).unwrap();
        assert!(!part_path(&zip).exists());
        assert_eq!(w.state.lock().unwrap().done, 8088);
        let folder = unpack_folder(&zip);
        assert_eq!(folder, dir.join("Bundle"));
        let content = extract(&zip, &folder, &work(), &mut Made::default()).unwrap();
        assert_eq!(
            fs::read(folder.join("media").join("clip (2).wav")).unwrap(),
            wav(1250, 2)
        );
        assert_eq!(
            fs::read_to_string(folder.join("version.txt")).unwrap(),
            "1.0.0"
        );
        let mut doc = otio::read(&fs::read_to_string(&content).unwrap()).unwrap();
        let rate = FrameRate::FPS_30;
        let found: Vec<bool> = doc
            .media
            .iter_mut()
            .map(|r| find(r, &folder, rate).is_some())
            .collect();
        assert_eq!(found, [true, true, false]);
        let inside = |r: &MediaRef| matches!(&r.source, MediaRefSource::File(p) if Path::new(p).starts_with(&folder));
        assert!(
            inside(&doc.media[0]) && inside(&doc.media[1]),
            "{:?}",
            doc.media
        );
        assert_eq!(doc.media[2].source, MediaRefSource::Missing);
        // Back where it was: found there.
        let gone = dir.join("gone.wav");
        fs::write(&gone, wav(10, 3)).unwrap();
        assert!(find(&mut doc.media[2], &folder, rate).is_some());
        assert!(matches!(&doc.media[2].source, MediaRefSource::File(p) if Path::new(p) == gone));
        // Taken: the next one is another folder.
        assert_eq!(unpack_folder(&zip), dir.join("Bundle (2)"));

        // The folder bundle; never over one already there.
        let otiod = dir.join("Bundle.otiod");
        write_folder(&otiod, &text, &plan.copies, &work()).unwrap();
        assert_eq!(
            fs::read(otiod.join("media").join("clip.wav")).unwrap(),
            wav(750, 1)
        );
        assert!(!otiod.join("version.txt").exists(), "only the zip's");
        assert!(write_folder(&otiod, &text, &plan.copies, &work()).is_err());
        assert!(otiod.join("content.otio").is_file(), "what was there stays");

        // Canceled: what it wrote goes.
        let w = work();
        w.cancel.store(true, Ordering::Relaxed);
        let gone = dir.join("Canceled.otiod");
        assert!(write_folder(&gone, &text, &plan.copies, &w).is_err());
        assert!(!gone.exists());
        let gone = dir.join("Canceled.otioz");
        assert!(write_zip(&gone, &text, &plan.copies, &w).is_err());
        assert!(!gone.exists() && !part_path(&gone).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// The demo project out and back in through File › Open: the same
    /// events, in a new project not saved yet.
    #[test]
    fn a_timeline_opens_as_a_new_project() {
        let dir = std::env::temp_dir().join(format!("bv-otio-open-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let before = app.project().clone();
        let path = dir.join("Demo.otio");
        fs::write(&path, otio::export(&before, "Demo")).unwrap();
        app.open_file(path);
        let after = app.project();
        assert_eq!(after.tracks.len(), before.tracks.len());
        assert_eq!(after.clips().count(), before.clips().count());
        let shown = |p: &Project| -> Vec<_> {
            p.clips()
                .map(|(t, c)| {
                    let media = p.media(c.media).map(|m| m.name.clone());
                    let what = (c.start, c.duration, c.source_in, c.fade_in, c.fade_out);
                    (
                        t.name.clone(),
                        media,
                        what,
                        c.transition,
                        c.effects.clone(),
                        c.level,
                    )
                })
                .collect()
        };
        assert_eq!(shown(after), shown(&before));
        assert_eq!(after.markers.len(), before.markers.len());
        assert_eq!(after.regions.len(), before.regions.len());
        assert!(app.project_path.is_none());
        assert!(app.is_dirty(), "a new project, to be saved");
        let _ = fs::remove_dir_all(&dir);
    }

    /// File › Import's Final Cut Pro 7 XML and FCPXML: the sequence as a
    /// new, unsaved project; files not found stay offline.
    #[test]
    fn final_cut_pro_files_open_as_new_projects() {
        let dir = std::env::temp_dir().join(format!("bv-fcp-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // A media file of the folder (a WAV) and one that is gone.
        let tone = dir.join("tone.wav");
        fs::write(&tone, wav(4800, 64)).unwrap();
        let wav_url = bv_timeline::interchange::file_url(&tone.display().to_string());
        let gone_url =
            bv_timeline::interchange::file_url(&dir.join("gone.wav").display().to_string());
        let xml = format!(
            "<?xml version=\"1.0\"?><xmeml version=\"5\"><sequence><name>S</name><rate><timebase>25</timebase></rate>\
             <media><audio><track>\
             <clipitem id=\"a\"><start>0</start><end>5</end><in>0</in><out>5</out><file id=\"f1\"><name>tone.wav</name><pathurl>{wav_url}</pathurl></file></clipitem>\
             <clipitem id=\"b\"><start>10</start><end>20</end><in>0</in><out>10</out><file id=\"f2\"><name>gone.wav</name><pathurl>{gone_url}</pathurl></file></clipitem>\
             </track></audio></media></sequence></xmeml>"
        );
        let path = dir.join("Cut.xml");
        fs::write(&path, xml).unwrap();
        let mut app = BananaApp::new(egui::Context::default());
        import_fcp_file(&mut app, &path, false);
        let p = app.project();
        assert_eq!(p.clips().count(), 2);
        assert_eq!(p.settings.frame_rate, bv_core::FrameRate::new(25, 1));
        assert!(app.project_path.is_none() && app.is_dirty());
        // File › Open opens it too.
        app.open_imported(app.blank_project());
        app.open_path(path.clone());
        assert_eq!(app.project().clips().count(), 2);
        // FCPXML too; a broken file is reported, the project left as it was.
        let fcpxml = format!(
            "<fcpxml version=\"1.10\"><resources><format id=\"r1\" frameDuration=\"1/25s\"/>\
             <asset id=\"r2\" name=\"tone\" src=\"{wav_url}\" hasAudio=\"1\" hasVideo=\"0\"/></resources>\
             <library><event><project name=\"P\"><sequence format=\"r1\"><spine>\
             <asset-clip ref=\"r2\" offset=\"0s\" start=\"0s\" duration=\"1/5s\"/></spine></sequence></project></event></library></fcpxml>"
        );
        let path = dir.join("Cut.fcpxml");
        fs::write(&path, fcpxml).unwrap();
        import_fcp_file(&mut app, &path, true);
        assert_eq!(app.project().clips().count(), 1);
        fs::write(&path, "<fcpxml><broken").unwrap();
        import_fcp_file(&mut app, &path, true);
        assert_eq!(app.project().clips().count(), 1, "left as it was");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A hostile bundle's file outside its folder: refused, nothing left.
    #[test]
    fn a_bundle_cannot_write_outside_its_folder() {
        let dir = std::env::temp_dir().join(format!("bv-otio-hostile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip = dir.join("evil.otioz");
        let mut w = bv_zip::ZipWriter::new(File::create(&zip).unwrap());
        w.add_deflated("content.otio", b"{}").unwrap();
        w.add_deflated("../outside.txt", b"no").unwrap();
        w.finish().unwrap();
        let folder = dir.join("evil");
        let mut made = Made::default();
        assert!(extract(&zip, &folder, &work(), &mut made).is_err());
        made.take_back();
        assert!(!dir.join("outside.txt").exists());
        assert!(!folder.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
