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

/// The presentation times of the first video track's keyframes (its sync
/// samples), from its first frame on — the times the decoders give, which
/// start the first frame at zero —, sorted. `None` when the file is not of
/// the MPEG-4 family, its samples are in fragments, or every sample is a
/// keyframe (no `stss`: seeking costs nothing to learn from it).
pub(crate) fn video_keyframes(path: &str) -> Option<Vec<bv_core::Tick>> {
    keyframes_in(&read_moov(path)?)
}

/// [`video_keyframes`], read once per file (while its size and time stay):
/// each of a file's readers is opened with it.
pub(crate) fn video_keyframes_cached(path: &str) -> Option<std::sync::Arc<Vec<bv_core::Tick>>> {
    type Kept = (
        u64,
        Option<std::time::SystemTime>,
        Option<std::sync::Arc<Vec<bv_core::Tick>>>,
    );
    static KEPT: std::sync::Mutex<Vec<(String, Kept)>> = std::sync::Mutex::new(Vec::new());
    let meta = std::fs::metadata(path).ok()?;
    let stamp = (meta.len(), meta.modified().ok());
    if let Ok(kept) = KEPT.lock()
        && let Some((_, (len, time, k))) = kept.iter().find(|(p, _)| p == path)
        && (*len, *time) == stamp
    {
        return k.clone();
    }
    let found = video_keyframes(path).map(std::sync::Arc::new);
    if let Ok(mut kept) = KEPT.lock() {
        kept.retain(|(p, _)| p != path);
        // A few hundred files at most: the oldest go.
        if kept.len() >= 256 {
            kept.remove(0);
        }
        kept.push((path.to_string(), (stamp.0, stamp.1, found.clone())));
    }
    found
}

/// [`video_keyframes`] of a `moov` box.
fn keyframes_in(moov: &[u8]) -> Option<Vec<bv_core::Tick>> {
    /// More samples than any file has: a damaged table stops here.
    const MOST: usize = 50_000_000;
    for (kind, trak) in boxes(moov) {
        if &kind != b"trak" {
            continue;
        }
        let Some(mdia) = child(trak, b"mdia") else {
            continue;
        };
        let video = child(mdia, b"hdlr")
            .and_then(|h| h.get(8..12))
            .is_some_and(|t| t == b"vide");
        if !video {
            continue;
        }
        let scale = i128::from(child(mdia, b"mdhd").and_then(timescale)?.max(1));
        let stbl = child(mdia, b"minf").and_then(|m| child(m, b"stbl"))?;
        let stss = child(stbl, b"stss")?;
        let stts = child(stbl, b"stts")?;
        // Decode times: (count, delta) runs.
        let mut pts: Vec<i64> = Vec::new();
        let mut t = 0i64;
        let runs = be_u32(stts, 4)? as usize;
        for i in 0..runs {
            let count = be_u32(stts, 8 + 8 * i)? as usize;
            let delta = i64::from(be_u32(stts, 12 + 8 * i)?);
            if pts.len() + count > MOST {
                return None;
            }
            for _ in 0..count {
                pts.push(t);
                t += delta;
            }
        }
        // Composition offsets (B-frames): signed in version 1.
        if let Some(ctts) = child(stbl, b"ctts") {
            let signed = ctts.first() == Some(&1);
            let runs = be_u32(ctts, 4)? as usize;
            let mut at = 0usize;
            for i in 0..runs {
                let count = be_u32(ctts, 8 + 8 * i)? as usize;
                let raw = be_u32(ctts, 12 + 8 * i)?;
                let offset = if signed {
                    i64::from(raw as i32)
                } else {
                    i64::from(raw)
                };
                for p in pts.iter_mut().skip(at).take(count) {
                    *p += offset;
                }
                at += count;
            }
        }
        let first = *pts.iter().min()?;
        let syncs = be_u32(stss, 4)? as usize;
        let mut out = Vec::with_capacity(syncs.min(MOST));
        for i in 0..syncs.min(MOST) {
            let n = be_u32(stss, 8 + 4 * i)? as usize;
            let Some(&p) = n.checked_sub(1).and_then(|k| pts.get(k)) else {
                continue;
            };
            let ticks = i128::from(p - first) * i128::from(bv_core::TICKS_PER_SECOND) / scale;
            out.push(bv_core::Tick(ticks as i64));
        }
        out.sort();
        out.dedup();
        return Some(out);
    }
    None
}

