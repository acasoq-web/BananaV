//! VST 3 modules: a bundle's library (or a lone library) loaded, its
//! factory and the classes it makes. A module is loaded once and never
//! unloaded (some plug-ins hang or crash in their shutdown code), as OFX
//! plug-ins' libraries.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_char;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bv_vst3_sys::{
    AUDIO_EFFECT_CLASS, GetFactoryProc, IPluginFactory2Vtbl, IPluginFactory3Vtbl,
    IPluginFactoryVtbl, PClassInfo, PClassInfo2, PClassInfoW, PFactoryInfo, Raw, Tuid, iid, result,
    text_of, text_of16, uid_from_hex, uid_to_hex,
};

use crate::com::{ComPtr, Own};
use crate::host::HostApplication;

/// Why a plug-in could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Its library would not load (the system's reason).
    Load(String),
    /// Its library has no factory, or the factory gave none.
    NoFactory,
    /// It has no such effect.
    NoClass,
    /// It refused a step (what, and its result code).
    Refused(&'static str, i32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Load(why) => write!(f, "the library would not load: {why}"),
            Error::NoFactory => write!(f, "the library is no VST 3 module"),
            Error::NoClass => write!(f, "the module has no such effect"),
            Error::Refused(what, code) => write!(f, "the plug-in refused {what} ({code:#x})"),
        }
    }
}

impl std::error::Error for Error {}

/// A class a module makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassInfo {
    pub cid: Tuid,
    /// "Audio Module Class", "Component Controller Class"…
    pub category: String,
    pub name: String,
    pub vendor: String,
    /// "Fx|EQ", "Instrument|Synth"…
    pub sub_categories: String,
    pub version: String,
}

impl ClassInfo {
    /// Its identifier as the SDK writes it (32 hexadecimal digits).
    pub fn id(&self) -> String {
        uid_to_hex(&self.cid)
    }

    /// An audio effect: an audio module that is no instrument.
    pub fn is_effect(&self) -> bool {
        self.category == AUDIO_EFFECT_CLASS
            && !self.sub_categories.split('|').any(|s| s == "Instrument")
    }
}

/// A loaded module.
pub struct Module {
    path: PathBuf,
    vendor: String,
    classes: Vec<ClassInfo>,
    pub(crate) factory: ComPtr<IPluginFactoryVtbl>,
    /// The host application its objects are given.
    pub(crate) host: Own<HostApplication>,
    _library: Option<std::mem::ManuallyDrop<libloading::Library>>,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("path", &self.path)
            .field("classes", &self.classes)
            .finish()
    }
}

// SAFETY: a module's factory makes instances for any thread (hosts make
// them on threads of their own); its host object is `Sync`.
unsafe impl Send for Module {}
// SAFETY: as above.
unsafe impl Sync for Module {}

