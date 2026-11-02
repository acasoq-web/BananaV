//! Finding plug-ins: every binary is described in a process of its own (a
//! plug-in that crashes or hangs there costs that process only), and what
//! binaries said is kept in a cache until they change.
//!
//! The child is BananaV itself (or the `bv-ofx-scan` tool), started with
//! `--ofx-scan describe <binary> <answer.json>`; [`child_main`] does its
//! part.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use bv_core::ImageRgbaF32;
use serde::{Deserialize, Serialize};

use crate::effect::ClipSource;
use crate::info::{Context, Error, PluginInfo};
use crate::instance::{Instance, RenderRequest, Setup};
use crate::interact::View;
use crate::plugin::{Binary, Plugin, bundle_binaries, find_bundles};
use crate::suites::{Message, Severity, missing_suites, recent_messages};
use crate::value::{Animation, ParamType, Value};

/// The argument that starts a child.
pub const CHILD_FLAG: &str = "--ofx-scan";

/// What a binary holds, or why it could not be described.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BinaryScan {
    pub file: PathBuf,
    /// The file's modification time (seconds) and size when described.
    pub modified: u64,
    pub size: u64,
    pub result: Result<Vec<PluginInfo>, Error>,
    /// Plug-ins of the binary that failed (index, identifier, error).
    pub failures: Vec<(u32, String, Error)>,
    /// Suites the plug-ins asked for and BananaV lacks.
    pub missing_suites: Vec<(String, i32)>,
    /// What they said while being described.
    pub messages: Vec<String>,
}

fn stamp(file: &Path) -> (u64, u64) {
    std::fs::metadata(file)
        .map(|m| {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            (t, m.len())
        })
        .unwrap_or((0, 0))
}

fn message_text(m: &Message) -> String {
    let kind = match m.severity {
        Severity::Fatal => "fatal",
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Message => "message",
        Severity::Log => "log",
        Severity::Question => "question",
    };
    format!("{kind}: {}", m.text)
}

/// Describes a binary in this process.
pub fn scan_here(file: &Path) -> BinaryScan {
    let (modified, size) = stamp(file);
    let loaded = Binary::load(file).and_then(|b| {
        let found = b.plugins()?;
        if !b.crashed() {
            return Ok(found);
        }
        // What it said after crashing is no answer.
        let why = crate::guard::take_crashes().pop().map_or_else(
            || "while being described".to_string(),
            |c| format!("{}, exception {:#010x} at {}", c.doing, c.code, c.at),
        );
        Err(Error::Crashed(why))
    });
    let (result, failures) = match loaded {
        Ok((plugins, failures)) => {
            for p in &plugins {
                p.unload();
            }
            (
                Ok(plugins.iter().map(|p| p.info.clone()).collect()),
                failures
                    .into_iter()
                    .map(|f| (f.index, f.identifier, f.error))
                    .collect(),
            )
        }
        Err(e) => (Err(e), Vec::new()),
    };
    BinaryScan {
        file: file.to_path_buf(),
        modified,
        size,
        result,
        failures,
        missing_suites: missing_suites(),
        messages: recent_messages().iter().map(message_text).collect(),
    }
}

/// Runs `exe` (then `args`) and waits for it, `timeout` at most: whether
/// it ended well, or how not (it crashed, or it hung and was stopped).
fn run_child(mut cmd: Command, timeout: Duration) -> Result<(), Error> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| Error::Other(e.to_string()))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(Error::Crashed(format!("it ended with {status}"))),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Hung(format!("no answer in {} s", timeout.as_secs())));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(Error::Other(e.to_string())),
        }
    }
}

/// A file for a child's answer.
/// `path` from the root (children run in another folder).
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn answer_file(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bananav-ofx-{tag}-{}-{n}.json", std::process::id()))
}

/// The program that describes plug-ins in its own process: `exe` with
/// `args` before the scan's own (BananaV passes `--ofx-scan` there).
#[derive(Clone, Debug)]
pub struct Scanner {
    pub exe: PathBuf,
    pub args: Vec<String>,
    /// Folders the child searches for the libraries plug-ins need.
    pub library_folders: Vec<PathBuf>,
    /// Environment variables for the child.
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

impl Scanner {
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.args(&self.args);
        // Some plug-ins write files where they run (G'MIC's Intarsia its
        // pattern): the system's temporary folder, not the user's.
        cmd.current_dir(std::env::temp_dir());
        cmd.envs(self.env.iter().map(|(k, v)| (k, v)));
        if !self.library_folders.is_empty() {
            let var = if cfg!(windows) {
                "PATH"
            } else {
                "LD_LIBRARY_PATH"
            };
            let mut paths = self.library_folders.clone();
            if let Some(old) = std::env::var_os(var) {
                paths.extend(std::env::split_paths(&old));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                cmd.env(var, joined);
            }
        }
        cmd
    }

