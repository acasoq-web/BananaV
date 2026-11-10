//! Finding VST 2 plug-ins without risk: each library is looked at in a
//! process of its own — BananaV for libraries made for its processor, the
//! 32-bit bridge for 32-bit ones — where it is loaded, its plug-ins (a
//! shell's too) made, set up for stereo at 48 kHz, given a block of silence
//! and asked what they are; a plug-in that crashes or hangs costs that
//! process only. Several are looked at at once, and what they said is kept
//! in a cache until their files change.
//!
//! The child is started with `--vst2-scan <library> <answer.json>`;
//! [`child_main`] does its part.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::abi::category;
use crate::local::Local;
use crate::module::{Error, Machine, load, machine, this_machine};

/// The argument that starts a child.
pub const CHILD_FLAG: &str = "--vst2-scan";

const RATE: f64 = 48_000.0;
const BLOCK: usize = 1024;

/// A plug-in a library holds, as it said and did when tried.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectInfo {
    /// A shell's plug-in's number (0: the library's own plug-in).
    pub id: i32,
    /// The number it is known by.
    pub unique_id: i32,
    pub name: String,
    pub vendor: String,
    pub product: String,
    pub version: i64,
    pub category: i32,
    pub inputs: u32,
    pub outputs: u32,
    pub params: u32,
    pub programs: u32,
    pub latency: u32,
    pub tail: Option<u32>,
    pub editor: bool,
    /// An instrument (no audio effect).
    pub synth: bool,
    /// Why it failed when tried: it is listed, never offered.
    pub error: Option<String>,
}

impl EffectInfo {
    pub fn works(&self) -> bool {
        self.error.is_none()
    }

    /// An audio effect (not an instrument nor a generator).
    pub fn is_effect(&self) -> bool {
        !self.synth
            && !matches!(
                isize::try_from(self.category).unwrap_or(0),
                category::SYNTH | category::GENERATOR
            )
    }
}

/// Why a library could not be looked at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    /// It would not load or start (the reason).
    Refused(String),
    /// Its process crashed (how it ended).
    Crashed(String),
    /// Its process gave no answer in this many seconds.
    Hung(u64),
    /// A 32-bit library and no 32-bit bridge (or 32-bit plug-ins off).
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

/// What a library holds, or why it could not be looked at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleScan {
    pub path: PathBuf,
    /// Its modification time (seconds) and size when looked at.
    pub modified: u64,
    pub size: u64,
    /// The processor it is made for.
    pub machine: Option<Machine>,
    pub result: Result<Vec<EffectInfo>, Failure>,
}

