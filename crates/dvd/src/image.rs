//! A DVD-Video disc's image: a UDF 1.02 file system (ECMA-167) bridged
//! with ISO 9660, as DVD-Video discs are, its files laid out in the order
//! the format wants — `VIDEO_TS.IFO`, `VIDEO_TS.BUP`, `VTS_01_0.IFO`, the
//! title's VOBs, `VTS_01_0.BUP` — each from a sector, so the information
//! files' sector numbers hold on the disc.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::vob::SECTOR;

/// Where the partition (the files' area) starts: after the anchor.
const PARTITION: u32 = 257;
/// The main and reserve volume descriptor sequences, the integrity
/// sequence.
const MAIN_VDS: u32 = 32;
const RESERVE_VDS: u32 = 48;
const INTEGRITY: u32 = 64;
/// ISO 9660's path tables and directories.
const L_PATH: u32 = 21;
const M_PATH: u32 = 22;
const ISO_ROOT: u32 = 23;
const ISO_AUDIO: u32 = 24;
const ISO_VIDEO: u32 = 25;

/// A file of the image: its name and where its bytes come from.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    path: PathBuf,
    size: u64,
    /// Its first sector in the partition (logical block).
    lbn: u32,
    /// Its file entry's logical block.
    fe: u32,
}

/// The `VIDEO_TS` folder's files in DVD-Video's order.
fn dvd_files(video_ts: &Path) -> io::Result<Vec<(String, PathBuf, u64)>> {
    let mut names = vec!["VIDEO_TS.IFO".to_string(), "VIDEO_TS.BUP".to_string()];
    names.push("VTS_01_0.IFO".into());
    for k in 1..=9 {
        let n = format!("VTS_01_{k}.VOB");
        if video_ts.join(&n).is_file() {
            names.push(n);
        }
    }
    names.push("VTS_01_0.BUP".into());
    names
        .into_iter()
        .map(|n| {
            let p = video_ts.join(&n);
            let size = std::fs::metadata(&p)?.len();
            Ok((n, p, size))
        })
        .collect()
}

fn sectors(bytes: u64) -> u32 {
    bytes.div_ceil(SECTOR as u64) as u32
}

/// CRC-ITU-T (ECMA-167 7.2.6): x^16 + x^12 + x^5 + 1, from 0.
pub(crate) fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// A descriptor's tag (16 bytes) at `location`, over `body` (the bytes
/// after the tag).
fn tag(id: u16, location: u32, body: &[u8]) -> [u8; 16] {
    let mut t = [0u8; 16];
    t[0..2].copy_from_slice(&id.to_le_bytes());
    t[2..4].copy_from_slice(&2u16.to_le_bytes());
    t[6..8].copy_from_slice(&1u16.to_le_bytes());
    t[8..10].copy_from_slice(&crc16(body).to_le_bytes());
    t[10..12].copy_from_slice(&(body.len() as u16).to_le_bytes());
    t[12..16].copy_from_slice(&location.to_le_bytes());
    t[4] = (0..16)
        .filter(|&i| i != 4)
        .fold(0u8, |s, i| s.wrapping_add(t[i]));
    t
}

/// A descriptor: its tag over `body`, then `body`, in a sector.
fn descriptor(id: u16, location: u32, body: &[u8]) -> Vec<u8> {
    let mut s = tag(id, location, body).to_vec();
    s.extend_from_slice(body);
    s.resize(SECTOR, 0);
    s
}

fn le16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn le32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn le64(b: &mut Vec<u8>, v: u64) {
    b.extend_from_slice(&v.to_le_bytes());
}

/// A dstring of `len` bytes: OSTA compressed Unicode (8-bit), its length
/// in the last byte.
fn dstring(text: &str, len: usize) -> Vec<u8> {
    let mut b = vec![0u8; len];
    if text.is_empty() {
        return b;
    }
    let chars: Vec<u8> = text.bytes().take(len - 2).collect();
    b[0] = 8;
    b[1..1 + chars.len()].copy_from_slice(&chars);
    b[len - 1] = (chars.len() + 1) as u8;
    b
}

