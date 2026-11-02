//! The computer's fonts, for Titles & Text: found in the system's font
//! folders (and the user's), named from their `name` tables and weighed
//! from their `OS/2` tables — only those tables are read, so finding them
//! all is quick — and their files read whole when text needs them.
//! Outlines and metrics come from `skrifa` (TrueType and CFF outlines).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// A face of a font family in a file (`index`: its place in a
/// collection).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Face {
    pub path: PathBuf,
    pub index: u32,
    pub family: String,
    /// 100 thin … 400 regular … 700 bold … 900 black.
    pub weight: u16,
    pub italic: bool,
}

/// Every face of the computer's fonts, found the first time they are
/// asked for.
pub(crate) fn faces() -> &'static [Face] {
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut files = Vec::new();
        for dir in folders() {
            font_files(&dir, &mut files, 0);
        }
        files.sort();
        files.dedup();
        let mut out = Vec::new();
        for f in files {
            out.extend(read_faces(&f).unwrap_or_default());
        }
        out
    })
}

/// The families, sorted as people read them (case aside).
pub(crate) fn families() -> Vec<String> {
    let mut names: Vec<String> = faces().iter().map(|f| f.family.clone()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

/// The family a new title starts with: Arial where there is one (as on
/// Windows), else a sans-serif most systems have, else the first.
pub(crate) fn default_family() -> String {
    let all = families();
    for want in [
        "Arial",
        "Liberation Sans",
        "Arimo",
        "DejaVu Sans",
        "Helvetica",
        "Noto Sans",
    ] {
        if let Some(f) = all.iter().find(|f| f.eq_ignore_ascii_case(want)) {
            return f.clone();
        }
    }
    all.into_iter().next().unwrap_or_else(|| "Arial".into())
}

/// A face of `family` for `bold` and `italic`, and what has to be made
/// up because the family lacks it: `(face, embolden, slant)`. Another
/// family stands in when `family` is not on the computer.
pub(crate) fn find(family: &str, bold: bool, italic: bool) -> Option<(&'static Face, bool, bool)> {
    let all = faces();
    let mut of: Vec<&Face> = all
        .iter()
        .filter(|f| f.family.eq_ignore_ascii_case(family))
        .collect();
    if of.is_empty() {
        let stand_in = default_family();
        of = all
            .iter()
            .filter(|f| f.family.eq_ignore_ascii_case(&stand_in))
            .collect();
    }
    if of.is_empty() {
        of = all.iter().collect();
    }
    let want = if bold { 700 } else { 400 };
    let best = of.iter().min_by_key(|f| {
        let slant = if f.italic == italic { 0 } else { 10_000 };
        slant + (f.weight as i32 - want).unsigned_abs()
    })?;
    Some((best, bold && best.weight < 600, italic && !best.italic))
}

/// A font file's bytes, read once and kept.
pub(crate) fn data(path: &Path) -> Option<Arc<Vec<u8>>> {
    static FILES: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>> = OnceLock::new();
    let files = FILES.get_or_init(Default::default);
    if let Some(d) = files.lock().ok()?.get(path) {
        return Some(Arc::clone(d));
    }
    let bytes = Arc::new(std::fs::read(path).ok()?);
    files
        .lock()
        .ok()?
        .insert(path.to_path_buf(), Arc::clone(&bytes));
    Some(bytes)
}

/// Where the system and the user keep fonts.
fn folders() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Some(w) = env("WINDIR").or_else(|| env("SystemRoot")) {
            out.push(w.join("Fonts"));
        }
        if let Some(l) = env("LOCALAPPDATA") {
            out.push(l.join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        out.push("/System/Library/Fonts".into());
        out.push("/Library/Fonts".into());
        if let Some(h) = env("HOME") {
            out.push(h.join("Library").join("Fonts"));
        }
    } else {
        out.push("/usr/share/fonts".into());
        out.push("/usr/local/share/fonts".into());
        if let Some(d) = env("XDG_DATA_HOME") {
            out.push(d.join("fonts"));
        }
        if let Some(h) = env("HOME") {
            out.push(h.join(".local").join("share").join("fonts"));
            out.push(h.join(".fonts"));
        }
    }
    out
}

/// The font files in `dir` and its folders.
fn font_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 6 {
                font_files(&p, out, depth + 1);
            }
            continue;
        }
        let ext = p
            .extension()
            .and_then(|x| x.to_str())
            .map(str::to_ascii_lowercase);
        if matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
            out.push(p);
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// `len` bytes of `file` from `at`.
fn read_at(file: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// The faces in a font file: one, or each of a collection's.
fn read_faces(path: &Path) -> Option<Vec<Face>> {
    let mut file = File::open(path).ok()?;
    let head = read_at(&mut file, 0, 12)?;
    let starts: Vec<u32> = if &head[..4] == b"ttcf" {
        let n = u32_at(&head, 8)?.min(256) as usize;
        let table = read_at(&mut file, 12, 4 * n)?;
        (0..n).filter_map(|i| u32_at(&table, 4 * i)).collect()
    } else {
        vec![0]
    };
    let mut out = Vec::new();
    for (index, start) in starts.into_iter().enumerate() {
        if let Some(f) = read_face(&mut file, start as u64, path, index as u32) {
            out.push(f);
        }
    }
    Some(out)
}

/// A face whose table directory starts at `start`.
fn read_face(file: &mut File, start: u64, path: &Path, index: u32) -> Option<Face> {
    let dir = read_at(file, start, 12)?;
    let tables = u16_at(&dir, 4)? as usize;
    let records = read_at(file, start + 12, 16 * tables.min(512))?;
    let mut name = None;
    let mut os2 = None;
    for i in 0..tables.min(512) {
        let r = &records[16 * i..16 * i + 16];
        let (offset, len) = (u32_at(r, 8)? as u64, u32_at(r, 12)? as usize);
        match &r[..4] {
            b"name" => name = Some((offset, len)),
            b"OS/2" => os2 = Some((offset, len)),
            _ => {}
        }
    }
    let (offset, len) = name?;
    let name = read_at(file, offset, len.min(1 << 20))?;
    let family = family_name(&name)?;
    let (weight, italic) = match os2 {
        Some((offset, len)) if len >= 64 => {
            let t = read_at(file, offset, 64)?;
            let weight = u16_at(&t, 4)?.clamp(1, 1000);
            let selection = u16_at(&t, 62)?;
            (weight, selection & 1 != 0)
        }
        _ => {
            let sub = name_string(&name, 2).unwrap_or_default().to_lowercase();
            let weight = if sub.contains("bold") { 700 } else { 400 };
            (weight, sub.contains("italic") || sub.contains("oblique"))
        }
    };
    Some(Face {
        path: path.to_path_buf(),
        index,
        family,
        weight,
        italic,
    })
}

/// The family's name: the typographic one (16) when there is one, as
/// families with many weights name it, else the legacy one (1).
fn family_name(table: &[u8]) -> Option<String> {
    name_string(table, 16)
        .or_else(|| name_string(table, 1))
        .filter(|n| !n.trim().is_empty() && !n.starts_with('.'))
}

/// Name `id` from a `name` table: Windows' Unicode in American English
/// first, then any Windows Unicode one, then the Macintosh Roman one.
fn name_string(t: &[u8], id: u16) -> Option<String> {
    let count = u16_at(t, 2)? as usize;
    let strings = u16_at(t, 4)? as usize;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let r = 6 + 12 * i;
        let (platform, encoding, language, name_id) = (
            u16_at(t, r)?,
            u16_at(t, r + 2)?,
            u16_at(t, r + 4)?,
            u16_at(t, r + 6)?,
        );
        if name_id != id {
            continue;
        }
        let (len, offset) = (u16_at(t, r + 8)? as usize, u16_at(t, r + 10)? as usize);
        let Some(bytes) = t.get(strings + offset..strings + offset + len) else {
            continue;
        };
        let (rank, text) = match (platform, encoding) {
            (3, 1 | 10) | (0, _) => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_be_bytes(c))
                    .collect();
                let rank = if platform == 3 && language == 0x0409 {
                    0
                } else {
                    1
                };
                (rank, String::from_utf16_lossy(&units))
            }
            (1, 0) => (2, bytes.iter().map(|&b| b as char).collect()),
            _ => continue,
        };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, s)| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `name` table with `records` (platform, encoding, language, id,
    /// text).
    fn name_table(records: &[(u16, u16, u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut t = Vec::new();
        t.extend(0u16.to_be_bytes());
        t.extend((records.len() as u16).to_be_bytes());
        t.extend(((6 + 12 * records.len()) as u16).to_be_bytes());
        for &(p, e, l, id, s) in records {
            let bytes: Vec<u8> = if p == 1 {
                s.bytes().collect()
            } else {
                s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
            };
            for v in [p, e, l, id, bytes.len() as u16, strings.len() as u16] {
                t.extend(v.to_be_bytes());
            }
            strings.extend(bytes);
        }
        t.extend(strings);
        t
    }

    #[test]
    fn families_come_from_the_name_table() {
        let t = name_table(&[
            (1, 0, 0, 1, "Mac Name"),
            (3, 1, 0x0416, 1, "Nome"),
            (3, 1, 0x0409, 1, "Sans Light"),
            (3, 1, 0x0409, 16, "Sans"),
            (3, 1, 0x0409, 2, "Light"),
        ]);
        assert_eq!(family_name(&t).as_deref(), Some("Sans"));
        assert_eq!(name_string(&t, 2).as_deref(), Some("Light"));
        let t = name_table(&[(1, 0, 0, 1, "Mac Name"), (3, 1, 0x0416, 1, "Nome")]);
        assert_eq!(family_name(&t).as_deref(), Some("Nome"));
        let t = name_table(&[(1, 0, 0, 1, "Mac Name")]);
        assert_eq!(family_name(&t).as_deref(), Some("Mac Name"));
    }

    #[test]
    #[cfg(windows)]
    fn windows_has_arial_with_its_faces() {
        assert_eq!(default_family(), "Arial");
        let (face, embolden, slant) = find("arial", true, true).expect("Arial");
        assert!(face.weight >= 600 && face.italic, "{face:?}");
        assert!(!embolden && !slant);
    }

    #[test]
    fn the_face_nearest_the_weight_and_slant_is_picked() {
        // Whatever the computer has: a bold of a family with one is its
        // own; a missing italic is slanted.
        let Some(family) = families().into_iter().next() else {
            return;
        };
        let (face, _, _) = find(&family, false, false).expect("a face");
        assert!(face.family.eq_ignore_ascii_case(&family));
        let (_, embolden, _) = find(&family, true, false).expect("a face");
        let has_bold = faces()
            .iter()
            .any(|f| f.family.eq_ignore_ascii_case(&family) && f.weight >= 600);
        assert_eq!(embolden, !has_bold);
        assert!(find("No Such Family Here", false, false).is_some());
    }
}
