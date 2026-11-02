//! What an MPEG-4 family file's boxes say of its tracks, read without its
//! media data (a media's Properties' General tab): each track's kind,
//! sample entry, length, bytes and so bit rate, and what its sample
//! description tells — a picture's size, bits per component, color range
//! and alpha; a sound's channels, rate, sample size and its decoder
//! configuration's bit rates —, and the file's major brand.

use std::io::{Read, Seek, SeekFrom};

use bv_bitstream::BitReader;
use bv_bitstream::isobmff::{be_u32, be_u64, boxes, child, moov};

/// One track of a file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackFacts {
    /// Its handler: `vide`, `soun`, `tmcd`…
    pub handler: [u8; 4],
    /// Its sample entry's code: `avc1`, `hvc1`, `mp4a`, `apcn`…
    pub codec: [u8; 4],
    /// Its length in seconds, as its media header says.
    pub seconds: f64,
    /// Its samples' bytes, in all.
    pub bytes: u64,
    /// Its samples are all as big.
    pub same_size: bool,
    /// A picture's size, as its sample entry says.
    pub width: u16,
    pub height: u16,
    /// A picture's bits per component, when its codec's configuration
    /// says (H.264's and HEVC's; ProRes's by its kind).
    pub bit_depth: Option<u8>,
    /// Its color range as the file says (`Some(true)`: full), from its
    /// `colr` box or H.264's sequence parameters.
    pub full_range: Option<bool>,
    /// A picture with alpha (a depth of 32).
    pub alpha: bool,
    /// A sound's channels, sample rate (Hz) and sample size (bits).
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_size: u16,
    /// Its decoder configuration's (`esds`) average and peak bit rates.
    pub avg_bitrate: Option<u32>,
    pub max_bitrate: Option<u32>,
}

impl TrackFacts {
    /// Its bit rate: its bytes over its length (bits a second).
    pub fn bit_rate(&self) -> Option<u64> {
        (self.seconds > 0.0 && self.bytes > 0)
            .then(|| (self.bytes as f64 * 8.0 / self.seconds).round() as u64)
    }

    /// Whether its bit rate is constant: its decoder configuration's peak no
    /// more than its average, else all its samples as big; `None` when
    /// nothing says.
    pub fn constant_rate(&self) -> Option<bool> {
        match (self.avg_bitrate, self.max_bitrate) {
            (Some(avg), Some(max)) if avg > 0 && max > 0 => Some(max as f64 <= avg as f64 * 1.005),
            _ => self.same_size.then_some(true),
        }
    }
}

/// The major brand of a file starting with `head` (its `ftyp` box's), if it
/// is an MPEG-4 family one.
pub fn major_brand(head: &[u8]) -> Option<[u8; 4]> {
    (head.get(4..8)? == b"ftyp").then(|| head.get(8..12)?.try_into().ok())?
}

/// The tracks of an MPEG-4 family file (none when it is not one).
pub fn tracks<R: Read + Seek>(f: &mut R) -> Vec<TrackFacts> {
    let Some(moov) = moov(f) else {
        return Vec::new();
    };
    let _ = f.seek(SeekFrom::Start(0));
    boxes(&moov)
        .filter(|(kind, _)| kind == b"trak")
        .filter_map(|(_, trak)| track(trak))
        .collect()
}

