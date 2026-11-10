//! Mach-O files — macOS's programs, libraries and plug-ins — read without
//! running them: which processors a file holds code for (a universal file
//! holds an image for each, one after the other) and the names of the
//! functions an image exports, from its symbol table or, when that has
//! none, from the dynamic linker's export trie. Little-endian images only
//! (Intel's and Apple's chips; PowerPC's are listed, never read).

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Processors, as Mach-O numbers them (`cputype`).
pub const CPU_X86: u32 = 7;
pub const CPU_X86_64: u32 = 0x0100_0007;
pub const CPU_ARM: u32 = 12;
pub const CPU_ARM64: u32 = 0x0100_000C;
pub const CPU_POWERPC: u32 = 18;
pub const CPU_POWERPC64: u32 = 0x0100_0012;

const MH_MAGIC: u32 = 0xFEED_FACE;
const MH_MAGIC_64: u32 = 0xFEED_FACF;
const FAT_MAGIC: u32 = 0xCAFE_BABE;
const FAT_MAGIC_64: u32 = 0xCAFE_BABF;

const LC_SYMTAB: u32 = 0x2;
const LC_DYLD_INFO: u32 = 0x22;
const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;

/// Tables larger than this are not read (a damaged file).
const MOST: u64 = 64 << 20;

/// This program's processor, as Mach-O numbers it.
pub fn this_cpu() -> u32 {
    if cfg!(target_arch = "aarch64") {
        CPU_ARM64
    } else if cfg!(target_arch = "x86_64") {
        CPU_X86_64
    } else if cfg!(target_arch = "x86") {
        CPU_X86
    } else {
        0
    }
}

/// A processor's name, as the system's tools write it ("arm64", "x86_64").
pub fn cpu_name(cpu: u32) -> &'static str {
    match cpu {
        CPU_ARM64 => "arm64",
        CPU_X86_64 => "x86_64",
        CPU_X86 => "i386",
        CPU_ARM => "arm",
        CPU_POWERPC => "ppc",
        CPU_POWERPC64 => "ppc64",
        _ => "unknown",
    }
}

/// The image for one processor in a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    pub cpu: u32,
    /// Where the image starts in the file, and its length.
    pub offset: u64,
    pub size: u64,
    /// A 64-bit image.
    pub bits64: bool,
    /// Its numbers are little-endian (Intel's and ARM's; PowerPC's are
    /// big-endian).
    pub little: bool,
}

