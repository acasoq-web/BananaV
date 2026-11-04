//! The container of `.veg` files: Wave64, the RIFF of 64 bits. Each
//! chunk is a GUID of 16 bytes (the first three fields little-endian, as
//! Windows writes GUIDs), a size of 8 (the 24 bytes of this header
//! included) and its data; every chunk starts on a multiple of 8 bytes
//! from the file's start, the zeros that pad it to there counted in its
//! size. The file is one `riff` chunk (its type's GUID, then chunks); a
//! `list` chunk is its type's GUID and its chunks.

use std::fmt;

/// A GUID as the file holds it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Guid(pub [u8; 16]);

impl Guid {
    /// The GUID written `AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE`.
    pub const fn parse(s: &str) -> Guid {
        const fn hex(c: u8) -> u8 {
            match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => panic!("not a GUID"),
            }
        }
        let b = s.as_bytes();
        // The text's byte pairs in the file's order.
        const ORDER: [usize; 16] = [6, 4, 2, 0, 11, 9, 16, 14, 19, 21, 24, 26, 28, 30, 32, 34];
        let mut out = [0u8; 16];
        let mut k = 0;
        while k < 16 {
            let i = ORDER[k];
            out[k] = (hex(b[i]) << 4) | hex(b[i + 1]);
            k += 1;
        }
        Guid(out)
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-",
            b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9]
        )?;
        b[10..].iter().try_for_each(|x| write!(f, "{x:02X}"))
    }
}

pub(crate) const RIFF: Guid = Guid::parse("66666972-912E-11CF-A5D6-28DB04C10000");
pub(crate) const LIST: Guid = Guid::parse("7473696C-912F-11CF-A5D6-28DB04C10000");

/// A chunk read: its GUID — a list's or the file's type for those —, its
/// data (after a list's type) and, for lists, its chunks.
#[derive(Clone, Debug)]
pub(crate) struct Chunk<'a> {
    pub guid: Guid,
    pub list: bool,
    pub data: &'a [u8],
    pub kids: Vec<Chunk<'a>>,
}

impl<'a> Chunk<'a> {
    /// Its chunks that are plain chunks of `guid`.
    pub fn blocks(&self, guid: Guid) -> impl Iterator<Item = &Chunk<'a>> {
        self.kids.iter().filter(move |c| !c.list && c.guid == guid)
    }

    /// Its first plain chunk of `guid`.
    pub fn block(&self, guid: Guid) -> Option<&Chunk<'a>> {
        self.blocks(guid).next()
    }

    /// Its lists of type `guid`.
    pub fn lists(&self, guid: Guid) -> impl Iterator<Item = &Chunk<'a>> {
        self.kids.iter().filter(move |c| c.list && c.guid == guid)
    }

    /// Its first list of type `guid`.
    pub fn list(&self, guid: Guid) -> Option<&Chunk<'a>> {
        self.lists(guid).next()
    }
}

/// Why a file is not a project.
#[derive(Clone, Debug, PartialEq)]
pub enum VegError {
    /// Not a Wave64 `riff`, or a chunk that does not fit.
    NotRiff,
    /// A `riff` of another kind than a project.
    NotProject,
    /// Its parts do not fit together.
    Damaged(&'static str),
}

impl fmt::Display for VegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VegError::NotRiff => write!(f, "not a .veg file"),
            VegError::NotProject => write!(f, "not a project"),
            VegError::Damaged(what) => write!(f, "damaged ({what})"),
        }
    }
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8)
        .map(|s| u64::from_le_bytes(s.try_into().expect("8 bytes")))
}

fn guid_at(b: &[u8], at: usize) -> Option<Guid> {
    b.get(at..at + 16)
        .map(|s| Guid(s.try_into().expect("16 bytes")))
}

/// The chunks of `file[start..end]` (the 8-byte steps count from the
/// file's start).
fn chunks(file: &[u8], start: usize, end: usize, depth: u32) -> Result<Vec<Chunk<'_>>, VegError> {
    if depth > 64 {
        return Err(VegError::Damaged("too deep"));
    }
    let mut out = Vec::new();
    let mut at = start;
    while at + 24 <= end {
        let guid = guid_at(file, at).ok_or(VegError::NotRiff)?;
        let size = u64_at(file, at + 16).ok_or(VegError::NotRiff)? as usize;
        if size < 24 || at.checked_add(size).is_none_or(|e| e > end) {
            return Err(VegError::Damaged("a chunk's size"));
        }
        let body = at + 24;
        let stop = at + size;
        let chunk = if guid == RIFF || guid == LIST {
            let kind = guid_at(file, body).ok_or(VegError::Damaged("a list's type"))?;
            Chunk {
                guid: kind,
                list: true,
                data: &file[body + 16..stop],
                kids: chunks(file, body + 16, stop, depth + 1)?,
            }
        } else {
            Chunk {
                guid,
                list: false,
                data: &file[body..stop],
                kids: Vec::new(),
            }
        };
        out.push(chunk);
        at = (stop + 7) & !7;
    }
    Ok(out)
}

