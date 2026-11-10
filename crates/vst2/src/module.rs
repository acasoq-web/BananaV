//! VST 2 plug-in libraries: loaded once and never unloaded (some plug-ins
//! hang or crash in their shutdown code), their entry point found; which
//! processor a library is for, read from its file (a 32-bit one goes to
//! BananaV's 32-bit bridge). On macOS a plug-in is a bundle (`Name.vst`)
//! loaded through Core Foundation.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::abi::{Entry, MAGIC, PluginObject};
use crate::guard;
use crate::host::{CURRENT_ID, callback};

/// Why a plug-in could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Its library would not load (the system's reason).
    Load(String),
    /// Its library has no entry point.
    NoEntry,
    /// It gave no plug-in, or refused a step.
    Refused(&'static str),
    /// It crashed, now or before (its library is switched off until
    /// BananaV starts again).
    Crashed,
    /// A 32-bit library and no 32-bit bridge to run it in (or 32-bit
    /// plug-ins turned off).
    NeedsBridge,
    /// The 32-bit bridge failed (its reason).
    Bridge(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Load(why) => write!(f, "the library would not load: {why}"),
            Error::NoEntry => write!(f, "the library is no VST 2 plug-in"),
            Error::Refused(what) => write!(f, "the plug-in refused {what}"),
            Error::Crashed => write!(f, "the plug-in crashed and was switched off"),
            Error::NeedsBridge => write!(f, "a 32-bit plug-in, and 32-bit plug-ins are off"),
            Error::Bridge(why) => write!(f, "the 32-bit bridge failed: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// The processor a library is made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Machine {
    X86,
    X64,
    Arm64,
    Other,
}

/// A macOS bundle's executable (`Name.vst/Contents/MacOS/Name`): its
/// Info.plist's on a Mac, else by the usual name.
pub fn bundle_executable(bundle: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(exe) = bv_apple::Bundle::open(bundle).and_then(|b| b.executable()) {
        return exe;
    }
    let name = bundle.file_stem().map(PathBuf::from).unwrap_or_default();
    bundle.join("Contents").join("MacOS").join(name)
}

/// The processor `path` (a library, or a macOS bundle) is made for, from
/// its header: a universal Mach-O file holding this processor's code is
/// this processor's.
pub fn machine(path: &Path) -> Option<Machine> {
    use std::io::{Read, Seek, SeekFrom};
    let path = if path.is_dir() {
        bundle_executable(path)
    } else {
        path.to_path_buf()
    };
    let path = path.as_path();
    if let Some(slices) = bv_apple::macho::file_slices(path) {
        use bv_apple::macho::{CPU_ARM64, CPU_X86, CPU_X86_64};
        if bv_apple::macho::native(&slices).is_some() {
            return Some(this_machine());
        }
        return Some(match slices.first()?.cpu {
            CPU_X86_64 => Machine::X64,
            CPU_ARM64 => Machine::Arm64,
            CPU_X86 => Machine::X86,
            _ => Machine::Other,
        });
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 64];
    f.read_exact(&mut head).ok()?;
    if &head[..2] == b"MZ" {
        let pe = u32::from_le_bytes(head[60..64].try_into().ok()?);
        f.seek(SeekFrom::Start(u64::from(pe))).ok()?;
        let mut sig = [0u8; 6];
        f.read_exact(&mut sig).ok()?;
        if &sig[..4] != b"PE\0\0" {
            return None;
        }
        return Some(match u16::from_le_bytes([sig[4], sig[5]]) {
            0x014C => Machine::X86,
            0x8664 => Machine::X64,
            0xAA64 => Machine::Arm64,
            _ => Machine::Other,
        });
    }
    if &head[..4] == b"\x7fELF" {
        return Some(match u16::from_le_bytes([head[18], head[19]]) {
            3 => Machine::X86,
            62 => Machine::X64,
            183 => Machine::Arm64,
            _ => Machine::Other,
        });
    }
    None
}

/// The processor this program runs on.
pub fn this_machine() -> Machine {
    match std::env::consts::ARCH {
        "x86" => Machine::X86,
        "x86_64" => Machine::X64,
        "aarch64" => Machine::Arm64,
        _ => Machine::Other,
    }
}

/// A loaded library.
pub struct Module {
    path: PathBuf,
    entry: Entry,
    crashed: AtomicBool,
    _library: Option<std::mem::ManuallyDrop<libloading::Library>>,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module").field("path", &self.path).finish()
    }
}

// SAFETY: the entry point is a plain function; hosts call it from any
// thread.
unsafe impl Send for Module {}
// SAFETY: as above.
unsafe impl Sync for Module {}

impl Module {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether one of its plug-ins crashed: it is switched off until
    /// BananaV starts again.
    pub fn crashed(&self) -> bool {
        self.crashed.load(Ordering::Acquire)
    }

    /// Switches it off after a crash of `effect` doing `doing`, noted.
    pub(crate) fn crash(
        &self,
        effect: &str,
        doing: &str,
        code: u32,
        address: *mut std::ffi::c_void,
    ) {
        self.crashed.store(true, Ordering::Release);
        guard::note(guard::Crash {
            module: self.path.clone(),
            effect: effect.to_string(),
            doing: doing.to_string(),
            code,
            at: guard::place(address),
        });
    }

    /// A new plug-in object (the shell's plug-in `id`, or the library's
    /// own with 0), checked to be one.
    pub(crate) fn make(&self, id: i32) -> Result<*mut PluginObject, Error> {
        if self.crashed() {
            return Err(Error::Crashed);
        }
        CURRENT_ID.with(|c| c.set(id));
        // SAFETY: the library's entry point with BananaV's callback.
        let made = unsafe { guard::entry(self.entry, callback) };
        CURRENT_ID.with(|c| c.set(0));
        let plugin = match made {
            Ok(p) => p,
            Err((code, at)) => {
                self.crash("", "its entry point", code, at);
                return Err(Error::Crashed);
            }
        };
        // SAFETY: the plug-in's object, read for its mark.
        if plugin.is_null() || unsafe { (*plugin).magic } != MAGIC {
            return Err(Error::Refused("to make a plug-in"));
        }
        // SAFETY: as above.
        if unsafe { (*plugin).dispatcher }.is_none() {
            return Err(Error::Refused("to make a plug-in"));
        }
        Ok(plugin)
    }
}

type Loaded = Result<Arc<Module>, Error>;

fn modules() -> &'static Mutex<HashMap<PathBuf, Loaded>> {
    static MODULES: OnceLock<Mutex<HashMap<PathBuf, Loaded>>> = OnceLock::new();
    MODULES.get_or_init(Default::default)
}

