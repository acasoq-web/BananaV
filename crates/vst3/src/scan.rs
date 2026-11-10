//! Finding VST 3 effects without risk: each module is looked at in a
//! process of its own — loaded, each of its effects made, set up for stereo
//! at 48 kHz, given a block of silence and asked what it has —, so a
//! plug-in that crashes or hangs there costs that process only, never
//! BananaV; several modules are looked at at once, and what they said is
//! kept in a cache until their files change.
//!
//! The child is BananaV itself (or the `bv-vst3-scan` tool), started with
//! `--vst3-scan <module> <answer.json>`, or for a 32-bit module BananaV's
//! 32-bit bridge, where that module runs; [`child_main`] does its part.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::guard::take_crashes;
use crate::instance::Instance;
use crate::module::{ClassInfo, Error, Module, is_32bit, library_32, library_of, load};

/// The argument that starts a child.
pub const CHILD_FLAG: &str = "--vst3-scan";

/// The rate and the block effects are tried at.
const RATE: f64 = 48_000.0;
const BLOCK: usize = 1024;

/// An effect a module holds, as it said and did when tried.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectInfo {
    /// Its class (32 hexadecimal digits).
    pub class_id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    /// "Fx|Delay"…
    pub sub_categories: String,
    /// Its main buses' channels (in, out), set up for stereo.
    pub inputs: u32,
    pub outputs: u32,
    /// Its parameters shown to the user.
    pub params: u32,
    /// Its programs (the presets it holds).
    pub programs: u32,
    /// Its latency (samples at 48 kHz).
    pub latency: u32,
    /// How long its sound lasts after its input stops (samples at 48 kHz;
    /// `None`: for ever).
    pub tail: Option<u32>,
    /// Why it failed when tried: it is listed, never offered.
    pub error: Option<String>,
}

impl EffectInfo {
    pub fn works(&self) -> bool {
        self.error.is_none()
    }
}

/// Why a module could not be looked at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    /// It would not load or start (the reason).
    Refused(String),
    /// Its process crashed (how it ended).
    Crashed(String),
    /// Its process gave no answer in this many seconds.
    Hung(u64),
    /// A 32-bit module and no 32-bit bridge (or 32-bit plug-ins off).
    NeedsBridge,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Refused(why) => write!(f, "{why}"),
            Failure::Crashed(how) => write!(f, "it crashed ({how})"),
            Failure::Hung(s) => write!(f, "no answer in {s} s"),
            Failure::NeedsBridge => write!(f, "a 32-bit plug-in, and 32-bit plug-ins are off"),
        }
    }
}

/// What a module holds, or why it could not be looked at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleScan {
    /// The module (a bundle's folder or a library).
    pub path: PathBuf,
    /// Its library's modification time (seconds) and size when looked at.
    pub modified: u64,
    pub size: u64,
    /// Who made it (its factory's vendor).
    pub vendor: String,
    /// A 32-bit module (it runs in the 32-bit bridge).
    #[serde(default)]
    pub bits32: bool,
    pub result: Result<Vec<EffectInfo>, Failure>,
}

/// The library module `path` runs from: this system's, or a 32-bit one.
fn library(module: &Path) -> PathBuf {
    if is_32bit(module) {
        library_32(module)
    } else {
        library_of(module)
    }
}

