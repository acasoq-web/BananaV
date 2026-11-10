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
    /// How wide its letters are: 1 ultra-condensed … 5 normal … 9
    /// ultra-expanded (a family's narrow faces share its name).
    pub stretch: u16,
    /// The blocks of Unicode it says it covers (its `OS/2` table's 128
    /// `ulUnicodeRange` bits; none when it has no such table).
    pub ranges: [u32; 4],
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
    // Its normal width first (Arial Narrow's faces are Arial's too), then
    // the slant, then the nearest weight.
    let want = if bold { 700 } else { 400 };
    let best = of.iter().min_by_key(|f| {
        let width = (f.stretch as i32 - 5).unsigned_abs() * 100_000;
        let slant = if f.italic == italic { 0 } else { 10_000 };
        width + slant + (f.weight as i32 - want).unsigned_abs()
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

/// Whose way Han characters are drawn in — Chinese, Japanese and Korean
/// fonts draw many of them differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cjk {
    Chinese,
    /// Traditional Chinese's faces first (the interface in Traditional
    /// Chinese); a text's letters never say it ([`Cjk::of`] gives
    /// `Chinese`).
    ChineseTraditional,
    Japanese,
    Korean,
}

impl Cjk {
    /// A text's, by its letters: Japanese when it has kana, Korean when it
    /// has hangul, Chinese otherwise.
    pub fn of(text: impl IntoIterator<Item = char>) -> Cjk {
        let mut korean = false;
        for c in text {
            match c as u32 {
                0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => return Cjk::Japanese,
                0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => korean = true,
                _ => {}
            }
        }
        if korean { Cjk::Korean } else { Cjk::Chinese }
    }

    /// The families that draw its characters, as Windows, macOS and Linux
    /// have them, the usual first.
    fn families(self) -> &'static [&'static str] {
        match self {
            Cjk::ChineseTraditional => &[
                "Microsoft JhengHei",
                "Microsoft JhengHei UI",
                "PingFang TC",
                "Noto Sans CJK TC",
                "Noto Sans TC",
                "Source Han Sans TC",
                "Source Han Sans TW",
                "Heiti TC",
                "PMingLiU",
                "MingLiU",
            ],
            Cjk::Chinese => &[
                "Microsoft YaHei",
                "PingFang SC",
                "Hiragino Sans GB",
                "Noto Sans CJK SC",
                "Noto Sans SC",
                "Source Han Sans SC",
                "Source Han Sans CN",
                "WenQuanYi Micro Hei",
                "WenQuanYi Zen Hei",
                "Droid Sans Fallback",
                "SimHei",
                "SimSun",
                "Heiti SC",
                "STHeiti",
                "Arial Unicode MS",
            ],
            Cjk::Japanese => &[
                "Yu Gothic",
                "Meiryo",
                "MS Gothic",
                "Hiragino Sans",
                "Hiragino Kaku Gothic ProN",
                "Hiragino Kaku Gothic Pro",
                "Noto Sans CJK JP",
                "Noto Sans JP",
                "Source Han Sans JP",
                "Source Han Sans",
                "IPAexGothic",
                "IPAGothic",
                "TakaoGothic",
                "VL Gothic",
            ],
            Cjk::Korean => &[
                "Malgun Gothic",
                "Apple SD Gothic Neo",
                "AppleGothic",
                "Noto Sans CJK KR",
                "Noto Sans KR",
                "Source Han Sans KR",
                "Source Han Sans K",
                "NanumGothic",
                "NanumBarunGothic",
                "UnDotum",
                "Gulim",
                "Dotum",
            ],
        }
    }
}

