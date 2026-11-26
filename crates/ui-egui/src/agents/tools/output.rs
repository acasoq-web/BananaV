//! What comes out of the project: a frame as a picture (for the agent to
//! see, or written to a file), the render templates, renders.

use std::path::{Path, PathBuf};

use bv_core::{Tick, TimeRange};
use bv_engine::{Availability, RenderSettings};
use serde_json::{Value, json};

use super::{Args, Kind, Tool, arg, fps, secs};
use crate::agents::{Outcome, Output};
use crate::app::BananaApp;
use crate::i18n::tr;
use crate::render::templates::Template;

fn done(v: Value) -> Outcome {
    Ok(Output::value(v))
}

/// `rgba` (`w` × `h`) made `width` wide, each pixel the average of those
/// it covers (never wider than it is).
fn shrink(w: u32, h: u32, rgba: &[u8], width: u32) -> (u32, u32, Vec<u8>) {
    if width >= w || w == 0 || h == 0 {
        return (w, h, rgba.to_vec());
    }
    let nw = width.max(1);
    let nh = ((u64::from(h) * u64::from(nw)).div_ceil(u64::from(w)) as u32).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h).div_ceil(nh).min(h));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w).div_ceil(nw).min(w));
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * w + sx) * 4) as usize;
                    for k in 0..4 {
                        sum[k] += u32::from(rgba[i + k]);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1);
            let o = ((y * nw + x) * 4) as usize;
            for k in 0..4 {
                out[o + k] = ((sum[k] + n / 2) / n) as u8;
            }
        }
    }
    (nw, nh, out)
}

/// The frame at `at`, drawn as a render draws it, over black.
fn frame(app: &mut BananaApp, at: Tick, width: Option<u32>) -> Result<(u32, u32, Vec<u8>), String> {
    app.agent_sync_engine();
    let (w, h, rgba) = app
        .engine
        .snapshot(at, [0.0, 0.0, 0.0, 1.0])
        .ok_or_else(|| tr("The frame could not be drawn.").to_string())?;
    Ok(match width {
        Some(nw) => shrink(w, h, &rgba, nw),
        None => (w, h, rgba),
    })
}

fn preview_frame(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let at = a.opt_time("at")?.unwrap_or(app.cursor);
    let width = a
        .opt_u64("width")?
        .map_or(640, |w| w.clamp(16, 7680) as u32);
    let (w, h, rgba) = frame(app, at, Some(width))?;
    let png = bv_png::encode_rgba8(w, h, &rgba, 6);
    Ok(Output {
        value: json!({"at": secs(at), "width": w, "height": h}),
        image: Some(png),
    })
}