/// Whether going on decoding from the frame at `last` to the one at `t`
/// (later) passes no keyframe of `keyframes`: then it is cheaper than a
/// seek, which would start at the keyframe before `last`. Times within
/// half a frame (`slack`) of `last` count as `last`'s.
pub(crate) fn same_group(
    keyframes: &[bv_core::Tick],
    last: bv_core::Tick,
    t: bv_core::Tick,
    slack: bv_core::Tick,
) -> bool {
    let from = keyframes.partition_point(|k| *k <= last + slack);
    keyframes.get(from).is_none_or(|k| *k > t + slack)
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

    /// A video track's sample tables: decode-time runs (count, delta),
    /// composition offsets (version, runs) and sync samples.
    fn video_trak(
        scale: u32,
        stts: &[(u32, u32)],
        ctts: Option<(u8, &[(u32, i32)])>,
        stss: &[u32],
    ) -> Vec<u8> {
        let mut hdlr = vec![0u8; 4];
        hdlr.extend_from_slice(b"vide");
        hdlr.extend_from_slice(&[0u8; 12]);
        let mut mdhd = vec![0u8; 8];
        mdhd.extend_from_slice(&scale.to_be_bytes());
        mdhd.extend_from_slice(&[0u8; 8]);
        let table = |runs: &[(u32, u32)]| {
            let mut v = (runs.len() as u32).to_be_bytes().to_vec();
            for (a, b) in runs {
                v.extend_from_slice(&a.to_be_bytes());
                v.extend_from_slice(&b.to_be_bytes());
            }
            v
        };
        let mut stbl = bx(b"stts", &full(0, &table(stts)));
        if let Some((version, runs)) = ctts {
            let runs: Vec<(u32, u32)> = runs.iter().map(|&(c, o)| (c, o as u32)).collect();
            stbl.extend(bx(b"ctts", &full(version, &table(&runs))));
        }
        let mut sync = (stss.len() as u32).to_be_bytes().to_vec();
        for n in stss {
            sync.extend_from_slice(&n.to_be_bytes());
        }
        stbl.extend(bx(b"stss", &full(0, &sync)));
        let mdia = [
            bx(b"mdhd", &full(0, &mdhd)),
            bx(b"hdlr", &full(0, &hdlr)),
            bx(b"minf", &bx(b"stbl", &stbl)),
        ]
        .concat();
        bx(b"mdia", &mdia)
    }

    #[test]
    fn keyframes_come_from_the_sample_tables() {
        use bv_core::{TICKS_PER_SECOND, Tick};
        let s = |f: i64| Tick(f * TICKS_PER_SECOND / 30);
        // 30 frames a second, a keyframe every ten frames, after an audio
        // track (passed over).
        let moov = [
            bx(b"trak", &trak(b"soun", 48_000, &[])),
            bx(b"trak", &video_trak(30, &[(30, 1)], None, &[1, 11, 21])),
        ]
        .concat();
        assert_eq!(keyframes_in(&moov), Some(vec![s(0), s(10), s(20)]));
        // B-frames: every frame two later (version 0), or the first one
        // earlier than the others (version 1, signed): the first frame
        // stays at zero.
        let moov = bx(
            b"trak",
            &video_trak(30, &[(30, 1)], Some((0, &[(30, 2)])), &[1, 11]),
        );
        assert_eq!(keyframes_in(&moov), Some(vec![s(0), s(10)]));
        let moov = bx(
            b"trak",
            &video_trak(30, &[(30, 1)], Some((1, &[(1, -1), (29, 0)])), &[1, 11]),
        );
        assert_eq!(keyframes_in(&moov), Some(vec![s(0), s(11)]));
        // Every frame a keyframe (no stss) or no video: nothing to learn.
        assert_eq!(
            keyframes_in(&bx(b"trak", &trak(b"soun", 48_000, &[]))),
            None
        );
    }

    #[test]
    fn going_on_beats_a_seek_within_a_group_of_pictures() {
        use bv_core::Tick;
        let k = [Tick(0), Tick(100), Tick(200)];
        let slack = Tick(1);
        assert!(same_group(&k, Tick(10), Tick(90), slack));
        assert!(!same_group(&k, Tick(10), Tick(150), slack), "100 between");
        assert!(same_group(&k, Tick(100), Tick(150), slack), "from on it");
        assert!(!same_group(&k, Tick(90), Tick(100), slack), "onto the next");
        assert!(
            same_group(&k, Tick(250), Tick(900), slack),
            "after the last"
        );
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