/// A charspec: OSTA Compressed Unicode.
fn charspec() -> Vec<u8> {
    let mut b = vec![0u8; 64];
    b[1..24].copy_from_slice(b"OSTA Compressed Unicode");
    b
}

/// A regid of `name`, its suffix UDF 1.02's revision where `udf`.
fn regid(name: &str, udf: bool) -> Vec<u8> {
    let mut b = vec![0u8; 32];
    let n = name.as_bytes();
    b[1..1 + n.len()].copy_from_slice(n);
    if udf {
        b[24..26].copy_from_slice(&0x0102u16.to_le_bytes());
    }
    b
}

/// A timestamp (UTC) of 2026-01-01: the image's files' time.
fn timestamp() -> Vec<u8> {
    let mut b = Vec::with_capacity(12);
    le16(&mut b, 0x1000);
    le16(&mut b, 2026);
    b.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0]);
    b
}

/// A long_ad: `length` bytes at logical block `lbn` of partition 0.
fn long_ad(length: u32, lbn: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(16);
    le32(&mut b, length);
    le32(&mut b, lbn);
    le16(&mut b, 0);
    b.extend_from_slice(&[0; 6]);
    b
}

/// A File Identifier Descriptor at logical block `at`.
fn fid(name: &str, characteristics: u8, fe: u32, at: u32) -> Vec<u8> {
    let ident: Vec<u8> = if name.is_empty() {
        Vec::new()
    } else {
        std::iter::once(8u8).chain(name.bytes()).collect()
    };
    let mut body = Vec::new();
    le16(&mut body, 1);
    body.push(characteristics);
    body.push(ident.len() as u8);
    body.extend_from_slice(&long_ad(SECTOR as u32, fe));
    le16(&mut body, 0);
    body.extend_from_slice(&ident);
    while (16 + body.len()) % 4 != 0 {
        body.push(0);
    }
    let mut d = tag(257, at, &body).to_vec();
    d.extend_from_slice(&body);
    d
}

/// A File Entry at logical block `at`: a directory (`dir`) or a file of
/// `size` bytes from logical block `lbn` (in extents under 1 GiB).
fn file_entry(at: u32, dir: bool, size: u64, lbn: u32, links: u16, unique: u64) -> Vec<u8> {
    let mut b = Vec::new();
    // The ICB tag.
    le32(&mut b, 0);
    le16(&mut b, 4);
    le16(&mut b, 0);
    le16(&mut b, 1);
    b.push(0);
    b.push(if dir { 4 } else { 5 });
    b.extend_from_slice(&[0; 6]);
    le16(&mut b, 0);
    le32(&mut b, u32::MAX);
    le32(&mut b, u32::MAX);
    // Read and execute for everyone.
    le32(&mut b, 0x14A5);
    le16(&mut b, links);
    b.push(0);
    b.push(0);
    le32(&mut b, 0);
    le64(&mut b, size);
    le64(&mut b, sectors(size) as u64);
    for _ in 0..3 {
        b.extend_from_slice(&timestamp());
    }
    le32(&mut b, 1);
    b.extend_from_slice(&long_ad(0, 0));
    b.extend_from_slice(&regid("*BananaV", false));
    le64(&mut b, unique);
    le32(&mut b, 0);
    // Short allocation descriptors: extents of at most 1 GiB less a
    // sector.
    let max = (1u64 << 30) - SECTOR as u64;
    let mut ads = Vec::new();
    let (mut left, mut pos) = (size, lbn);
    while left > 0 {
        let n = left.min(max);
        le32(&mut ads, n as u32);
        le32(&mut ads, pos);
        left -= n;
        pos += sectors(n);
    }
    le32(&mut b, ads.len() as u32);
    b.extend_from_slice(&ads);
    descriptor(261, at, &b)
}