    /// Describes `file` in a child — again, twice at most, when it
    /// crashed: some crash only now and then.
    pub fn describe(&self, file: &Path) -> BinaryScan {
        let mut scan = self.describe_once(file);
        for _ in 0..2 {
            if !matches!(scan.result, Err(Error::Crashed(_))) {
                break;
            }
            scan = self.describe_once(file);
        }
        scan
    }

    /// Describes `file` in a child, once.
    fn describe_once(&self, file: &Path) -> BinaryScan {
        let answer = answer_file("describe");
        let mut cmd = self.command();
        cmd.arg(CHILD_FLAG)
            .arg("describe")
            .arg(absolute(file))
            .arg(&answer);
        let outcome = run_child(cmd, self.timeout);
        let read = std::fs::read(&answer)
            .ok()
            .and_then(|b| serde_json::from_slice::<BinaryScan>(&b).ok());
        let _ = std::fs::remove_file(&answer);
        match (outcome, read) {
            (Ok(()), Some(scan)) => scan,
            (outcome, _) => {
                let (modified, size) = stamp(file);
                let error = outcome
                    .err()
                    .unwrap_or_else(|| Error::Crashed("no answer".into()));
                BinaryScan {
                    file: file.to_path_buf(),
                    modified,
                    size,
                    result: Err(error),
                    failures: Vec::new(),
                    missing_suites: Vec::new(),
                    messages: Vec::new(),
                }
            }
        }
    }

    /// Tries plug-in `index` of `file` in a child (see [`try_plugin`]),
    /// its pictures written to `out_dir`.
    pub fn try_plugin(&self, file: &Path, index: u32, out_dir: &Path) -> Trial {
        let answer = answer_file("try");
        let mut cmd = self.command();
        // A trial's files go with its pictures.
        let out_dir = absolute(out_dir);
        cmd.current_dir(&out_dir);
        cmd.arg(CHILD_FLAG)
            .arg("try")
            .arg(absolute(file))
            .arg(index.to_string())
            .arg(&out_dir)
            .arg(&answer);
        let outcome = run_child(cmd, self.timeout);
        let read = std::fs::read(&answer)
            .ok()
            .and_then(|b| serde_json::from_slice::<Trial>(&b).ok());
        let _ = std::fs::remove_file(&answer);
        match (outcome, read) {
            (Ok(()), Some(t)) => t,
            (outcome, _) => Trial {
                identifier: String::new(),
                steps: vec![Step {
                    what: "run".into(),
                    result: Err(outcome
                        .err()
                        .unwrap_or_else(|| Error::Crashed("no answer".into()))),
                }],
                pictures: Vec::new(),
                messages: Vec::new(),
                missing_suites: Vec::new(),
            },
        }
    }
}

/// Plug-ins described before, by binary.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cache {
    pub binaries: Vec<BinaryScan>,
}

