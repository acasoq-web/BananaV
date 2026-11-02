//! Just enough of the ISO base media file format (MP4, MOV, M4A) to read
//! what Media Foundation leaves out: how many samples at the start of each
//! audio track are the encoder's priming (the edit list, `elst`). AAC
//! encoders put about a thousand samples there; a decoder that plays them
//! runs late (ffmpeg and VEGAS skip them).

use bv_bitstream::isobmff::{be_u32, be_u64, boxes, child, read_moov, timescale};

/// The edit list's entries: (segment duration in the movie's timescale,
/// media time in the track's timescale; −1 is an empty edit).
fn edits(elst: &[u8]) -> Vec<(u64, i64)> {
    let version = elst.first().copied().unwrap_or(0);
    let count = be_u32(elst, 4).unwrap_or(0) as usize;
    let mut out = Vec::new();
    let mut at = 8;
    for _ in 0..count.min(1000) {
        let entry = if version == 1 {
            let d = be_u64(elst, at);
            let t = be_u64(elst, at + 8).map(|t| t as i64);
            at += 20;
            d.zip(t)
        } else {
            let d = be_u32(elst, at).map(u64::from);
            let t = be_u32(elst, at + 4).map(|t| t as i32 as i64);
            at += 12;
            d.zip(t)
        };
        match entry {
            Some(e) => out.push(e),
            None => break,
        }
    }
    out
}

/// For each audio track of a `moov` box, in file order: how many samples
/// (at `rate`) to skip at the start (priming), negative when the edit list
/// starts the track late (silence first).
fn audio_skips(moov: &[u8], rate: u32) -> Vec<i64> {
    let movie_scale = child(moov, b"mvhd")
        .and_then(timescale)
        .unwrap_or(1000)
        .max(1);
    let mut out = Vec::new();
    for (kind, trak) in boxes(moov) {
        if &kind != b"trak" {
            continue;
        }
        let Some(mdia) = child(trak, b"mdia") else {
            continue;
        };
        // `hdlr`: version/flags, pre_defined, then the handler type.
        let audio = child(mdia, b"hdlr")
            .and_then(|h| h.get(8..12))
            .is_some_and(|t| t == b"soun");
        if !audio {
            continue;
        }
        let track_scale = child(mdia, b"mdhd")
            .and_then(timescale)
            .unwrap_or(rate)
            .max(1);
        let edits = child(trak, b"edts")
            .and_then(|e| child(e, b"elst"))
            .map(edits)
            .unwrap_or_default();
        // Empty edits first delay the track; the first real one says where
        // in the media it starts.
        let mut delay = 0u64;
        let mut start = 0i64;
        for (duration, media_time) in edits {
            if media_time < 0 {
                delay += duration;
            } else {
                start = media_time;
                break;
            }
        }
        let skip = start as i128 * rate as i128 / track_scale as i128
            - delay as i128 * rate as i128 / movie_scale as i128;
        out.push(skip as i64);
    }
    out
}

/// Samples (at `rate`) to skip at the start of audio track `index` (0 = the
/// file's first audio track) of an MPEG-4 family file; 0 when there is no
/// edit list.
pub(crate) fn audio_skip(path: &str, index: usize, rate: u32) -> i64 {
    read_moov(path)
        .and_then(|moov| audio_skips(&moov, rate).get(index).copied())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    fn full(version: u8, rest: &[u8]) -> Vec<u8> {
        let mut v = vec![version, 0, 0, 0];
        v.extend_from_slice(rest);
        v
    }

    /// A track: handler `kind`, timescale `scale` and an edit list.
    fn trak(kind: &[u8; 4], scale: u32, elst: &[(u32, i32)]) -> Vec<u8> {
        let mut hdlr = vec![0u8; 4];
        hdlr.extend_from_slice(kind);
        hdlr.extend_from_slice(&[0u8; 12]);
        let mut mdhd = vec![0u8; 8];
        mdhd.extend_from_slice(&scale.to_be_bytes());
        mdhd.extend_from_slice(&[0u8; 8]);
        let mut list = (elst.len() as u32).to_be_bytes().to_vec();
        for (d, t) in elst {
            list.extend_from_slice(&d.to_be_bytes());
            list.extend_from_slice(&t.to_be_bytes());
            list.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        }
        let mdia = [bx(b"mdhd", &full(0, &mdhd)), bx(b"hdlr", &full(0, &hdlr))].concat();
        let edts = bx(b"elst", &full(0, &list));
        [bx(b"edts", &edts), bx(b"mdia", &mdia)].concat()
    }

    #[test]
    fn priming_and_delays_from_the_edit_lists() {
        let mut mvhd = vec![0u8; 8];
        mvhd.extend_from_slice(&1000u32.to_be_bytes());
        let moov = [
            bx(b"mvhd", &full(0, &mvhd)),
            bx(b"trak", &trak(b"vide", 15360, &[(60_000, 0)])),
            // ffmpeg's AAC: skip 1024 priming samples.
            bx(b"trak", &trak(b"soun", 48_000, &[(60_000, 1024)])),
            // Starts 0.5 s late (empty edit of 500 ms), then from 2112.
            bx(
                b"trak",
                &trak(b"soun", 44_100, &[(500, -1), (60_000, 2112)]),
            ),
            // No edit list.
            bx(b"trak", &trak(b"soun", 48_000, &[])),
        ]
        .concat();
        let skips = audio_skips(&moov, 48_000);
        assert_eq!(skips, vec![1024, 2298 - 24_000, 0]);
    }
}