/// The file's `riff` chunk (its type the list's GUID).
pub(crate) fn parse(file: &[u8]) -> Result<Chunk<'_>, VegError> {
    if file.len() < 40 || guid_at(file, 0) != Some(RIFF) {
        return Err(VegError::NotRiff);
    }
    // The size some writers leave short: the file's end is the riff's.
    let size = (u64_at(file, 16).unwrap_or(0) as usize).clamp(40, file.len());
    let kind = guid_at(file, 24).ok_or(VegError::NotRiff)?;
    Ok(Chunk {
        guid: kind,
        list: true,
        data: &file[40..size],
        kids: chunks(file, 40, size, 0)?,
    })
}

/// A file being written: chunks opened and closed in order.
#[derive(Default)]
pub(crate) struct Writer {
    pub out: Vec<u8>,
    /// Where each chunk still open starts.
    open: Vec<usize>,
}

impl Writer {
    /// A `riff` file of type `kind`, its chunks to come.
    pub fn new(kind: Guid) -> Writer {
        let mut w = Writer::default();
        w.begin(RIFF);
        w.out.extend_from_slice(&kind.0);
        w
    }

    /// Opens a chunk of `guid` (its size written when it closes).
    pub fn begin(&mut self, guid: Guid) {
        debug_assert_eq!(self.out.len() % 8, 0);
        self.open.push(self.out.len());
        self.out.extend_from_slice(&guid.0);
        self.out.extend_from_slice(&24u64.to_le_bytes());
    }

    /// Opens a list of type `kind`.
    pub fn begin_list(&mut self, kind: Guid) {
        self.begin(LIST);
        self.out.extend_from_slice(&kind.0);
    }

    /// Closes the chunk opened last: zeros to the next multiple of 8, its
    /// size (with them).
    pub fn end(&mut self) {
        let start = self.open.pop().expect("a chunk open");
        while !self.out.len().is_multiple_of(8) {
            self.out.push(0);
        }
        let size = (self.out.len() - start) as u64;
        self.out[start + 16..start + 24].copy_from_slice(&size.to_le_bytes());
    }

    /// A whole chunk of `guid` holding `data`.
    pub fn block(&mut self, guid: Guid, data: &[u8]) {
        self.begin(guid);
        self.out.extend_from_slice(data);
        self.end();
    }

    /// The file, its `riff` closed.
    pub fn finish(mut self) -> Vec<u8> {
        while !self.open.is_empty() {
            self.end();
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_read_as_windows_writes_them() {
        let g = Guid::parse("66666972-912E-11CF-A5D6-28DB04C10000");
        assert_eq!(&g.0[..4], b"riff");
        assert_eq!(format!("{g:?}"), "66666972-912E-11CF-A5D6-28DB04C10000");
    }

    #[test]
    fn chunks_pad_to_eight_bytes_and_nest() {
        let kind = Guid::parse("46C429EF-904A-11D2-8722-00C04F8EDB8A");
        let a = Guid::parse("B28F2D5A-230F-11D2-86AF-00C04F8EDB8A");
        let l = Guid::parse("4D6C0747-2316-11D2-86B0-00C04F8EDB8A");
        let mut w = Writer::new(kind);
        w.block(a, &[1; 36]);
        w.begin_list(l);
        w.block(a, &[2; 3]);
        w.end();
        let file = w.finish();
        // 24 + 36 = 60, padded to 64, as the format has it.
        assert_eq!(u64::from_le_bytes(file[56..64].try_into().unwrap()), 64);
        assert_eq!(file.len() % 8, 0);
        let root = parse(&file).unwrap();
        assert_eq!(root.guid, kind);
        assert_eq!(root.kids.len(), 2);
        assert_eq!(root.kids[0].data.len(), 40);
        assert_eq!(&root.kids[0].data[..36], &[1; 36]);
        let list = root.list(l).unwrap();
        assert_eq!(list.block(a).unwrap().data[..3], [2; 3]);
        assert!(parse(b"not a riff at all, not at all, not at all").is_err());
    }
}
