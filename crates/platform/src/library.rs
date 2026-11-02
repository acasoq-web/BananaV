//! What a program library (a plug-in) says about itself without running
//! it: the processor it is for and the functions it exports, read from its
//! headers (PE on Windows, ELF on Linux), and the texts of its version
//! resource (who made it, what it is; Windows libraries carry one).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// A program library's headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    /// Made for this computer's processor (a 64-bit library on a 64-bit
    /// BananaV): it could be loaded.
    pub native: bool,
    /// Made for a 64-bit processor.
    pub bits64: bool,
    /// The names of the functions it exports.
    pub exports: Vec<String>,
}

/// The headers of the library at `path`; `None` when it is not one (not a
/// PE or ELF file, or unreadable).
pub fn read_library(path: &Path) -> Option<Library> {
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut magic)
        .ok()?;
    if &magic == b"\x7fELF" {
        return read_elf(path);
    }
    let mut f = std::fs::File::open(path).ok()?;
    // `len` bytes from `pos`; with `exact` off, as many as there are.
    let mut read = |pos: u64, len: usize, exact: bool| -> Option<Vec<u8>> {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut buf = Vec::with_capacity(len);
        (&mut f).take(len as u64).read_to_end(&mut buf).ok()?;
        (!exact || buf.len() == len).then_some(buf)
    };
    let u16_at = |b: &[u8], i: usize| -> Option<u16> {
        Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
    };
    let u32_at = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
    };
    let dos = read(0, 64, true)?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    let pe = u32_at(&dos, 0x3C)? as u64;
    let head = read(pe, 24, true)?;
    if &head[..4] != b"PE\0\0" {
        return None;
    }
    let machine = u16_at(&head, 4)?;
    let sections = u16_at(&head, 6)? as usize;
    let optional_size = u16_at(&head, 20)? as usize;
    let optional = read(pe + 24, optional_size, true)?;
    let bits64 = match u16_at(&optional, 0)? {
        0x20B => true,
        0x10B => false,
        _ => return None,
    };
    // The export table: the first data directory.
    let dirs = if bits64 { 112 } else { 96 };
    let (export_rva, export_size) = if u32_at(&optional, dirs - 4)? > 0 {
        (u32_at(&optional, dirs)?, u32_at(&optional, dirs + 4)?)
    } else {
        (0, 0)
    };
    let table = read(pe + 24 + optional_size as u64, sections * 40, true)?;
    let sections: Vec<(u32, u32, u32)> = table
        .as_chunks::<40>()
        .0
        .iter()
        .filter_map(|s| {
            let size = u32_at(s, 8)?.max(u32_at(s, 16)?);
            Some((u32_at(s, 12)?, size, u32_at(s, 20)?))
        })
        .collect();
    let offset = |rva: u32| -> Option<u64> {
        sections
            .iter()
            .find(|&&(va, size, _)| rva >= va && rva - va < size)
            .map(|&(va, _, raw)| (rva - va + raw) as u64)
    };
    let mut exports = Vec::new();
    if export_rva > 0 && export_size >= 40 {
        let dir = read(offset(export_rva)?, 40, true)?;
        let count = (u32_at(&dir, 24)? as usize).min(65_536);
        let names_rva = u32_at(&dir, 32)?;
        if count > 0 {
            let names = read(offset(names_rva)?, count * 4, true)?;
            for i in 0..count {
                let Some(pos) = u32_at(&names, i * 4).and_then(offset) else {
                    continue;
                };
                let Some(raw) = read(pos, 256, false) else {
                    continue;
                };
                let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                exports.push(String::from_utf8_lossy(&raw[..end]).into_owned());
            }
        }
    }
    let native = match machine {
        0x8664 => cfg!(target_arch = "x86_64"),
        0xAA64 => cfg!(target_arch = "aarch64"),
        0x014C => cfg!(target_arch = "x86"),
        _ => false,
    };
    Some(Library {
        native,
        bits64,
        exports,
    })
}

