//! Plug-in binaries and the plug-ins in them: loading a binary, and
//! loading and describing each of its image effects in every context
//! BananaV runs.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::effect::Effect;
use crate::info::{Context, Error, PluginInfo, context_info};
use crate::pixels::Depth;
use crate::props::{PropSet, SetKind, cstring};
use crate::suites::host;

/// A loaded binary (`.ofx`). Binaries stay loaded: plug-ins and the
/// effects they made point into them.
#[derive(Debug)]
pub struct Binary {
    pub path: PathBuf,
    entries: Entries,
    /// Switched off: one of its plug-ins crashed (`guard.rs`), so its
    /// code is not called again until BananaV restarts.
    off: std::sync::atomic::AtomicBool,
    /// Whether it took the host (`OfxSetHost`, given once).
    took_host: std::sync::OnceLock<bool>,
}

/// Where a binary's entry points are.
#[derive(Debug)]
enum Entries {
    /// A library loaded for good: unloading one runs its shutdown code,
    /// which in some plug-ins waits for their own threads and hangs (on
    /// Windows, under the loader lock), so hosts keep them loaded.
    #[cfg(not(target_family = "wasm"))]
    Library(std::mem::ManuallyDrop<libloading::Library>),
    /// Plug-ins built into the program (and the tests').
    Static {
        count: OfxGetNumberOfPluginsFn,
        get: OfxGetPluginFn,
        set_host: Option<OfxSetHostFn>,
    },
}

/// A plug-in that failed while loading, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub index: u32,
    /// Its identifier, when it got that far.
    pub identifier: String,
    pub error: Error,
}

/// Whether an action's answer is a success (it did it, or let the host do
/// the default).
pub(crate) fn succeeded(status: OfxStatus) -> bool {
    status == kOfxStatOK || status == kOfxStatReplyDefault
}

/// Windows' "the file name or extension is too long".
#[cfg(windows)]
const ERROR_FILENAME_EXCED_RANGE: i32 = 206;

/// The system's error code under a library's loading error.
#[cfg(windows)]
fn os_error(e: &libloading::Error) -> Option<i32> {
    std::error::Error::source(e)?
        .downcast_ref::<std::io::Error>()?
        .raw_os_error()
}

/// Puts a per-drive current folder ("=C:=C:\…", the hidden variables
/// cmd.exe keeps) in this process's environment when it has none. Plug-ins
/// start programs with the C runtime's `_spawnle` (Sapphire its preset
/// browser), which looks for those variables and, without one, reads on
/// past the environment's end: a crash now and then. Programs Rust's start
/// (BananaV's scans, its tests) have none. The variable is the current
/// folder's drive's, as cmd.exe would set it (C:'s root without a drive).
#[cfg(windows)]
pub(crate) fn keep_a_drive_folder() {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetEnvironmentStringsW() -> *mut u16;
        fn FreeEnvironmentStringsW(block: *mut u16) -> i32;
        fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
    }
    // SAFETY: the process's environment block, strings each ended by a
    // NUL and the block by an empty one; freed once read.
    let has_one = unsafe {
        let block = GetEnvironmentStringsW();
        if block.is_null() {
            return;
        }
        let (mut at, mut found) = (block, false);
        while *at != 0 && !found {
            found = *at == u16::from(b'=');
            while *at != 0 {
                at = at.add(1);
            }
            at = at.add(1);
        }
        FreeEnvironmentStringsW(block);
        found
    };
    if has_one {
        return;
    }
    let dir = std::env::current_dir().unwrap_or_default();
    let (drive, folder) = match dir.as_os_str().as_encoded_bytes() {
        [d, b':', b'\\', ..] if d.is_ascii_alphabetic() => {
            (d.to_ascii_uppercase() as char, dir.into_os_string())
        }
        _ => ('C', OsString::from("C:\\")),
    };
    let wide = |s: &OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let name = wide(OsStr::new(&format!("={drive}:")));
    let value = wide(&folder);
    // SAFETY: two NUL-terminated strings.
    unsafe { SetEnvironmentVariableW(name.as_ptr(), value.as_ptr()) };
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetThreadErrorMode(mode: u32, old: *mut u32) -> i32;
}