/// A module's library's modification time and size.
fn stamp(module: &Path) -> (u64, u64) {
    std::fs::metadata(library(module))
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

/// Tries `class` of `m`, noting in `info` what it has.
fn tried(m: &Arc<Module>, class: &ClassInfo, info: &mut EffectInfo) -> Result<(), Error> {
    let mut inst = Instance::new(m, class, true)?;
    info.params = inst.params().iter().filter(|p| p.shown()).count() as u32;
    info.programs = inst.programs().len() as u32;
    inst.setup(RATE, BLOCK, 2)?;
    let (ins, outs) = inst.main_channels();
    info.inputs = ins as u32;
    info.outputs = outs as u32;
    info.latency = inst.latency();
    info.tail = inst.tail();
    let mut silence = vec![vec![0.0f32; BLOCK]; 2];
    inst.process(&mut silence);
    if inst.crashed() {
        return Err(Error::Crashed);
    }
    Ok(())
}

/// What `class` of `m` is and does, tried here.
fn try_effect(m: &Arc<Module>, class: &ClassInfo) -> EffectInfo {
    let mut info = EffectInfo {
        class_id: class.id(),
        name: class.name.clone(),
        vendor: class.vendor.clone(),
        version: class.version.clone(),
        sub_categories: class.sub_categories.clone(),
        tail: Some(0),
        ..EffectInfo::default()
    };
    if let Err(e) = tried(m, class, &mut info) {
        info.error = Some(match (e, take_crashes().pop()) {
            (Error::Crashed, Some(c)) => format!(
                "it crashed in {} (exception {:#010x} at {})",
                c.doing, c.code, c.at
            ),
            (e, _) => e.to_string(),
        });
    }
    info
}

/// Looks at `module` in this process.
pub fn scan_here(module: &Path) -> ModuleScan {
    let (modified, size) = stamp(module);
    let (vendor, result) = match load(module) {
        Err(e) => (String::new(), Err(Failure::Refused(e.to_string()))),
        Ok(m) => {
            let classes: Vec<ClassInfo> = m.effects().cloned().collect();
            let effects = classes.iter().map(|c| try_effect(&m, c)).collect();
            (m.vendor().to_string(), Ok(effects))
        }
    };
    ModuleScan {
        path: module.to_path_buf(),
        modified,
        size,
        vendor,
        bits32: is_32bit(module),
        result,
    }
}

/// `path` from the root (children run in another folder).
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A file for a child's answer.
fn answer_file() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bananav-vst3-scan-{}-{n}.json", std::process::id()))
}

/// Runs `cmd` and waits for it, `timeout` at most: whether it ended well,
/// or how not (it crashed, or it hung and was stopped).
fn run_child(mut cmd: Command, timeout: Duration) -> Result<(), Failure> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| Failure::Refused(format!("the scan could not start: {e}")))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(Failure::Crashed(format!("it ended with {status}"))),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Hung(timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Err(Failure::Refused(e.to_string())),
        }
    }
}

/// The program that looks at modules in processes of their own: `exe`
/// with `args` before the scan's own (BananaV passes nothing there; its
/// `--vst3-scan` comes with the scan's); `bridge` for 32-bit modules.
#[derive(Clone, Debug)]
pub struct Scanner {
    pub exe: PathBuf,
    pub args: Vec<String>,
    /// BananaV's 32-bit bridge (`None`: 32-bit modules wait for it).
    pub bridge: Option<PathBuf>,
    /// Environment variables for the child.
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

impl Scanner {
    /// BananaV's own: the program running, when it is BananaV (a test's
    /// program is not, and has no scan of its own), and its bridge when
    /// there is one and 32-bit plug-ins are on.
    pub fn bananav(timeout: Duration) -> Option<Scanner> {
        let exe = std::env::current_exe().ok()?;
        let stem = exe.file_stem()?.to_string_lossy().to_lowercase();
        (stem == "bananav").then(|| Scanner {
            exe,
            args: Vec::new(),
            bridge: bv_bridge::enabled().then(bv_bridge::exe).flatten(),
            env: Vec::new(),
            timeout,
        })
    }

    /// Looks at `module` in a child — again, once, when it crashed: some
    /// crash only now and then.
    pub fn describe(&self, module: &Path) -> ModuleScan {
        let scan = self.describe_once(module);
        if matches!(scan.result, Err(Failure::Crashed(_))) {
            return self.describe_once(module);
        }
        scan
    }