/// The library at `path`, loaded the first time it is asked for; later
/// calls get the same (or the same failure). Only a library made for this
/// processor loads here ([`machine`]).
pub fn load(path: &Path) -> Loaded {
    let mut all = modules().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = all.get(path) {
        return m.clone();
    }
    let loaded = load_library(path).map(Arc::new);
    all.insert(path.to_path_buf(), loaded.clone());
    loaded
}

/// A plug-in linked into the program (the tests'), listed under `name`.
pub fn linked(name: &str, entry: Entry) -> Loaded {
    let path = PathBuf::from(format!("<linked>/{name}"));
    let mut all = modules().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = all.get(&path) {
        return m.clone();
    }
    let loaded = Ok(Arc::new(Module {
        path: path.clone(),
        entry,
        crashed: AtomicBool::new(false),
        _library: None,
    }));
    all.insert(path, loaded.clone());
    loaded
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetThreadErrorMode(mode: u32, old: *mut u32) -> i32;
}

/// Windows' own error boxes kept off this thread while it lives.
#[cfg(windows)]
struct QuietErrors(u32);

#[cfg(windows)]
impl QuietErrors {
    fn new() -> QuietErrors {
        let mut old = 0;
        // SAFETY: the thread's error mode set (no critical-error and no
        // open-file boxes), the old one written to `old`.
        unsafe { SetThreadErrorMode(0x0001 | 0x8000, &mut old) };
        QuietErrors(old)
    }
}

