//! Markers and regions of media files (the Trimmer's): WAV files keep them
//! inside (`cue ` points, `labl` names, `ltxt` region lengths); other types
//! in a file beside them, `<file>.bvmarkers`.

use std::path::{Path, PathBuf};

use bv_core::{SampleRate, Tick};
use bv_timeline::MediaMarker;

/// Where a media file's markers are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerStore {
    /// Inside the file (WAV).
    InFile,
    /// In `<file>.bvmarkers` beside it.
    Beside,
}

/// The file beside `media` that keeps its markers.
pub fn markers_path(media: &str) -> PathBuf {
    let mut name = Path::new(media).as_os_str().to_os_string();
    name.push(".bvmarkers");
    PathBuf::from(name)
}

/// A WAV file's sample rate, if `path` is one BananaV writes markers into
/// (plain RIFF).
fn wav_rate(path: &str) -> Option<u32> {
    let mut f = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut head = [0u8; 4];
    std::io::Read::read_exact(&mut f, &mut head).ok()?;
    if &head != b"RIFF" {
        return None;
    }
    bv_wav::read_info(&mut f).ok().map(|i| i.sample_rate)
}

/// The markers and regions a media file has (none when it has none).
pub fn read_markers(path: &str) -> Vec<MediaMarker> {
    if let Some(text) = std::fs::read_to_string(markers_path(path)).ok()
        && let Some(markers) = bv_timeline::markers_from_json(&text)
    {
        return markers;
    }
    let Some(rate) = wav_rate(path) else {
        return Vec::new();
    };
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let rate = SampleRate(rate);
    bv_wav::read_cues(&mut std::io::BufReader::new(file))
        .map(|cues| {
            cues.into_iter()
                .map(|c| MediaMarker {
                    time: rate.sample_start(c.frame as i64),
                    length: rate.sample_start(c.length as i64),
                    name: c.name,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether markers can be saved with `path`: a file that is there and not
/// read-only.
pub fn can_save_markers(path: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && !m.permissions().readonly())
}

/// Saves `markers` with the media file `path`: into it (WAV) or beside it.
pub fn save_markers(path: &str, markers: &[MediaMarker]) -> Result<MarkerStore, String> {
    if !can_save_markers(path) {
        return Err("read-only".into());
    }
    if let Some(rate) = wav_rate(path) {
        let rate = SampleRate(rate);
        let frames = |t: Tick| rate.sample_at(t).clamp(0, u32::MAX as i64) as u32;
        let cues: Vec<bv_wav::Cue> = markers
            .iter()
            .map(|m| bv_wav::Cue {
                frame: frames(m.time),
                length: if m.is_region() {
                    frames(m.end()).saturating_sub(frames(m.time)).max(1)
                } else {
                    0
                },
                name: m.name.clone(),
            })
            .collect();
        let written = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())
            .and_then(|mut f| bv_wav::write_cues(&mut f, &cues).map_err(|e| e.to_string()));
        if written.is_ok() {
            // Kept inside from now on.
            let _ = std::fs::remove_file(markers_path(path));
            return Ok(MarkerStore::InFile);
        }
    }
    let beside = markers_path(path);
    if markers.is_empty() {
        return match std::fs::remove_file(&beside) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(MarkerStore::Beside),
        };
    }
    std::fs::write(&beside, bv_timeline::markers_to_json(markers))
        .map(|()| MarkerStore::Beside)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks() -> Vec<MediaMarker> {
        vec![
            MediaMarker {
                time: Tick::from_seconds(1),
                length: Tick::ZERO,
                name: "Hit".into(),
            },
            MediaMarker {
                time: Tick::from_seconds(2),
                length: Tick::from_seconds(1),
                name: "Chorus".into(),
            },
        ]
    }

    #[test]
    fn wav_files_keep_them_inside_and_others_beside() {
        let dir = std::env::temp_dir().join(format!("bv-media-markers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A WAV of 4 s at 48 kHz.
        let wav = dir.join("take.wav");
        let mut w = bv_wav::WavWriter::new(
            std::io::BufWriter::new(std::fs::File::create(&wav).unwrap()),
            48_000,
            1,
            bv_wav::SampleFormat::Int(16),
        )
        .unwrap();
        w.write_planar(&[vec![0.0; 48_000 * 4]]).unwrap();
        w.finish().unwrap();
        let wav = wav.to_string_lossy().to_string();
        assert!(read_markers(&wav).is_empty());
        assert_eq!(save_markers(&wav, &marks()), Ok(MarkerStore::InFile));
        assert_eq!(read_markers(&wav), marks());
        assert!(!markers_path(&wav).exists());
        let probed = crate::probe_file(&wav).unwrap();
        assert_eq!(probed.markers, marks(), "found when imported");
        assert_eq!(probed.duration, Tick::from_seconds(4));

        // Any other file: beside it.
        let other = dir.join("clip.mp4");
        std::fs::write(&other, b"not really a video").unwrap();
        let other = other.to_string_lossy().to_string();
        assert_eq!(save_markers(&other, &marks()), Ok(MarkerStore::Beside));
        assert_eq!(read_markers(&other), marks());
        assert_eq!(save_markers(&other, &[]), Ok(MarkerStore::Beside));
        assert!(!markers_path(&other).exists());
        assert!(save_markers(&dir.join("gone.wav").to_string_lossy(), &marks()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