fn read_at<R: Read + Seek>(r: &mut R, at: u64, len: usize) -> Option<Vec<u8>> {
    r.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// What a thin image's first bytes say: its processor, whether it is
/// 64-bit and little-endian.
fn thin(head: &[u8]) -> Option<(u32, bool, bool)> {
    let le = le32(head, 0)?;
    if le == MH_MAGIC || le == MH_MAGIC_64 {
        return Some((le32(head, 4)?, le == MH_MAGIC_64, true));
    }
    let be = be32(head, 0)?;
    if be == MH_MAGIC || be == MH_MAGIC_64 {
        return Some((be32(head, 4)?, be == MH_MAGIC_64, false));
    }
    None
}

/// The images a file holds, one per processor (a universal file's each, a
/// thin file's one); `None` when it is no Mach-O file.
pub fn slices<R: Read + Seek>(r: &mut R) -> Option<Vec<Slice>> {
    let len = r.seek(SeekFrom::End(0)).ok()?;
    let head = read_at(r, 0, 8)?;
    let magic = be32(&head, 0)?;
    if magic == FAT_MAGIC || magic == FAT_MAGIC_64 {
        let n = be32(&head, 4)?;
        // Java's class files begin the same way, with a version (45 and
        // more) where a universal file counts its images.
        if n == 0 || n > 30 {
            return None;
        }
        let wide = magic == FAT_MAGIC_64;
        let entry = if wide { 32 } else { 20 };
        let table = read_at(r, 8, n as usize * entry)?;
        let mut out = Vec::new();
        for i in 0..n as usize {
            let e = &table[i * entry..(i + 1) * entry];
            let cpu = be32(e, 0)?;
            let (offset, size) = if wide {
                (be64(e, 8)?, be64(e, 16)?)
            } else {
                (u64::from(be32(e, 8)?), u64::from(be32(e, 12)?))
            };
            if offset.checked_add(size).is_none_or(|end| end > len) {
                continue;
            }
            let Some((_, bits64, little)) = read_at(r, offset, 8).and_then(|h| thin(&h)) else {
                continue;
            };
            out.push(Slice {
                cpu,
                offset,
                size,
                bits64,
                little,
            });
        }
        return (!out.is_empty()).then_some(out);
    }
    let (cpu, bits64, little) = thin(&head)?;
    Some(vec![Slice {
        cpu,
        offset: 0,
        size: len,
        bits64,
        little,
    }])
}

/// The image of `slices` for this program's processor.
pub fn native(slices: &[Slice]) -> Option<&Slice> {
    slices.iter().find(|s| s.cpu == this_cpu())
}

/// The images of the file at `path` (see [`slices`]).
pub fn file_slices(path: &Path) -> Option<Vec<Slice>> {
    slices(&mut std::fs::File::open(path).ok()?)
}

/// The functions the file at `path` exports: those of its image for this
/// program's processor, else of its first image.
pub fn file_exports(path: &Path) -> Option<Vec<String>> {
    let mut f = std::fs::File::open(path).ok()?;
    let all = slices(&mut f)?;
    let slice = *native(&all).or(all.first())?;
    exports(&mut f, &slice)
}

/// The names an image exports, without the underscore C names begin with
/// in Mach-O (`VSTPluginMain`, `GetPluginFactory`).
pub fn exports<R: Read + Seek>(r: &mut R, slice: &Slice) -> Option<Vec<String>> {
    if !slice.little {
        return None;
    }
    let header = if slice.bits64 { 32 } else { 28 };
    let head = read_at(r, slice.offset, header)?;
    let ncmds = le32(&head, 16)?;
    let sizeofcmds = le32(&head, 20)?;
    if u64::from(sizeofcmds) > MOST {
        return None;
    }
    let cmds = read_at(r, slice.offset + header as u64, sizeofcmds as usize)?;
    let mut symtab = None;
    let mut trie = None;
    let mut at = 0usize;
    for _ in 0..ncmds {
        let (Some(cmd), Some(size)) = (le32(&cmds, at), le32(&cmds, at + 4)) else {
            break;
        };
        let size = size as usize;
        if size < 8 || at + size > cmds.len() {
            break;
        }
        match cmd {
            LC_SYMTAB => {
                symtab = (|| {
                    Some((
                        le32(&cmds, at + 8)?,
                        le32(&cmds, at + 12)?,
                        le32(&cmds, at + 16)?,
                        le32(&cmds, at + 20)?,
                    ))
                })();
            }
            LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
                if let (Some(off), Some(len)) = (le32(&cmds, at + 40), le32(&cmds, at + 44))
                    && len > 0
                {
                    trie = Some((off, len));
                }
            }
            LC_DYLD_EXPORTS_TRIE => {
                if let (Some(off), Some(len)) = (le32(&cmds, at + 8), le32(&cmds, at + 12))
                    && len > 0
                {
                    trie = Some((off, len));
                }
            }
            _ => {}
        }
        at += size;
    }
    let mut names = Vec::new();
    if let Some((symoff, nsyms, stroff, strsize)) = symtab {
        names = symbols(r, slice, symoff, nsyms, stroff, strsize).unwrap_or_default();
    }
    if names.is_empty()
        && let Some((off, len)) = trie
        && u64::from(len) <= MOST
        && let Some(data) = read_at(r, slice.offset + u64::from(off), len as usize)
    {
        names = trie_names(&data);
    }
    names.sort();
    names.dedup();
    Some(names)
}

