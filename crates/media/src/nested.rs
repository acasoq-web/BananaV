//! Nested projects (VEGAS's "Nesting projects"): a BananaV project file
//! brought into another project as media. What it is — its length, picture
//! size, sound, markers and regions — is read from the file here; its
//! pictures and its sound come from readers a layer above draws and mixes
//! them with ([`set_nested_readers`]), since this crate neither composites
//! nor mixes.

use std::path::Path;
use std::sync::OnceLock;

use bv_timeline::{
    AudioStreamInfo, MediaInfo, MediaMarker, MediaSource, Project, TrackKind, VideoStreamInfo,
};

use crate::{AudioReader, MediaError, Result, VideoReader};

/// The extension of BananaV's project files.
pub const PROJECT_EXTENSION: &str = "bvproj";

/// Whether `path` is a BananaV project file (by its extension).
pub fn is_project(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(PROJECT_EXTENSION))
}

/// Who reads a nested project's pictures and sound: functions given the
/// project, set once by the layer that draws and mixes projects.
#[derive(Clone, Copy, Debug)]
pub struct NestedReaders {
    pub video: fn(Project) -> Result<Box<dyn VideoReader>>,
    pub audio: fn(Project) -> Result<Box<dyn AudioReader>>,
}

static READERS: OnceLock<NestedReaders> = OnceLock::new();

/// Sets who reads nested projects (the first call counts).
pub fn set_nested_readers(readers: NestedReaders) {
    let _ = READERS.set(readers);
}

/// The project in the file at `path`.
pub fn read_project(path: &str) -> Result<Project> {
    let text = std::fs::read_to_string(path).map_err(|e| MediaError::Io(e.to_string()))?;
    let p = Project::from_json(&text).map_err(|e| MediaError::Corrupt(e.to_string()))?;
    p.check_invariants().map_err(MediaError::Corrupt)?;
    Ok(p)
}

/// Whether `a` and `b` name the same file (as the system resolves them;
/// else their text, ignoring case and the slashes' direction).
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            let plain = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
            plain(a) == plain(b)
        }
    }
}

/// The project files nested in the project at `path`, all the way down
/// (each once; files that do not read are left out).
pub fn nested_projects(path: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut todo = vec![path.to_string()];
    while let Some(next) = todo.pop() {
        let Ok(p) = read_project(&next) else {
            continue;
        };
        for m in &p.media {
            if let MediaSource::File { path } = &m.source
                && is_project(path)
                && !found
                    .iter()
                    .any(|f| same_file(Path::new(f), Path::new(path)))
            {
                found.push(path.clone());
                todo.push(path.clone());
            }
        }
    }
    found
}

/// Whether the project at `path` is nested in itself, at some depth (it
/// would draw itself forever).
pub fn nests_itself(path: &str) -> bool {
    nested_projects(path)
        .iter()
        .any(|p| same_file(Path::new(p), Path::new(path)))
}

/// The project at `path` as media: as long as its last event's end, its
/// output's picture when it has video events, its master bus's sound
/// folded to stereo when it has audio events (VEGAS: "If you nest a 5.1
/// surround project, the audio event will be a stereo downmix"), and its
/// markers and regions as the media's.
pub(crate) fn probe(path: &str) -> Result<MediaInfo> {
    let p = read_project(path)?;
    if nests_itself(path) {
        return Err(MediaError::NestedInItself);
    }
    let has = |kind| {
        p.tracks
            .iter()
            .any(|t| t.kind == kind && !t.clips.is_empty())
    };
    let s = &p.settings;
    let (width, height) = s.output_size();
    let video = has(TrackKind::Video).then_some(VideoStreamInfo {
        width,
        height,
        frame_rate: s.frame_rate,
        pixel_aspect: s.pixel_aspect,
        field_order: s.field_order,
        spherical: s.output_360,
        full_range: false,
        reading: Default::default(),
    });
    let audio = has(TrackKind::Audio).then_some(AudioStreamInfo {
        sample_rate: s.sample_rate,
        channels: 2,
    });
    if video.is_none() && audio.is_none() {
        return Err(MediaError::NoStream);
    }
    let markers = p
        .markers
        .iter()
        .map(|m| MediaMarker {
            time: m.time,
            length: bv_core::Tick::ZERO,
            name: m.name.clone(),
        })
        .chain(p.regions.iter().map(|r| MediaMarker {
            time: r.range.start,
            length: r.range.duration,
            name: r.name.clone(),
        }))
        .collect();
    Ok(MediaInfo {
        duration: p.duration(),
        video,
        audio,
        markers,
        ..crate::files::base(path)
    })
}

