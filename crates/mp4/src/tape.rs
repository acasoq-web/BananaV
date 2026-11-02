//! The start timecode of MXF and DV files (multicamera editing's Sync
//! Media Timecode): an MXF file's timecode component (SMPTE 377M's header
//! metadata: its start, rounded timecode base and drop frame), and a DV
//! frame's timecode pack in its subcode (IEC 61834's pack 0x13, the
//! label in BCD) — of a raw DV file or of the first DV frame in another
//! container (an AVI's, a QuickTime's).

use std::io::{Read, Seek, SeekFrom};

use crate::read::Timecode;

/// How far into a file its header metadata or its first DV frame is
/// looked for.
const LOOK: u64 = 8 << 20;

/// The start timecode of an MXF file (none when it is not one or has
/// none): the first timecode component that starts past midnight, else
/// the first.
pub fn mxf_timecode<R: Read + Seek>(f: &mut R) -> Option<Timecode> {
    f.seek(SeekFrom::Start(0)).ok()?;
    let mut head = Vec::new();
    f.take(LOOK).read_to_end(&mut head).ok()?;
    // Its first key: a partition pack (an optional run-in before it).
    let start = head
        .windows(4)
        .take(65_536)
        .position(|w| w == [0x06, 0x0e, 0x2b, 0x34])?;
    let mut at = start;
    let mut found: Vec<Timecode> = Vec::new();
    while at + 17 <= head.len() {
        let key = &head[at..at + 16];
        if key[..4] != [0x06, 0x0e, 0x2b, 0x34] {
            break;
        }
        let (len, used) = ber(&head[at + 16..])?;
        let body = at + 16 + used;
        let end = body.checked_add(len)?;
        // The essence starts: the header metadata is over.
        if key[4] == 0x01 && key[5] == 0x02 && key[8..12] == [0x0d, 0x01, 0x03, 0x01] {
            break;
        }
        if key[4] == 0x02
            && key[5] == 0x53
            && key[8..15] == [0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x14]
            && end <= head.len()
            && let Some(t) = timecode_component(&head[body..end])
        {
            found.push(t);
        }
        if end > head.len() {
            break;
        }
        at = end;
    }
    found
        .iter()
        .copied()
        .find(|t| t.frame > 0)
        .or(found.first().copied())
}

/// A BER length: its value and how many bytes it took.
fn ber(b: &[u8]) -> Option<(usize, usize)> {
    let first = *b.first()?;
    if first < 0x80 {
        return Some((first as usize, 1));
    }
    let n = (first & 0x7f) as usize;
    if n == 0 || n > 8 || b.len() < 1 + n {
        return None;
    }
    let len = b[1..=n].iter().fold(0u64, |v, &x| v << 8 | x as u64);
    Some((usize::try_from(len).ok()?, 1 + n))
}

/// A timecode component's local set: its start (0x1501), rounded base
/// (0x1502) and drop frame (0x1503).
fn timecode_component(set: &[u8]) -> Option<Timecode> {
    let (mut start, mut base, mut drop) = (None, None, false);
    let mut i = 0;
    while i + 4 <= set.len() {
        let tag = u16::from_be_bytes([set[i], set[i + 1]]);
        let len = u16::from_be_bytes([set[i + 2], set[i + 3]]) as usize;
        let v = set.get(i + 4..i + 4 + len)?;
        match (tag, len) {
            (0x1501, 8) => start = Some(i64::from_be_bytes(v.try_into().ok()?)),
            (0x1502, 2) => base = Some(u16::from_be_bytes([v[0], v[1]])),
            (0x1503, 1) => drop = v[0] != 0,
            _ => {}
        }
        i += 4 + len;
    }
    let base = base.filter(|&b| (1..=255).contains(&b))?;
    Some(Timecode {
        frame: u32::try_from(start?.max(0)).ok()?,
        fps: base as u8,
        drop_frame: drop,
        timescale: u32::from(base) * if drop { 1000 } else { 1 },
        frame_duration: if drop { 1001 } else { 1 },
    })
}

