//! Core Foundation's plain values, as BananaV uses them: strings, numbers,
//! booleans, dictionaries, arrays and file URLs. [`Owned`] releases what
//! a `Create` or `Copy` function gave; the other references are borrowed
//! from the object holding them.

use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};

/// Any Core Foundation object (`CFTypeRef`).
pub type Ref = *const c_void;

const UTF8: u32 = 0x0800_0100;
const NUMBER_SINT64: isize = 4;
const NUMBER_FLOAT64: isize = 6;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRelease(cf: Ref);
    pub fn CFRetain(cf: Ref) -> Ref;
    fn CFGetTypeID(cf: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFStringCreateWithBytes(
        alloc: Ref,
        bytes: *const u8,
        len: isize,
        encoding: u32,
        external: u8,
    ) -> Ref;
    fn CFStringGetLength(s: Ref) -> isize;
    fn CFStringGetMaximumSizeForEncoding(len: isize, encoding: u32) -> isize;
    fn CFStringGetCString(s: Ref, buf: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFNumberGetValue(n: Ref, kind: isize, out: *mut c_void) -> u8;
    fn CFNumberCreate(alloc: Ref, kind: isize, value: *const c_void) -> Ref;
    fn CFBooleanGetValue(b: Ref) -> u8;
    fn CFDictionaryGetValue(d: Ref, key: Ref) -> Ref;
    fn CFArrayGetCount(a: Ref) -> isize;
    fn CFArrayGetValueAtIndex(a: Ref, i: isize) -> Ref;
    fn CFURLCreateFromFileSystemRepresentation(
        alloc: Ref,
        buf: *const u8,
        len: isize,
        is_dir: u8,
    ) -> Ref;
    fn CFURLGetFileSystemRepresentation(url: Ref, resolve: u8, buf: *mut u8, max: isize) -> u8;
    fn CFErrorCopyDescription(e: Ref) -> Ref;
}

/// A reference BananaV owns: released when dropped.
#[derive(Debug)]
pub struct Owned(Ref);

// SAFETY: Core Foundation's immutable objects (strings, numbers, URLs,
// bundles) may be used and released from any thread.
unsafe impl Send for Owned {}
// SAFETY: as above.
unsafe impl Sync for Owned {}

impl Owned {
    /// Takes over `r` (from a `Create` or `Copy` function); `None` for null.
    ///
    /// # Safety
    /// `r` is null or a reference the caller owns.
    pub unsafe fn take(r: Ref) -> Option<Owned> {
        (!r.is_null()).then_some(Owned(r))
    }

    /// A reference of its own to `r`, which someone else owns.
    ///
    /// # Safety
    /// `r` is null or a live Core Foundation object.
    pub unsafe fn retain(r: Ref) -> Option<Owned> {
        if r.is_null() {
            return None;
        }
        // SAFETY: as the caller says.
        Some(Owned(unsafe { CFRetain(r) }))
    }

    pub fn get(&self) -> Ref {
        self.0
    }

    /// Lets it go without releasing it (kept for as long as the program
    /// runs).
    pub fn leak(self) -> Ref {
        let r = self.0;
        std::mem::forget(self);
        r
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: a reference this owns.
        unsafe { CFRelease(self.0) };
    }
}

/// A string of Core Foundation's.
pub fn string(text: &str) -> Owned {
    // SAFETY: the bytes and their length; a new string, owned.
    let r = unsafe {
        CFStringCreateWithBytes(
            std::ptr::null(),
            text.as_ptr(),
            text.len() as isize,
            UTF8,
            0,
        )
    };
    // SAFETY: as above (only memory running out gives null).
    unsafe { Owned::take(r) }.expect("a Core Foundation string")
}

/// A number of Core Foundation's.
pub fn number(value: i64) -> Owned {
    // SAFETY: a 64-bit integer read; a new number, owned.
    let r = unsafe { CFNumberCreate(std::ptr::null(), NUMBER_SINT64, (&raw const value).cast()) };
    // SAFETY: as above.
    unsafe { Owned::take(r) }.expect("a Core Foundation number")
}

/// The type of object `r` is.
fn is(r: Ref, type_id: usize) -> bool {
    // SAFETY: a live object (callers pass none other).
    !r.is_null() && unsafe { CFGetTypeID(r) } == type_id
}

/// The text of `r` when it is a string.
///
/// # Safety
/// `r` is null or a live Core Foundation object (so for each function
/// here taking a reference).
pub unsafe fn to_string(r: Ref) -> Option<String> {
    // SAFETY: a type's id, a plain query.
    if !is(r, unsafe { CFStringGetTypeID() }) {
        return None;
    }
    // SAFETY: a string's length and the buffer's size it needs in UTF-8;
    // the text copied into a buffer of that size, ended by a 0.
    unsafe {
        let len = CFStringGetLength(r);
        let size = CFStringGetMaximumSizeForEncoding(len, UTF8) + 1;
        let mut buf = vec![0u8; size.max(1) as usize];
        if CFStringGetCString(r, buf.as_mut_ptr().cast(), buf.len() as isize, UTF8) == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(end);
        String::from_utf8(buf).ok()
    }
}

/// The value of `r` when it is a number, as an integer.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn to_i64(r: Ref) -> Option<i64> {
    // SAFETY: a type's id, then a number's value into an integer.
    unsafe {
        if !is(r, CFNumberGetTypeID()) {
            return None;
        }
        let mut v = 0i64;
        CFNumberGetValue(r, NUMBER_SINT64, (&raw mut v).cast());
        Some(v)
    }
}

