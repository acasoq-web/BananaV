//! The project's tools: what it has, new, open and save, its settings,
//! its media.

use std::path::{Path, PathBuf};

use bv_core::{FrameRate, SampleRate};
use serde_json::{Value, json};

use super::{Args, Kind, Tool, arg, event_json, fps, media_json, media_of, secs, track_json};
use crate::agents::{Outcome, Output};
use crate::app::BananaApp;
use crate::i18n::tr;
use crate::media_files::FileKind;

fn done(v: Value) -> Outcome {
    Ok(Output::value(v))
}

/// A frame rate from frames a second: the NTSC ones exact.
pub(super) fn frame_rate(r: f64) -> Result<FrameRate, String> {
    if !(r > 0.5 && r <= 1000.0) {
        return Err(trf!("Not a frame rate: {}", r));
    }
    for (n, d) in [(24000, 1001), (30000, 1001), (60000, 1001), (120000, 1001)] {
        if (r - f64::from(n) / f64::from(d)).abs() < 0.005 {
            return Ok(FrameRate::new(n, d));
        }
    }
    if (r - r.round()).abs() < 1e-6 {
        return Ok(FrameRate::new(r.round() as u32, 1));
    }
    Ok(FrameRate::new((r * 1000.0).round() as u32, 1000))
}

fn info(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let detail = a.bool_or("detail", false)?;
    let p = app.project();
    let s = &p.settings;
    let mut v = json!({
        "path": app.project_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        "dirty": app.is_dirty(),
        "settings": {
            "width": s.width,
            "height": s.height,
            "frame_rate": fps(s.frame_rate),
            "sample_rate": s.sample_rate.0,
            "channels": serde_json::to_value(s.channels).unwrap_or(Value::Null),
            "pixel_format": serde_json::to_value(s.pixel_format).unwrap_or(Value::Null),
        },
        "length": secs(p.duration()),
        "tracks": p.tracks.len(),
        "events": p.tracks.iter().map(|t| t.clips.len()).sum::<usize>(),
        "media": p.media.len(),
        "markers": p.markers.len(),
        "regions": p.regions.len(),
        "cursor": secs(app.cursor),
        "loop_region": p.loop_region.map(|r| json!({"start": secs(r.start), "end": secs(r.end())})),
        "selection": app.selection.iter().map(|c| c.0).collect::<Vec<_>>(),
        "can_undo": app.history.can_undo(),
        "can_redo": app.history.can_redo(),
        "undo": app.undo_label().map(|l| tr(l).to_string()),
        "redo": app.redo_label().map(|l| tr(l).to_string()),
    });
    if detail {
        v["track_list"] = json!(
            p.tracks
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let mut j = track_json(i, t);
                    j["event_list"] = json!(
                        t.clips
                            .iter()
                            .map(|c| event_json(p, i, c))
                            .collect::<Vec<_>>()
                    );
                    j
                })
                .collect::<Vec<_>>()
        );
        v["media_list"] = json!(p.media.iter().map(media_json).collect::<Vec<_>>());
        v["marker_list"] = json!(
            p.markers
                .iter()
                .map(|m| json!({"id": m.id.0, "at": secs(m.time), "name": m.name}))
                .collect::<Vec<_>>()
        );
        v["region_list"] = json!(p.regions.iter().map(|r| json!({"id": r.id.0, "start": secs(r.range.start), "end": secs(r.range.end()), "name": r.name})).collect::<Vec<_>>());
    }
    done(v)
}

fn new_project(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let s = app.project().settings;
    let width = a.opt_u64("width")?.map_or(s.width, |w| w as u32);
    let height = a.opt_u64("height")?.map_or(s.height, |h| h as u32);
    if !(16..=16384).contains(&width) || !(16..=16384).contains(&height) {
        return Err(tr("The size must be from 16 to 16384 pixels each way.").into());
    }
    let rate = match a.opt_f64("frame_rate")? {
        Some(r) => frame_rate(r)?,
        None => s.frame_rate,
    };
    app.new_project_sized(width, height, rate);
    if let Some(sr) = a.opt_u64("sample_rate")? {
        app.agent_modify(|p| p.settings.sample_rate = SampleRate(sr as u32));
        app.agent_saved_now();
    }
    info(app, &Args::empty())
}