/// ISO 9660's both-endian numbers.
fn both32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
    b.extend_from_slice(&v.to_be_bytes());
}

fn both16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
    b.extend_from_slice(&v.to_be_bytes());
}

/// An ISO 9660 directory record.
fn dir_record(name: &[u8], sector: u32, size: u32, dir: bool) -> Vec<u8> {
    let mut b = vec![0u8, 0];
    both32(&mut b, sector);
    both32(&mut b, size);
    // 2026-01-01 00:00:00 UTC.
    b.extend_from_slice(&[126, 1, 1, 0, 0, 0, 0]);
    b.push(if dir { 2 } else { 0 });
    b.extend_from_slice(&[0, 0]);
    both16(&mut b, 1);
    b.push(name.len() as u8);
    b.extend_from_slice(name);
    if b.len() % 2 == 1 {
        b.push(0);
    }
    b[0] = b.len() as u8;
    b
}

/// An ISO 9660 directory's sector: ".", "..", then `entries`.
fn iso_dir(own: u32, parent: u32, entries: &[Vec<u8>]) -> Vec<u8> {
    let mut b = dir_record(&[0], own, SECTOR as u32, true);
    b.extend_from_slice(&dir_record(&[1], parent, SECTOR as u32, true));
    for e in entries {
        b.extend_from_slice(e);
    }
    b.resize(SECTOR, 0);
    b
}

