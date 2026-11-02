//! What an MPEG-4 family file (MP4, MOV, M4A) says about itself, read
//! without its media data (its boxes walked by `bv_bitstream::isobmff`):
//! the stamps multicamera editing lines events up by — the start of its
//! timecode track (QuickTime's `tmcd`, which cameras write into their MP4
//! files too) and when its movie was made (`mvhd`) — and whether its
//! video is 360° (Google's spherical video metadata).

use std::io::{Read, Seek, SeekFrom};

use bv_bitstream::isobmff::{be_u32, be_u64, boxes, child, moov};

/// Where a timecode track starts: its first sample's frame count, and how
/// it counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timecode {
    /// Frames since 00:00:00:00.
    pub frame: u32,
    /// Frames a timecode second (30 for 29.97).
    pub fps: u8,
    /// Drop-frame counting: labels skipped to keep up with the clock.
    pub drop_frame: bool,
    /// A frame lasts `frame_duration` ⁄ `timescale` seconds.
    pub timescale: u32,
    pub frame_duration: u32,
}

impl Timecode {
    /// The time of day it names, in seconds: its label's
    /// (hh:mm:ss + ff ⁄ fps), which drop-frame counts keep on the clock.
    pub fn seconds(&self) -> f64 {
        let fps = self.fps.max(1) as u64;
        let mut frame = self.frame as u64;
        if self.drop_frame && fps >= 30 {
            // The labels skipped: two (four at 60) at each minute but the
            // tenth ones (SMPTE 12M's drop-frame counting).
            let drop = 2 * fps / 30;
            let per_ten = 600 * fps - 9 * drop;
            let per_minute = 60 * fps - drop;
            let (tens, rest) = (frame / per_ten, frame % per_ten);
            frame += 9 * drop * tens;
            if rest > drop {
                frame += drop * ((rest - drop) / per_minute);
            }
        }
        frame as f64 / fps as f64
    }
}

/// What a file says about when it was recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamps {
    /// Where its timecode track starts.
    pub timecode: Option<Timecode>,
    /// When its movie was made, in seconds since 1970 (UTC as the
    /// specification asks; many cameras write their own clock's time).
    pub created: Option<i64>,
}

/// The stamps of an MPEG-4 family file (none when it is not one).
pub fn stamps<R: Read + Seek>(f: &mut R) -> Stamps {
    let Some(moov) = moov(f) else {
        return Stamps::default();
    };
    Stamps {
        timecode: timecode(&moov, f),
        created: created(&moov),
    }
}

/// The project a file was rendered from, when its renderer kept the path
/// (BananaV's `bvpr` box in the movie's user data: Render's "Save project
/// as path reference in rendered file").
pub fn project_reference<R: Read + Seek>(f: &mut R) -> Option<String> {
    let moov = moov(f)?;
    let udta = child(&moov, b"udta")?;
    let text = std::str::from_utf8(child(udta, b"bvpr")?).ok()?;
    (!text.is_empty()).then(|| text.to_string())
}

/// The kind of Spherical Video V1's `uuid` box (its RDF XML follows).
pub const SPHERICAL_V1: [u8; 16] = [
    0xff, 0xcc, 0x82, 0x63, 0xf8, 0x55, 0x4a, 0x93, 0x88, 0x14, 0x58, 0x7a, 0x02, 0x52, 0x1f, 0xdd,
];

/// Whether a movie's video is 360°: Spherical Video V2's `sv3d` box in a
/// track's sample description, or V1's `uuid` box in a track whose XML
/// says `<GSpherical:Spherical>true`.
pub fn spherical<R: Read + Seek>(f: &mut R) -> bool {
    let Some(moov) = moov(f) else {
        return false;
    };
    boxes(&moov)
        .filter(|(kind, _)| kind == b"trak")
        .any(|(_, trak)| spherical_v1(trak) || spherical_v2(trak).unwrap_or(false))
}

fn spherical_v1(trak: &[u8]) -> bool {
    const SAYS: &[u8] = b"GSpherical:Spherical>true";
    boxes(trak).any(|(kind, body)| {
        &kind == b"uuid"
            && body.starts_with(&SPHERICAL_V1)
            && body[16..]
                .windows(SAYS.len())
                .any(|w| w.eq_ignore_ascii_case(SAYS))
    })
}

fn spherical_v2(trak: &[u8]) -> Option<bool> {
    let stbl = child(child(child(trak, b"mdia")?, b"minf")?, b"stbl")?;
    // `stsd`: version/flags and a count, then the entries; a visual one's
    // boxes follow its 78 bytes of fields.
    let entry = boxes(child(stbl, b"stsd")?.get(8..)?).next()?.1;
    Some(boxes(entry.get(78..)?).any(|(kind, _)| &kind == b"sv3d"))
}

/// Seconds from 1904-01-01, the files' epoch, to 1970-01-01.
const FROM_1904: i64 = 2_082_844_800;

fn created(moov: &[u8]) -> Option<i64> {
    let mvhd = child(moov, b"mvhd")?;
    let secs = match mvhd.first()? {
        1 => be_u64(mvhd, 4)?,
        _ => be_u32(mvhd, 4)? as u64,
    };
    (secs > 0).then(|| secs as i64 - FROM_1904)
}