/// The external symbols an image defines, from its symbol table.
fn symbols<R: Read + Seek>(
    r: &mut R,
    slice: &Slice,
    symoff: u32,
    nsyms: u32,
    stroff: u32,
    strsize: u32,
) -> Option<Vec<String>> {
    let entry = if slice.bits64 { 16 } else { 12 };
    let table_len = u64::from(nsyms) * entry as u64;
    if table_len > MOST || u64::from(strsize) > MOST {
        return None;
    }
    let table = read_at(r, slice.offset + u64::from(symoff), table_len as usize)?;
    let strings = read_at(r, slice.offset + u64::from(stroff), strsize as usize)?;
    let mut out = Vec::new();
    for e in table.chunks_exact(entry) {
        let strx = le32(e, 0)? as usize;
        let kind = e[4];
        // Not a debugger's entry, not private, defined in a section and
        // external: exported.
        let exported = kind & 0xE0 == 0 && kind & 0x10 == 0 && kind & 0x0E == 0x0E && kind & 1 != 0;
        if !exported || strx >= strings.len() {
            continue;
        }
        let end = strings[strx..]
            .iter()
            .position(|&b| b == 0)
            .map_or(strings.len(), |p| strx + p);
        out.push(c_name(&strings[strx..end]));
    }
    Some(out)
}

/// A symbol's name without its first underscore.
fn c_name(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"_").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// An unsigned LEB128 number at `at`, and where what follows starts.
fn uleb(data: &[u8], mut at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let b = *data.get(at)?;
        at += 1;
        if shift < 64 {
            value |= u64::from(b & 0x7F) << shift;
        }
        if b & 0x80 == 0 {
            return Some((value, at));
        }
        shift += 7;
        if shift > 70 {
            return None;
        }
    }
}

