//! File › Import / Export of other programs' formats: picks files, probes
//! media through the engine and edits the project.

use crate::i18n::tr;
use std::path::{Path, PathBuf};

use bv_engine::Engine;
use bv_timeline::interchange::{self, EdlEvent};
use bv_timeline::{MediaId, MediaSource, Project, TrackId, TrackKind};

use crate::app::BananaApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Xmeml,
    Fcpxml,
    Edl,
}

pub(crate) fn project_name(app: &BananaApp) -> String {
    app.project_path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map_or(tr("Untitled").to_string(), |s| {
            s.to_string_lossy().to_string()
        })
}

/// File › Export › XML / FCPXML / EDL.
pub fn export(app: &mut BananaApp, format: Format) {
    let name = project_name(app);
    let (title, filter, ext) = match format {
        Format::Xmeml => (
            tr("Export XML (Final Cut Pro 7 / DaVinci Resolve)"),
            "XML",
            "xml",
        ),
        Format::Fcpxml => (tr("Export Final Cut Pro X"), "FCPXML", "fcpxml"),
        Format::Edl => (tr("Export EDL"), "EDL", "edl"),
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter, &[ext])
        .set_file_name(format!("{name}.{ext}"))
        .save_file()
    else {
        return;
    };
    let (text, skipped) = match format {
        Format::Xmeml => bv_timeline::xmeml::export(app.project(), &name),
        Format::Fcpxml => interchange::export_fcpxml(app.project(), &name),
        Format::Edl => interchange::export_edl(app.project(), &name),
    };
    match std::fs::write(&path, text) {
        Ok(()) if skipped > 0 && format == Format::Xmeml => app.notify(trn!(
            skipped,
            "Exported to {} ({} event could only be written in part: its effects, Pan/Crop or generated media).",
            "Exported to {} ({} events could only be written in part: their effects, Pan/Crop or generated media).",
            path.display(),
            skipped
        )),
        Ok(()) if skipped > 0 => app.notify(trn!(
            skipped,
            "Exported to {} ({} event with generated media or a changed rate could not be represented).",
            "Exported to {} ({} events with generated media or a changed rate could not be represented).",
            path.display(),
            skipped
        )),
        Ok(()) => app.notify(trf!("Exported to {}", path.display())),
        Err(e) => app.report_error(trf!("Could not export: {}", e)),
    }
}

/// Imports (or reuses) the media file at `path`.
fn media_for(app: &mut BananaApp, path: &Path) -> Option<MediaId> {
    app.import_paths(&[path.to_path_buf()]).first().copied()
}

/// The `n`-th track of `kind` (1-based), created when missing.
fn nth_track(p: &mut Project, kind: TrackKind, n: usize) -> TrackId {
    loop {
        let ids: Vec<TrackId> = p
            .tracks
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.id)
            .collect();
        if let Some(&id) = ids.get(n.saturating_sub(1)) {
            return id;
        }
        p.add_track(kind);
    }
}

/// Finds the file of an EDL event: its `SOURCE FILE`, or a file named like
/// the clip next to the EDL.
fn resolve(e: &EdlEvent, dir: &Path) -> Option<PathBuf> {
    if let Some(f) = &e.source_file {
        let p = PathBuf::from(f);
        if p.exists() {
            return Some(p);
        }
    }
    let name = e.clip_name.as_ref().or(e.source_file.as_ref())?;
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let p = dir.join(base);
    p.exists().then_some(p)
}

/// File › Import › EDL text file.
pub fn import_edl(app: &mut BananaApp) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Import EDL"))
        .add_filter("EDL", &["edl", "txt"])
        .pick_file()
    else {
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            app.report_error(trf!("Could not read the EDL: {}", e));
            return;
        }
    };
    let events = interchange::parse_edl(&text, app.frame_rate());
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut placed = 0;
    let mut missing = 0;
    for e in &events {
        let Some(file) = resolve(e, &dir) else {
            missing += 1;
            continue;
        };
        let Some(media) = media_for(app, &file) else {
            missing += 1;
            continue;
        };
        let has = app.project().media(media).is_some_and(|m| m.has(e.kind));
        if !has {
            missing += 1;
            continue;
        }
        let ok = app.edit(|p| {
            let track = nth_track(p, e.kind, e.track);
            let (v, a) = match e.kind {
                TrackKind::Video => (Some(track), None),
                TrackKind::Audio => (None, Some(track)),
            };
            p.insert_media_range(
                media,
                e.record_in,
                e.source_in,
                e.record_out - e.record_in,
                v,
                a,
            )
        });
        if ok.is_some() {
            placed += 1;
        }
    }
    app.fit_timeline = true;
    if missing > 0 {
        app.report_error(trn!(
            placed,
            "EDL: {} event imported; {} without a media file found.",
            "EDL: {} events imported; {} without a media file found.",
            placed,
            missing
        ));
    } else {
        app.notify(trn!(
            placed,
            "EDL: {} event imported.",
            "EDL: {} events imported.",
            placed
        ));
    }
}

/// File › Import › Project Media: brings the media list of another
/// BananaV project into this one.
pub fn import_project_media(app: &mut BananaApp) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Import Media from Another Project"))
        .add_filter(tr("BananaV Project"), &["bvproj"])
        .pick_file()
    else {
        return;
    };
    let other = match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|s| Project::from_json(&s).map_err(|e| e.to_string()))
    {
        Ok(p) => p,
        Err(e) => {
            app.report_error(trf!("Could not read the project: {}", e));
            return;
        }
    };
    let mut files = Vec::new();
    let mut generated = 0;
    for m in other.media {
        match &m.source {
            MediaSource::File { path } => files.push(PathBuf::from(path)),
            MediaSource::Generated(_) => {
                app.modify(|p| p.add_media(m.clone()));
                generated += 1;
            }
        }
    }
    let ids = app.import_paths(&files);
    app.notify(trn!(
        ids.len() + generated,
        "{} media file imported from {}.",
        "{} media files imported from {}.",
        ids.len() + generated,
        path.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string())
    ));
}

/// File › Import › Broadcast Wave: places each file at the time stored in
/// it (or at the cursor when it has none).
pub fn import_bwf(app: &mut BananaApp) {
    let Some(paths) = rfd::FileDialog::new()
        .set_title(tr("Import Broadcast Wave"))
        .add_filter("Wave", &["wav", "bwf"])
        .pick_files()
    else {
        return;
    };
    for path in paths {
        let Some(media) = media_for(app, &path) else {
            continue;
        };
        let at = Engine::bwf_start(&path.to_string_lossy()).unwrap_or(app.cursor);
        app.place_media(media, at, None);
    }
    app.fit_timeline = true;
}