/// The readers set, or why there are none.
fn readers() -> Result<NestedReaders> {
    READERS
        .get()
        .copied()
        .ok_or_else(|| MediaError::Unsupported("nested projects".into()))
}

/// The project at `path`, to be read as media (never one nested in
/// itself).
fn nested(path: &str) -> Result<Project> {
    let p = read_project(path)?;
    if nests_itself(path) {
        return Err(MediaError::NestedInItself);
    }
    Ok(p)
}

pub(crate) fn open_video(path: &str) -> Result<Box<dyn VideoReader>> {
    let readers = readers()?;
    (readers.video)(nested(path)?)
}

pub(crate) fn open_audio(path: &str, stream: u16) -> Result<Box<dyn AudioReader>> {
    if stream > 0 {
        return Err(MediaError::NoStream);
    }
    let readers = readers()?;
    (readers.audio)(nested(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::{Tick, TimeRange};
    use bv_timeline::{Generator, Marker, MarkerId, ProjectSettings, Region};

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bv-nested-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Two seconds of color on the first video track, a marker at 0.5 s
    /// and a region from 1 s to 1.5 s.
    fn colored() -> Project {
        let mut p = Project::new(ProjectSettings::default());
        let v = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| t.id);
        let m = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings,
        ));
        let clip = p.insert_media(m, Tick::ZERO, v, None).unwrap()[0];
        p.set_clip_duration(clip, Tick::from_seconds(2)).unwrap();
        p.markers.push(Marker {
            id: MarkerId(1),
            time: Tick::from_seconds_f64(0.5),
            name: "Hit".into(),
        });
        p.regions.push(Region {
            id: MarkerId(2),
            range: TimeRange::new(Tick::from_seconds(1), Tick::from_seconds_f64(0.5)),
            name: "Chorus".into(),
        });
        p
    }

    #[test]
    fn a_project_file_is_media_of_its_length_and_size() {
        let d = dir("probe");
        let path = d.join("Lower third.bvproj");
        std::fs::write(&path, colored().to_json()).unwrap();
        let info = crate::probe_file(&path.to_string_lossy()).unwrap();
        assert_eq!(info.duration, Tick::from_seconds(2));
        let v = info.video.expect("its picture");
        let (w, h) = ProjectSettings::default().output_size();
        assert_eq!((v.width, v.height), (w, h));
        assert!(info.audio.is_none(), "no audio events, no sound");
        assert_eq!(info.markers.len(), 2);
        assert_eq!(info.markers[0].name, "Hit");
        assert!(info.markers[1].is_region());
        assert_eq!(info.name, "Lower third.bvproj");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_empty_project_has_nothing_to_nest() {
        let d = dir("empty");
        let path = d.join("Empty.bvproj");
        std::fs::write(&path, Project::new(ProjectSettings::default()).to_json()).unwrap();
        assert_eq!(
            crate::probe_file(&path.to_string_lossy()),
            Err(MediaError::NoStream)
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_project_nested_in_itself_is_refused() {
        let d = dir("cycle");
        let a = d.join("A.bvproj");
        let b = d.join("B.bvproj");
        // A nests B, and B nests A.
        let nest = |p: &mut Project, path: &Path| {
            let mut info = colored().media[0].clone();
            info.source = MediaSource::File {
                path: path.to_string_lossy().into(),
            };
            p.add_media(info);
        };
        let mut pa = colored();
        nest(&mut pa, &b);
        let mut pb = colored();
        nest(&mut pb, &a);
        std::fs::write(&a, pa.to_json()).unwrap();
        std::fs::write(&b, pb.to_json()).unwrap();
        let a_text = a.to_string_lossy().to_string();
        assert!(nests_itself(&a_text));
        assert_eq!(nested_projects(&a_text).len(), 2);
        assert_eq!(
            crate::probe_file(&a_text).err(),
            Some(MediaError::NestedInItself)
        );
        // B on its own nests nothing.
        std::fs::write(&b, colored().to_json()).unwrap();
        assert!(!nests_itself(&a_text));
        assert!(crate::probe_file(&a_text).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn project_files_are_told_by_their_extension() {
        assert!(is_project("C:/Shows/Intro.BVPROJ"));
        assert!(is_project("intro.bvproj"));
        assert!(!is_project("intro.mp4"));
        assert!(!is_project("bvproj"));
    }
}