/// An ELF shared library's headers: its processor and the functions its
/// dynamic symbol table defines (64-bit little-endian files; Linux
/// plug-ins).
fn read_elf(path: &Path) -> Option<Library> {
    let data = std::fs::read(path).ok()?;
    let u16_at = |i: usize| Some(u16::from_le_bytes(data.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?));
    let u64_at = |i: usize| Some(u64::from_le_bytes(data.get(i..i + 8)?.try_into().ok()?));
    let bits64 = match data.get(4)? {
        2 => true,
        1 => false,
        _ => return None,
    };
    let machine = u16_at(18)?;
    let native = match machine {
        62 => cfg!(target_arch = "x86_64"),
        183 => cfg!(target_arch = "aarch64"),
        3 => cfg!(target_arch = "x86"),
        _ => false,
    };
    let mut exports = Vec::new();
    // Little-endian 64-bit files: the section headers, the dynamic symbol
    // table (type 11) and its string table.
    if bits64 && data.get(5) == Some(&1) {
        let (shoff, shentsize, shnum) = (
            u64_at(0x28)? as usize,
            u16_at(0x3A)? as usize,
            u16_at(0x3C)? as usize,
        );
        let section = |i: usize| {
            let at = shoff + i * shentsize;
            Some((
                u32_at(at + 4)?,
                u64_at(at + 24)? as usize,
                u64_at(at + 32)? as usize,
                u32_at(at + 40)? as usize,
                u64_at(at + 56)? as usize,
            ))
        };
        let dynsym = (0..shnum.min(4096)).filter_map(section).find(|s| s.0 == 11);
        if let Some((_, off, size, link, entsize)) = dynsym
            && let Some((_, str_off, str_size, _, _)) = section(link)
        {
            let entsize = if entsize == 0 { 24 } else { entsize };
            for sym in (off..off + size).step_by(entsize).take(65_536) {
                let (Some(name), Some(&info), Some(shndx)) =
                    (u32_at(sym), data.get(sym + 4), u16_at(sym + 6))
                else {
                    break;
                };
                // Defined functions, global or weak.
                let function = info & 0xF == 2;
                let visible = matches!(info >> 4, 1 | 2);
                if !function || !visible || shndx == 0 || name as usize >= str_size {
                    continue;
                }
                let start = str_off + name as usize;
                let Some(bytes) = data.get(start..(str_off + str_size).min(data.len())) else {
                    continue;
                };
                let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
                exports.push(String::from_utf8_lossy(&bytes[..end]).into_owned());
            }
        }
    }
    Some(Library {
        native,
        bits64,
        exports,
    })
}

/// What a file's version resource says about it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VersionTexts {
    pub company: Option<String>,
    pub product: Option<String>,
    pub description: Option<String>,
    /// Its file version ("10.0.26100.1").
    pub version: Option<String>,
}

/// The texts of the version resource of the file at `path` (Windows
/// programs and libraries carry one); `None` without one.
pub fn version_texts(path: &Path) -> Option<VersionTexts> {
    imp::version_texts(path)
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::core::PCWSTR;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn version_texts(path: &Path) -> Option<super::VersionTexts> {
        let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `file` is a NUL-terminated wide string.
        let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: `data` is `size` bytes, writable.
        unsafe { GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr().cast()) }
            .ok()?;
        // A value's bytes (`chars`: its length counts characters).
        let query = |sub: &str, chars: bool| -> Option<&[u8]> {
            let sub = wide(sub);
            let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut len = 0u32;
            // SAFETY: `data` holds the version resource read above; the
            // pointer given back points into it.
            let ok = unsafe {
                VerQueryValueW(
                    data.as_ptr().cast(),
                    PCWSTR(sub.as_ptr()),
                    &mut ptr,
                    &mut len,
                )
            };
            if !ok.as_bool() || ptr.is_null() || len == 0 {
                return None;
            }
            let start = ptr as usize - data.as_ptr() as usize;
            let bytes = if chars {
                len as usize * 2
            } else {
                len as usize
            };
            data.get(start..).map(|rest| &rest[..rest.len().min(bytes)])
        };
        // The languages it has texts in; then the usual English ones.
        let mut languages: Vec<String> = query("\\VarFileInfo\\Translation", false)
            .map(|t| {
                t.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| {
                        let lang = u16::from_le_bytes([c[0], c[1]]);
                        let page = u16::from_le_bytes([c[2], c[3]]);
                        format!("{lang:04x}{page:04x}")
                    })
                    .collect()
            })
            .unwrap_or_default();
        languages.extend(["040904b0".into(), "040904e4".into(), "000004b0".into()]);
        let text = |name: &str| -> Option<String> {
            languages.iter().find_map(|l| {
                let raw = query(&format!("\\StringFileInfo\\{l}\\{name}"), true)?;
                let chars: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_le_bytes(c))
                    .take_while(|&c| c != 0)
                    .collect();
                let s = String::from_utf16_lossy(&chars).trim().to_string();
                (!s.is_empty()).then_some(s)
            })
        };
        Some(super::VersionTexts {
            company: text("CompanyName"),
            product: text("ProductName"),
            description: text("FileDescription"),
            version: text("FileVersion"),
        })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn version_texts(_path: &std::path::Path) -> Option<super::VersionTexts> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn system_library(name: &str) -> std::path::PathBuf {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        std::path::Path::new(&root).join("System32").join(name)
    }

    #[test]
    fn a_system_library_read_without_running_it() {
        let path = system_library("kernel32.dll");
        let lib = read_library(&path).expect("a library");
        assert!(lib.native && lib.bits64);
        assert!(
            lib.exports.iter().any(|e| e == "GetTickCount"),
            "its exports"
        );
        let texts = version_texts(&path).expect("a version resource");
        assert!(texts.company.is_some_and(|c| c.contains("Microsoft")));
        // Not a library.
        let not = std::env::temp_dir().join("bananav-not-a-library.txt");
        std::fs::write(&not, b"plain text, not a program").unwrap();
        assert_eq!(read_library(&not), None);
        let _ = std::fs::remove_file(&not);
    }
}
