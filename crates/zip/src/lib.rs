//! ZIP archives (PKWARE's APPNOTE), written from the specification: what
//! OpenTimelineIO's `.otioz` bundles are. Files are stored as they are
//! (media, already compressed) or deflated (text); ZIP64's records carry
//! files and archives past 4 GiB. Reading checks each file's CRC-32, and
//! [`safe_parts`] refuses names that would land outside the folder they
//! are extracted to.

use std::io::{self, Read, Seek, SeekFrom, Write};

use bv_deflate::Crc32;

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const END64: u32 = 0x0606_4b50;
const LOCATOR64: u32 = 0x0706_4b50;
const ZIP64_EXTRA: u16 = 0x0001;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// General purpose flag 11: the name is UTF-8.
const UTF8: u16 = 1 << 11;
const MAX32: u64 = 0xFFFF_FFFF;
const MAX16: u64 = 0xFFFF;
/// Files are copied a mebibyte at a time.
const CHUNK: usize = 1 << 20;

/// A file of an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its path in the archive ("media/clip.mov"), `/`-separated.
    pub name: String,
    /// Its size.
    pub size: u64,
    compressed: u64,
    method: u16,
    crc: u32,
    offset: u64,
}

impl Entry {
    /// A folder's entry (its name ends in `/`).
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put64(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn le64(b: &[u8]) -> u64 {
    let mut a = [0; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}

fn bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("ZIP: {why}"))
}

/// MS-DOS's date and time of `unix` seconds (UTC), as ZIP keeps them:
/// two-second steps, years 1980 to 2107.
fn dos_time(unix: u64) -> (u16, u16) {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    if year < 1980 {
        return (0, 0x21);
    }
    let year = year.min(2107);
    let time = ((secs / 3600) << 11) | (((secs / 60) % 60) << 5) | ((secs % 60) / 2);
    let date = ((year - 1980) << 9) | (month << 5) | day;
    (time as u16, date as u16)
}

/// Writes an archive: files one after the other, then the central
/// directory at [`ZipWriter::finish`].
#[derive(Debug)]
pub struct ZipWriter<W: Write + Seek> {
    out: W,
    entries: Vec<Entry>,
    /// ZIP64's records for every file, as past 4 GiB (the tests' way to
    /// check them with small files).
    force_zip64: bool,
    time: (u16, u16),
}

impl<W: Write + Seek> ZipWriter<W> {
    pub fn new(out: W) -> ZipWriter<W> {
        // WebAssembly in a browser has no clock to ask.
        #[cfg(not(target_family = "wasm"))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        #[cfg(target_family = "wasm")]
        let now = 0;
        ZipWriter {
            out,
            entries: Vec::new(),
            force_zip64: false,
            time: dos_time(now),
        }
    }

    /// Adds a file compressed (deflate): text, small files.
    pub fn add_deflated(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let packed = bv_deflate::deflate(data, 6);
        let mut crc = Crc32::new();
        crc.update(data);
        let size = data.len() as u64;
        let compressed = packed.len() as u64;
        self.add(
            name,
            DEFLATED,
            (size, compressed),
            &mut &packed[..],
            Some(crc.finish()),
            &mut |_| {},
        )
    }

    /// Adds a file stored as it is (media, already compressed), copied from
    /// `data`, `size` bytes; `progress` hears each mebibyte copied.
    pub fn add_stored(
        &mut self,
        name: &str,
        data: &mut dyn Read,
        size: u64,
        progress: &mut dyn FnMut(u64),
    ) -> io::Result<()> {
        self.add(name, STORED, (size, size), data, None, progress)
    }

