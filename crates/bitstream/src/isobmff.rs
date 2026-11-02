//! The boxes of the ISO base media file format (MP4, MOV, M4A, HEIF…):
//! walking them in memory, and reading a file's `moov` box without its
//! media data.

use std::io::{Read, Seek, SeekFrom};

/// The boxes one after another in `data`: each one's type and contents.
pub fn boxes(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 8 > data.len() {
            return None;
        }
        let size = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as u64;
        let kind: [u8; 4] = data[pos + 4..pos + 8].try_into().ok()?;
        let (header, size) = match size {
            1 => {
                let large = u64::from_be_bytes(data.get(pos + 8..pos + 16)?.try_into().ok()?);
                (16usize, large)
            }
            0 => (8, (data.len() - pos) as u64),
            n => (8, n),
        };
        let end = pos.checked_add(usize::try_from(size).ok()?)?;
        if size < header as u64 || end > data.len() {
            return None;
        }
        let body = &data[pos + header..end];
        pos = end;
        Some((kind, body))
    })
}

/// The contents of the first box of type `kind` in `data`.
pub fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).find(|(k, _)| k == kind).map(|(_, b)| b)
}

pub fn be_u32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

pub fn be_u64(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// Timescale of `mvhd` or `mdhd` (both: version, flags, two times, then the
/// timescale).
pub fn timescale(full_box: &[u8]) -> Option<u32> {
    match full_box.first()? {
        1 => be_u32(full_box, 20),
        _ => be_u32(full_box, 12),
    }
}

/// The `moov` box of an MPEG-4 family file, read without the media data.
pub fn moov<R: Read + Seek>(f: &mut R) -> Option<Vec<u8>> {
    let len = f.seek(SeekFrom::End(0)).ok()?;
    let mut pos = 0u64;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut head = [0u8; 16];
        f.read_exact(&mut head[..8]).ok()?;
        let size = u32::from_be_bytes(head[..4].try_into().ok()?) as u64;
        let (header, size) = match size {
            1 => {
                f.read_exact(&mut head[8..16]).ok()?;
                (16, u64::from_be_bytes(head[8..16].try_into().ok()?))
            }
            0 => (8, len - pos),
            n => (8, n),
        };
        if size < header {
            return None;
        }
        if &head[4..8] == b"moov" {
            // A movie header of more than 64 MB is not one.
            let body = usize::try_from(size - header)
                .ok()
                .filter(|&n| n < 64 << 20)?;
            let mut data = vec![0u8; body];
            f.read_exact(&mut data).ok()?;
            return Some(data);
        }
        pos += size;
    }
    None
}

/// The `moov` box of the file at `path`.
pub fn read_moov(path: &str) -> Option<Vec<u8>> {
    moov(&mut std::fs::File::open(path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn boxes_are_walked_and_the_movie_found() {
        let mut file = Vec::new();
        file.extend(16u32.to_be_bytes());
        file.extend(b"ftypisom\0\0\0\0");
        // A 64-bit size.
        file.extend(1u32.to_be_bytes());
        file.extend(b"free");
        file.extend(20u64.to_be_bytes());
        file.extend([7u8; 4]);
        file.extend(20u32.to_be_bytes());
        file.extend(b"moov");
        file.extend(12u32.to_be_bytes());
        file.extend(b"mvhd");
        file.extend([1u8, 2, 3, 4]);
        let kinds: Vec<[u8; 4]> = boxes(&file).map(|(k, _)| k).collect();
        assert_eq!(kinds, [*b"ftyp", *b"free", *b"moov"]);
        assert_eq!(child(&file, b"free"), Some(&[7u8; 4][..]));
        let movie = moov(&mut Cursor::new(&file)).expect("a movie");
        assert_eq!(child(&movie, b"mvhd"), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(be_u32(&[0, 0, 1, 0], 0), Some(256));
    }
}