/// Windows' own error boxes kept off this thread while it lives. A binary
/// whose libraries are missing, or are another program's of the same name
/// (a `Universe.dll` in the system's folder without the functions it calls),
/// fails to load with the error saying so, where Windows would show its
/// "Entry Point Not Found" box for it — at every start, for each such
/// binary the scan describes.
#[cfg(windows)]
pub(crate) struct QuietErrors(u32);

#[cfg(windows)]
impl QuietErrors {
    /// `SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX`.
    pub(crate) const MODE: u32 = 0x0001 | 0x8000;

    pub(crate) fn new() -> QuietErrors {
        let mut old = 0;
        // SAFETY: the thread's error mode set, the old one written to `old`.
        unsafe { SetThreadErrorMode(QuietErrors::MODE, &mut old) };
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

/// `path` by its short (8.3) name, when the volume has one.
#[cfg(windows)]
fn short_path(path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(long: *const u16, short: *mut u16, len: u32) -> u32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buf = vec![0u16; 1024];
    // SAFETY: a NUL-terminated path, and a buffer of the length given.
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    let n = n as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])))
}

/// An error and the errors under it ("LoadLibraryExW failed: The
/// specified module could not be found. (os error 126)").
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}

impl Binary {
    /// Whether one of its plug-ins crashed: its code is not called again.
    pub fn crashed(&self) -> bool {
        self.off.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Switches it off (one of its plug-ins crashed).
    pub(crate) fn switch_off(&self) {
        self.off.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Loads a binary. Its own folder is searched for the libraries it
    /// needs; when they cannot be loaded, the error says so (no box of the
    /// system's: `QuietErrors`).
    #[cfg(not(target_family = "wasm"))]
    pub fn load(path: &Path) -> Result<Arc<Binary>, Error> {
        // macOS: the guard's handlers before the plug-in's code (it may
        // add its own).
        #[cfg(target_os = "macos")]
        bv_apple::guard::install();
        #[cfg(windows)]
        let _quiet = QuietErrors::new();
        #[cfg(windows)]
        // SAFETY: loading runs the binary's initialisers: loading is what
        // the user asked for.
        let load = |path: &Path| {
            // SAFETY: as above.
            unsafe {
                libloading::os::windows::Library::load_with_flags(
                    path,
                    libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
                )
            }
            .map(libloading::Library::from)
        };
        #[cfg(windows)]
        let lib = load(path).or_else(|e| {
            // Windows looks for the binary's libraries beside it: in a
            // deep folder their paths pass its length limit. The folder's
            // short (8.3) name keeps them under it.
            match short_path(path) {
                Some(short) if os_error(&e) == Some(ERROR_FILENAME_EXCED_RANGE) => load(&short),
                _ => Err(e),
            }
        });
        #[cfg(unix)]
        // SAFETY: as above.
        let lib = unsafe {
            libloading::os::unix::Library::open(
                Some(path),
                libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_LOCAL,
            )
        }
        .map(libloading::Library::from);
        let lib = lib.map_err(|e| Error::Load(error_chain(&e)))?;
        Ok(Arc::new(Binary {
            path: path.to_path_buf(),
            entries: Entries::Library(std::mem::ManuallyDrop::new(lib)),
            off: Default::default(),
            took_host: Default::default(),
        }))
    }

    /// Plug-ins built into the program: the entry points a binary would
    /// export, under the name `label`.
    pub fn built_in(
        label: &str,
        count: OfxGetNumberOfPluginsFn,
        get: OfxGetPluginFn,
        set_host: Option<OfxSetHostFn>,
    ) -> Arc<Binary> {
        Arc::new(Binary {
            path: PathBuf::from(label),
            off: Default::default(),
            took_host: Default::default(),
            entries: Entries::Static {
                count,
                get,
                set_host,
            },
        })
    }

    #[cfg(target_family = "wasm")]
    pub fn load(_path: &Path) -> Result<Arc<Binary>, Error> {
        Err(Error::Load("plug-ins do not run here".into()))
    }

    /// The binary's entry points: `OfxSetHost` (when it has one),
    /// `OfxGetNumberOfPlugins` and `OfxGetPlugin`.
    fn entry_points(
        &self,
    ) -> Option<(
        Option<OfxSetHostFn>,
        OfxGetNumberOfPluginsFn,
        OfxGetPluginFn,
    )> {
        match &self.entries {
            #[cfg(not(target_family = "wasm"))]
            Entries::Library(lib) => {
                // SAFETY: each symbol has the type the specification gives it.
                unsafe {
                    let set_host = lib.get::<OfxSetHostFn>(b"OfxSetHost\0").ok().map(|s| *s);
                    let count = *lib
                        .get::<OfxGetNumberOfPluginsFn>(b"OfxGetNumberOfPlugins\0")
                        .ok()?;
                    let get = *lib.get::<OfxGetPluginFn>(b"OfxGetPlugin\0").ok()?;
                    Some((set_host, count, get))
                }
            }
            Entries::Static {
                count,
                get,
                set_host,
            } => Some((*set_host, *count, *get)),
        }
    }

    /// The binary's functions listing its plug-ins, once it has taken the
    /// host (`OfxSetHost`, given once, when it has one).
    fn connected(&self) -> Result<(OfxGetNumberOfPluginsFn, OfxGetPluginFn), Error> {
        let (set_host, count, get) = self.entry_points().ok_or(Error::NotOfx)?;
        let took = *self.took_host.get_or_init(|| {
            set_host.is_none_or(|set_host| {
                // SAFETY: the host lives as long as the program.
                let status = unsafe { set_host(host().ofx()) };
                status != kOfxStatFailed
            })
        });
        if took {
            Ok((count, get))
        } else {
            Err(Error::NotForThisHost)
        }
    }

    /// Loads and describes plug-in `index` of the binary alone: a binary
    /// of hundreds (Sapphire's) takes seconds to describe them all.
    pub fn plugin(self: &Arc<Self>, index: u32) -> Result<Arc<Plugin>, Error> {
        let (count, get) = self.connected()?;
        // SAFETY: the binary's own function.
        let n = unsafe { count() }.max(0);
        let i = std::ffi::c_int::try_from(index)
            .ok()
            .filter(|i| *i < n)
            .ok_or_else(|| Error::Other(format!("its binary has {n} plug-ins")))?;
        // SAFETY: the binary's own function, for an index it counted.
        let raw = unsafe { get(i) };
        Plugin::load(self, raw, index)
            .map(Arc::new)
            .map_err(|(_, e)| e)
    }

    /// Loads and describes every plug-in of the binary.
    pub fn plugins(self: &Arc<Self>) -> Result<(Vec<Arc<Plugin>>, Vec<Failure>), Error> {
        let (count, get) = self.connected()?;
        let (mut plugins, mut failures) = (Vec::new(), Vec::new());
        // SAFETY: the binary's own function.
        let n = unsafe { count() }.max(0);
        for i in 0..n {
            // SAFETY: the binary's own function, for an index it counted.
            let raw = unsafe { get(i) };
            match Plugin::load(self, raw, i as u32) {
                Ok(p) => plugins.push(Arc::new(p)),
                Err((identifier, error)) => failures.push(Failure {
                    index: i as u32,
                    identifier,
                    error,
                }),
            }
        }
        Ok((plugins, failures))
    }
}

/// A plug-in's raw structure, owned by its binary.
#[derive(Debug)]
struct Raw(*mut OfxPlugin);

// SAFETY: the structure belongs to the binary, which stays loaded; the
// host only reads it.
unsafe impl Send for Raw {}
// SAFETY: as above.
unsafe impl Sync for Raw {}

/// One image effect plug-in, loaded and described.
#[derive(Debug)]
pub struct Plugin {
    pub info: PluginInfo,
    binary: Arc<Binary>,
    raw: Raw,
    entry: OfxPluginEntryPoint,
    descriptor: Box<Effect>,
    contexts: Vec<(Context, Box<Effect>)>,
    /// Held while a plug-in that cannot render twice at once renders.
    pub(crate) exclusive: Mutex<()>,
    /// Told it is done with ([`Plugin::unload`]).
    unloaded: std::sync::atomic::AtomicBool,
}

impl Plugin {
    /// Loads plug-in `raw` (number `index` of `binary`), describes it and
    /// describes it in every context BananaV runs it in.
    fn load(
        binary: &Arc<Binary>,
        raw: *mut OfxPlugin,
        index: u32,
    ) -> Result<Plugin, (String, Error)> {
        if raw.is_null() {
            return Err((String::new(), Error::NotOfx));
        }
        // SAFETY: the binary's plug-in structure.
        let p = unsafe { &*raw };
        // SAFETY: the structure's strings, or null.
        let text = |s: *const std::ffi::c_char| unsafe { crate::props::text(s) };
        let identifier = text(p.plugin_identifier);
        let fail = |e: Error| (identifier.clone(), e);
        if p.plugin_api.is_null()
            // SAFETY: a C string.
            || unsafe { CStr::from_ptr(p.plugin_api) } != kOfxImageEffectPluginApi
            || p.api_version < 1
        {
            return Err(fail(Error::NotOfx));
        }
        let entry = p.main_entry.ok_or_else(|| fail(Error::NotOfx))?;
        if let Some(set_host) = p.set_host {
            // SAFETY: the host lives as long as the program.
            unsafe { set_host(host().ofx()) };
        }
        let call = |action: &CStr, handle: *const c_void, args: Option<&PropSet>| {
            let args = args.map_or(std::ptr::null_mut(), PropSet::handle);
            let trace = crate::suites::tracing();
            if trace {
                eprintln!("ofx: plug-in {index} {action:?} …");
            }
            // SAFETY: the plug-in's entry point, with what the action takes.
            let status = unsafe {
                crate::guard::entry(binary, entry, action, handle, args, std::ptr::null_mut())
            };
            if trace {
                eprintln!(
                    "ofx: plug-in {index} {action:?} → {}",
                    crate::info::status_name(status)
                );
            }
            status
        };
        let status = call(kOfxActionLoad, std::ptr::null(), None);
        if !succeeded(status) {
            return Err(fail(Error::Action {
                action: "Load".into(),
                status,
            }));
        }
        // Loaded, it hears it is done with however describing goes.
        let fail = |e: Error| {
            call(kOfxActionUnload, std::ptr::null(), None);
            fail(e)
        };
        let file = cstring(&binary.path.to_string_lossy());
        let descriptor = Effect::descriptor(&file);
        let status = call(
            kOfxActionDescribe,
            descriptor.handle() as *const c_void,
            None,
        );
        if !succeeded(status) {
            return Err(fail(Error::Action {
                action: "Describe".into(),
                status,
            }));
        }
        let supported = descriptor.props.strs(kOfxImageEffectPropSupportedContexts);
        let mut contexts = Vec::new();
        let mut refused = None;
        for c in Context::ALL {
            if !supported
                .iter()
                .any(|s| s.as_bytes() == c.ofx_name().to_bytes())
            {
                continue;
            }
            let described = Effect::for_context(&descriptor);
            let args = PropSet::new(SetKind::Arguments);
            args.set_cstr(kOfxImageEffectPropContext, c.ofx_name());
            let status = call(
                kOfxImageEffectActionDescribeInContext,
                described.handle() as *const c_void,
                Some(&args),
            );
            if succeeded(status) {
                contexts.push((c, described));
            } else {
                refused.get_or_insert(Error::Action {
                    action: format!("DescribeInContext ({c:?})"),
                    status,
                });
            }
        }
        if contexts.is_empty() {
            // None of its contexts is one BananaV runs, or it refused them.
            let none = || {
                let names: Vec<_> = supported
                    .iter()
                    .map(|s| s.trim_start_matches("OfxImageEffectContext"))
                    .collect();
                Error::Other(format!("its contexts: {}", names.join(", ")))
            };
            return Err(fail(refused.unwrap_or_else(none)));
        }
        let props = &descriptor.props;
        let s = |n| props.str(n, 0).unwrap_or_default();
        let label = [kOfxPropLabel, kOfxPropLongLabel, kOfxPropShortLabel]
            .into_iter()
            .map(s)
            .find(|l| !l.is_empty())
            .unwrap_or_default();
        let info = PluginInfo {
            identifier: identifier.clone(),
            version: [p.plugin_version_major, p.plugin_version_minor],
            file: binary.path.clone(),
            index,
            label,
            grouping: s(kOfxImageEffectPluginPropGrouping),
            description: s(kOfxPropPluginDescription),
            contexts: contexts.iter().map(|(c, _)| *c).collect(),
            depths: props.strs(kOfxImageEffectPropSupportedPixelDepths),
            multi_resolution: props
                .flag(kOfxImageEffectPropSupportsMultiResolution)
                .unwrap_or(true),
            tiles: props.flag(kOfxImageEffectPropSupportsTiles).unwrap_or(true),
            temporal: props
                .flag(kOfxImageEffectPropTemporalClipAccess)
                .unwrap_or(false),
            overlay: [
                kOfxImageEffectPluginPropOverlayInteractV1,
                kOfxImageEffectPluginPropOverlayInteractV2,
            ]
            .into_iter()
            .any(|n| props.ptr(n, 0).is_some_and(|p| !p.is_null())),
            thread_safety: s(kOfxImageEffectPluginRenderThreadSafety),
            single_instance: props
                .flag(kOfxImageEffectPluginPropSingleInstance)
                .unwrap_or(false),
            files: supported
                .iter()
                .any(|s| s == "OfxImageEffectContextReader" || s == "OfxImageEffectContextWriter"),
            described: contexts.iter().map(|(c, d)| context_info(*c, d)).collect(),
        };
        Ok(Plugin {
            info,
            binary: Arc::clone(binary),
            raw: Raw(raw),
            entry,
            descriptor,
            contexts,
            exclusive: Mutex::new(()),
            unloaded: Default::default(),
        })
    }

    /// Tells the plug-in it is done with — OpenFX's unload action, its
    /// last, once its instances are gone —, so it lets go of what it
    /// keeps: other programs it started (Sapphire's preset browser waits
    /// in the background until then). Its library stays loaded. Once.
    pub fn unload(&self) {
        if !self
            .unloaded
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.call(kOfxActionUnload, std::ptr::null(), None, None);
        }
    }

    /// Runs `action` on `handle` (an effect of this plug-in).
    pub(crate) fn call(
        &self,
        action: &CStr,
        handle: *const c_void,
        in_args: Option<&PropSet>,
        out_args: Option<&PropSet>,
    ) -> OfxStatus {
        let i = in_args.map_or(std::ptr::null_mut(), PropSet::handle);
        let o = out_args.map_or(std::ptr::null_mut(), PropSet::handle);
        let trace = crate::suites::tracing();
        if trace {
            eprintln!("ofx: {} {:?} …", self.info.identifier, action);
        }
        let start = std::time::Instant::now();
        // SAFETY: the plug-in's entry point, with what the action takes.
        let status = unsafe { crate::guard::entry(&self.binary, self.entry, action, handle, i, o) };
        if trace {
            eprintln!(
                "ofx: {} {:?} → {} ({:.1} ms)",
                self.info.identifier,
                action,
                crate::info::status_name(status),
                start.elapsed().as_secs_f64() * 1e3
            );
        }
        status
    }

    /// The plug-in's descriptor.
    pub fn descriptor(&self) -> &Effect {
        &self.descriptor
    }

    /// Its descriptor in `context`.
    pub fn described(&self, context: Context) -> Option<&Effect> {
        self.contexts
            .iter()
            .find(|(c, _)| *c == context)
            .map(|(_, d)| &**d)
    }

    /// The channel type BananaV gives it: floats when it takes them.
    pub fn depth(&self) -> Depth {
        let depths = &self.info.depths;
        [Depth::Float, Depth::Short, Depth::Byte]
            .into_iter()
            .find(|d| depths.iter().any(|s| Depth::from_ofx_name(s) == Some(*d)))
            .unwrap_or(Depth::Float)
    }

    /// Whether only one render of the plug-in may run at a time.
    pub fn renders_alone(&self) -> bool {
        self.info.thread_safety.as_bytes() == kOfxImageEffectRenderUnsafe.to_bytes()
    }

    pub fn binary(&self) -> &Arc<Binary> {
        &self.binary
    }

    /// The plug-in's own structure.
    pub fn raw(&self) -> *mut OfxPlugin {
        self.raw.0
    }
}

/// The folders' names in a bundle's `Contents` for this system.
pub fn architectures() -> &'static [&'static str] {
    if cfg!(all(windows, target_arch = "x86_64")) {
        &["Win64"]
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        &["WinArm64", "Win-arm64", "Win64-arm64"]
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        &["Linux-x86-64"]
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        &["Linux-arm64", "Linux-aarch64"]
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        // Universal binaries, then the older Intel-only folder.
        &["MacOS", "MacOS-x86-64"]
    } else if cfg!(target_os = "macos") {
        &["MacOS"]
    } else {
        &[]
    }
}