impl Cache {
    pub fn load(path: &Path) -> Cache {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Compact: one binary of hundreds of plug-ins (Sapphire's) is tens
        // of megabytes of descriptions.
        let json = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// The binaries in `folders`' bundles, described: from the cache when
    /// unchanged, by `describe` otherwise. Binaries no longer there leave
    /// the cache. Whether it changed (worth saving: one binary of hundreds
    /// of plug-ins, Sapphire's, is tens of megabytes).
    pub fn refresh(
        &mut self,
        folders: &[PathBuf],
        mut describe: impl FnMut(&Path) -> BinaryScan,
    ) -> bool {
        let mut files = Vec::new();
        for f in folders {
            for b in find_bundles(f) {
                files.extend(bundle_binaries(&b));
            }
        }
        files.sort();
        files.dedup();
        let old = std::mem::take(&mut self.binaries);
        let mut changed = old.len() != files.len();
        for file in files {
            let (modified, size) = stamp(&file);
            // A crash may not come again: described anew.
            let known = old.iter().find(|s| {
                s.file == file
                    && s.modified == modified
                    && s.size == size
                    && !matches!(s.result, Err(Error::Crashed(_)))
            });
            changed |= known.is_none();
            self.binaries.push(match known {
                Some(s) => s.clone(),
                None => describe(&file),
            });
        }
        changed
    }

    /// Every plug-in described well, by identifier (the newest version of
    /// each).
    pub fn plugins(&self) -> Vec<&PluginInfo> {
        let mut out: Vec<&PluginInfo> = Vec::new();
        for p in self
            .binaries
            .iter()
            .filter_map(|b| b.result.as_ref().ok())
            .flatten()
        {
            match out.iter_mut().find(|q| q.identifier == p.identifier) {
                Some(q) if q.version < p.version => *q = p,
                Some(_) => {}
                None => out.push(p),
            }
        }
        out
    }
}

/// One step of a trial and how it went.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub what: String,
    pub result: Result<(), Error>,
}

/// How a plug-in did when tried: created, changed, rendered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trial {
    pub identifier: String,
    pub steps: Vec<Step>,
    /// The pictures it rendered (raw RGBA floats, see [`write_raw`]).
    pub pictures: Vec<PathBuf>,
    pub messages: Vec<String>,
    pub missing_suites: Vec<(String, i32)>,
}

impl Trial {
    pub fn passed(&self) -> bool {
        self.steps.iter().all(|s| s.result.is_ok())
    }
}

/// A test card: color bars over a gray ramp, opaque.
pub fn test_card(width: u32, height: u32) -> ImageRgbaF32 {
    const BARS: [[f32; 3]; 7] = [
        [0.75, 0.75, 0.75],
        [0.75, 0.75, 0.0],
        [0.0, 0.75, 0.75],
        [0.0, 0.75, 0.0],
        [0.75, 0.0, 0.75],
        [0.75, 0.0, 0.0],
        [0.0, 0.0, 0.75],
    ];
    let mut img = ImageRgbaF32::transparent(width, height);
    for y in 0..height {
        for x in 0..width {
            let px = if y < height * 2 / 3 {
                let bar = BARS[(x * 7 / width.max(1)) as usize % 7];
                [bar[0], bar[1], bar[2], 1.0]
            } else {
                let v = x as f32 / (width.max(2) - 1) as f32;
                [v, v, v, 1.0]
            };
            *img.pixel_mut(x, y) = px;
        }
    }
    img
}

/// Writes a picture as raw floats: "BVRGBA32F", width and height (u32
/// little-endian), then the pixels, top row first.
pub fn write_raw(path: &Path, img: &ImageRgbaF32) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(17 + img.data.len() * 16);
    out.extend_from_slice(b"BVRGBA32F");
    out.extend_from_slice(&img.width.to_le_bytes());
    out.extend_from_slice(&img.height.to_le_bytes());
    for px in &img.data {
        for c in px {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    std::fs::write(path, out)
}

/// Reads a picture [`write_raw`] wrote.
pub fn read_raw(path: &Path) -> Option<ImageRgbaF32> {
    let b = std::fs::read(path).ok()?;
    let head = b.get(..17)?;
    if &head[..9] != b"BVRGBA32F" {
        return None;
    }
    let w = u32::from_le_bytes(head[9..13].try_into().ok()?);
    let h = u32::from_le_bytes(head[13..17].try_into().ok()?);
    let mut img = ImageRgbaF32::transparent(w, h);
    let body = &b[17..];
    if body.len() < img.data.len() * 16 {
        return None;
    }
    for (i, px) in img.data.iter_mut().enumerate() {
        for (c, v) in px.iter_mut().enumerate() {
            let at = i * 16 + c * 4;
            *v = f32::from_le_bytes(body[at..at + 4].try_into().ok()?);
        }
    }
    Some(img)
}

struct Card(Arc<ImageRgbaF32>);

impl ClipSource for Card {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        (self.0.width as f64, self.0.height as f64)
    }
}

/// Tries plug-in `index` of `file` here: describes it, creates an instance
/// in its first context, feeds it the test card, renders a frame at its
/// defaults, changes each numeric parameter as a user would and renders
/// again; pictures go to `out_dir`.
pub fn try_plugin(file: &Path, index: u32, out_dir: &Path) -> Trial {
    let mut trial = Trial {
        identifier: String::new(),
        steps: Vec::new(),
        pictures: Vec::new(),
        messages: Vec::new(),
        missing_suites: Vec::new(),
    };
    let finish = |mut t: Trial| {
        t.messages = recent_messages().iter().map(message_text).collect();
        t.missing_suites = missing_suites();
        t
    };
    let plugin = match Binary::load(file).and_then(|b| b.plugin(index)) {
        Ok(p) => p,
        Err(e) => {
            trial.steps.push(Step {
                what: "load".into(),
                result: Err(e),
            });
            return finish(trial);
        }
    };
    let trial = tried(&plugin, out_dir, trial);
    plugin.unload();
    finish(trial)
}