/// The image of the DVD-Video folder `root` (holding `VIDEO_TS` and
/// `AUDIO_TS`), named `label`, written to `out`; its size in bytes.
pub fn write_image(root: &Path, label: &str, out: &mut impl Write) -> io::Result<u64> {
    let files = dvd_files(&root.join("VIDEO_TS"))?;
    // The partition's logical blocks: the file set descriptor and its
    // terminator, the directories' entries and data, the files' entries,
    // then the files.
    let (root_fe, root_data, audio_fe, audio_data, video_fe, video_data) = (2, 3, 4, 5, 6, 7);
    let first_fe = 8u32;
    let mut next = first_fe + files.len() as u32;
    let mut entries: Vec<Entry> = Vec::new();
    for (k, (name, path, size)) in files.into_iter().enumerate() {
        entries.push(Entry {
            name,
            path,
            size,
            lbn: next,
            fe: first_fe + k as u32,
        });
        next += sectors(size).max(1);
    }
    let partition_len = next;
    let total = PARTITION + partition_len + 1;
    let label = label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(32)
        .collect::<String>()
        .to_ascii_uppercase();
    let label = if label.is_empty() {
        "DVD_VIDEO".into()
    } else {
        label
    };

    let mut img = vec![vec![0u8; SECTOR]; PARTITION as usize + first_fe as usize + entries.len()];
    // ISO 9660: the primary volume descriptor and the terminator.
    let mut pvd = vec![1u8];
    pvd.extend_from_slice(b"CD001");
    pvd.push(1);
    pvd.push(0);
    pvd.extend_from_slice(&[b' '; 32]);
    let mut vol = [b' '; 32];
    vol[..label.len()].copy_from_slice(label.as_bytes());
    pvd.extend_from_slice(&vol);
    pvd.extend_from_slice(&[0; 8]);
    both32(&mut pvd, total);
    pvd.extend_from_slice(&[0; 32]);
    both16(&mut pvd, 1);
    both16(&mut pvd, 1);
    both16(&mut pvd, SECTOR as u16);
    // Path tables: the root, AUDIO_TS, VIDEO_TS.
    let path_entry = |name: &[u8], sector: u32, big: bool| -> Vec<u8> {
        let mut e = vec![name.len() as u8, 0];
        e.extend_from_slice(&if big {
            sector.to_be_bytes()
        } else {
            sector.to_le_bytes()
        });
        e.extend_from_slice(&if big {
            1u16.to_be_bytes()
        } else {
            1u16.to_le_bytes()
        });
        e.extend_from_slice(name);
        if name.len() % 2 == 1 {
            e.push(0);
        }
        e
    };
    let table = |big: bool| -> Vec<u8> {
        let mut t = path_entry(&[0], ISO_ROOT, big);
        t.extend(path_entry(b"AUDIO_TS", ISO_AUDIO, big));
        t.extend(path_entry(b"VIDEO_TS", ISO_VIDEO, big));
        t
    };
    let (lt, mt) = (table(false), table(true));
    both32(&mut pvd, lt.len() as u32);
    pvd.extend_from_slice(&L_PATH.to_le_bytes());
    pvd.extend_from_slice(&[0; 4]);
    pvd.extend_from_slice(&M_PATH.to_be_bytes());
    pvd.extend_from_slice(&[0; 4]);
    pvd.extend_from_slice(&dir_record(&[0], ISO_ROOT, SECTOR as u32, true));
    pvd.extend_from_slice(&[b' '; 128 * 4]);
    pvd.extend_from_slice(&[b' '; 37 * 3]);
    // Created and modified; no expiration, no effective date.
    for date in [
        b"2026010100000000",
        b"2026010100000000",
        &[b'0'; 16],
        &[b'0'; 16],
    ] {
        pvd.extend_from_slice(date);
        pvd.push(0);
    }
    pvd.push(1);
    pvd.resize(SECTOR, 0);
    img[16] = pvd;
    let mut term = vec![255u8];
    term.extend_from_slice(b"CD001");
    term.push(1);
    term.resize(SECTOR, 0);
    img[17] = term;
    // The UDF recognition sequence.
    for (k, id) in [b"BEA01", b"NSR02", b"TEA01"].into_iter().enumerate() {
        let mut d = vec![0u8];
        d.extend_from_slice(id);
        d.push(1);
        d.resize(SECTOR, 0);
        img[18 + k] = d;
    }
    for (at, mut t) in [(L_PATH, lt), (M_PATH, mt)] {
        t.resize(SECTOR, 0);
        img[at as usize] = t;
    }
    // ISO 9660's directories, sorted by name.
    let abs = |lbn: u32| PARTITION + lbn;
    img[ISO_ROOT as usize] = iso_dir(
        ISO_ROOT,
        ISO_ROOT,
        &[
            dir_record(b"AUDIO_TS", ISO_AUDIO, SECTOR as u32, true),
            dir_record(b"VIDEO_TS", ISO_VIDEO, SECTOR as u32, true),
        ],
    );
    img[ISO_AUDIO as usize] = iso_dir(ISO_AUDIO, ISO_ROOT, &[]);
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.name.clone());
    let records: Vec<Vec<u8>> = sorted
        .iter()
        .map(|e| {
            let name = format!("{};1", e.name);
            dir_record(name.as_bytes(), abs(e.lbn), e.size as u32, false)
        })
        .collect();
    img[ISO_VIDEO as usize] = iso_dir(ISO_VIDEO, ISO_ROOT, &records);

    // UDF's volume descriptors.
    let vds = |first: u32| -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut b = Vec::new();
        le32(&mut b, 0);
        le32(&mut b, 0);
        b.extend_from_slice(&dstring(&label, 32));
        le16(&mut b, 1);
        le16(&mut b, 1);
        le16(&mut b, 2);
        le16(&mut b, 2);
        le32(&mut b, 1);
        le32(&mut b, 1);
        b.extend_from_slice(&dstring(&label, 128));
        b.extend_from_slice(&charspec());
        b.extend_from_slice(&charspec());
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(&regid("*BananaV", false));
        b.extend_from_slice(&timestamp());
        b.extend_from_slice(&regid("*BananaV", false));
        b.extend_from_slice(&[0; 64]);
        le32(&mut b, 0);
        le16(&mut b, 0);
        b.resize(512 - 16, 0);
        out.push(descriptor(1, first, &b));
        // Implementation use: the logical volume's information.
        let mut b = Vec::new();
        le32(&mut b, 1);
        b.extend_from_slice(&regid("*UDF LV Info", true));
        b.extend_from_slice(&charspec());
        b.extend_from_slice(&dstring(&label, 128));
        b.extend_from_slice(&[0; 36 * 3]);
        b.extend_from_slice(&regid("*BananaV", false));
        b.resize(512 - 16, 0);
        out.push(descriptor(4, first + 1, &b));
        // The partition.
        let mut b = Vec::new();
        le32(&mut b, 2);
        le16(&mut b, 1);
        le16(&mut b, 0);
        b.extend_from_slice(&regid("+NSR02", false));
        b.extend_from_slice(&[0; 128]);
        le32(&mut b, 1);
        le32(&mut b, PARTITION);
        le32(&mut b, partition_len);
        b.extend_from_slice(&regid("*BananaV", false));
        b.resize(512 - 16, 0);
        out.push(descriptor(5, first + 2, &b));
        // The logical volume.
        let mut b = Vec::new();
        le32(&mut b, 3);
        b.extend_from_slice(&charspec());
        b.extend_from_slice(&dstring(&label, 128));
        le32(&mut b, SECTOR as u32);
        b.extend_from_slice(&regid("*OSTA UDF Compliant", true));
        b.extend_from_slice(&long_ad(SECTOR as u32, 0));
        le32(&mut b, 6);
        le32(&mut b, 1);
        b.extend_from_slice(&regid("*BananaV", false));
        b.extend_from_slice(&[0; 128]);
        le32(&mut b, 2 * SECTOR as u32);
        le32(&mut b, INTEGRITY);
        b.extend_from_slice(&[1, 6]);
        le16(&mut b, 1);
        le16(&mut b, 0);
        out.push(descriptor(6, first + 3, &b));
        // No unallocated space.
        let mut b = Vec::new();
        le32(&mut b, 4);
        le32(&mut b, 0);
        out.push(descriptor(7, first + 4, &b));
        out.push(descriptor(8, first + 5, &[0u8; 496]));
        out
    };
    for (k, d) in vds(MAIN_VDS).into_iter().enumerate() {
        img[MAIN_VDS as usize + k] = d;
    }
    for (k, d) in vds(RESERVE_VDS).into_iter().enumerate() {
        img[RESERVE_VDS as usize + k] = d;
    }
    // The integrity descriptor: closed.
    let mut b = Vec::new();
    b.extend_from_slice(&timestamp());
    le32(&mut b, 1);
    b.extend_from_slice(&[0; 8]);
    le64(&mut b, 16 + entries.len() as u64 + 3);
    b.extend_from_slice(&[0; 24]);
    le32(&mut b, 1);
    le32(&mut b, 46);
    le32(&mut b, 0);
    le32(&mut b, partition_len);
    b.extend_from_slice(&regid("*BananaV", false));
    // The files, and the directories with the root.
    le32(&mut b, entries.len() as u32);
    le32(&mut b, 3);
    le16(&mut b, 0x0102);
    le16(&mut b, 0x0102);
    le16(&mut b, 0x0102);
    img[INTEGRITY as usize] = descriptor(9, INTEGRITY, &b);
    img[INTEGRITY as usize + 1] = descriptor(8, INTEGRITY + 1, &[0u8; 496]);
    // The anchor.
    let anchor = |at: u32| -> Vec<u8> {
        let mut b = Vec::new();
        le32(&mut b, 16 * SECTOR as u32);
        le32(&mut b, MAIN_VDS);
        le32(&mut b, 16 * SECTOR as u32);
        le32(&mut b, RESERVE_VDS);
        descriptor(2, at, &b)
    };
    img[256] = anchor(256);

    // The partition: the file set, the directories, the files' entries.
    let p = PARTITION as usize;
    let mut b = Vec::new();
    b.extend_from_slice(&timestamp());
    le16(&mut b, 3);
    le16(&mut b, 3);
    le32(&mut b, 1);
    le32(&mut b, 1);
    le32(&mut b, 0);
    le32(&mut b, 0);
    b.extend_from_slice(&charspec());
    b.extend_from_slice(&dstring(&label, 128));
    b.extend_from_slice(&charspec());
    b.extend_from_slice(&dstring(&label, 32));
    b.extend_from_slice(&[0; 64]);
    b.extend_from_slice(&long_ad(SECTOR as u32, root_fe));
    b.extend_from_slice(&regid("*OSTA UDF Compliant", true));
    b.extend_from_slice(&[0; 16]);
    b.resize(512 - 16, 0);
    img[p] = descriptor(256, 0, &b);
    img[p + 1] = descriptor(8, 1, &[0u8; 496]);
    // The root: its parent (itself), AUDIO_TS, VIDEO_TS.
    let mut data = fid("", 0x0A, root_fe, root_data);
    data.extend(fid("AUDIO_TS", 0x02, audio_fe, root_data));
    data.extend(fid("VIDEO_TS", 0x02, video_fe, root_data));
    img[p + root_fe as usize] = file_entry(root_fe, true, data.len() as u64, root_data, 3, 0);
    data.resize(SECTOR, 0);
    img[p + root_data as usize] = data;
    let data = fid("", 0x0A, root_fe, audio_data);
    img[p + audio_fe as usize] = file_entry(audio_fe, true, data.len() as u64, audio_data, 1, 16);
    let mut d = data;
    d.resize(SECTOR, 0);
    img[p + audio_data as usize] = d;
    let mut data = fid("", 0x0A, root_fe, video_data);
    for e in &entries {
        data.extend(fid(&e.name, 0x00, e.fe, video_data));
    }
    img[p + video_fe as usize] = file_entry(video_fe, true, data.len() as u64, video_data, 1, 17);
    data.resize(SECTOR, 0);
    img[p + video_data as usize] = data;
    for (k, e) in entries.iter().enumerate() {
        img[p + e.fe as usize] = file_entry(e.fe, false, e.size, e.lbn, 1, 18 + k as u64);
    }
    // Out: the descriptors, the files, the last anchor.
    let mut written = 0u64;
    for s in &img {
        out.write_all(s)?;
        written += SECTOR as u64;
    }
    let mut buf = vec![0u8; 1 << 20];
    for e in &entries {
        let mut f = File::open(&e.path)?;
        let mut left = e.size;
        while left > 0 {
            let want = left.min(buf.len() as u64) as usize;
            let n = f.read(&mut buf[..want])?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, e.name.clone()));
            }
            out.write_all(&buf[..n])?;
            left -= n as u64;
        }
        let pad = (sectors(e.size).max(1) as u64 * SECTOR as u64) - e.size;
        out.write_all(&vec![0u8; pad as usize])?;
        written += e.size + pad;
    }
    out.write_all(&anchor(total - 1))?;
    written += SECTOR as u64;
    out.flush()?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32le(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// Whether a descriptor's tag adds up and its CRC holds.
    fn tag_ok(d: &[u8]) -> bool {
        let sum = (0..16)
            .filter(|&i| i != 4)
            .fold(0u8, |s, i| s.wrapping_add(d[i]));
        let len = u16::from_le_bytes([d[10], d[11]]) as usize;
        sum == d[4] && crc16(&d[16..16 + len]) == u16::from_le_bytes([d[8], d[9]])
    }

    #[test]
    fn the_crc_is_ecma_167_s() {
        // ECMA-167's example: the CRC of the bytes 0x70 0x6A 0x77 is 0x3299.
        assert_eq!(crc16(&[0x70, 0x6A, 0x77]), 0x3299);
    }

    #[test]
    fn an_image_holds_the_files_where_both_file_systems_say() {
        let dir = std::env::temp_dir().join(format!("bv-dvd-image-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let video = dir.join("VIDEO_TS");
        std::fs::create_dir_all(&video).unwrap();
        std::fs::create_dir_all(dir.join("AUDIO_TS")).unwrap();
        let contents = [
            ("VIDEO_TS.IFO", 3 * SECTOR, 1u8),
            ("VIDEO_TS.BUP", 3 * SECTOR, 1),
            ("VTS_01_0.IFO", 6 * SECTOR, 2),
            ("VTS_01_1.VOB", 10 * SECTOR, 3),
            ("VTS_01_0.BUP", 6 * SECTOR, 2),
        ];
        for (name, size, v) in contents {
            std::fs::write(video.join(name), vec![v; size]).unwrap();
        }
        let mut image = Vec::new();
        let n = write_image(&dir, "My Disc", &mut image).unwrap();
        assert_eq!(n as usize, image.len());
        assert_eq!(image.len() % SECTOR, 0);
        let sector = |s: u32| &image[s as usize * SECTOR..][..SECTOR];
        // ISO 9660 and UDF's recognition.
        assert_eq!(&sector(16)[1..6], b"CD001");
        assert_eq!(&sector(19)[1..6], b"NSR02");
        // Both anchors, every descriptor's tag good.
        let last = (image.len() / SECTOR) as u32 - 1;
        for s in [256, last] {
            assert_eq!(u16::from_le_bytes([sector(s)[0], sector(s)[1]]), 2);
            assert!(tag_ok(sector(s)));
        }
        for s in 32..38 {
            assert!(tag_ok(sector(s)), "descriptor at {s}");
        }
        // The file set, the root, VIDEO_TS's entries and the files.
        let fsd = sector(PARTITION);
        assert_eq!(u16::from_le_bytes([fsd[0], fsd[1]]), 256);
        assert!(tag_ok(fsd));
        let video_fe = sector(PARTITION + 6);
        assert!(tag_ok(video_fe));
        let video_dir = sector(PARTITION + 7);
        // Its FIDs: the parent, then the five files in DVD's order.
        let mut at = 0;
        let mut names = Vec::new();
        let mut places = Vec::new();
        while at + 38 <= SECTOR && video_dir[at] == 1 && video_dir[at + 1] == 1 {
            let l_fi = video_dir[at + 19] as usize;
            let fe = u32le(video_dir, at + 24);
            let name = String::from_utf8_lossy(&video_dir[at + 38 + 1.min(l_fi)..at + 38 + l_fi])
                .to_string();
            if !name.is_empty() {
                // The file's entry and its first extent.
                let entry = sector(PARTITION + fe);
                assert!(tag_ok(entry), "{name}");
                let ad_len = u32le(entry, 172) as usize;
                assert_eq!(ad_len, 8);
                let lbn = u32le(entry, 176 + 4);
                let size = u32le(entry, 176) as usize;
                let (_, want, v) = contents.iter().find(|c| c.0 == name).unwrap();
                assert_eq!(size, *want);
                assert!(
                    image[(PARTITION + lbn) as usize * SECTOR..][..size]
                        .iter()
                        .all(|b| b == v)
                );
                names.push(name);
                places.push((lbn, size.div_ceil(SECTOR) as u32));
            }
            at += (38 + l_fi).div_ceil(4) * 4;
        }
        assert_eq!(
            names,
            [
                "VIDEO_TS.IFO",
                "VIDEO_TS.BUP",
                "VTS_01_0.IFO",
                "VTS_01_1.VOB",
                "VTS_01_0.BUP"
            ]
        );
        // One after another, as the information files count their places.
        assert!(
            places.windows(2).all(|w| w[0].0 + w[0].1 == w[1].0),
            "{places:?}"
        );
        // ISO 9660 finds them too.
        let iso_video = sector(ISO_VIDEO);
        assert!(iso_video.windows(14).any(|w| w == b"VTS_01_1.VOB;1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