/// The value of `r` when it is a number, as a float.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn to_f64(r: Ref) -> Option<f64> {
    // SAFETY: as in `to_i64`.
    unsafe {
        if !is(r, CFNumberGetTypeID()) {
            return None;
        }
        let mut v = 0f64;
        CFNumberGetValue(r, NUMBER_FLOAT64, (&raw mut v).cast());
        Some(v)
    }
}

/// The value of `r` when it is a boolean.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn to_bool(r: Ref) -> Option<bool> {
    // SAFETY: a type's id, then a boolean's value.
    unsafe { is(r, CFBooleanGetTypeID()).then(|| CFBooleanGetValue(r) != 0) }
}

/// `dict`'s value for `key` (borrowed from it), when `dict` is a dictionary.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn get(dict: Ref, key: &str) -> Ref {
    // SAFETY: a type's id.
    if !is(dict, unsafe { CFDictionaryGetTypeID() }) {
        return std::ptr::null();
    }
    let key = string(key);
    // SAFETY: a lookup in a live dictionary with a live key.
    unsafe { CFDictionaryGetValue(dict, key.get()) }
}

/// `dict`'s value for the key `key` (an object of the system's, such as
/// one of its constants), when `dict` is a dictionary.
///
/// # Safety
/// As [`to_string`], for both.
pub unsafe fn get_by(dict: Ref, key: Ref) -> Ref {
    // SAFETY: a type's id.
    if key.is_null() || !is(dict, unsafe { CFDictionaryGetTypeID() }) {
        return std::ptr::null();
    }
    // SAFETY: a lookup in a live dictionary with a live key.
    unsafe { CFDictionaryGetValue(dict, key) }
}

/// The items of `array` (borrowed from it), when it is an array.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn items(array: Ref) -> Vec<Ref> {
    // SAFETY: a type's id, its count, then each item.
    unsafe {
        if !is(array, CFArrayGetTypeID()) {
            return Vec::new();
        }
        (0..CFArrayGetCount(array))
            .map(|i| CFArrayGetValueAtIndex(array, i))
            .collect()
    }
}

/// A file URL of `path` (a folder's when `dir`).
pub fn file_url(path: &Path, dir: bool) -> Option<Owned> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: the path's bytes and length; a new URL, owned.
    unsafe {
        Owned::take(CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            bytes.as_ptr(),
            bytes.len() as isize,
            u8::from(dir),
        ))
    }
}

/// The path of the file URL `url`.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn url_path(url: Ref) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    if url.is_null() {
        return None;
    }
    let mut buf = vec![0u8; 4096];
    // SAFETY: a live URL, a buffer of the size given.
    let ok =
        unsafe { CFURLGetFileSystemRepresentation(url, 1, buf.as_mut_ptr(), buf.len() as isize) };
    if ok == 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buf[..end])))
}

/// What an error object (`CFErrorRef`) says.
///
/// # Safety
/// As [`to_string`].
pub unsafe fn error_text(error: Ref) -> Option<String> {
    if error.is_null() {
        return None;
    }
    // SAFETY: a live error's description, owned.
    let text = unsafe { Owned::take(CFErrorCopyDescription(error)) }?;
    // SAFETY: a live string.
    unsafe { to_string(text.get()) }
}