/// [`try_plugin`]'s trial of `plugin`, noted in `trial`.
fn tried(plugin: &Arc<Plugin>, out_dir: &Path, mut trial: Trial) -> Trial {
    trial.identifier = plugin.info.identifier.clone();
    let contexts = plugin.info.contexts.clone();
    let (w, h) = (640, 360);
    let setup = Setup {
        frame_rate: 30.0,
        duration: 150.0,
        size: (w as f64, h as f64),
        pixel_aspect: 1.0,
        interactive: false,
    };
    let instance = match Instance::new_in(plugin, &contexts, &setup) {
        Ok(i) => {
            trial.steps.push(Step {
                what: format!("create ({:?})", i.context()),
                result: Ok(()),
            });
            i
        }
        Err(e) => {
            trial.steps.push(Step {
                what: format!("create ({:?})", contexts[0]),
                result: Err(e),
            });
            return trial;
        }
    };
    let context = instance.context();
    let card: Arc<dyn ClipSource> = Arc::new(Card(Arc::new(test_card(w, h))));
    feed(&instance, &card);
    let request = RenderRequest {
        time: 0.0,
        width: w,
        height: h,
        scale: 1.0,
        draft: false,
        interactive: false,
        sequential: false,
        timeline: None,
    };
    let render = |instance: &Instance, what: &str, trial: &mut Trial| {
        rendered(instance, &request, out_dir, what, trial);
    };
    render(&instance, "defaults", &mut trial);
    // Its on-screen controls, drawn with the draw suite (one drawing with
    // OpenGL itself draws nothing here).
    if instance.has_overlay() {
        let view = View {
            time: 0.0,
            render_scale: 1.0,
            pixel_scale: [1.0, 1.0],
            background: [0.0; 3],
            viewport: [w as i32, h as i32],
            to_frame: Some([[1.0 / w as f64, 0.0, 0.0], [0.0, -1.0 / h as f64, 1.0]]),
        };
        let marks = instance.overlay_draw(&view);
        let drawn = match marks.as_slice() {
            [crate::interact::Mark::Picture { rgba, .. }] => format!(
                "with OpenGL, {} pixels",
                rgba.as_chunks::<4>().0.iter().filter(|p| p[3] != 0).count()
            ),
            _ => format!("{} marks", marks.len()),
        };
        trial.steps.push(Step {
            what: format!("draw its overlay ({drawn})"),
            result: Ok(()),
        });
    }
    // Each number a user could change, a quarter of its range away.
    let mut changed = Vec::new();
    for p in instance.param_infos() {
        if !matches!(p.ty, ParamType::Double | ParamType::Integer) || p.secret || !p.enabled {
            continue;
        }
        // Its display range, or its range when that one has no bounds.
        let bounded = |min: &[f64], max: &[f64]| {
            let (lo, hi) = (*min.first()?, *max.first()?);
            (lo.is_finite() && hi.is_finite() && hi > lo && hi - lo <= 1e6).then_some((lo, hi))
        };
        let Some((lo, hi)) =
            bounded(&p.display_min, &p.display_max).or_else(|| bounded(&p.min, &p.max))
        else {
            continue;
        };
        let v = p.default.num(0) + (hi - lo) * 0.25;
        let v = if v > hi {
            p.default.num(0) - (hi - lo) * 0.25
        } else {
            v
        };
        let v = if p.ty == ParamType::Integer {
            v.round()
        } else {
            v
        };
        instance.set_values(&[(p.name.clone(), Animation::constant(Value::Num(vec![v])))]);
        changed.push(p.name);
    }
    if !changed.is_empty() {
        let _ = instance.changed(&changed, crate::instance::ChangeReason::User, 0.0);
        trial.steps.push(Step {
            what: format!("change {} parameters", changed.len()),
            result: Ok(()),
        });
        render(&instance, "changed", &mut trial);
    }
    drop(instance);
    // As a transition and as a generator too, when it is one: BananaV lists
    // it among those.
    for extra in [Context::Transition, Context::Generator] {
        if extra == context || !contexts.contains(&extra) {
            continue;
        }
        match Instance::new(plugin, extra, &setup) {
            Ok(instance) => {
                feed(&instance, &card);
                render(&instance, &format!("{extra:?}"), &mut trial);
            }
            Err(e) => trial.steps.push(Step {
                what: format!("create ({extra:?})"),
                result: Err(e),
            }),
        }
    }
    trial
}