    fn describe_once(&self, module: &Path) -> ModuleScan {
        let bits32 = is_32bit(module);
        let mut cmd = if bits32 {
            let Some(bridge) = &self.bridge else {
                return failed(module, true, Failure::NeedsBridge);
            };
            Command::new(bridge)
        } else {
            let mut cmd = Command::new(&self.exe);
            cmd.args(&self.args);
            cmd
        };
        let answer = answer_file();
        cmd.arg(CHILD_FLAG)
            .arg(absolute(module))
            .arg(&answer)
            // Some plug-ins write files where they run: the system's
            // temporary folder, not the user's.
            .current_dir(std::env::temp_dir())
            .envs(self.env.iter().map(|(k, v)| (k, v)));
        let outcome = run_child(cmd, self.timeout);
        let read = std::fs::read(&answer)
            .ok()
            .and_then(|b| serde_json::from_slice::<ModuleScan>(&b).ok());
        let _ = std::fs::remove_file(&answer);
        match (outcome, read) {
            (Ok(()), Some(mut scan)) => {
                scan.path = module.to_path_buf();
                scan.bits32 = bits32;
                scan
            }
            (outcome, _) => failed(
                module,
                bits32,
                outcome
                    .err()
                    .unwrap_or_else(|| Failure::Crashed("no answer".into())),
            ),
        }
    }
}

/// `module` could not be looked at: `why`.
fn failed(module: &Path, bits32: bool, why: Failure) -> ModuleScan {
    let (modified, size) = stamp(module);
    ModuleScan {
        path: module.to_path_buf(),
        modified,
        size,
        vendor: String::new(),
        bits32,
        result: Err(why),
    }
}

/// Modules looked at before.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cache {
    pub modules: Vec<ModuleScan>,
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
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// What the cache knows of `module`, when its library has not changed
    /// since and it did not crash, hang or wait for the bridge then (that
    /// may not come again). A 32-bit module looked at before BananaV ran
    /// them in its bridge is looked at again.
    pub fn known(&self, module: &Path) -> Option<&ModuleScan> {
        let (modified, size) = stamp(module);
        let bits32 = is_32bit(module);
        self.modules.iter().find(|s| {
            s.path == module
                && s.modified == modified
                && s.size == size
                && s.bits32 == bits32
                && !matches!(
                    s.result,
                    Err(Failure::Crashed(_) | Failure::Hung(_) | Failure::NeedsBridge)
                )
        })
    }

    /// `modules` looked at: from the cache when unchanged, by `describe`
    /// otherwise, `workers` at once (each in a process of its own). The
    /// cache's other modules stay while their files do (another list of
    /// folders may have them). `progress` hears how many of how many were
    /// looked at. Whether the cache changed.
    pub fn refresh(
        &mut self,
        modules: &[PathBuf],
        workers: usize,
        describe: impl Fn(&Path) -> ModuleScan + Sync,
        progress: impl Fn(usize, usize) + Sync,
    ) -> bool {
        let mut modules = modules.to_vec();
        modules.sort();
        modules.dedup();
        let known: Vec<Option<ModuleScan>> =
            modules.iter().map(|m| self.known(m).cloned()).collect();
        let todo: Vec<usize> = (0..modules.len()).filter(|&i| known[i].is_none()).collect();
        let (others, gone): (Vec<ModuleScan>, Vec<ModuleScan>) = std::mem::take(&mut self.modules)
            .into_iter()
            .filter(|s| !modules.contains(&s.path))
            .partition(|s| library(&s.path).exists());
        let changed = !todo.is_empty() || !gone.is_empty();
        let found: Vec<std::sync::Mutex<Option<ModuleScan>>> =
            known.into_iter().map(std::sync::Mutex::new).collect();
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        progress(0, todo.len());
        std::thread::scope(|s| {
            for _ in 0..workers.clamp(1, todo.len().max(1)) {
                s.spawn(|| {
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&i) = todo.get(k) else {
                            return;
                        };
                        let scan = describe(&modules[i]);
                        if let Ok(mut slot) = found[i].lock() {
                            *slot = Some(scan);
                        }
                        progress(done.fetch_add(1, Ordering::Relaxed) + 1, todo.len());
                    }
                });
            }
        });
        self.modules = found
            .into_iter()
            .filter_map(|m| m.into_inner().ok().flatten())
            .chain(others)
            .collect();
        self.modules.sort_by(|a, b| a.path.cmp(&b.path));
        changed
    }

    /// Every effect that works, with its module, in the modules' order.
    pub fn effects(&self) -> impl Iterator<Item = (&ModuleScan, &EffectInfo)> {
        self.modules.iter().flat_map(|m| {
            m.result
                .iter()
                .flatten()
                .filter(|e| e.works())
                .map(move |e| (m, e))
        })
    }
}

/// Names to list effects by, one each: an effect whose name an earlier one
/// has takes its module's file name too ("Echo (Plugs)"), then a number
/// ("Echo (2)").
pub fn unique_names(effects: &[(&str, &Path)]) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(effects.len());
    for &(name, module) in effects {
        let taken = |n: &str, names: &[String]| names.iter().any(|m| m.eq_ignore_ascii_case(n));
        let mut shown = name.to_string();
        if taken(&shown, &names) {
            let stem = module
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            shown = format!("{name} ({stem})");
            let mut k = 2;
            while taken(&shown, &names) {
                shown = format!("{name} ({k})");
                k += 1;
            }
        }
        names.push(shown);
    }
    names
}

