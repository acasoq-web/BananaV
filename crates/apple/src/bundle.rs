//! Bundles: a folder holding a program's or a plug-in's executable and its
//! resources (`Name.vst3`, `Name.vst`, `Name.ofx.bundle`, BananaV.app),
//! read through Core Foundation's `CFBundle`, which knows where the
//! executable is (Info.plist's `CFBundleExecutable`) and loads it.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use crate::cf::{self, Owned, Ref};

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFBundleCreate(alloc: Ref, url: Ref) -> Ref;
    fn CFBundleGetMainBundle() -> Ref;
    fn CFBundleCopyExecutableURL(bundle: Ref) -> Ref;
    fn CFBundleGetValueForInfoDictionaryKey(bundle: Ref, key: Ref) -> Ref;
    fn CFBundleGetIdentifier(bundle: Ref) -> Ref;
    fn CFBundleLoadExecutableAndReturnError(bundle: Ref, error: *mut Ref) -> u8;
    fn CFBundleIsExecutableLoaded(bundle: Ref) -> u8;
    fn CFBundleGetFunctionPointerForName(bundle: Ref, name: Ref) -> *mut c_void;
    fn CFBundleCopyExecutableArchitectures(bundle: Ref) -> Ref;
}

/// A bundle.
#[derive(Debug)]
pub struct Bundle {
    bundle: Owned,
    path: PathBuf,
}

impl Bundle {
    /// The bundle at `path` (a folder); `None` when it is none.
    pub fn open(path: &Path) -> Option<Bundle> {
        let url = cf::file_url(path, true)?;
        // SAFETY: a live URL; a new bundle object, owned.
        let bundle = unsafe { Owned::take(CFBundleCreate(std::ptr::null(), url.get())) }?;
        Some(Bundle {
            bundle,
            path: path.to_path_buf(),
        })
    }

    /// The application's own bundle (BananaV.app; when it runs from a
    /// plain folder, that folder, with no identifier).
    pub fn main() -> Option<Bundle> {
        // SAFETY: the main bundle, retained for this object.
        let bundle = unsafe { Owned::retain(CFBundleGetMainBundle()) }?;
        let path = std::env::current_exe().ok()?;
        Some(Bundle { bundle, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Its executable (`Contents/MacOS/Name`), when it has one.
    pub fn executable(&self) -> Option<PathBuf> {
        // SAFETY: a live bundle; the URL is the caller's.
        let url = unsafe { Owned::take(CFBundleCopyExecutableURL(self.bundle.get())) }?;
        // SAFETY: a live URL.
        unsafe { cf::url_path(url.get()) }
    }

    /// Info.plist's text for `key` (`CFBundleShortVersionString`…).
    pub fn info(&self, key: &str) -> Option<String> {
        let key = cf::string(key);
        // SAFETY: a lookup in the live bundle's dictionary (borrowed); its
        // value lives as long as the bundle.
        unsafe {
            let value = CFBundleGetValueForInfoDictionaryKey(self.bundle.get(), key.get());
            cf::to_string(value)
        }
    }

    /// Its identifier (Info.plist's `CFBundleIdentifier`).
    pub fn identifier(&self) -> Option<String> {
        // SAFETY: a live bundle's identifier (borrowed).
        unsafe { cf::to_string(CFBundleGetIdentifier(self.bundle.get())) }
    }

    /// The processors its executable has code for (Mach-O's numbers,
    /// `crate::macho::CPU_ARM64`…).
    pub fn architectures(&self) -> Vec<u32> {
        // SAFETY: a live bundle; the array is the caller's.
        let Some(list) =
            (unsafe { Owned::take(CFBundleCopyExecutableArchitectures(self.bundle.get())) })
        else {
            return Vec::new();
        };
        // SAFETY: a live array of numbers, borrowed while it lives.
        unsafe {
            cf::items(list.get())
                .into_iter()
                .filter_map(|n| cf::to_i64(n).map(|v| v as u32))
                .collect()
        }
    }

    /// Loads its executable (running its initialisers); the system's
    /// reason when it cannot.
    pub fn load(&self) -> Result<(), String> {
        let mut error: Ref = std::ptr::null();
        // SAFETY: a live bundle; the error, if any, is the caller's.
        let ok = unsafe { CFBundleLoadExecutableAndReturnError(self.bundle.get(), &mut error) };
        // SAFETY: as above.
        let error = unsafe { Owned::take(error) };
        if ok != 0 {
            return Ok(());
        }
        // SAFETY: a live error object.
        Err(error
            .and_then(|e| unsafe { cf::error_text(e.get()) })
            .unwrap_or_else(|| "the bundle would not load".to_string()))
    }

    /// Whether its executable is loaded.
    pub fn loaded(&self) -> bool {
        // SAFETY: a plain query of a live bundle.
        unsafe { CFBundleIsExecutableLoaded(self.bundle.get()) != 0 }
    }

    /// The function its loaded executable exports as `name` (as C names it,
    /// without Mach-O's underscore: `GetPluginFactory`).
    pub fn function(&self, name: &str) -> Option<std::ptr::NonNull<c_void>> {
        let name = cf::string(name);
        // SAFETY: a lookup in a live bundle.
        std::ptr::NonNull::new(unsafe {
            CFBundleGetFunctionPointerForName(self.bundle.get(), name.get())
        })
    }

    /// The `CFBundleRef` itself (what VST 3's `bundleEntry` takes).
    pub fn raw(&self) -> *mut c_void {
        self.bundle.get().cast_mut()
    }

    /// Keeps it for as long as the program runs (a plug-in's code is never
    /// unloaded): its `CFBundleRef`.
    pub fn leak(self) -> *mut c_void {
        self.bundle.leak().cast_mut()
    }
}