fn track(trak: &[u8]) -> Option<TrackFacts> {
    let mdia = child(trak, b"mdia")?;
    let mut t = TrackFacts {
        handler: child(mdia, b"hdlr")?.get(8..12)?.try_into().ok()?,
        ..TrackFacts::default()
    };
    // `mdhd`: version/flags, two times, the timescale and the duration.
    let mdhd = child(mdia, b"mdhd")?;
    let (scale, length) = match mdhd.first()? {
        1 => (be_u32(mdhd, 20)?, be_u64(mdhd, 24)?),
        _ => (be_u32(mdhd, 12)?, be_u32(mdhd, 16)? as u64),
    };
    if scale > 0 && length != u32::MAX as u64 && length != u64::MAX {
        t.seconds = length as f64 / scale as f64;
    }
    let stbl = child(child(mdia, b"minf")?, b"stbl")?;
    if let Some(stsz) = child(stbl, b"stsz") {
        // Version/flags, one size for all (else 0), the count, the sizes.
        let (size, count) = (be_u32(stsz, 4)?, be_u32(stsz, 8)?);
        if size > 0 {
            t.bytes = size as u64 * count as u64;
            t.same_size = true;
        } else {
            let sizes: Vec<u32> = (0..count as usize)
                .map_while(|i| be_u32(stsz, 12 + 4 * i))
                .collect();
            t.bytes = sizes.iter().map(|&s| s as u64).sum();
            t.same_size = sizes.windows(2).all(|w| w[0] == w[1]);
        }
    }
    // `stsd`: version/flags and a count, then the entries.
    let (code, entry) = boxes(child(stbl, b"stsd")?.get(8..)?).next()?;
    t.codec = code;
    match &t.handler {
        b"vide" => visual(&mut t, entry),
        b"soun" => sound(&mut t, entry),
        _ => {}
    }
    Some(t)
}