/// The first timecode track's start: its sample description says how it
/// counts, its first sample (in its first chunk) the frame.
fn timecode<R: Read + Seek>(moov: &[u8], f: &mut R) -> Option<Timecode> {
    boxes(moov)
        .filter(|(kind, _)| kind == b"trak")
        .find_map(|(_, trak)| {
            let mdia = child(trak, b"mdia")?;
            // `hdlr`: version/flags, pre_defined, then the handler type.
            if child(mdia, b"hdlr")?.get(8..12)? != b"tmcd" {
                return None;
            }
            let stbl = child(child(mdia, b"minf")?, b"stbl")?;
            // `stsd`: version/flags and a count, then the entries: size,
            // type, six reserved bytes and a data reference index, then
            // the timecode's reserved word, flags, timescale, frame
            // duration and frames a second.
            let entry = child(stbl, b"stsd")?.get(8..)?;
            if entry.get(4..8)? != b"tmcd" {
                return None;
            }
            let fields = entry.get(16..)?;
            let flags = be_u32(fields, 4)?;
            let timescale = be_u32(fields, 8)?;
            let frame_duration = be_u32(fields, 12)?;
            let fps = *fields.get(16)?;
            let offset = match child(stbl, b"stco") {
                Some(stco) => be_u32(stco, 8)? as u64,
                None => be_u64(child(stbl, b"co64")?, 8)?,
            };
            f.seek(SeekFrom::Start(offset)).ok()?;
            let mut b = [0u8; 4];
            f.read_exact(&mut b).ok()?;
            Some(Timecode {
                frame: u32::from_be_bytes(b),
                fps,
                drop_frame: flags & 1 != 0,
                timescale,
                frame_duration,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    /// A movie made at `created` (1904's seconds) with a timecode track
    /// whose one sample, `frame`, sits in `mdat`.
    fn movie(created: u32, frame: u32, drop: bool, fps: u8) -> Vec<u8> {
        let mut mvhd = vec![0u8; 4];
        mvhd.extend(created.to_be_bytes());
        mvhd.extend(created.to_be_bytes());
        mvhd.extend(30_000u32.to_be_bytes());
        mvhd.extend([0u8; 84]);
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"tmcd");
        hdlr.extend([0u8; 13]);
        let mut entry = vec![0u8; 8];
        entry.extend([0u8; 4]);
        entry.extend(u32::from(drop).to_be_bytes());
        entry.extend(30_000u32.to_be_bytes());
        entry.extend(1001u32.to_be_bytes());
        entry.push(fps);
        entry.push(0);
        let mut stsd = vec![0u8; 4];
        stsd.extend(1u32.to_be_bytes());
        stsd.extend(bx(b"tmcd", &entry));
        // The sample's offset is known once the movie's size is.
        let build = |offset: u32| {
            let mut stco = vec![0u8; 4];
            stco.extend(1u32.to_be_bytes());
            stco.extend(offset.to_be_bytes());
            let stbl = bx(b"stbl", &[bx(b"stsd", &stsd), bx(b"stco", &stco)].concat());
            let minf = bx(b"minf", &stbl);
            let mdia = bx(b"mdia", &[bx(b"hdlr", &hdlr), minf].concat());
            let moov = bx(b"moov", &[bx(b"mvhd", &mvhd), bx(b"trak", &mdia)].concat());
            [bx(b"ftyp", b"qt  \0\0\0\0qt  "), moov].concat()
        };
        let head = build(0).len() as u32;
        [build(head + 8), bx(b"mdat", &frame.to_be_bytes())].concat()
    }

    #[test]
    fn the_timecode_and_the_date_are_read() {
        // 2026-10-20 12:00:00 UTC, and 01:00:00;00 at 29.97 drop-frame.
        let created = (1_792_497_600 + FROM_1904) as u32;
        let s = stamps(&mut Cursor::new(movie(created, 107_892, true, 30)));
        assert_eq!(s.created, Some(1_792_497_600));
        let tc = s.timecode.expect("a timecode track");
        assert!(tc.drop_frame && tc.fps == 30);
        assert!((tc.seconds() - 3600.0).abs() < 1e-9, "{}", tc.seconds());
        // Not drop-frame: 10:00:00:15 at 25.
        let s = stamps(&mut Cursor::new(movie(created, 900_015, false, 25)));
        assert!((s.timecode.unwrap().seconds() - 36_000.6).abs() < 1e-9);
        // Not a movie.
        assert_eq!(
            stamps(&mut Cursor::new(b"RIFF....WAVE".to_vec())),
            Stamps::default()
        );
    }

    #[test]
    fn drop_frame_labels_skip_two_a_minute_but_every_tenth() {
        let tc = |frame| Timecode {
            frame,
            fps: 30,
            drop_frame: true,
            timescale: 30_000,
            frame_duration: 1001,
        };
        // 00:00:59;29 is frame 1799; the next is 00:01:00;02.
        assert!((tc(1799).seconds() - (59.0 + 29.0 / 30.0)).abs() < 1e-9);
        assert!((tc(1800).seconds() - (60.0 + 2.0 / 30.0)).abs() < 1e-9);
        // 00:10:00;00 is frame 17982.
        assert!((tc(17_982).seconds() - 600.0).abs() < 1e-9);
    }
}