impl Module {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Who made it (its factory's vendor).
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    pub fn classes(&self) -> &[ClassInfo] {
        &self.classes
    }

    /// Its audio effects.
    pub fn effects(&self) -> impl Iterator<Item = &ClassInfo> {
        self.classes.iter().filter(|c| c.is_effect())
    }

    /// Effect `id` (32 hexadecimal digits); with no identifier, its first.
    pub fn effect(&self, id: &str) -> Option<&ClassInfo> {
        match uid_from_hex(id) {
            Some(cid) => self.effects().find(|c| c.cid == cid),
            None if id.trim().is_empty() => self.effects().next(),
            None => None,
        }
    }

    /// A new object of class `cid`, its interface `id` (`V` its table).
    pub(crate) fn create<V>(&self, cid: &Tuid, id: &Tuid) -> Option<ComPtr<V>> {
        let mut obj: Raw = std::ptr::null_mut();
        // SAFETY: the factory's createInstance with two 16-byte
        // identifiers; it fills `obj` with a reference of the caller's.
        let r = unsafe {
            (self.factory.vt().create_instance)(
                self.factory.raw(),
                cid.as_ptr().cast::<c_char>(),
                id.as_ptr().cast::<c_char>(),
                &mut obj,
            )
        };
        if r == result::OK {
            // SAFETY: the factory made interface `id`, whose table is `V`.
            unsafe { ComPtr::from_raw(obj) }
        } else {
            None
        }
    }

    /// A module from its factory: its vendor and classes read, the host
    /// application given to it.
    fn from_factory(
        path: PathBuf,
        factory: ComPtr<IPluginFactoryVtbl>,
        library: Option<libloading::Library>,
    ) -> Module {
        let host = HostApplication::new();
        let f3: Option<ComPtr<IPluginFactory3Vtbl>> = factory.query(&iid::PLUGIN_FACTORY3);
        let f2: Option<ComPtr<IPluginFactory2Vtbl>> = factory.query(&iid::PLUGIN_FACTORY2);
        if let Some(f3) = &f3 {
            // SAFETY: setHostContext with the host's application object.
            unsafe { (f3.vt().set_host_context)(f3.raw(), host.raw(0)) };
        }
        // SAFETY: the factory's own functions, with structures to fill
        // (all of whose bits may be zero).
        let (vendor, classes) = unsafe {
            let mut info: PFactoryInfo = std::mem::zeroed();
            let vendor = if (factory.vt().get_factory_info)(factory.raw(), &mut info) == result::OK
            {
                text_of(&info.vendor)
            } else {
                String::new()
            };
            let n = (factory.vt().count_classes)(factory.raw()).max(0);
            let mut classes = Vec::new();
            for i in 0..n {
                let mut w: PClassInfoW = std::mem::zeroed();
                let mut c2: PClassInfo2 = std::mem::zeroed();
                let mut c1: PClassInfo = std::mem::zeroed();
                let class = if let Some(f3) = &f3
                    && (f3.vt().get_class_info_unicode)(f3.raw(), i, &mut w) == result::OK
                {
                    ClassInfo {
                        cid: w.cid,
                        category: text_of(&w.category),
                        name: text_of16(&w.name),
                        vendor: text_of16(&w.vendor),
                        sub_categories: text_of(&w.sub_categories),
                        version: text_of16(&w.version),
                    }
                } else if let Some(f2) = &f2
                    && (f2.vt().get_class_info2)(f2.raw(), i, &mut c2) == result::OK
                {
                    ClassInfo {
                        cid: c2.cid,
                        category: text_of(&c2.category),
                        name: text_of(&c2.name),
                        vendor: text_of(&c2.vendor),
                        sub_categories: text_of(&c2.sub_categories),
                        version: text_of(&c2.version),
                    }
                } else if (factory.vt().get_class_info)(factory.raw(), i, &mut c1) == result::OK {
                    ClassInfo {
                        cid: c1.cid,
                        category: text_of(&c1.category),
                        name: text_of(&c1.name),
                        vendor: String::new(),
                        sub_categories: String::new(),
                        version: String::new(),
                    }
                } else {
                    continue;
                };
                classes.push(class);
            }
            (vendor, classes)
        };
        let classes = classes
            .into_iter()
            .map(|mut c| {
                if c.vendor.is_empty() {
                    c.vendor = vendor.clone();
                }
                c
            })
            .collect();
        Module {
            path,
            vendor,
            classes,
            factory,
            host,
            _library: library.map(std::mem::ManuallyDrop::new),
        }
    }
}

/// The folder of a bundle's libraries for this system and processor, as
/// the VST 3 bundle format names them.
fn architecture() -> String {
    let cpu = match std::env::consts::ARCH {
        "aarch64" if cfg!(windows) => "arm64",
        other => other,
    };
    format!("{cpu}-{}", if cfg!(windows) { "win" } else { "linux" })
}

/// The library to load for `path`: in a bundle (a folder) the one for
/// this system — `Contents/x86_64-win/Name.vst3`,
/// `Contents/x86_64-linux/Name.so` —, else the file itself.
pub fn library_of(path: &Path) -> PathBuf {
    if !path.is_dir() {
        return path.to_path_buf();
    }
    let name = if cfg!(windows) {
        path.file_name().map(PathBuf::from)
    } else {
        path.file_stem()
            .map(|s| PathBuf::from(s).with_extension("so"))
    };
    path.join("Contents")
        .join(architecture())
        .join(name.unwrap_or_default())
}

type Loaded = Result<Arc<Module>, Error>;

fn modules() -> &'static Mutex<HashMap<PathBuf, Loaded>> {
    static MODULES: OnceLock<Mutex<HashMap<PathBuf, Loaded>>> = OnceLock::new();
    MODULES.get_or_init(Default::default)
}