/// The binaries of a bundle (`Name.ofx.bundle`) for this system.
pub fn bundle_binaries(bundle: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for arch in architectures() {
        let dir = bundle.join("Contents").join(arch);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ofx")))
            .collect();
        files.sort();
        out.extend(files);
    }
    out
}

/// Every bundle in `folder` and its subfolders.
pub fn find_bundles(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if !path.is_dir() {
                continue;
            }
            let bundle = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .is_some_and(|n| n.ends_with(".ofx.bundle"));
            if bundle {
                out.push(path);
            } else {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Where plug-ins are installed: the folders in `OFX_PLUGIN_PATH`, then
/// the system's standard folder.
pub fn standard_folders() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(list) = std::env::var_os("OFX_PLUGIN_PATH") {
        out.extend(std::env::split_paths(&list).filter(|p| !p.as_os_str().is_empty()));
    }
    #[cfg(windows)]
    {
        let common = std::env::var_os("CommonProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Common Files"));
        out.push(common.join("OFX").join("Plugins"));
    }
    #[cfg(target_os = "linux")]
    out.push(PathBuf::from("/usr/OFX/Plugins"));
    #[cfg(target_os = "macos")]
    out.push(PathBuf::from("/Library/OFX/Plugins"));
    out.dedup();
    out
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetThreadErrorMode() -> u32;
    }

    /// While a binary loads, Windows' error boxes are off on its thread;
    /// after, the thread's mode is as it was.
    #[test]
    fn loading_keeps_the_system_s_error_boxes_off() {
        // SAFETY: reads the thread's error mode.
        let before = unsafe { GetThreadErrorMode() };
        {
            let _quiet = QuietErrors::new();
            // SAFETY: as above.
            let during = unsafe { GetThreadErrorMode() };
            assert_eq!(during & QuietErrors::MODE, QuietErrors::MODE);
        }
        // SAFETY: as above.
        assert_eq!(unsafe { GetThreadErrorMode() }, before);
    }
}