/// The child's part: `args` are what followed [`CHILD_FLAG`]. Returns the
/// process's exit code.
pub fn child_main(args: &[String]) -> i32 {
    #[cfg(windows)]
    end_with_the_child();
    // On macOS on the main thread, where plug-ins expect their first calls
    // (its stack is as deep as a plug-in needs).
    if cfg!(target_os = "macos") {
        return child_work(args);
    }
    // On a thread with a plug-in's stack (the main thread's is small).
    let args = args.to_vec();
    std::thread::Builder::new()
        .name("bv-vst3-child".into())
        .stack_size(8 << 20)
        .spawn(move || child_work(&args))
        .ok()
        .and_then(|t| t.join().ok())
        .unwrap_or(3)
}

fn child_work(args: &[String]) -> i32 {
    let [module, out] = args else {
        return 1;
    };
    #[cfg(windows)]
    com_on_this_thread();
    let scan = scan_here(Path::new(module));
    match serde_json::to_vec(&scan) {
        Ok(json) if std::fs::write(out, &json).is_ok() => 0,
        _ => 2,
    }
}

/// The thread joins a single-threaded COM apartment, as the interface's
/// thread a plug-in starts on in BananaV.
#[cfg(windows)]
#[allow(unsafe_code)]
fn com_on_this_thread() {
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    const APARTMENT_THREADED: u32 = 0x2;
    // SAFETY: a plain call; the apartment ends with the thread.
    unsafe { CoInitializeEx(std::ptr::null_mut(), APARTMENT_THREADED) };
}

/// Programs the plug-ins start from this child end with it: the child
/// joins a job of its own that ends its processes once it is gone. Those
/// that ask to leave the job may.
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
    fn repeated_names_are_told_apart() {
        let a = Path::new("C:/VST3/Plugs.vst3");
        let b = Path::new("C:/VST3/Other.vst3");
        let names = unique_names(&[("Echo", a), ("Gate", a), ("Echo", b), ("ECHO", b)]);
        assert_eq!(names, ["Echo", "Gate", "Echo (Other)", "ECHO (2)"]);
    }

    #[test]
    fn the_cache_keeps_what_did_not_change_and_looks_again_at_crashes() {
        let dir = std::env::temp_dir().join(format!("bananav-vst3-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.vst3"), dir.join("b.vst3"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let looked = std::sync::Mutex::new(Vec::new());
        let describe = |m: &Path| {
            looked.lock().unwrap().push(m.to_path_buf());
            let (modified, size) = stamp(m);
            ModuleScan {
                path: m.to_path_buf(),
                modified,
                size,
                vendor: String::new(),
                bits32: false,
                result: if m == b.as_path() {
                    Err(Failure::Crashed("exit code 3".into()))
                } else {
                    Ok(vec![EffectInfo {
                        name: "A".into(),
                        ..EffectInfo::default()
                    }])
                },
            }
        };
        let mut cache = Cache::default();
        assert!(cache.refresh(&[a.clone(), b.clone()], 2, describe, |_, _| {}));
        assert_eq!(looked.lock().unwrap().len(), 2);
        assert_eq!(cache.effects().count(), 1);
        // Saved and loaded: the same.
        let file = dir.join("cache.json");
        cache.save(&file).unwrap();
        let mut cache = Cache::load(&file);
        // Unchanged: from the cache; the crash looked at again.
        looked.lock().unwrap().clear();
        cache.refresh(&[a.clone(), b.clone()], 2, describe, |_, _| {});
        assert_eq!(looked.lock().unwrap().as_slice(), std::slice::from_ref(&b));
        // A module of other folders stays; once its file is gone, it
        // leaves.
        assert!(!cache.refresh(std::slice::from_ref(&a), 1, describe, |_, _| {}));
        assert_eq!(cache.modules.len(), 2);
        std::fs::remove_file(&b).unwrap();
        assert!(cache.refresh(std::slice::from_ref(&a), 1, describe, |_, _| {}));
        assert_eq!(cache.modules.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
