//! Just enough of MP3 files to read what Media Foundation leaves out: the
//! encoder's delay that LAME (and ffmpeg's LAME) note in the first frame
//! (the "Xing"/"Info" frame and its LAME extension). Windows plays that
//! frame as silence and the encoder's padding too; players that support
//! gapless MP3 skip them.

/// Samples at the start of an MP3 file's sound that are not part of it, as
/// Windows decodes it: the info frame, the encoder's padding and one more.
/// `None` for files without an info frame.
pub(crate) fn info_skip(path: &str) -> Option<i64> {
    use std::io::Read;
    let mut d = vec![0u8; 1 << 16];
    let n = std::fs::File::open(path).ok()?.read(&mut d).ok()?;
    skip_of(&d[..n])
}

fn skip_of(d: &[u8]) -> Option<i64> {
    // An ID3v2 tag first (its size is "syncsafe": 7 bits a byte).
    let mut pos = 0usize;
    if d.starts_with(b"ID3") && d.len() >= 10 {
        let size = ((d[6] as usize) << 21)
            | ((d[7] as usize) << 14)
            | ((d[8] as usize) << 7)
            | d[9] as usize;
        pos = 10 + size + if d[5] & 0x10 != 0 { 10 } else { 0 };
    }
    let h = u32::from_be_bytes(d.get(pos..pos + 4)?.try_into().ok()?);
    // Frame sync, layer III.
    if h >> 21 != 0x7ff || (h >> 17) & 3 != 1 {
        return None;
    }
    let mpeg1 = (h >> 19) & 3 == 3;
    let mono = (h >> 6) & 3 == 3;
    let side = match (mpeg1, mono) {
        (true, true) => 17,
        (true, false) => 32,
        (false, true) => 9,
        (false, false) => 17,
    };
    let x = pos + 4 + side;
    let tag = d.get(x..x + 4)?;
    if tag != b"Xing" && tag != b"Info" {
        return None;
    }
    let flags = u32::from_be_bytes(d.get(x + 4..x + 8)?.try_into().ok()?);
    let mut y = x + 8;
    for (bit, size) in [(1, 4), (2, 4), (4, 100), (8, 4)] {
        if flags & bit != 0 {
            y += size;
        }
    }
    let frame = if mpeg1 { 1152 } else { 576 };
    // The LAME extension's 12-bit encoder delay, 21 bytes in.
    let pad = match d.get(y..y + 24) {
        Some(lame) if lame[..4].iter().all(|b| b.is_ascii_alphanumeric()) => {
            ((lame[21] as i64) << 4) | (lame[22] as i64 >> 4)
        }
        _ => 0,
    };
    Some(frame + pad + 1)
}

#[cfg(test)]
mod tests {
    use super::skip_of;

    #[test]
    fn lame_delay() {
        // A 44-byte ID3 tag, an MPEG-1 mono frame with Info and a LAME tag
        // whose delay is 576.
        let mut d = b"ID3\x04\0\0\0\0\0\x22".to_vec();
        d.resize(44, 0);
        d.extend_from_slice(&[0xff, 0xfb, 0x54, 0xc0]);
        d.extend_from_slice(&[0; 17]);
        d.extend_from_slice(b"Info");
        d.extend_from_slice(&0u32.to_be_bytes());
        let mut lame = b"LAME3.100".to_vec();
        lame.resize(21, 0);
        lame.extend_from_slice(&[0x24, 0x05, 0x40]);
        d.extend_from_slice(&lame);
        assert_eq!(skip_of(&d), Some(1152 + 576 + 1));
    }
}