/// Families with many scripts (Greek, Cyrillic, Arabic, Hebrew, Thai,
/// Indian ones…) and symbols, tried before any face that only says it has
/// a character.
const WIDE: &[&str] = &[
    "Segoe UI",
    "Nirmala UI",
    "Leelawadee UI",
    "Ebrima",
    "Gadugi",
    "Segoe UI Historic",
    "Segoe UI Symbol",
    "Segoe UI Emoji",
    "Noto Sans",
    "DejaVu Sans",
    "FreeSans",
    "Lucida Grande",
    "Arial Unicode MS",
    "Noto Emoji",
    "Symbola",
];

/// The `ulUnicodeRange` bit of the block `c` is in (the `OS/2` table's).
fn range_bit(c: char) -> Option<u8> {
    const BLOCKS: &[(u32, u32, u8)] = &[
        (0x0000, 0x007F, 0),
        (0x0080, 0x00FF, 1),
        (0x0100, 0x017F, 2),
        (0x0180, 0x024F, 3),
        (0x0250, 0x02AF, 4),
        (0x02B0, 0x02FF, 5),
        (0x0300, 0x036F, 6),
        (0x0370, 0x03FF, 7),
        (0x0400, 0x052F, 9),
        (0x0530, 0x058F, 10),
        (0x0590, 0x05FF, 11),
        (0x0600, 0x06FF, 13),
        (0x0700, 0x074F, 71),
        (0x0750, 0x077F, 13),
        (0x0780, 0x07BF, 72),
        (0x07C0, 0x07FF, 14),
        (0x0900, 0x097F, 15),
        (0x0980, 0x09FF, 16),
        (0x0A00, 0x0A7F, 17),
        (0x0A80, 0x0AFF, 18),
        (0x0B00, 0x0B7F, 19),
        (0x0B80, 0x0BFF, 20),
        (0x0C00, 0x0C7F, 21),
        (0x0C80, 0x0CFF, 22),
        (0x0D00, 0x0D7F, 23),
        (0x0D80, 0x0DFF, 73),
        (0x0E00, 0x0E7F, 24),
        (0x0E80, 0x0EFF, 25),
        (0x0F00, 0x0FFF, 70),
        (0x1000, 0x109F, 74),
        (0x10A0, 0x10FF, 26),
        (0x1100, 0x11FF, 28),
        (0x1200, 0x139F, 75),
        (0x13A0, 0x13FF, 76),
        (0x1400, 0x167F, 77),
        (0x1780, 0x17FF, 80),
        (0x1800, 0x18AF, 81),
        (0x1E00, 0x1EFF, 29),
        (0x1F00, 0x1FFF, 30),
        (0x2000, 0x206F, 31),
        (0x2070, 0x209F, 32),
        (0x20A0, 0x20CF, 33),
        (0x20D0, 0x20FF, 34),
        (0x2100, 0x214F, 35),
        (0x2150, 0x218F, 36),
        (0x2190, 0x21FF, 37),
        (0x2200, 0x22FF, 38),
        (0x2300, 0x23FF, 39),
        (0x2400, 0x243F, 40),
        (0x2440, 0x245F, 41),
        (0x2460, 0x24FF, 42),
        (0x2500, 0x257F, 43),
        (0x2580, 0x259F, 44),
        (0x25A0, 0x25FF, 45),
        (0x2600, 0x26FF, 46),
        (0x2700, 0x27BF, 47),
        (0x2800, 0x28FF, 82),
        (0x2C80, 0x2CFF, 8),
        (0x2DE0, 0x2DFF, 9),
        (0x2E80, 0x2FDF, 59),
        (0x3000, 0x303F, 48),
        (0x3040, 0x309F, 49),
        (0x30A0, 0x30FF, 50),
        (0x3100, 0x312F, 51),
        (0x3130, 0x318F, 52),
        (0x31A0, 0x31BF, 51),
        (0x31C0, 0x31EF, 61),
        (0x31F0, 0x31FF, 50),
        (0x3200, 0x32FF, 54),
        (0x3300, 0x33FF, 55),
        (0x3400, 0x4DBF, 59),
        (0x4E00, 0x9FFF, 59),
        (0xA000, 0xA4CF, 83),
        (0xA640, 0xA69F, 9),
        (0xAC00, 0xD7AF, 56),
        (0xF900, 0xFAFF, 61),
        (0xFB00, 0xFB4F, 62),
        (0xFB50, 0xFDFF, 63),
        (0xFE10, 0xFE1F, 65),
        (0xFE20, 0xFE2F, 64),
        (0xFE30, 0xFE4F, 65),
        (0xFE50, 0xFE6F, 66),
        (0xFE70, 0xFEFF, 67),
        (0xFF00, 0xFFEF, 68),
        (0x20000, 0x2FA1F, 59),
    ];
    let c = c as u32;
    let i = BLOCKS.partition_point(|b| b.1 < c);
    BLOCKS
        .get(i)
        .filter(|b| b.0 <= c && c <= b.1)
        .map(|b| b.2)
        .or((c > 0xFFFF).then_some(57))
}