    fn add(
        &mut self,
        name: &str,
        method: u16,
        (size, compressed): (u64, u64),
        data: &mut dyn Read,
        crc: Option<u32>,
        progress: &mut dyn FnMut(u64),
    ) -> io::Result<()> {
        if name.len() > MAX16 as usize {
            return Err(bad("name too long"));
        }
        let offset = self.out.stream_position()?;
        let zip64 = self.force_zip64 || size >= MAX32 || compressed >= MAX32;
        let mut h = Vec::with_capacity(30 + name.len() + 20);
        put32(&mut h, LOCAL);
        put16(&mut h, if zip64 { 45 } else { 20 });
        put16(&mut h, flags(name));
        put16(&mut h, method);
        put16(&mut h, self.time.0);
        put16(&mut h, self.time.1);
        put32(&mut h, crc.unwrap_or(0));
        if zip64 {
            put32(&mut h, MAX32 as u32);
            put32(&mut h, MAX32 as u32);
        } else {
            put32(&mut h, compressed as u32);
            put32(&mut h, size as u32);
        }
        put16(&mut h, name.len() as u16);
        put16(&mut h, if zip64 { 20 } else { 0 });
        h.extend_from_slice(name.as_bytes());
        if zip64 {
            put16(&mut h, ZIP64_EXTRA);
            put16(&mut h, 16);
            put64(&mut h, size);
            put64(&mut h, compressed);
        }
        self.out.write_all(&h)?;
        let mut sum = Crc32::new();
        let mut left = compressed;
        let mut buf = vec![0; CHUNK.min(compressed as usize).max(1)];
        while left > 0 {
            let n = (left as usize).min(buf.len());
            data.read_exact(&mut buf[..n])?;
            if crc.is_none() {
                sum.update(&buf[..n]);
            }
            self.out.write_all(&buf[..n])?;
            left -= n as u64;
            progress(n as u64);
        }
        // A stored file's CRC is known once it is read: written back into
        // its header.
        let crc = match crc {
            Some(c) => c,
            None => {
                let c = sum.finish();
                let end = self.out.stream_position()?;
                self.out.seek(SeekFrom::Start(offset + 14))?;
                self.out.write_all(&c.to_le_bytes())?;
                self.out.seek(SeekFrom::Start(end))?;
                c
            }
        };
        self.entries.push(Entry {
            name: name.to_string(),
            size,
            compressed,
            method,
            crc,
            offset,
        });
        Ok(())
    }