/// The module at `path` (a bundle or a library), loaded the first time it
/// is asked for; later calls get the same (or the same failure).
pub fn load(path: &Path) -> Loaded {
    let mut all = modules().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = all.get(path) {
        return m.clone();
    }
    let loaded = load_library(path).map(Arc::new);
    all.insert(path.to_path_buf(), loaded.clone());
    loaded
}

/// A module whose factory `entry` gives: a plug-in linked into the
/// program (the tests' plug-ins), listed under `name`.
pub fn linked(name: &str, entry: GetFactoryProc) -> Loaded {
    let path = PathBuf::from(format!("<linked>/{name}"));
    let mut all = modules().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = all.get(&path) {
        return m.clone();
    }
    // SAFETY: the plug-in's entry point, as a host calls it.
    let factory = unsafe { ComPtr::from_raw(entry()) }.ok_or(Error::NoFactory);
    let loaded = factory.map(|f| Arc::new(Module::from_factory(path.clone(), f, None)));
    all.insert(path, loaded.clone());
    loaded
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetThreadErrorMode(mode: u32, old: *mut u32) -> i32;
}

/// Windows' own error boxes kept off this thread while it lives: a library
/// whose own libraries are missing, or are another program's of the same
/// name, fails to load with the error saying so instead of the system's
/// "Entry Point Not Found" box.
#[cfg(windows)]
struct QuietErrors(u32);

#[cfg(windows)]
impl QuietErrors {
    fn new() -> QuietErrors {
        let mut old = 0;
        // SAFETY: the thread's error mode set (SEM_FAILCRITICALERRORS |
        // SEM_NOOPENFILEERRORBOX), the old one written to `old`.
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

fn load_library(path: &Path) -> Result<Module, Error> {
    let file = library_of(path);
    #[cfg(windows)]
    let _quiet = QuietErrors::new();
    #[cfg(windows)]
    // SAFETY: loading runs the library's initialisers: loading is what the
    // user asked for. Its own folder is searched for the libraries it
    // needs.
    let library = unsafe {
        libloading::os::windows::Library::load_with_flags(
            &file,
            libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    }
    .map(libloading::Library::from)
    .map_err(|e| Error::Load(e.to_string()))?;
    #[cfg(unix)]
    // SAFETY: as above.
    let library = unsafe {
        libloading::os::unix::Library::open(
            Some(&file),
            libloading::os::unix::RTLD_LAZY | libloading::os::unix::RTLD_LOCAL,
        )
    }
    .map(libloading::Library::from)
    .map_err(|e| Error::Load(e.to_string()))?;
    // SAFETY: the module's entry points, with the types the SDK gives them.
    unsafe {
        let entry: GetFactoryProc = *library
            .get::<GetFactoryProc>(b"GetPluginFactory\0")
            .map_err(|_| Error::NoFactory)?;
        #[cfg(windows)]
        if let Ok(init) = library.get::<unsafe extern "system" fn() -> u8>(b"InitDll\0")
            && init() == 0
        {
            return Err(Error::Refused("to start (InitDll)", 0));
        }
        // Linux's modules must be told their library's handle.
        #[cfg(unix)]
        let library = {
            let module_entry = *library
                .get::<unsafe extern "system" fn(*mut std::ffi::c_void) -> u8>(b"ModuleEntry\0")
                .map_err(|_| Error::NoFactory)?;
            let handle = libloading::os::unix::Library::from(library).into_raw();
            let library =
                libloading::Library::from(libloading::os::unix::Library::from_raw(handle));
            if module_entry(handle) == 0 {
                return Err(Error::Refused("to start (ModuleEntry)", 0));
            }
            library
        };
        let factory = ComPtr::from_raw(entry()).ok_or(Error::NoFactory)?;
        Ok(Module::from_factory(
            path.to_path_buf(),
            factory,
            Some(library),
        ))
    }
}