/// The time of day a DV file's first frame was recorded at, by its
/// timecode pack: seconds since midnight (its label, hh:mm:ss + ff ⁄ fps).
pub fn dv_timecode<R: Read + Seek>(f: &mut R) -> Option<f64> {
    f.seek(SeekFrom::Start(0)).ok()?;
    let mut head = Vec::new();
    f.take(LOOK).read_to_end(&mut head).ok()?;
    // A DIF sequence's start: its header block, then its first subcode
    // block 80 bytes on.
    let start = (0..head.len().saturating_sub(83)).find(|&i| {
        head[i] == 0x1f
            && head[i + 1] & 0x0f == 0x07
            && head[i + 2] == 0
            && head[i + 80] == 0x3f
            && head[i + 81] & 0x0f == 0x07
    })?;
    // 625/50 when the header's DSF bit is set, else 525/60.
    let fps = if head.get(start + 3)? & 0x80 != 0 {
        25.0
    } else {
        30.0
    };
    let sequences = if fps == 25.0 { 12 } else { 10 };
    for seq in 0..sequences {
        for block in [1, 2] {
            let b = start + (seq * 150 + block) * 80;
            let dif = head.get(b..b + 80)?;
            if dif[0] >> 5 != 1 {
                continue;
            }
            // Six sync blocks of 8 bytes: two of ID, one reserved, a pack.
            for k in 0..6 {
                let pack = &dif[3 + k * 8 + 3..3 + k * 8 + 8];
                if pack[0] == 0x13
                    && let Some(t) = dv_label(pack, fps)
                {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// A timecode pack's label (frames, seconds, minutes, hours in BCD) as
/// seconds since midnight; none when it is blank or not BCD.
fn dv_label(pack: &[u8], fps: f64) -> Option<f64> {
    let bcd = |v: u8, tens_mask: u8| -> Option<u32> {
        let (t, u) = ((v >> 4) & tens_mask, v & 0x0f);
        (u <= 9).then_some(u32::from(t) * 10 + u32::from(u))
    };
    let frames = bcd(pack[1], 0x03)?;
    let seconds = bcd(pack[2], 0x07)?;
    let minutes = bcd(pack[3], 0x07)?;
    let hours = bcd(pack[4], 0x03)?;
    if pack[1..5].iter().all(|&b| b == 0xff) || seconds > 59 || minutes > 59 || hours > 23 {
        return None;
    }
    Some(f64::from(hours * 3600 + minutes * 60 + seconds) + f64::from(frames) / fps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A KLV of `key` holding `value` (a long-form length).
    fn klv(key: [u8; 16], value: &[u8]) -> Vec<u8> {
        let mut v = key.to_vec();
        v.push(0x83);
        v.extend(&(value.len() as u32).to_be_bytes()[1..]);
        v.extend(value);
        v
    }

    fn component(start: i64, base: u16, drop: bool) -> Vec<u8> {
        let mut set = Vec::new();
        set.extend([0x15, 0x02, 0, 2]);
        set.extend(base.to_be_bytes());
        set.extend([0x15, 0x01, 0, 8]);
        set.extend(start.to_be_bytes());
        set.extend([0x15, 0x03, 0, 1, u8::from(drop)]);
        let key = [
            0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x14, 0x00,
        ];
        klv(key, &set)
    }

    #[test]
    fn an_mxf_file_tells_its_start_timecode() {
        let partition = [
            0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02,
            0x04, 0x00,
        ];
        let mut file = klv(partition, &[0; 88]);
        // The material package's at zero, the source's at 10:00:00:12.
        file.extend(component(0, 25, false));
        let start = ((10 * 3600) * 25 + 12) as i64;
        file.extend(component(start, 25, false));
        let t = mxf_timecode(&mut Cursor::new(file)).unwrap();
        assert_eq!((t.frame, t.fps, t.drop_frame), (start as u32, 25, false));
        assert!((t.seconds() - (36_000.0 + 12.0 / 25.0)).abs() < 1e-9);
        // Drop-frame counting at 29.97: one minute in, 00:01:00;02.
        let mut file = klv(partition, &[0; 88]);
        file.extend(component(1800, 30, true));
        let t = mxf_timecode(&mut Cursor::new(file)).unwrap();
        assert!((t.seconds() - (60.0 + 2.0 / 30.0)).abs() < 1e-9);
        assert!(mxf_timecode(&mut Cursor::new(b"not an mxf file".to_vec())).is_none());
    }

    /// A DV frame of `sequences` DIF sequences whose sequence 5's second
    /// subcode block holds a timecode pack (13:42:07:19).
    fn dv_frame(pal: bool) -> Vec<u8> {
        let sequences = if pal { 12 } else { 10 };
        let mut v = vec![0u8; sequences * 150 * 80];
        for s in 0..sequences {
            let b = s * 150 * 80;
            v[b] = 0x1f;
            v[b + 1] = (s as u8) << 4 | 0x07;
            v[b + 3] = if pal { 0x80 } else { 0 };
            for (k, block) in [1usize, 2].iter().enumerate() {
                let o = b + block * 80;
                v[o] = 0x3f;
                v[o + 1] = (s as u8) << 4 | 0x07;
                v[o + 2] = k as u8;
                for byte in &mut v[o + 3..o + 80] {
                    *byte = 0xff;
                }
            }
        }
        let o = (5 * 150 + 2) * 80 + 3 + 2 * 8 + 3;
        v[o..o + 5].copy_from_slice(&[0x13, 0x19, 0x07, 0x42, 0x13]);
        v
    }

    #[test]
    fn a_dv_frame_tells_its_timecode() {
        let at = |fps: f64| 13.0 * 3600.0 + 42.0 * 60.0 + 7.0 + 19.0 / fps;
        let t = dv_timecode(&mut Cursor::new(dv_frame(true))).unwrap();
        assert!((t - at(25.0)).abs() < 1e-9);
        let t = dv_timecode(&mut Cursor::new(dv_frame(false))).unwrap();
        assert!((t - at(30.0)).abs() < 1e-9);
        // Inside another container, after its headers.
        let mut avi = b"RIFF\0\0\0\0AVI LIST".to_vec();
        avi.extend(vec![0u8; 1000]);
        avi.extend(dv_frame(true));
        assert!(dv_timecode(&mut Cursor::new(avi)).is_some());
        assert!(dv_timecode(&mut Cursor::new(vec![0u8; 5000])).is_none());
    }
}