/// A visual sample entry: 8 bytes of every entry, then its fields, its
/// boxes after them (78 bytes in).
fn visual(t: &mut TrackFacts, entry: &[u8]) {
    let u16_at = |at: usize| {
        entry
            .get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    t.width = u16_at(24).unwrap_or(0);
    t.height = u16_at(26).unwrap_or(0);
    t.alpha = u16_at(74) == Some(32);
    let inner = entry.get(78..).unwrap_or(&[]);
    t.bit_depth = match &t.codec {
        b"avc1" | b"avc3" => child(inner, b"avcC").and_then(avc_bit_depth),
        b"hvc1" | b"hev1" => child(inner, b"hvcC")
            .and_then(|c| c.get(17))
            .map(|b| (b & 7) + 8),
        b"apco" | b"apcs" | b"apcn" | b"apch" => Some(10),
        b"ap4h" | b"ap4x" => Some(12),
        _ => None,
    };
    // `colr` with `nclx`: primaries, transfer, matrix, then the range flag.
    t.full_range = child(inner, b"colr")
        .filter(|c| c.get(..4) == Some(b"nclx"))
        .and_then(|c| c.get(10))
        .map(|b| b & 0x80 != 0)
        .or_else(|| {
            matches!(&t.codec, b"avc1" | b"avc3")
                .then(|| child(inner, b"avcC").and_then(avc_full_range))
                .flatten()
        });
}

/// An audio sample entry: 8 bytes of every entry, then QuickTime's
/// version (0 for MP4's), its channels, sample size and rate, its boxes
/// after 28 bytes (QuickTime's versions 1 and 2 have more).
fn sound(t: &mut TrackFacts, entry: &[u8]) {
    let u16_at = |at: usize| {
        entry
            .get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    let version = u16_at(8).unwrap_or(0);
    t.channels = u16_at(16).unwrap_or(0);
    t.sample_size = u16_at(18).unwrap_or(0);
    t.sample_rate = be_u32(entry, 24).map_or(0, |r| r >> 16);
    let skip = match version {
        1 => 28 + 16,
        2 => 28 + 36,
        _ => 28,
    };
    if version == 2 {
        // Version 2 keeps its rate as a 64-bit float and its channels apart.
        if let Some(rate) = be_u64(entry, 32) {
            t.sample_rate = f64::from_bits(rate).round() as u32;
        }
        if let Some(ch) = be_u32(entry, 40) {
            t.channels = ch as u16;
        }
    }
    let inner = entry.get(skip..).unwrap_or(&[]);
    let esds =
        child(inner, b"esds").or_else(|| child(inner, b"wave").and_then(|w| child(w, b"esds")));
    if let Some((avg, max)) = esds.and_then(|e| decoder_bitrates(e.get(4..)?)) {
        t.avg_bitrate = Some(avg);
        t.max_bitrate = Some(max);
    }
}

/// An `esds` descriptor's decoder configuration's average and peak bit
/// rates (MPEG-4 Systems' descriptors: a tag, a size in 7-bit groups).
fn decoder_bitrates(mut d: &[u8]) -> Option<(u32, u32)> {
    // The tag and the length that follows it; the rest.
    fn open(d: &[u8]) -> Option<(u8, &[u8])> {
        let tag = *d.first()?;
        let mut len = 0usize;
        let mut at = 1;
        loop {
            let b = *d.get(at)?;
            len = (len << 7) | (b & 0x7f) as usize;
            at += 1;
            if b & 0x80 == 0 || at > 4 {
                break;
            }
        }
        Some((tag, d.get(at..at + len.min(d.len() - at))?))
    }
    let (tag, es) = open(d)?;
    if tag != 0x03 {
        return None;
    }
    // ES_ID, then flags saying what follows.
    let flags = *es.get(2)?;
    let mut at = 3;
    if flags & 0x80 != 0 {
        at += 2;
    }
    if flags & 0x40 != 0 {
        at += 1 + *es.get(at)? as usize;
    }
    if flags & 0x20 != 0 {
        at += 2;
    }
    d = es.get(at..)?;
    let (tag, config) = open(d)?;
    if tag != 0x04 {
        return None;
    }
    // Object type, stream type, buffer size (3), then the peak and average.
    Some((be_u32(config, 9)?, be_u32(config, 5)?))
}

/// H.264's sequence parameters, in an `avcC`'s first: its NAL unit without
/// emulation prevention bytes.
fn avc_sps(avcc: &[u8]) -> Option<Vec<u8>> {
    if *avcc.get(5)? & 0x1f == 0 {
        return None;
    }
    let len = u16::from_be_bytes([*avcc.get(6)?, *avcc.get(7)?]) as usize;
    let nal = avcc.get(8..8 + len)?;
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &b in nal.get(1..)? {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    Some(out)
}

/// The profiles whose sequence parameters say the chroma format and depth.
fn high_profile(profile: u32) -> bool {
    matches!(
        profile,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    )
}

/// H.264's bits per component.
fn avc_bit_depth(avcc: &[u8]) -> Option<u8> {
    let sps = avc_sps(avcc)?;
    let mut r = BitReader::new(&sps);
    let profile = r.read_bits(8).ok()?;
    if !high_profile(profile) {
        return Some(8);
    }
    r.skip_bits(16).ok()?;
    r.read_ue().ok()?;
    if r.read_ue().ok()? == 3 {
        r.skip_bits(1).ok()?;
    }
    Some(r.read_ue().ok()? as u8 + 8)
}

/// H.264's color range, from its sequence parameters' video signal type
/// (`None` when they do not say).
fn avc_full_range(avcc: &[u8]) -> Option<bool> {
    let sps = avc_sps(avcc)?;
    let mut r = BitReader::new(&sps);
    let profile = r.read_bits(8).ok()?;
    r.skip_bits(16).ok()?;
    r.read_ue().ok()?;
    if high_profile(profile) {
        let chroma = r.read_ue().ok()?;
        if chroma == 3 {
            r.skip_bits(1).ok()?;
        }
        r.read_ue().ok()?;
        r.read_ue().ok()?;
        r.skip_bits(1).ok()?;
        if r.read_bit().ok()? {
            // The scaling lists, skipped.
            for i in 0..if chroma == 3 { 12 } else { 8 } {
                if r.read_bit().ok()? {
                    let size = if i < 6 { 16 } else { 64 };
                    let (mut last, mut next) = (8i32, 8i32);
                    for _ in 0..size {
                        if next != 0 {
                            let delta = r.read_se().ok()?;
                            next = (last + delta + 256) % 256;
                        }
                        if next != 0 {
                            last = next;
                        }
                    }
                }
            }
        }
    }
    r.read_ue().ok()?;
    match r.read_ue().ok()? {
        0 => {
            r.read_ue().ok()?;
        }
        1 => {
            r.skip_bits(1).ok()?;
            r.read_se().ok()?;
            r.read_se().ok()?;
            for _ in 0..r.read_ue().ok()? {
                r.read_se().ok()?;
            }
        }
        _ => {}
    }
    r.read_ue().ok()?;
    r.skip_bits(1).ok()?;
    r.read_ue().ok()?;
    r.read_ue().ok()?;
    if !r.read_bit().ok()? {
        r.skip_bits(1).ok()?;
    }
    r.skip_bits(1).ok()?;
    if r.read_bit().ok()? {
        for _ in 0..4 {
            r.read_ue().ok()?;
        }
    }
    // The VUI: the aspect ratio, overscan, then the video signal type.
    if !r.read_bit().ok()? {
        return None;
    }
    if r.read_bit().ok()? && r.read_bits(8).ok()? == 255 {
        r.skip_bits(32).ok()?;
    }
    if r.read_bit().ok()? {
        r.skip_bits(1).ok()?;
    }
    if !r.read_bit().ok()? {
        return None;
    }
    r.skip_bits(3).ok()?;
    r.read_bit().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An H.264 configuration of the High profile, 8 bits, its video
    /// signal type limited range: what its sequence parameters say.
    #[test]
    fn h264_depth_and_range() {
        // profile 100, level 40, sps id 0, chroma 1, depths 8, no scaling,
        // frame num 4, poc type 0 (lsb 4), refs 4, no gaps, 120 × 68
        // macroblocks, frame only, 8×8, no crop, VUI: no aspect, no
        // overscan, video signal type (format 5, range 0).
        let mut w = bv_bitstream::BitWriter::new();
        w.write_bits(100, 8);
        w.write_bits(0, 8);
        w.write_bits(40, 8);
        w.write_ue(0);
        w.write_ue(1);
        w.write_ue(0);
        w.write_ue(0);
        w.write_bits(0, 1);
        w.write_bits(0, 1);
        w.write_ue(0);
        w.write_ue(0);
        w.write_ue(0);
        w.write_ue(4);
        w.write_bits(0, 1);
        w.write_ue(119);
        w.write_ue(67);
        w.write_bits(1, 1);
        w.write_bits(1, 1);
        w.write_bits(0, 1);
        w.write_bits(1, 1);
        w.write_bits(0, 1);
        w.write_bits(0, 1);
        w.write_bits(1, 1);
        w.write_bits(5, 3);
        w.write_bits(0, 1);
        w.write_bits(0, 1);
        w.write_bits(1, 1);
        w.write_trailing_bits();
        let rbsp = w.into_bytes();
        let mut sps = vec![0x67];
        sps.extend_from_slice(&rbsp);
        let mut avcc = vec![1, 100, 0, 40, 0xff, 0xe1];
        avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(&sps);
        avcc.extend_from_slice(&[0]);
        assert_eq!(avc_bit_depth(&avcc), Some(8));
        assert_eq!(avc_full_range(&avcc), Some(false));
    }

    /// An `esds` with its decoder configuration's rates.
    #[test]
    fn esds_rates() {
        let mut config = vec![0x40, 0x15, 0, 0, 0];
        config.extend_from_slice(&140_000u32.to_be_bytes());
        config.extend_from_slice(&128_000u32.to_be_bytes());
        let mut es = vec![0, 1, 0];
        es.push(0x04);
        es.push(config.len() as u8);
        es.extend_from_slice(&config);
        let mut d = vec![0x03, es.len() as u8];
        d.extend_from_slice(&es);
        assert_eq!(decoder_bitrates(&d), Some((128_000, 140_000)));
    }

    #[test]
    fn brands() {
        assert_eq!(major_brand(b"\0\0\0\x18ftypisom\0\0\x02\0"), Some(*b"isom"));
        assert_eq!(major_brand(b"RIFF\0\0\0\0AVI "), None);
    }
}