#[cfg(windows)]
impl Drop for QuietErrors {
    fn drop(&mut self) {
        // SAFETY: as in `new`; the old mode is not wanted.
        unsafe { SetThreadErrorMode(self.0, std::ptr::null_mut()) };
    }
}

/// A macOS bundle (`Name.vst`): loaded through Core Foundation, its entry
/// point `VSTPluginMain` (or older plug-ins' `main_macho`).
#[cfg(target_os = "macos")]
fn load_bundle(path: &Path) -> Result<Module, Error> {
    // The guard's handlers before the plug-in's code (it may add its own).
    bv_apple::guard::install();
    let bundle = bv_apple::Bundle::open(path)
        .ok_or_else(|| Error::Load("not a bundle with an executable".to_string()))?;
    bundle.load().map_err(Error::Load)?;
    let entry = bundle
        .function("VSTPluginMain")
        .or_else(|| bundle.function("main_macho"))
        .ok_or(Error::NoEntry)?;
    // SAFETY: the plug-in's entry point, with the type plug-ins give it.
    let entry = unsafe { std::mem::transmute::<*mut std::ffi::c_void, Entry>(entry.as_ptr()) };
    // Never unloaded.
    let _ = bundle.leak();
    Ok(Module {
        path: path.to_path_buf(),
        entry,
        crashed: AtomicBool::new(false),
        _library: None,
    })
}

fn load_library(path: &Path) -> Result<Module, Error> {
    if let Some(m) = machine(path)
        && m != this_machine()
    {
        // A Mac's plug-in made for its other processor (an Intel-only one
        // on Apple's chips runs with BananaV opened through Rosetta).
        if cfg!(target_os = "macos") {
            return Err(Error::Load(format!(
                "made for {:?} Macs only (open BananaV with Rosetta for Intel-only plug-ins)",
                m
            )));
        }
        return Err(Error::NeedsBridge);
    }
    #[cfg(target_os = "macos")]
    if path.is_dir() {
        return load_bundle(path);
    }
    #[cfg(windows)]
    let _quiet = QuietErrors::new();
    #[cfg(windows)]
    // SAFETY: loading runs the library's initialisers: loading is what the
    // user asked for. Its own folder is searched for the libraries it
    // needs.
    let library = unsafe {
        libloading::os::windows::Library::load_with_flags(
            path,
            libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    }
    .map(libloading::Library::from)
    .map_err(|e| Error::Load(e.to_string()))?;
    #[cfg(unix)]
    // SAFETY: as above.
    let library = unsafe {
        libloading::os::unix::Library::open(
            Some(path),
            libloading::os::unix::RTLD_LAZY | libloading::os::unix::RTLD_LOCAL,
        )
    }
    .map(libloading::Library::from)
    .map_err(|e| Error::Load(e.to_string()))?;
    // SAFETY: the library's entry point, with the type plug-ins give it.
    let entry: Entry = unsafe {
        match library.get::<Entry>(b"VSTPluginMain\0") {
            Ok(f) => *f,
            Err(_) => *library
                .get::<Entry>(b"main\0")
                .map_err(|_| Error::NoEntry)?,
        }
    };
    Ok(Module {
        path: path.to_path_buf(),
        entry,
        crashed: AtomicBool::new(false),
        _library: Some(std::mem::ManuallyDrop::new(library)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_tells_its_processor() {
        let me = std::env::current_exe().unwrap();
        assert_eq!(machine(&me), Some(this_machine()));
        let dir = std::env::temp_dir().join(format!("bananav-vst2-machine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.dll");
        std::fs::write(&fake, b"not a library at all, too short or not").unwrap();
        assert_eq!(machine(&fake), None);
        assert!(load(&dir.join("missing.dll")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