/// Whether `face`'s character map has `c` (its file read, and kept).
fn covers(face: &Face, c: char) -> bool {
    use skrifa::MetadataProvider;
    data(&face.path).is_some_and(|d| {
        skrifa::FontRef::from_index(&d, face.index)
            .is_ok_and(|f| f.charmap().map(c).is_some_and(|g| g.to_u32() != 0))
    })
}

/// `family`'s face nearest `bold` and `italic`, and what has to be made up
/// (`(face, embolden, slant)`); none when the computer lacks the family.
fn in_family(family: &str, bold: bool, italic: bool) -> Option<(&'static Face, bool, bool)> {
    let want = if bold { 700 } else { 400 };
    let best = faces()
        .iter()
        .filter(|f| f.family.eq_ignore_ascii_case(family))
        .min_by_key(|f| {
            let width = (f.stretch as i32 - 5).unsigned_abs() * 100_000;
            let slant = if f.italic == italic { 0 } else { 10_000 };
            width + slant + (f.weight as i32 - want).unsigned_abs()
        })?;
    Some((best, bold && best.weight < 600, italic && !best.italic))
}

/// A face that has `c`, when the face asked for lacks it: the usual
/// families of `cjk`'s language for Han characters, kana and hangul (and
/// of the others after them), the wide families for the rest, then any
/// face that says it covers `c`'s block — each checked against its
/// character map. Nearest `bold` and `italic`: `(face, embolden, slant)`.
pub(crate) fn fallback(
    c: char,
    cjk: Cjk,
    bold: bool,
    italic: bool,
) -> Option<(&'static Face, bool, bool)> {
    type Key = (char, Cjk, bool, bool);
    static FOUND: OnceLock<Mutex<HashMap<Key, Option<usize>>>> = OnceLock::new();
    let found = FOUND.get_or_init(Default::default);
    let key = (c, cjk, bold, italic);
    let all = faces();
    let known = found.lock().ok().and_then(|m| m.get(&key).copied());
    let index = match known {
        Some(i) => i,
        None => {
            let i = search(c, cjk, bold, italic)
                .and_then(|f| all.iter().position(|g| std::ptr::eq(g, f)));
            if let Ok(mut m) = found.lock() {
                m.insert(key, i);
            }
            i
        }
    }?;
    let face = all.get(index)?;
    Some((face, bold && face.weight < 600, italic && !face.italic))
}

fn search(c: char, cjk: Cjk, bold: bool, italic: bool) -> Option<&'static Face> {
    let bit = range_bit(c);
    let east_asian = matches!(bit, Some(28 | 48..=56 | 59 | 61 | 65 | 68));
    let mut families: Vec<&str> = Vec::new();
    if east_asian {
        families.extend(order(cjk).iter().flat_map(|l| l.families().iter().copied()));
    }
    families.extend(WIDE.iter().copied());
    for family in families {
        if let Some((face, _, _)) = in_family(family, bold, italic)
            && covers(face, c)
        {
            return Some(face);
        }
    }
    // Any face whose table says it covers the block: the regular ones
    // first, a few at most (each is read whole to look).
    let bit = bit?;
    let has = |f: &&Face| f.ranges[bit as usize / 32] & (1 << (bit % 32)) != 0;
    let mut said: Vec<&'static Face> = faces().iter().filter(has).collect();
    let want = if bold { 700 } else { 400 };
    said.sort_by_key(|f| {
        (
            f.italic != italic,
            (f.weight as i32 - want).unsigned_abs(),
            f.stretch.abs_diff(5),
        )
    });
    said.into_iter().take(8).find(|f| covers(f, c))
}