/// The names the dynamic linker's export trie holds: each node that ends
/// a name has export information; its edges add text to the name.
fn trie_names(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![(0usize, Vec::<u8>::new())];
    while let Some((node, name)) = stack.pop() {
        if !seen.insert(node) || out.len() > 1_000_000 {
            continue;
        }
        let Some((terminal, at)) = uleb(data, node) else {
            continue;
        };
        if terminal > 0 {
            out.push(c_name(&name));
        }
        let mut at = at + terminal as usize;
        let Some(&children) = data.get(at) else {
            continue;
        };
        at += 1;
        for _ in 0..children {
            let Some(len) = data.get(at..).and_then(|d| d.iter().position(|&b| b == 0)) else {
                break;
            };
            let edge = &data[at..at + len];
            at += len + 1;
            let Some((child, next)) = uleb(data, at) else {
                break;
            };
            at = next;
            let mut longer = name.clone();
            longer.extend_from_slice(edge);
            stack.push((child as usize, longer));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A thin 64-bit image for `cpu` exporting `names` (and one private
    /// symbol) through its symbol table.
    fn image(cpu: u32, names: &[&str]) -> Vec<u8> {
        let mut strings = vec![0u8];
        let mut table = Vec::new();
        let mut add = |name: &str, kind: u8, strings: &mut Vec<u8>| {
            let strx = strings.len() as u32;
            strings.extend_from_slice(name.as_bytes());
            strings.push(0);
            table.extend_from_slice(&strx.to_le_bytes());
            table.push(kind);
            table.push(1);
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&0x1000u64.to_le_bytes());
        };
        for n in names {
            add(&format!("_{n}"), 0x0F, &mut strings);
        }
        // A private one (not external) and a debugger's entry.
        add("_hidden", 0x0E, &mut strings);
        add("_stab", 0x24, &mut strings);
        let header = 32u32;
        let cmd = 24u32;
        let symoff = header + cmd;
        let stroff = symoff + table.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&MH_MAGIC_64.to_le_bytes());
        out.extend_from_slice(&cpu.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // subtype
        out.extend_from_slice(&8u32.to_le_bytes()); // a bundle
        out.extend_from_slice(&1u32.to_le_bytes()); // one command
        out.extend_from_slice(&cmd.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(&LC_SYMTAB.to_le_bytes());
        out.extend_from_slice(&cmd.to_le_bytes());
        out.extend_from_slice(&symoff.to_le_bytes());
        out.extend_from_slice(&((table.len() / 16) as u32).to_le_bytes());
        out.extend_from_slice(&stroff.to_le_bytes());
        out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&strings);
        out
    }

    /// A universal file of `images`, each aligned to 4 KiB.
    fn universal(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&FAT_MAGIC.to_be_bytes());
        out.extend_from_slice(&(images.len() as u32).to_be_bytes());
        let mut offset = 4096u32;
        let mut places = Vec::new();
        for (cpu, img) in images {
            out.extend_from_slice(&cpu.to_be_bytes());
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&offset.to_be_bytes());
            out.extend_from_slice(&(img.len() as u32).to_be_bytes());
            out.extend_from_slice(&12u32.to_be_bytes());
            places.push(offset);
            offset += (img.len() as u32).div_ceil(4096) * 4096;
        }
        for ((_, img), at) in images.iter().zip(places) {
            out.resize(at as usize, 0);
            out.extend_from_slice(img);
        }
        out
    }

    #[test]
    fn a_thin_image_tells_its_processor_and_exports() {
        let bytes = image(CPU_ARM64, &["VSTPluginMain", "main_macho"]);
        let mut r = Cursor::new(bytes);
        let s = slices(&mut r).expect("a Mach-O file");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].cpu, CPU_ARM64);
        assert!(s[0].bits64 && s[0].little);
        assert_eq!(
            exports(&mut r, &s[0]).unwrap(),
            vec!["VSTPluginMain".to_string(), "main_macho".to_string()]
        );
    }

    #[test]
    fn a_universal_file_holds_an_image_per_processor() {
        let bytes = universal(&[
            (
                CPU_X86_64,
                image(CPU_X86_64, &["GetPluginFactory", "bundleEntry"]),
            ),
            (
                CPU_ARM64,
                image(CPU_ARM64, &["GetPluginFactory", "bundleExit"]),
            ),
        ]);
        let mut r = Cursor::new(bytes);
        let s = slices(&mut r).expect("a universal file");
        assert_eq!(
            s.iter().map(|s| s.cpu).collect::<Vec<_>>(),
            [CPU_X86_64, CPU_ARM64]
        );
        let arm = s.iter().find(|s| s.cpu == CPU_ARM64).unwrap();
        assert_eq!(
            exports(&mut r, arm).unwrap(),
            vec!["GetPluginFactory".to_string(), "bundleExit".to_string()]
        );
        assert_eq!(cpu_name(arm.cpu), "arm64");
    }

    #[test]
    fn other_files_are_no_mach_o() {
        assert_eq!(slices(&mut Cursor::new(b"MZ\x90\0not one".to_vec())), None);
        assert_eq!(slices(&mut Cursor::new(b"\x7fELF".to_vec())), None);
        // A Java class: the universal magic and a version of 52.
        assert_eq!(
            slices(&mut Cursor::new(b"\xCA\xFE\xBA\xBE\0\0\0\x34".to_vec())),
            None
        );
        assert_eq!(slices(&mut Cursor::new(Vec::new())), None);
    }

    #[test]
    fn the_export_trie_spells_its_names() {
        // Root (at 0): no export, one edge "_" to node A (at 5); A: two
        // edges, "foo" to 17 and "bar" to 21, nodes that export (two bytes
        // of information each) and end there.
        let mut t = vec![0u8, 1, b'_', 0, 5];
        t.extend_from_slice(&[0, 2]);
        t.extend_from_slice(b"foo\0");
        t.push(17);
        t.extend_from_slice(b"bar\0");
        t.push(21);
        assert_eq!(t.len(), 17);
        t.extend_from_slice(&[2, 0, 0x10, 0]);
        t.extend_from_slice(&[2, 0, 0x20, 0]);
        let mut names = trie_names(&t);
        names.sort();
        assert_eq!(names, ["bar", "foo"]);
        // A loop back to the root ends.
        assert!(trie_names(&[0, 1, b'x', 0, 0]).is_empty());
    }

    #[test]
    fn this_program_is_a_known_processor() {
        if cfg!(any(target_arch = "aarch64", target_arch = "x86_64")) {
            assert_ne!(this_cpu(), 0);
        }
        if cfg!(target_os = "macos") {
            let me = std::env::current_exe().unwrap();
            let s = file_slices(&me).expect("this test program is a Mach-O file");
            assert!(native(&s).is_some());
        }
    }
}