/// What a file is, as BananaV opens it.
fn project_kind(path: &Path) -> Result<FileKind, String> {
    match crate::media_files::kind_of(path) {
        Some(k @ (FileKind::Project | FileKind::VegProject | FileKind::Timeline)) => Ok(k),
        _ => Err(trf!(
            "{} is not a project BananaV opens (.bvproj, .veg, .otio, .xml, .fcpxml).",
            path.display()
        )),
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn open_project(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let path = PathBuf::from(a.str("path")?);
    if !path.exists() {
        return Err(trf!("There is no file {}.", path.display()));
    }
    let kind = project_kind(&path)?;
    let mark = std::time::Instant::now();
    match kind {
        FileKind::Project => app.open_path(path.clone()),
        FileKind::VegProject => crate::veg_ui::open(app, &path),
        _ => match extension(&path).as_str() {
            "xml" => crate::otio_ui::import_fcp_file(app, &path, false),
            "fcpxml" => crate::otio_ui::import_fcp_file(app, &path, true),
            "otio" => crate::otio_ui::open(app, path.clone()),
            _ => {
                return Err(tr("OpenTimelineIO bundles (.otioz, .otiod) open in the window: unpack it, or use its .otio.").into());
            }
        },
    }
    if let Some(e) = app.status_error_since(mark) {
        return Err(e);
    }
    info(app, &Args::empty())
}

fn save_project(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let path = match a.opt_str("path")? {
        Some(p) => PathBuf::from(p),
        None => app
            .project_path
            .clone()
            .ok_or_else(|| tr("The project has no file yet: give \"path\".").to_string())?,
    };
    let overwrite = a.bool_or("overwrite", false)?;
    let ours = app.project_path.as_deref() == Some(path.as_path());
    if path.exists() && !ours && !overwrite {
        return Err(trf!(
            "{} exists: give \"overwrite\": true to write over it.",
            path.display()
        ));
    }
    let ext = extension(&path);
    let project = app.project();
    let name = path.file_stem().map_or_else(
        || tr("Untitled").to_string(),
        |s| s.to_string_lossy().into_owned(),
    );
    let written = |r: std::io::Result<()>| r.map_err(|e| trf!("Could not save: {}", e));
    let format = match ext.as_str() {
        "bvproj" => {
            app.agent_save(&path)?;
            "bvproj"
        }
        "veg" => {
            written(crate::veg_ui::write_to(app, &path))?;
            "veg"
        }
        "otio" => {
            written(crate::safe_file::save(
                &path,
                bv_timeline::otio::export(project, &name).as_bytes(),
            ))?;
            "otio"
        }
        "xml" => {
            let (text, _) = bv_timeline::xmeml::export(project, &name);
            written(crate::safe_file::save(&path, text.as_bytes()))?;
            "xml"
        }
        "fcpxml" => {
            let (text, _) = bv_timeline::interchange::export_fcpxml(project, &name);
            written(crate::safe_file::save(&path, text.as_bytes()))?;
            "fcpxml"
        }
        "edl" => {
            let (text, _) = bv_timeline::interchange::export_edl(project, &name);
            written(crate::safe_file::save(&path, text.as_bytes()))?;
            "edl"
        }
        _ => {
            return Err(tr("Save as .bvproj (BananaV's project), or export as .veg, .otio, .xml, .fcpxml or .edl.").into());
        }
    };
    done(json!({
        "path": path.to_string_lossy(),
        "format": format,
        // Only BananaV's own file becomes the project's.
        "project_file": format == "bvproj",
    }))
}

fn set_project_settings(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let width = a.opt_u64("width")?;
    let height = a.opt_u64("height")?;
    if width.is_some_and(|w| !(16..=16384).contains(&w))
        || height.is_some_and(|h| !(16..=16384).contains(&h))
    {
        return Err(tr("The size must be from 16 to 16384 pixels each way.").into());
    }
    let rate = a.opt_f64("frame_rate")?.map(frame_rate).transpose()?;
    let sample_rate = a.opt_u64("sample_rate")?;
    if sample_rate.is_some_and(|r| !(8000..=384_000).contains(&r)) {
        return Err(tr("The sample rate must be from 8000 to 384000 Hz.").into());
    }
    app.agent_modify(|p| {
        let s = &mut p.settings;
        if let Some(w) = width {
            s.width = w as u32;
        }
        if let Some(h) = height {
            s.height = h as u32;
        }
        if let Some(r) = rate {
            s.frame_rate = r;
        }
        if let Some(r) = sample_rate {
            s.sample_rate = SampleRate(r as u32);
        }
    });
    info(app, &Args::empty())
}

fn list_media(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    let p = app.project();
    done(json!({"media": p.media.iter().map(media_json).collect::<Vec<_>>()}))
}

fn import_media(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let Some(list) = a.get("paths").and_then(Value::as_array) else {
        return Err(tr("\"paths\" is a list of files.").into());
    };
    let mut paths = Vec::new();
    for p in list {
        let Some(s) = p.as_str() else {
            return Err(tr("\"paths\" is a list of files.").into());
        };
        let path = PathBuf::from(s);
        if !path.exists() {
            return Err(trf!("There is no file {}.", path.display()));
        }
        paths.push(path);
    }
    let mark = std::time::Instant::now();
    let ids = app.as_one_undo(|app| app.import_paths(&paths));
    if ids.len() < paths.len()
        && let Some(e) = app.status_error_since(mark)
    {
        return Err(e);
    }
    let p = app.project();
    done(json!({
        "media": ids.iter().filter_map(|&id| p.media(id)).map(media_json).collect::<Vec<_>>(),
    }))
}

fn remove_media(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let id = media_of(app.project(), a.u64("media")?)?.id;
    app.agent_edit(|p| p.remove_media(id))?;
    app.prune_selection();
    done(json!({"removed": true}))
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "project_info",
            title: "Project info",
            description: "The project: its file, whether it has changes not saved, its settings (size, frame rate, sample rate), length, how many tracks, events, media and markers, the cursor, the loop region, the selection, what undo and redo would do. With `detail`, every track with its events, every media, marker and region.",
            properties: || json!({"detail": arg("boolean", "Also list tracks, events, media, markers and regions.")}),
            required: &[],
            kind: Kind::Read,
            run: info,
        },
        Tool {
            name: "new_project",
            title: "New project",
            description: "Starts a new, empty project (no tracks: add_track or add_event make them) of a size and frame rate (the current project's if not given).",
            properties: || {
                json!({
                    "width": arg("integer", "Width in pixels."),
                    "height": arg("integer", "Height in pixels."),
                    "frame_rate": arg("number", "Frames a second (29.97, 23.976 and 59.94 are NTSC's exact rates)."),
                    "sample_rate": arg("integer", "Audio samples a second (48000…)."),
                })
            },
            required: &[],
            kind: Kind::Replace,
            run: new_project,
        },
        Tool {
            name: "open_project",
            title: "Open a project",
            description: "Opens a project: BananaV's (.bvproj), or another editor's (.veg, OpenTimelineIO .otio, Final Cut Pro .xml or .fcpxml) as a new project not saved yet.",
            properties: || json!({"path": arg("string", "The file (a full path).")}),
            required: &["path"],
            kind: Kind::Replace,
            run: open_project,
        },
        Tool {
            name: "save_project",
            title: "Save the project",
            description: "Saves the project as BananaV's .bvproj (its own file from then on), or exports it as .veg, .otio, .xml (Final Cut Pro 7), .fcpxml or .edl. Without `path`, to its own file.",
            properties: || {
                json!({
                    "path": arg("string", "The file (a full path); its extension picks the format."),
                    "overwrite": arg("boolean", "Write over a file that is there (default false)."),
                })
            },
            required: &[],
            kind: Kind::Write,
            run: save_project,
        },
        Tool {
            name: "set_project_settings",
            title: "Change the project's settings",
            description: "Changes the project's width, height, frame rate or sample rate.",
            properties: || {
                json!({
                    "width": arg("integer", "Width in pixels."),
                    "height": arg("integer", "Height in pixels."),
                    "frame_rate": arg("number", "Frames a second."),
                    "sample_rate": arg("integer", "Audio samples a second."),
                })
            },
            required: &[],
            kind: Kind::Edit,
            run: set_project_settings,
        },
        Tool {
            name: "list_media",
            title: "List media",
            description: "The project's media: id, name, file, length, picture size and frame rate, sound, effects.",
            properties: || json!({}),
            required: &[],
            kind: Kind::Read,
            run: list_media,
        },
        Tool {
            name: "import_media",
            title: "Import media",
            description: "Adds video, audio or picture files to the project's media (not to the timeline: add_event places them). A file already there gives its id.",
            properties: || json!({"paths": {"type": "array", "items": {"type": "string"}, "description": "The files (full paths)."}}),
            required: &["paths"],
            kind: Kind::Edit,
            run: import_media,
        },
        Tool {
            name: "remove_media",
            title: "Remove a media",
            description: "Takes a media out of the project, with its events.",
            properties: || json!({"media": arg("integer", "The media's id.")}),
            required: &["media"],
            kind: Kind::Remove,
            run: remove_media,
        },
    ]
}