    /// Writes the central directory and the end records; gives the output
    /// back.
    pub fn finish(mut self) -> io::Result<W> {
        let start = self.out.stream_position()?;
        let mut cd = Vec::new();
        for e in &self.entries {
            let force = self.force_zip64;
            let big_size = force || e.size >= MAX32;
            let big_packed = force || e.compressed >= MAX32;
            let big_offset = force || e.offset >= MAX32;
            let mut extra = Vec::new();
            if big_size || big_packed || big_offset {
                let mut fields = Vec::new();
                if big_size {
                    put64(&mut fields, e.size);
                }
                if big_packed {
                    put64(&mut fields, e.compressed);
                }
                if big_offset {
                    put64(&mut fields, e.offset);
                }
                put16(&mut extra, ZIP64_EXTRA);
                put16(&mut extra, fields.len() as u16);
                extra.extend_from_slice(&fields);
            }
            let version = if extra.is_empty() { 20 } else { 45 };
            put32(&mut cd, CENTRAL);
            put16(&mut cd, version);
            put16(&mut cd, version);
            put16(&mut cd, flags(&e.name));
            put16(&mut cd, e.method);
            put16(&mut cd, self.time.0);
            put16(&mut cd, self.time.1);
            put32(&mut cd, e.crc);
            put32(
                &mut cd,
                if big_packed { MAX32 } else { e.compressed } as u32,
            );
            put32(&mut cd, if big_size { MAX32 } else { e.size } as u32);
            put16(&mut cd, e.name.len() as u16);
            put16(&mut cd, extra.len() as u16);
            put16(&mut cd, 0);
            put16(&mut cd, 0);
            put16(&mut cd, 0);
            put32(&mut cd, 0);
            put32(&mut cd, if big_offset { MAX32 } else { e.offset } as u32);
            cd.extend_from_slice(e.name.as_bytes());
            cd.extend_from_slice(&extra);
        }
        self.out.write_all(&cd)?;
        let size = cd.len() as u64;
        let count = self.entries.len() as u64;
        let zip64 = self.force_zip64 || count >= MAX16 || start >= MAX32 || size >= MAX32;
        let mut end = Vec::new();
        if zip64 {
            let at = start + size;
            put32(&mut end, END64);
            put64(&mut end, 44);
            put16(&mut end, 45);
            put16(&mut end, 45);
            put32(&mut end, 0);
            put32(&mut end, 0);
            put64(&mut end, count);
            put64(&mut end, count);
            put64(&mut end, size);
            put64(&mut end, start);
            put32(&mut end, LOCATOR64);
            put32(&mut end, 0);
            put64(&mut end, at);
            put32(&mut end, 1);
        }
        put32(&mut end, END);
        put16(&mut end, 0);
        put16(&mut end, 0);
        let count16 = if zip64 { MAX16 } else { count } as u16;
        put16(&mut end, count16);
        put16(&mut end, count16);
        put32(&mut end, if zip64 { MAX32 } else { size } as u32);
        put32(&mut end, if zip64 { MAX32 } else { start } as u32);
        put16(&mut end, 0);
        self.out.write_all(&end)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

fn flags(name: &str) -> u16 {
    if name.is_ascii() { 0 } else { UTF8 }
}

/// Reads an archive: its files' list, then each file.
#[derive(Debug)]
pub struct ZipReader<R: Read + Seek> {
    input: R,
    entries: Vec<Entry>,
}

impl<R: Read + Seek> ZipReader<R> {
    /// Reads the central directory (ZIP64's too).
    pub fn new(mut input: R) -> io::Result<ZipReader<R>> {
        let len = input.seek(SeekFrom::End(0))?;
        let back = len.min(22 + MAX16);
        if back < 22 {
            return Err(bad("not an archive"));
        }
        input.seek(SeekFrom::Start(len - back))?;
        let mut tail = vec![0; back as usize];
        input.read_exact(&mut tail)?;
        let at = (0..=tail.len() - 22)
            .rev()
            .find(|&i| le32(&tail[i..]) == END)
            .ok_or_else(|| bad("not an archive"))?;
        let end = &tail[at..];
        let mut count = u64::from(le16(&end[10..]));
        let mut size = u64::from(le32(&end[12..]));
        let mut start = u64::from(le32(&end[16..]));
        let end_at = len - back + at as u64;
        if (count == MAX16 || size == MAX32 || start == MAX32) && end_at >= 20 {
            input.seek(SeekFrom::Start(end_at - 20))?;
            let mut locator = [0; 20];
            input.read_exact(&mut locator)?;
            if le32(&locator) == LOCATOR64 {
                input.seek(SeekFrom::Start(le64(&locator[8..])))?;
                let mut record = [0; 56];
                input.read_exact(&mut record)?;
                if le32(&record) != END64 {
                    return Err(bad("damaged ZIP64 end record"));
                }
                count = le64(&record[32..]);
                size = le64(&record[40..]);
                start = le64(&record[48..]);
            }
        }
        if start.saturating_add(size) > len {
            return Err(bad("damaged central directory"));
        }
        input.seek(SeekFrom::Start(start))?;
        let mut cd = vec![0; size as usize];
        input.read_exact(&mut cd)?;
        let mut entries = Vec::new();
        let mut p = 0;
        for _ in 0..count {
            if p + 46 > cd.len() || le32(&cd[p..]) != CENTRAL {
                return Err(bad("damaged central directory"));
            }
            let h = &cd[p..];
            let flag = le16(&h[8..]);
            let method = le16(&h[10..]);
            let crc = le32(&h[16..]);
            let mut compressed = u64::from(le32(&h[20..]));
            let mut file_size = u64::from(le32(&h[24..]));
            let name_len = le16(&h[28..]) as usize;
            let extra_len = le16(&h[30..]) as usize;
            let comment_len = le16(&h[32..]) as usize;
            let mut offset = u64::from(le32(&h[42..]));
            let total = 46 + name_len + extra_len + comment_len;
            if p + total > cd.len() {
                return Err(bad("damaged central directory"));
            }
            let raw = &h[46..46 + name_len];
            let name = if flag & UTF8 != 0 {
                String::from_utf8_lossy(raw).into_owned()
            } else {
                // Without the flag, ASCII (CP437 beyond it: read as best
                // it can be).
                String::from_utf8_lossy(raw).into_owned()
            };
            let mut extra = &h[46 + name_len..46 + name_len + extra_len];
            while extra.len() >= 4 {
                let id = le16(extra);
                let n = (le16(&extra[2..]) as usize).min(extra.len() - 4);
                if id == ZIP64_EXTRA {
                    let mut fields = &extra[4..4 + n];
                    let mut next = |v: &mut u64| {
                        if *v == MAX32 && fields.len() >= 8 {
                            *v = le64(fields);
                            fields = &fields[8..];
                        }
                    };
                    next(&mut file_size);
                    next(&mut compressed);
                    next(&mut offset);
                }
                extra = &extra[4 + n..];
            }
            entries.push(Entry {
                name,
                size: file_size,
                compressed,
                method,
                crc,
                offset,
            });
            p += total;
        }
        Ok(ZipReader { input, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The index of the file called `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    /// File `index`'s bytes, checked against its CRC-32.
    pub fn read(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        self.extract(index, &mut out, &mut |_| {})?;
        Ok(out)
    }

    /// Writes file `index` into `out` (a stored one a mebibyte at a time,
    /// `progress` hearing each), checked against its CRC-32.
    pub fn extract(
        &mut self,
        index: usize,
        out: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> io::Result<()> {
        let e = self
            .entries
            .get(index)
            .cloned()
            .ok_or_else(|| bad("no such file"))?;
        self.input.seek(SeekFrom::Start(e.offset))?;
        let mut h = [0; 30];
        self.input.read_exact(&mut h)?;
        if le32(&h) != LOCAL {
            return Err(bad("damaged file header"));
        }
        let skip = i64::from(le16(&h[26..])) + i64::from(le16(&h[28..]));
        self.input.seek(SeekFrom::Current(skip))?;
        let mut crc = Crc32::new();
        match e.method {
            STORED => {
                if e.compressed != e.size {
                    return Err(bad("damaged stored file"));
                }
                let mut left = e.size;
                let mut buf = vec![0; CHUNK.min(e.size as usize).max(1)];
                while left > 0 {
                    let n = (left as usize).min(buf.len());
                    self.input.read_exact(&mut buf[..n])?;
                    crc.update(&buf[..n]);
                    out.write_all(&buf[..n])?;
                    left -= n as u64;
                    progress(n as u64);
                }
            }
            DEFLATED => {
                let mut packed = vec![0; e.compressed as usize];
                self.input.read_exact(&mut packed)?;
                let data = bv_deflate::inflate(&packed).map_err(|err| bad(&err.to_string()))?;
                if data.len() as u64 != e.size {
                    return Err(bad("wrong size"));
                }
                crc.update(&data);
                out.write_all(&data)?;
                progress(e.size);
            }
            m => return Err(bad(&format!("compression method {m} is not supported"))),
        }
        if crc.finish() != e.crc {
            return Err(bad("the file is damaged (CRC)"));
        }
        Ok(())
    }
}

/// The parts of an entry's name as a path inside a folder, or `None` when
/// it would leave that folder (absolute, a drive, `..`): a hostile
/// archive's.
pub fn safe_parts(name: &str) -> Option<Vec<&str>> {
    let name = name.strip_suffix('/').unwrap_or(name);
    if name.is_empty() || name.starts_with(['/', '\\']) || name.contains(':') {
        return None;
    }
    let parts: Vec<&str> = name.split(['/', '\\']).collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return None;
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn archive(force_zip64: bool) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        w.force_zip64 = force_zip64;
        w.add_deflated("content.otio", "{\"a\": 1}\n".repeat(50).as_bytes())
            .unwrap();
        w.add_deflated("version.txt", b"1.0.0").unwrap();
        let media: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut copied = 0;
        w.add_stored(
            "media/clip é.mov",
            &mut &media[..],
            media.len() as u64,
            &mut |n| copied += n,
        )
        .unwrap();
        assert_eq!(copied, media.len() as u64);
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn files_come_back_as_they_went() {
        for force in [false, true] {
            let bytes = archive(force);
            let mut r = ZipReader::new(Cursor::new(bytes)).unwrap();
            let names: Vec<&str> = r.entries().iter().map(|e| e.name.as_str()).collect();
            assert_eq!(names, ["content.otio", "version.txt", "media/clip é.mov"]);
            assert_eq!(r.read(1).unwrap(), b"1.0.0");
            assert_eq!(r.read(0).unwrap(), "{\"a\": 1}\n".repeat(50).as_bytes());
            let media = r.read(r.find("media/clip é.mov").unwrap()).unwrap();
            assert_eq!(media.len(), 300_000);
            assert!(media.iter().enumerate().all(|(i, &b)| b == (i % 251) as u8));
        }
    }

    #[test]
    fn a_damaged_file_is_noticed() {
        let mut bytes = archive(false);
        // A byte of the stored media flipped.
        let at = bytes.len() / 2;
        bytes[at] ^= 0xFF;
        let mut r = ZipReader::new(Cursor::new(bytes)).unwrap();
        let i = r.find("media/clip é.mov").unwrap();
        assert!(r.read(i).is_err());
    }

    #[test]
    fn names_stay_inside_their_folder() {
        assert_eq!(safe_parts("media/a.mov"), Some(vec!["media", "a.mov"]));
        assert_eq!(safe_parts("media/"), Some(vec!["media"]));
        for hostile in [
            "../a",
            "a/../../b",
            "/etc/passwd",
            "\\x",
            "C:/x",
            "a//b",
            ".",
        ] {
            assert_eq!(safe_parts(hostile), None, "{hostile}");
        }
    }

    #[test]
    fn dos_dates() {
        // 2026-10-20 12:34:56 UTC.
        let (time, date) = dos_time(1_792_499_696);
        assert_eq!(date >> 9, 46, "1980 + 46");
        assert_eq!((date >> 5) & 15, 10);
        assert_eq!(date & 31, 20);
        assert_eq!(time >> 11, 12);
        assert_eq!((time >> 5) & 63, 34);
        assert_eq!((time & 31) * 2, 56);
        assert_eq!(dos_time(0), (0, 0x21), "before 1980: its first day");
    }
}
