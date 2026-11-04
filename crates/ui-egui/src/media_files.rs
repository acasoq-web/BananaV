//! The files BananaV imports: the filters of the file dialogs, what kind a
//! file is (the Explorer's icons) and the words for import errors.

use std::path::Path;

use bv_engine::{AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, MediaError, VIDEO_EXTENSIONS};

use crate::i18n::tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileKind {
    Video,
    Audio,
    Image,
    Project,
    /// A timeline: OpenTimelineIO's (`.otio`, a `.otioz` bundle; a
    /// `.otiod` bundle is a folder), Final Cut Pro 7's XML (`.xml`) or
    /// FCPXML (`.fcpxml`).
    Timeline,
    /// Another editor's project (`.veg`).
    VegProject,
}

/// What a file is, by its extension (`None`: nothing BananaV opens).
pub(crate) fn kind_of(path: &Path) -> Option<FileKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = ext.as_str();
    if ext == "bvproj" {
        Some(FileKind::Project)
    } else if ext == "veg" {
        Some(FileKind::VegProject)
    } else if matches!(ext, "otio" | "otioz" | "otiod" | "xml" | "fcpxml") {
        Some(FileKind::Timeline)
    } else if VIDEO_EXTENSIONS.contains(&ext) {
        Some(FileKind::Video)
    } else if AUDIO_EXTENSIONS.contains(&ext) {
        Some(FileKind::Audio)
    } else if IMAGE_EXTENSIONS.contains(&ext) {
        Some(FileKind::Image)
    } else {
        None
    }
}

fn all_media() -> Vec<&'static str> {
    VIDEO_EXTENSIONS
        .iter()
        .chain(AUDIO_EXTENSIONS)
        .chain(IMAGE_EXTENSIONS)
        .copied()
        .collect()
}

/// Each kind of media's filter, then any file's.
fn kinds(d: rfd::FileDialog) -> rfd::FileDialog {
    d.add_filter(tr("Video Files"), VIDEO_EXTENSIONS)
        .add_filter(tr("Audio Files"), AUDIO_EXTENSIONS)
        .add_filter(tr("Image Files"), IMAGE_EXTENSIONS)
}

/// Import Media's filters: all media (projects too, which are nested),
/// video, audio, images, projects and any file.
pub(crate) fn media_filters(d: rfd::FileDialog) -> rfd::FileDialog {
    let mut all = all_media();
    all.push("bvproj");
    kinds(d.add_filter(tr("All Media Files"), &all))
        .add_filter(tr("BananaV Project"), &["bvproj"])
        .add_filter(tr("All Files"), &["*"])
}

/// File › Open's filters: projects, timelines and media together first
/// (VEGAS's Open opens both), then each kind.
pub(crate) fn open_filters(d: rfd::FileDialog) -> rfd::FileDialog {
    let mut all = vec!["bvproj", "veg", "otio", "otioz", "xml", "fcpxml"];
    all.extend(all_media());
    let d = d
        .add_filter(tr("All Project and Media Files"), &all)
        .add_filter(tr("BananaV Project"), &["bvproj"])
        .add_filter(tr("VEG Project (*.veg)"), &["veg"])
        .add_filter("OpenTimelineIO (*.otio, *.otioz)", &["otio", "otioz"])
        .add_filter("Final Cut Pro 7/DaVinci Resolve (*.xml)", &["xml"])
        .add_filter("Final Cut Pro X (*.fcpxml)", &["fcpxml"])
        .add_filter(tr("All Media Files"), &all_media());
    kinds(d).add_filter(tr("All Files"), &["*"])
}

/// Why a file could not be imported, in the interface's language.
pub(crate) fn error_text(e: &MediaError) -> String {
    match e {
        MediaError::UnsupportedCodec { codec, video: true } => {
            trf!("{} video is not supported yet", codec)
        }
        MediaError::UnsupportedCodec {
            codec,
            video: false,
        } => trf!("{} audio is not supported yet", codec),
        MediaError::UnsupportedFile(ext) if ext.is_empty() => {
            tr("This kind of file is not supported yet").to_string()
        }
        MediaError::UnsupportedFile(ext) => {
            trf!("{} files are not supported yet", ext.to_uppercase())
        }
        MediaError::NoStream => tr("The file has no picture or sound").to_string(),
        MediaError::Corrupt(s) => trf!("The file is damaged ({})", s),
        MediaError::Io(s) => trf!("The file could not be read ({})", s),
        MediaError::Unsupported(s) => trf!("Not supported ({})", s),
        MediaError::Interrupted => tr("The reading was interrupted").to_string(),
        MediaError::NestedInItself => tr("A project can't be nested in itself").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind_of(Path::new("a.MP4")), Some(FileKind::Video));
        assert_eq!(kind_of(Path::new("a.flac")), Some(FileKind::Audio));
        assert_eq!(kind_of(Path::new("a.heic")), Some(FileKind::Image));
        assert_eq!(kind_of(Path::new("a.gif")), Some(FileKind::Image));
        assert_eq!(kind_of(Path::new("a.bvproj")), Some(FileKind::Project));
        assert_eq!(kind_of(Path::new("a.OTIOZ")), Some(FileKind::Timeline));
        assert_eq!(
            kind_of(Path::new("My Project.xml")),
            Some(FileKind::Timeline)
        );
        assert_eq!(kind_of(Path::new("a.fcpxml")), Some(FileKind::Timeline));
        assert_eq!(kind_of(Path::new("a.txt")), None);
    }
}