impl ModuleScan {
    /// Made for 32-bit x86 (run in the bridge).
    pub fn bits32(&self) -> bool {
        self.machine == Some(Machine::X86) && this_machine() != Machine::X86
    }
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

/// Tries `local`, noting in `info` what it does.
fn tried(local: &mut Local, info: &mut EffectInfo) -> Result<(), Error> {
    local.setup(RATE, BLOCK, 2)?;
    let mut silence = vec![vec![0.0f32; BLOCK]; 2];
    local.process(&mut silence);
    if local.crashed() {
        return Err(Error::Crashed);
    }
    info.latency = local.latency();
    local.stop();
    Ok(())
}

/// What plug-in `id` of the library at `path` is and does.
fn try_plugin(path: &Path, id: i32, name: Option<String>) -> EffectInfo {
    let mut info = EffectInfo {
        id,
        name: name.unwrap_or_default(),
        ..EffectInfo::default()
    };
    let made = load(path).and_then(|m| Local::new(&m, id));
    let outcome = made.and_then(|mut local| {
        let d = local.describe();
        if info.name.is_empty() {
            info.name.clone_from(&d.name);
        }
        info.unique_id = d.unique_id;
        info.vendor = d.vendor;
        info.product = d.product;
        info.version = d.version;
        info.category = d.category;
        info.inputs = d.inputs;
        info.outputs = d.outputs;
        info.params = d.params;
        info.programs = d.programs;
        info.latency = d.latency;
        info.tail = d.tail;
        info.editor = d.editor;
        info.synth = d.synth;
        tried(&mut local, &mut info)
    });
    if let Err(e) = outcome {
        info.error = Some(match (e, crate::guard::take_crashes().pop()) {
            (Error::Crashed, Some(c)) => format!(
                "it crashed in {} (exception {:#010x} at {})",
                c.doing, c.code, c.at
            ),
            (e, _) => e.to_string(),
        });
    }
    info
}

/// Looks at the library at `path` in this process: its plug-in, or a
/// shell's plug-ins.
pub fn scan_here(path: &Path) -> ModuleScan {
    let (modified, size) = stamp(path);
    let shells = load(path).and_then(|m| Local::new(&m, 0)).map(|local| {
        let d = local.describe();
        if isize::try_from(d.category).unwrap_or(0) == category::SHELL {
            local.shell_plugins()
        } else {
            Vec::new()
        }
    });
    let result = match shells {
        Err(e) => Err(Failure::Refused(e.to_string())),
        Ok(shells) if shells.is_empty() => Ok(vec![try_plugin(path, 0, None)]),
        Ok(shells) => Ok(shells
            .into_iter()
            .map(|(id, name)| try_plugin(path, id, Some(name)))
            .collect()),
    };
    ModuleScan {
        path: path.to_path_buf(),
        modified,
        size,
        machine: machine(path),
        result,
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn answer_file() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bananav-vst2-scan-{}-{n}.json", std::process::id()))
}

/// Runs `cmd` and waits for it, `timeout` at most.
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

/// The programs that look at libraries in processes of their own: one for
/// this processor's, the bridge for 32-bit ones.
#[derive(Clone, Debug)]
pub struct Scanner {
    pub exe: PathBuf,
    pub bridge: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

impl Scanner {
    /// BananaV's own: the program running, when it is BananaV, and its
    /// bridge when there is one and 32-bit plug-ins are on.
    pub fn bananav(timeout: Duration) -> Option<Scanner> {
        let exe = std::env::current_exe().ok()?;
        let stem = exe.file_stem()?.to_string_lossy().to_lowercase();
        (stem == "bananav").then(|| Scanner {
            exe,
            bridge: crate::bridge::enabled().then(crate::bridge::exe).flatten(),
            env: Vec::new(),
            timeout,
        })
    }

    /// Looks at `path` in a child — again, once, when it crashed.
    pub fn describe(&self, path: &Path) -> ModuleScan {
        let scan = self.describe_once(path);
        if matches!(scan.result, Err(Failure::Crashed(_))) {
            return self.describe_once(path);
        }
        scan
    }

    fn describe_once(&self, path: &Path) -> ModuleScan {
        let m = machine(path);
        let exe = match m {
            Some(Machine::X86) if this_machine() != Machine::X86 => match &self.bridge {
                Some(b) => b.clone(),
                None => return self.failed(path, m, Failure::NeedsBridge),
            },
            _ => self.exe.clone(),
        };
        let answer = answer_file();
        let mut cmd = Command::new(&exe);
        cmd.arg(CHILD_FLAG)
            .arg(absolute(path))
            .arg(&answer)
            .current_dir(std::env::temp_dir())
            .envs(self.env.iter().map(|(k, v)| (k, v)));
        let outcome = run_child(cmd, self.timeout);
        let read = std::fs::read(&answer)
            .ok()
            .and_then(|b| serde_json::from_slice::<ModuleScan>(&b).ok());
        let _ = std::fs::remove_file(&answer);
        match (outcome, read) {
            (Ok(()), Some(mut scan)) => {
                scan.path = path.to_path_buf();
                scan
            }
            (outcome, _) => self.failed(
                path,
                m,
                outcome
                    .err()
                    .unwrap_or_else(|| Failure::Crashed("no answer".into())),
            ),
        }
    }

    fn failed(&self, path: &Path, m: Option<Machine>, why: Failure) -> ModuleScan {
        let (modified, size) = stamp(path);
        ModuleScan {
            path: path.to_path_buf(),
            modified,
            size,
            machine: m,
            result: Err(why),
        }
    }
}

/// Libraries looked at before.
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

    /// What the cache knows of `module`, when its file has not changed
    /// since and it did not crash, hang or wait for the bridge then.
    pub fn known(&self, module: &Path) -> Option<&ModuleScan> {
        let (modified, size) = stamp(module);
        self.modules.iter().find(|s| {
            s.path == module
                && s.modified == modified
                && s.size == size
                && !matches!(
                    s.result,
                    Err(Failure::Crashed(_) | Failure::Hung(_) | Failure::NeedsBridge)
                )
        })
    }

    /// `modules` looked at: from the cache when unchanged, by `describe`
    /// otherwise, `workers` at once. The cache's other libraries stay while
    /// their files do. Whether the cache changed.
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
            .partition(|s| s.path.exists());
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
}

/// The child's part: `args` are what followed [`CHILD_FLAG`]. Returns the
/// process's exit code.
pub fn child_main(args: &[String]) -> i32 {
    // On macOS on the main thread, where plug-ins expect their first calls
    // (its stack is as deep as a plug-in needs).
    if cfg!(target_os = "macos")
        && let [file, out] = args
    {
        let scan = scan_here(Path::new(file));
        return match serde_json::to_vec(&scan) {
            Ok(json) if std::fs::write(out, &json).is_ok() => 0,
            _ => 2,
        };
    }
    let args = args.to_vec();
    std::thread::Builder::new()
        .name("bv-vst2-child".into())
        .stack_size(8 << 20)
        .spawn(move || {
            let [file, out] = args.as_slice() else {
                return 1;
            };
            let scan = scan_here(Path::new(file));
            match serde_json::to_vec(&scan) {
                Ok(json) if std::fs::write(out, &json).is_ok() => 0,
                _ => 2,
            }
        })
        .ok()
        .and_then(|t| t.join().ok())
        .unwrap_or(3)
}
