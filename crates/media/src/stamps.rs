//! When a media file was recorded, which multicamera editing lines events
//! up by (Tools › Multicamera › Sync Media Timecode and Sync
//! Date/Timestamp to Align Events): its start timecode — an MPEG-4 family
//! file's timecode track, an MXF file's timecode component, a DV frame's
//! timecode pack (`bv_mp4::tape`), a Broadcast Wave file's time
//! reference — and the
//! date and time of its start, by what the file says (the movie's creation
//! time; the Broadcast Wave origination date with its time reference) and
//! by when the file was last written less its length.

use std::io::{Read, Seek, SeekFrom};

use bv_core::date::days_from_civil;
use bv_timeline::{MediaInfo, MediaSource};

/// What a media file tells of when it was recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MediaStamps {
    /// The time of day its first frame was recorded at, by its timecode:
    /// seconds since midnight.
    pub timecode: Option<f64>,
    /// When it started by what the file says: seconds since 1970 (the
    /// clock's own time of the camera or recorder, often local).
    pub recorded: Option<f64>,
    /// When it started by when the file was last written less its length:
    /// seconds since 1970.
    pub written: Option<f64>,
}

/// The stamps of `info`'s file (none for generated media).
pub fn stamps(info: &MediaInfo) -> MediaStamps {
    let MediaSource::File { path } = &info.source else {
        return MediaStamps::default();
    };
    let path = crate::local(path);
    let Ok(mut f) = std::fs::File::open(&path) else {
        return MediaStamps::default();
    };
    let written = f
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() - info.duration.as_seconds_f64());
    let mut head = [0u8; 12];
    let read = f.read_exact(&mut head).is_ok();
    let riff = read && matches!(&head[..4], b"RIFF" | b"RF64" | b"BW64");
    let ext = std::path::Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // DV's frames inside files that may hold them.
    let dv = |f: &mut std::fs::File| {
        matches!(ext.as_str(), "dv" | "dif" | "avi" | "mov")
            .then(|| bv_mp4::tape::dv_timecode(f))
            .flatten()
    };
    let (timecode, recorded) = if riff && &head[8..12] == b"WAVE" {
        broadcast_wave(&mut f)
    } else if riff {
        (dv(&mut f), None)
    } else if read && (ext == "mxf" || head[..4] == [0x06, 0x0e, 0x2b, 0x34]) {
        (
            bv_mp4::tape::mxf_timecode(&mut f).map(|t| t.seconds()),
            None,
        )
    } else if f.seek(SeekFrom::Start(0)).is_ok() {
        let s = bv_mp4::read::stamps(&mut f);
        let timecode = s.timecode.map(|t| t.seconds()).or_else(|| dv(&mut f));
        (timecode, s.created.map(|c| c as f64))
    } else {
        (None, None)
    };
    MediaStamps {
        timecode,
        recorded,
        written,
    }
}

/// A Broadcast Wave file's start: the time of day of its time reference,
/// and its origination date at that time (or at its origination time).
fn broadcast_wave<R: Read + Seek>(f: &mut R) -> (Option<f64>, Option<f64>) {
    let Ok(Some(bext)) = bv_wav::read_bext(f) else {
        return (None, None);
    };
    let rate = bv_wav::read_info(f).ok().map(|i| i.sample_rate);
    // A file without one says 0: midnight is as likely a start as any.
    let timecode = rate
        .filter(|&r| r > 0)
        .map(|r| bext.time_reference as f64 / r as f64);
    let numbers = |text: &str| -> Vec<u32> {
        text.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let recorded = match numbers(&bext.date)[..] {
        [y, m, d] if (1..=12).contains(&m) && (1..=31).contains(&d) => {
            let day = days_from_civil(y as i64, m, d) as f64 * 86_400.0;
            let time = match numbers(&bext.time)[..] {
                [h, mi, s] => Some((h * 3600 + mi * 60 + s) as f64),
                _ => None,
            };
            timecode.or(time).map(|t| day + t)
        }
        _ => None,
    };
    (timecode, recorded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broadcast_wave_file_tells_its_start() {
        let bext = bv_wav::Bext {
            date: "2026-10-20".into(),
            time: "14:30:00".into(),
            time_reference: 48_000 * (14 * 3600 + 30 * 60) + 24_000,
            ..bv_wav::Bext::default()
        };
        let mut w = bv_wav::WavWriter::with_metadata(
            std::io::Cursor::new(Vec::new()),
            48_000,
            1,
            bv_wav::SampleFormat::Int(16),
            &[],
            Some(&bext),
        )
        .unwrap();
        w.write_planar(&[vec![0.0; 16]]).unwrap();
        let mut c = std::io::Cursor::new(w.finish().unwrap().into_inner());
        let (timecode, recorded) = broadcast_wave(&mut c);
        assert_eq!(timecode, Some(52_200.5));
        let day = days_from_civil(2026, 10, 20) as f64 * 86_400.0;
        assert_eq!(recorded, Some(day + 52_200.5));
    }
}