/// The faces the interface draws characters its own font lacks with:
/// the computer's Chinese, Japanese and Korean faces (`first`'s language
/// first; one file each), one with many other scripts and one with
/// Thai's. Their files and
/// places in them.
pub fn interface_fallbacks(first: Cjk) -> Vec<(PathBuf, u32)> {
    let mut out: Vec<(PathBuf, u32)> = Vec::new();
    let mut add = |families: &[&str]| {
        if let Some((face, _, _)) = families.iter().find_map(|f| in_family(f, false, false))
            && !out.iter().any(|(p, _)| *p == face.path)
        {
            out.push((face.path.clone(), face.index));
        }
    };
    // Traditional Chinese's faces only for its interface (Simplified
    // Chinese's draw its characters for the others: one big file less).
    for l in order(first)
        .iter()
        .filter(|&&l| l != Cjk::ChineseTraditional || first == l)
    {
        add(l.families());
    }
    add(&["Segoe UI", "Noto Sans", "DejaVu Sans", "Lucida Grande"]);
    // Thai: no face above has its letters.
    add(&[
        "Leelawadee UI",
        "Leelawadee",
        "Thonburi",
        "Noto Sans Thai UI",
        "Noto Sans Thai",
        "Tahoma",
        "Loma",
        "Garuda",
    ]);
    out
}

/// The faces of Han characters tried for `first`'s way, in turn.
fn order(first: Cjk) -> &'static [Cjk] {
    match first {
        Cjk::Chinese => &[
            Cjk::Chinese,
            Cjk::ChineseTraditional,
            Cjk::Japanese,
            Cjk::Korean,
        ],
        Cjk::ChineseTraditional => &[
            Cjk::ChineseTraditional,
            Cjk::Chinese,
            Cjk::Japanese,
            Cjk::Korean,
        ],
        Cjk::Japanese => &[Cjk::Japanese, Cjk::Chinese, Cjk::Korean],
        Cjk::Korean => &[Cjk::Korean, Cjk::Chinese, Cjk::Japanese],
    }
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
    let mut ranges = [0u32; 4];
    let (weight, italic, stretch) = match os2 {
        Some((offset, len)) if len >= 64 => {
            let t = read_at(file, offset, 64)?;
            let weight = u16_at(&t, 4)?.clamp(1, 1000);
            let stretch = u16_at(&t, 6)?.clamp(1, 9);
            let selection = u16_at(&t, 62)?;
            for (i, r) in ranges.iter_mut().enumerate() {
                *r = u32_at(&t, 42 + 4 * i)?;
            }
            (weight, selection & 1 != 0, stretch)
        }
        _ => {
            let sub = name_string(&name, 2).unwrap_or_default().to_lowercase();
            let weight = if sub.contains("bold") { 700 } else { 400 };
            let stretch = if sub.contains("narrow") || sub.contains("condensed") {
                3
            } else {
                5
            };
            (
                weight,
                sub.contains("italic") || sub.contains("oblique"),
                stretch,
            )
        }
    };
    Some(Face {
        path: path.to_path_buf(),
        index,
        family,
        weight,
        italic,
        stretch,
        ranges,
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
        // Arial itself, not its narrow faces (they share its name).
        let (face, _, _) = find("Arial", false, false).expect("Arial");
        assert_eq!(face.stretch, 5, "{face:?}");
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