/// A file about to be written: its folder there, and itself not, unless
/// `overwrite`.
fn writable(path: &Path, overwrite: bool) -> Result<(), String> {
    if path.is_dir() {
        return Err(trf!("{} is a folder.", path.display()));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && !dir.is_dir()
    {
        return Err(trf!("There is no folder {}.", dir.display()));
    }
    if path.exists() && !overwrite {
        return Err(trf!(
            "{} exists: give \"overwrite\": true to write over it.",
            path.display()
        ));
    }
    Ok(())
}

fn export_frame(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let path = PathBuf::from(a.str("path")?);
    writable(&path, a.bool_or("overwrite", false)?)?;
    let at = a.opt_time("at")?.unwrap_or(app.cursor);
    let width = a.opt_u64("width")?.map(|w| w.clamp(16, 7680) as u32);
    let (w, h, rgba) = frame(app, at, width)?;
    let png = bv_png::encode_rgba8(w, h, &rgba, 6);
    crate::safe_file::save(&path, &png).map_err(|e| trf!("Could not save: {}", e))?;
    done(json!({"path": path.to_string_lossy(), "at": secs(at), "width": w, "height": h}))
}

fn template_json(t: &Template, app: &BananaApp) -> Value {
    let s = &app.project().settings;
    json!({
        "id": t.id,
        "name": t.name(),
        "format": t.format.name(),
        "extension": t.format.extension(),
        "ready": t.availability() == Availability::Ready,
        "custom": t.custom_name.is_some(),
        "video": t.video_for(s).map(|v| json!({"width": v.width, "height": v.height, "frame_rate": fps(v.frame_rate)})),
        "audio": t.audio_for(s).map(|a| json!({"sample_rate": a.sample_rate, "channels": a.channels})),
    })
}

fn list_render_templates(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let search = a.opt_str("search")?.map(str::to_lowercase);
    let ready_only = a.bool_or("ready_only", true)?;
    let list: Vec<Value> = crate::render::all_templates(app)
        .iter()
        .filter(|t| !ready_only || t.availability() == Availability::Ready)
        .filter(|t| {
            search.as_ref().is_none_or(|q| {
                t.id.to_lowercase().contains(q)
                    || t.name().to_lowercase().contains(q)
                    || t.format.name().to_lowercase().contains(q)
            })
        })
        .map(|t| template_json(t, app))
        .collect();
    done(json!({"templates": list}))
}

/// The template `wanted` names (its id or its name), else the best for a
/// file of `ext`: ready, its picture the project's size and rate.
fn pick_template(app: &BananaApp, wanted: Option<&str>, ext: &str) -> Result<Template, String> {
    let all = crate::render::all_templates(app);
    if let Some(w) = wanted {
        let t = all
            .iter()
            .find(|t| t.id == w)
            .or_else(|| all.iter().find(|t| t.name().eq_ignore_ascii_case(w)))
            .ok_or_else(|| {
                trf!(
                    "There is no render template {} (list_render_templates gives them).",
                    w
                )
            })?;
        if t.availability() != Availability::Ready {
            return Err(trf!(
                "The template {} cannot render on this computer (no encoder for it).",
                w
            ));
        }
        return Ok(t.clone());
    }
    let s = &app.project().settings;
    let (w, h) = s.output_size();
    let fits: Vec<&Template> = all
        .iter()
        .filter(|t| {
            t.availability() == Availability::Ready
                && t.format.extension().eq_ignore_ascii_case(ext)
        })
        .collect();
    fits.iter()
        .find(|t| t.video_for(s).is_some_and(|v| (v.width, v.height) == (w, h) && v.frame_rate == s.frame_rate))
        .or_else(|| fits.first())
        .map(|t| (*t).clone())
        .ok_or_else(|| trf!("No render template makes .{} files here: give \"template\" (list_render_templates gives them).", ext))
}

fn render(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    if app.agent_render_running() {
        return Err(tr(
            "A render is going on: wait for it (render_status) or cancel it (cancel_render).",
        )
        .into());
    }
    let mut path = PathBuf::from(a.str("path")?);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let t = pick_template(app, a.opt_str("template")?, &ext)?;
    if !t.format.extension().eq_ignore_ascii_case(&ext) {
        path.set_extension(t.format.extension());
    }
    writable(&path, a.bool_or("overwrite", false)?)?;
    let p = app.project();
    let from = a.opt_time("from")?.unwrap_or(Tick::ZERO);
    let to = a.opt_time("to")?.unwrap_or_else(|| p.duration());
    if to <= from {
        return Err(
            tr("There is nothing to render there: the end must come after the start.").into(),
        );
    }
    let s = RenderSettings {
        format: t.format,
        path: path.clone(),
        range: TimeRange::from_bounds(from, to),
        video: t.video_for(&p.settings),
        audio: t.audio_for(&p.settings),
        options: bv_engine::RenderOptions {
            no_recompress: app.settings.prefs.no_recompress,
            ..app.settings.render.options
        },
        project_file: app.project_path.clone(),
    };
    let job = app.agent_start_render(s);
    done(json!({
        "job": job,
        "path": path.to_string_lossy(),
        "template": t.id,
        "from": secs(from),
        "to": secs(to),
    }))
}

fn render_status(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    let Some((job, path, p)) = app.agent_render_status() else {
        return Err(tr("No render was started.").into());
    };
    let state = match &p {
        None => "starting",
        Some(p) if !p.finished => "running",
        Some(p) if p.cancelled => "cancelled",
        Some(p) if p.error.is_some() => "failed",
        Some(_) => "done",
    };
    done(json!({
        "job": job,
        "path": path.to_string_lossy(),
        "state": state,
        "progress": p.as_ref().map_or(0.0, |p| f64::from(p.fraction())),
        "elapsed": p.as_ref().map(|p| (p.elapsed_secs * 10.0).round() / 10.0),
        "error": p.as_ref().and_then(|p| p.error.clone()),
        "files": p.as_ref().map(|p| p.files.iter().map(|f| f.to_string_lossy().into_owned()).collect::<Vec<_>>()),
    }))
}

fn cancel_render(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    if !app.agent_render_running() {
        return Err(tr("No render is going on.").into());
    }
    app.engine.cancel_render();
    done(json!({"cancelled": true}))
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "preview_frame",
            title: "See a frame",
            description: "The project's frame at a time (the cursor if not given) as a picture, drawn as a render draws it (every effect, full quality), made `width` pixels wide (640 if not given).",
            properties: || {
                json!({
                    "at": arg("number", "The time (seconds)."),
                    "width": arg("integer", "The picture's width in pixels (640 if not given)."),
                })
            },
            required: &[],
            kind: Kind::Read,
            run: preview_frame,
        },
        Tool {
            name: "export_frame",
            title: "Save a frame as a picture",
            description: "Writes the project's frame at a time as a PNG file (the project's output size, or `width` pixels wide).",
            properties: || {
                json!({
                    "path": arg("string", "The PNG file (a full path)."),
                    "at": arg("number", "The time (seconds)."),
                    "width": arg("integer", "The picture's width in pixels."),
                    "overwrite": arg("boolean", "Write over a file that is there."),
                })
            },
            required: &["path"],
            kind: Kind::Write,
            run: export_frame,
        },
        Tool {
            name: "list_render_templates",
            title: "List render templates",
            description: "The render templates (BananaV's and the user's): id, name, file format and extension, picture size and frame rate, sound, whether this computer can render them.",
            properties: || {
                json!({
                    "search": arg("string", "Only those whose id, name or format has this text."),
                    "ready_only": arg("boolean", "Only those this computer can render (default true)."),
                })
            },
            required: &[],
            kind: Kind::Read,
            run: list_render_templates,
        },
        Tool {
            name: "render",
            title: "Render",
            description: "Starts rendering the project (or from `from` to `to`) to a file with a template (its id or name; without one, the best for the file's extension at the project's size and rate). Returns at once: render_status follows it.",
            properties: || {
                json!({
                    "path": arg("string", "The file (a full path)."),
                    "template": arg("string", "The template's id or name."),
                    "from": arg("number", "Where to start (seconds)."),
                    "to": arg("number", "Where to end (seconds)."),
                    "overwrite": arg("boolean", "Write over a file that is there."),
                })
            },
            required: &["path"],
            kind: Kind::Write,
            run: render,
        },
        Tool {
            name: "render_status",
            title: "Render status",
            description: "How the last render goes: starting, running, done, failed or cancelled, its progress from 0 to 1, the time it took, its error, its files.",
            properties: || json!({}),
            required: &[],
            kind: Kind::Read,
            run: render_status,
        },
        Tool {
            name: "cancel_render",
            title: "Cancel the render",
            description: "Cancels the render going on (its file is deleted).",
            properties: || json!({}),
            required: &[],
            kind: Kind::Edit,
            run: cancel_render,
        },
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_picture_shrinks_to_the_averages() {
        // 4 × 2, two halves: black and white.
        let mut rgba = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let v = if x < 2 { 0 } else { 255 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let (w, h, out) = super::shrink(4, 2, &rgba, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out, [0, 0, 0, 255, 255, 255, 255, 255]);
        let (w, _, same) = super::shrink(4, 2, &rgba, 8);
        assert_eq!(w, 4);
        assert_eq!(same, rgba);
    }
}