/// The test card on every input of `instance`; a transition halfway.
fn feed(instance: &Instance, card: &Arc<dyn ClipSource>) {
    for clip in instance.effect().clips() {
        if !clip.is_output() {
            instance.connect(&clip.name, Arc::clone(card));
        }
    }
    if instance.context() == Context::Transition {
        instance.set_values(&[(
            "Transition".into(),
            Animation::constant(Value::Num(vec![0.5])),
        )]);
    }
}

/// `instance` rendered for `request`, its picture written to `out_dir`
/// and the step noted in `trial` as `what`.
fn rendered(
    instance: &Instance,
    request: &RenderRequest,
    out_dir: &Path,
    what: &str,
    trial: &mut Trial,
) {
    let result = instance.render(request).and_then(|img| {
        let name = format!(
            "{}-{}.rgba",
            trial
                .identifier
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
            what
        );
        let path = out_dir.join(name);
        write_raw(&path, &img).map_err(|e| Error::Other(e.to_string()))?;
        trial.pictures.push(path);
        Ok(())
    });
    trial.steps.push(Step {
        what: format!("render {what}"),
        result,
    });
}

/// The child's part: `args` are what followed [`CHILD_FLAG`]. Returns the
/// process's exit code.
pub fn child_main(args: &[String]) -> i32 {
    #[cfg(windows)]
    end_with_the_child();
    // On a thread with a plug-in's stack (the main thread's is small).
    let args = args.to_vec();
    std::thread::Builder::new()
        .name("bv-ofx-child".into())
        .stack_size(crate::PLUGIN_STACK)
        .spawn(move || child_work(&args))
        .ok()
        .and_then(|t| t.join().ok())
        .unwrap_or(3)
}

fn child_work(args: &[String]) -> i32 {
    let answer = |path: &str, json: Result<Vec<u8>, serde_json::Error>| match json {
        Ok(j) if std::fs::write(path, &j).is_ok() => 0,
        _ => 2,
    };
    match args {
        [mode, file, out] if mode == "describe" => {
            let scan = scan_here(Path::new(file));
            answer(out, serde_json::to_vec(&scan))
        }
        [mode, file, index, dir, out] if mode == "try" => {
            let index = index.parse().unwrap_or(0);
            let trial = try_plugin(Path::new(file), index, Path::new(dir));
            answer(out, serde_json::to_vec(&trial))
        }
        _ => 1,
    }
}

/// Programs the plug-ins start from this child (Sapphire's preset browser
/// waits in the background) end with it, even when they missed the unload
/// action: the child joins a job of its own that ends its processes once
/// it is gone. Those that ask to leave the job may.
#[cfg(windows)]
#[allow(unsafe_code)]
fn end_with_the_child() {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        per_process_user_time: i64,
        per_job_user_time: i64,
        flags: u32,
        min_working_set: usize,
        max_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }
    const KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const BREAKAWAY_OK: u32 = 0x0800;
    const EXTENDED_LIMIT_INFORMATION: i32 = 9;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            info: *const c_void,
            len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
    }
    let limits = ExtendedLimits {
        basic: BasicLimits {
            flags: KILL_ON_JOB_CLOSE | BREAKAWAY_OK,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: plain calls with a structure of the size given; the job's
    // handle stays open until the process ends, which closes it.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let set = SetInformationJobObject(
            job,
            EXTENDED_LIMIT_INFORMATION,
            &limits as *const ExtendedLimits as *const c_void,
            std::mem::size_of::<ExtendedLimits>() as u32,
        );
        if set != 0 {
            AssignProcessToJobObject(job, GetCurrentProcess());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_pictures_go_and_come_back() {
        let img = test_card(16, 9);
        let path = std::env::temp_dir().join(format!("bananav-raw-{}.rgba", std::process::id()));
        write_raw(&path, &img).unwrap();
        assert_eq!(read_raw(&path), Some(img));
        let _ = std::fs::remove_file(&path);
    }
}
