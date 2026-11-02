//! Project Media's shortcut menu on its media, as VEGAS 23's: its items in
//! its order and groups, greyed as VEGAS greys them for a video file, a
//! sound file and generated media; with several picked, the commands that
//! can take them all act on all of them. The commands that live here:
//! Explore Containing Folder, Swap Video Files, Remove from Project and
//! Delete File(s), Rename, Replace…, Refresh, Rotate 90°, Lay Out Tracks
//! Using Media Timecode or Date/Time Stamp, and Detect Scenes — the media
//! looked through for its cuts, then added to the timeline an event a
//! scene (also the Trimmer's from its cursor) or put in Project Media a
//! subclip a scene.

use std::path::{Path, PathBuf};

use bv_core::{Tick, TimeRange};
use bv_engine::Engine;
use bv_timeline::{ClipId, MediaId, MediaInfo, MediaRotation, MediaSource, TrackKind};

use crate::app::BananaApp;
use crate::commands::Cmd;
use crate::dock::Tab;
use crate::egui::{self, Ui};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::keymap::Action;
use crate::menus::icon_item;
use crate::multicam::SyncBy;
use crate::tabs::media::BinChoice;

/// The media a command takes: the picked ones when `id` is one of them
/// (it first), else `id` alone.
fn targets(app: &BananaApp, id: MediaId) -> Vec<MediaId> {
    let picked = &app.media_window.picked;
    if picked.len() > 1 && picked.contains(&id) {
        std::iter::once(id)
            .chain(picked.iter().copied().filter(|&p| p != id))
            .collect()
    } else {
        vec![id]
    }
}

/// The file of media `id` (a subclip's parent's), if it is one.
fn file_of(app: &BananaApp, id: MediaId) -> Option<PathBuf> {
    let p = app.project();
    let id = p.subclip(id).map_or(id, |s| s.parent);
    match &p.media(id)?.source {
        MediaSource::File { path } => Some(PathBuf::from(path)),
        MediaSource::Generated(_) => None,
    }
}

/// The menu of media `m`, right-clicked in Project Media; `at` is where
/// (Rename's box goes there).
pub(crate) fn media_menu(ui: &mut Ui, app: &mut BananaApp, m: &MediaInfo, at: egui::Pos2) {
    let id = m.id;
    let sub = app.project().subclip(id).cloned();
    let subclip = sub.is_some();
    let path = file_of(app, id);
    // What it is: VEGAS greys its items by these.
    let file = path.is_some();
    let picture = m.video.is_some();
    let sound = m.audio.is_some();
    let own = !subclip;
    let nested = path
        .as_ref()
        .is_some_and(|p| bv_engine::is_project(&p.to_string_lossy()));
    let source_project = own && path.as_deref().is_some_and(crate::archive::keeps_reference);
    let proxy = own && crate::proxies::can_have_proxy(m);
    let targets = targets(app, id);
    let in_bin = matches!(
        app.media_window.bin,
        BinChoice::Custom(_) | BinChoice::Tag(_)
    );
    let nested_keys = app.keymap.text_for(Action::Cmd(Cmd::OpenNestedTimeline));

    if icon_item(ui, tr("Open in Trimmer"), None, "", true) {
        app.open_in_trimmer(id);
        app.show_tab(Tab::Trimmer, false);
        ui.close();
    }
    if icon_item(ui, tr("Open Parent Media in Trimmer"), None, "", subclip)
        && let Some(s) = &sub
    {
        app.trimmer.open_part(s.parent, s.range);
        app.show_tab(Tab::Trimmer, false);
        ui.close();
    }
    if icon_item(ui, tr("Add as CD Track"), None, "", own && sound) {
        app.add_as_cd_track(&targets);
        ui.close();
    }
    if icon_item(
        ui,
        tr("Add at Project Frame Rate"),
        None,
        "",
        own && file && picture && !m.still,
    ) {
        app.add_at_project_frame_rate(id);
        ui.close();
    }
    if icon_item(
        ui,
        tr("Detect Scenes and Add to Timeline"),
        Some(Icon::DetectScenes),
        "",
        own && picture && !m.still,
    ) {
        let at = app.cursor;
        app.detect_media_scenes(id, None, AfterScenes::AddAt(at), (true, true));
        ui.close();
    }
    if icon_item(ui, tr("Tempo Detection"), None, "", own && sound) {
        app.tempo_detection(id);
        ui.close();
    }
    if icon_item(ui, tr("Select Timeline Events"), None, "", true) {
        // Ctrl or Shift held: added to the selection.
        let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        app.select_timeline_events(id, add);
        ui.close();
    }
    if icon_item(
        ui,
        tr("Match Project Video Settings"),
        None,
        "",
        own && file && picture,
    ) {
        app.match_project_video_settings(id);
        ui.close();
    }
    if icon_item(ui, tr("Edit Source Project"), None, "", source_project)
        && let Some(p) = &path
    {
        app.edit_source_project(p);
        ui.close();
    }
    if icon_item(
        ui,
        tr("Open Nested Timeline"),
        Some(Icon::NestedTimeline),
        &nested_keys,
        own && nested,
    ) && let Some(p) = &path
    {
        app.open_nested_timeline_at(p.clone());
        ui.close();
    }
    if icon_item(ui, tr("Explore Containing Folder"), None, "", file)
        && let Some(p) = &path
    {
        if let Err(e) = crate::app::more_actions::reveal_in_folder(p) {
            app.report_error(e.to_string());
        }
        ui.close();
    }
    // A proxy made: Delete Video Proxy in its place.
    if proxy && app.project().media(id).is_some_and(Engine::has_proxy) {
        if icon_item(ui, tr("Delete Video Proxy"), None, "", true) {
            app.delete_proxy(id);
            ui.close();
        }
    } else if icon_item(ui, tr("Create Video Proxy"), None, "", proxy) {
        for t in &targets {
            if app
                .project()
                .media(*t)
                .is_some_and(crate::proxies::can_have_proxy)
            {
                app.create_proxy(*t);
            }
        }
        ui.close();
    }
    if icon_item(ui, tr("Swap Video Files"), None, "", true) {
        app.swap_video_files_dialog();
        ui.close();
    }
    ui.separator();
    if icon_item(ui, tr("Remove from Current Bin"), None, "", in_bin) {
        match app.media_window.bin.clone() {
            BinChoice::Custom(b) => app.modify(|p| {
                for t in &targets {
                    let _ = p.remove_from_bin(b, *t);
                }
            }),
            // A tag's bin: the tag taken off.
            BinChoice::Tag(tag) => app.modify(|p| {
                for t in &targets {
                    p.remove_tag(*t, &tag);
                }
            }),
            _ => {}
        }
        ui.close();
    }
    if icon_item(ui, tr("Remove from Project"), None, tr("Delete"), true) {
        for t in &targets {
            app.request_remove_media(*t);
        }
        ui.close();
    }
    if icon_item(
        ui,
        tr("Remove from Project and Delete File(s)"),
        None,
        "",
        own && file,
    ) {
        app.ask_remove_and_delete(&targets);
        ui.close();
    }
    ui.separator();
    if icon_item(
        ui,
        tr("Detect Scenes and Create Subclips"),
        Some(Icon::DetectScenes),
        "",
        own && picture && !m.still,
    ) {
        app.detect_media_scenes(id, None, AfterScenes::Subclips, (true, true));
        ui.close();
    }
    if icon_item(ui, tr("Search Media Bins…"), None, "", true) {
        let within = match app.media_window.bin {
            BinChoice::Custom(b) => bv_timeline::SearchIn::Bin(b),
            _ => bv_timeline::SearchIn::All,
        };
        crate::tabs::media_tags::open_search(app, within);
        ui.close();
    }
    ui.separator();
    if icon_item(ui, tr("Rename"), None, "", true) {
        app.menu_dialogs.more.media_rename = Some((id, crate::i18n::media_name(m), at));
        ui.close();
    }
    if icon_item(ui, tr("Replace…"), None, "", own) {
        app.replace_media_dialog(id);
        ui.close();
    }
    if icon_item(ui, tr("Refresh"), None, "", true) {
        app.refresh_media(&targets);
        ui.close();
    }
    ui.separator();
    if icon_item(ui, tr("Media FX…"), None, "", own && picture) {
        app.clip_windows.media_fx = Some((id, false));
        ui.close();
    }
    if icon_item(ui, tr("Color Grading"), None, "", true) {
        app.selected_media = Some(id);
        crate::tabs::color::open_at(app, crate::tabs::color::Level::Media);
        ui.close();
    }
    for (label, quarters) in [
        (tr("Rotate 90° Counterclockwise"), -1),
        (tr("Rotate 90° Clockwise"), 1),
    ] {
        if icon_item(ui, label, None, "", own && picture) {
            app.rotate_media(&targets, quarters);
            ui.close();
        }
    }
    ui.separator();
    for (label, by) in [
        (tr("Lay Out Tracks Using Media Timecode"), SyncBy::Timecode),
        (
            tr("Lay Out Tracks Using Media Date/Time Stamp"),
            SyncBy::Date,
        ),
    ] {
        if icon_item(ui, label, None, "", targets.len() > 1) {
            app.lay_out_tracks(&targets, by);
            ui.close();
        }
    }
    ui.separator();
    // DV AVI files' 2-3 pulldown and timecode offset: none here.
    let _ = icon_item(ui, tr("File Format Properties…"), None, "", false);
    if icon_item(ui, tr("Properties…"), None, "", true) {
        app.dialogs.media_properties = if targets.len() > 1 {
            crate::media_properties::MediaPropertiesDialog::new_many(app, &targets)
        } else {
            crate::media_properties::MediaPropertiesDialog::new(app, id)
        };
        ui.close();
    }
}

impl BananaApp {
    /// Swap Video Files (and Project Media's Swap Media button): a folder
    /// asked for, then the project's video files swapped for those there.
    pub(crate) fn swap_video_files_dialog(&mut self) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title(tr("Swap Video Files"))
            .pick_folder()
        else {
            return;
        };
        let n = self.swap_video_files(&dir);
        if n == 0 {
            self.notify(tr(
                "No file in that folder has the name of a video file of the project.",
            ));
        } else {
            self.notify(trn!(
                n,
                "{} video file swapped.",
                "{} video files swapped.",
                n
            ));
        }
    }

    /// The project's video files swapped for the files of `dir` that have
    /// their names, whatever their extension (VEGAS's Help: a proxy's
    /// small files for the full ones, or back) — their events kept, one
    /// undo. How many were.
    pub(crate) fn swap_video_files(&mut self, dir: &Path) -> usize {
        let files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|d| {
                d.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_file())
                    .collect()
            })
            .unwrap_or_default();
        let stem = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().to_lowercase());
        let mut swaps: Vec<(MediaId, MediaInfo)> = Vec::new();
        for m in &self.project().media {
            let MediaSource::File { path } = &m.source else {
                continue;
            };
            if m.video.is_none() || m.still {
                continue;
            }
            let own = Path::new(path);
            let Some(other) = files
                .iter()
                .find(|f| stem(f) == stem(own) && f.as_path() != own)
            else {
                continue;
            };
            if let Ok(new) = Engine::probe_file(&other.to_string_lossy()) {
                swaps.push((m.id, new));
            }
        }
        if swaps.is_empty() {
            return 0;
        }
        self.modify(|p| {
            for (id, new) in &swaps {
                if let Some(m) = p.media.iter_mut().find(|m| m.id == *id) {
                    take_file(m, new.clone());
                }
            }
        });
        self.engine.release_media();
        self.engine.refresh_thumbnails();
        swaps.len()
    }

    /// Replace…: a file asked for, which media `id` becomes.
    pub(crate) fn replace_media_dialog(&mut self, id: MediaId) {
        let dialog = rfd::FileDialog::new().set_title(tr("Replace Media File"));
        let Some(path) = crate::media_files::media_filters(dialog).pick_file() else {
            return;
        };
        if let Err(e) = self.replace_media_file(id, &path) {
            self.report_error(trf!("Could not replace the media file: {}", e));
        }
    }

    /// Media `id` becomes the file at `path`: every event of it plays the
    /// new file (one undo); its effects, color space, tape name and
    /// rotation stay.
    pub(crate) fn replace_media_file(&mut self, id: MediaId, path: &Path) -> Result<(), String> {
        let new = Engine::probe_file(&path.to_string_lossy())
            .map_err(|e| crate::media_files::error_text(&e))?;
        self.modify(|p| {
            if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                take_file(m, new);
            }
        });
        self.engine.release_media();
        self.engine.refresh_thumbnails();
        Ok(())
    }

    /// Refresh: the files of `ids` read again (their length, picture,
    /// sound and markers), their thumbnails made anew.
    pub(crate) fn refresh_media(&mut self, ids: &[MediaId]) {
        let mut paths: Vec<String> = ids
            .iter()
            .filter_map(|&id| file_of(self, id))
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        paths.dedup();
        if paths.is_empty() {
            self.engine.refresh_thumbnails();
        } else {
            self.read_media_again(&paths);
        }
    }

    /// Rename: media `id` (or a subclip) named `name` (one undo).
    pub(crate) fn rename_media(&mut self, id: MediaId, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.modify(|p| {
            if let Some(s) = p.subclips.iter_mut().find(|s| s.id == id) {
                s.name = name;
            } else if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                m.name = name;
            }
        });
    }

    /// Rotate 90° Clockwise (`quarters` 1) or Counterclockwise (−1): the
    /// pictures of `ids` turned a quarter more, as their Properties'
    /// Rotation (one undo).
    pub(crate) fn rotate_media(&mut self, ids: &[MediaId], quarters: i8) {
        self.modify(|p| {
            for id in ids {
                if let Some(m) = p.media.iter_mut().find(|m| m.id == *id)
                    && let Some(v) = &mut m.video
                {
                    let q = (v.reading.rotation.quarters() as i8 + quarters).rem_euclid(4) as u8;
                    if let Some(r) = MediaRotation::ALL.into_iter().find(|r| r.quarters() == q) {
                        *v = v.turned_to(r);
                    }
                }
            }
        });
    }

    /// Remove from Project and Delete File(s): asks, then removes the
    /// files of `ids` from the project and moves them to the Recycle Bin.
    pub(crate) fn ask_remove_and_delete(&mut self, ids: &[MediaId]) {
        let files: Vec<(MediaId, PathBuf)> = ids
            .iter()
            .filter(|&&id| self.project().media(id).is_some())
            .filter_map(|&id| Some((id, file_of(self, id)?)))
            .collect();
        let text = match files.as_slice() {
            [] => return,
            [(_, path)] => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                trf!(
                    "Remove \"{}\" from the project and move its file to the Recycle Bin?",
                    name
                )
            }
            _ => trn!(
                files.len(),
                "Remove these {} media file from the project and move it to the Recycle Bin?",
                "Remove these {} media files from the project and move them to the Recycle Bin?",
                files.len()
            ),
        };
        self.ask_user(crate::ask::Ask {
            title: tr("Remove from Project and Delete File(s)").to_string(),
            text,
            what: crate::ask::Asked::RemoveAndRecycle(files),
            check: None,
        });
    }

    /// The answer's Yes: the media out of the project (their events with
    /// them), their files let go and moved to the Recycle Bin.
    pub(crate) fn remove_and_recycle(&mut self, files: Vec<(MediaId, PathBuf)>) {
        for (id, _) in &files {
            self.remove_media(*id);
        }
        self.engine.release_media();
        let paths: Vec<PathBuf> = files.into_iter().map(|(_, p)| p).collect();
        self.recycle_files(&paths);
    }

    /// Lay Out Tracks Using Media Timecode / Date/Time Stamp: each of
    /// `ids` on tracks of its own, where its timecode or recording time
    /// puts it, the earliest at the cursor (one undo). Dates are what the
    /// files say when all of them say one, else when each was written less
    /// its length.
    pub(crate) fn lay_out_tracks(&mut self, ids: &[MediaId], by: SyncBy) {
        let stamps: Vec<(MediaId, bv_engine::MediaStamps)> = ids
            .iter()
            .filter_map(|&id| Some((id, bv_engine::media_stamps(self.project().media(id)?))))
            .collect();
        let said_by_files = stamps.iter().all(|(_, s)| s.recorded.is_some());
        let values: Vec<(MediaId, f64)> = stamps
            .iter()
            .filter_map(|(id, s)| {
                let v = match by {
                    SyncBy::Timecode => s.timecode,
                    SyncBy::Date if said_by_files => s.recorded,
                    SyncBy::Date => s.written,
                };
                Some((*id, v?))
            })
            .collect();
        let missing = ids.len() - values.len();
        let Some(first) = values.iter().map(|(_, v)| *v).reduce(f64::min) else {
            self.notify(match by {
                SyncBy::Timecode => tr("None of these media has a timecode."),
                SyncBy::Date => tr("None of these media has a date or time stamp."),
            });
            return;
        };
        let at = self.cursor;
        let placed = self.as_one_undo(|app| {
            let mut placed = 0usize;
            for (id, v) in &values {
                let Some(info) = app.project().media(*id).cloned() else {
                    continue;
                };
                let start = at + Tick::from_seconds_f64(v - first);
                let video = info
                    .video
                    .is_some()
                    .then(|| app.modify(|p| p.add_track(TrackKind::Video)));
                let audio = info
                    .audio
                    .is_some()
                    .then(|| app.modify(|p| p.add_track(TrackKind::Audio)));
                if app
                    .edit(|p| p.insert_media(*id, start, video, audio))
                    .is_some()
                {
                    placed += 1;
                }
            }
            placed
        });
        let mut said = trn!(
            placed,
            "{} media laid out on tracks of its own.",
            "{} media laid out on tracks of their own.",
            placed
        );
        if missing > 0 {
            said.push(' ');
            said.push_str(&match by {
                SyncBy::Timecode => trn!(
                    missing,
                    "{} has no timecode.",
                    "{} have no timecode.",
                    missing
                ),
                SyncBy::Date => trn!(
                    missing,
                    "{} has no date or time stamp.",
                    "{} have no date or time stamp.",
                    missing
                ),
            });
        }
        self.notify(said);
    }
}

/// `m` becomes the file `new` describes: its source, name, length,
/// streams, still-ness and markers; its effects, color space and tape name
/// stay, its picture turned as it was.
fn take_file(m: &mut MediaInfo, new: MediaInfo) {
    let rotation = m.video.map(|v| v.reading.rotation);
    m.source = new.source;
    m.name = new.name;
    m.duration = new.duration;
    m.video = match (new.video, rotation) {
        (Some(v), Some(r)) => Some(v.turned_to(r)),
        (v, _) => v,
    };
    m.audio = new.audio;
    m.extra_audio = new.extra_audio;
    m.still = new.still;
    m.sequence = new.sequence;
    m.markers = new.markers;
    m.layer = new.layer;
}

/// What follows a media's scenes found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AfterScenes {
    /// The part on the timeline from `at`, an event a scene.
    AddAt(Tick),
    /// A subclip a scene in Project Media.
    Subclips,
}

/// Detect Scenes looking through a media: its progress window.
#[derive(Debug)]
pub(crate) struct MediaScenes {
    media: MediaId,
    part: TimeRange,
    after: AfterScenes,
    /// The streams the events take (video, audio: the Trimmer's Select
    /// Video Only…).
    streams: (bool, bool),
}

impl BananaApp {
    /// Detect Scenes on media `id`: `part` of it (all of it: `None`)
    /// looked through in the background, then `after`.
    pub(crate) fn detect_media_scenes(
        &mut self,
        id: MediaId,
        part: Option<TimeRange>,
        after: AfterScenes,
        streams: (bool, bool),
    ) {
        let Some(info) = self.project().media(id).cloned() else {
            return;
        };
        let length = self.settings.prefs.new_events().length(&info);
        let part = part.unwrap_or(TimeRange::new(Tick::ZERO, length));
        if self.engine.start_media_scene_detection(info, part, 0.5) {
            self.menu_dialogs.more.media_scenes = Some(MediaScenes {
                media: id,
                part,
                after,
                streams,
            });
        } else {
            self.notify(tr("This media has no picture to look through."));
        }
    }

    /// The scenes of `job` found (`cuts`: where new ones start, in the
    /// media's time): added or made subclips.
    fn scenes_found(&mut self, job: &MediaScenes, cuts: &[Tick]) {
        let part = job.part;
        let mut bounds = vec![part.start];
        bounds.extend(
            cuts.iter()
                .copied()
                .filter(|&t| t > part.start && t < part.end()),
        );
        bounds.push(part.end());
        let scenes: Vec<TimeRange> = bounds
            .windows(2)
            .map(|w| TimeRange::from_bounds(w[0], w[1]))
            .filter(|r| r.duration > Tick::ZERO)
            .collect();
        let n = scenes.len();
        match job.after {
            AfterScenes::Subclips => {
                let parent = job.media;
                self.modify(|p| {
                    for r in &scenes {
                        let name = p.next_subclip_name(parent);
                        p.add_subclip(parent, *r, false, &name);
                    }
                });
                self.notify(trn!(
                    n,
                    "{} subclip added to Project Media.",
                    "{} subclips added to Project Media.",
                    n
                ));
            }
            AfterScenes::AddAt(at) => {
                let (video, audio) = job.streams;
                let cuts: Vec<Tick> = bounds[1..bounds.len() - 1]
                    .iter()
                    .map(|&t| at + (t - part.start))
                    .collect();
                self.as_one_undo(|app| {
                    let track = app.selected_track;
                    app.place_media_streams(job.media, at, Some(part), track, video, audio);
                    let placed: Vec<ClipId> = app.selection.iter().copied().collect();
                    // From the end back: each event stays the part left of
                    // the cuts still to make.
                    app.edit(|p| -> Result<(), bv_timeline::EditError> {
                        for id in &placed {
                            for &t in cuts.iter().rev() {
                                if p.clip(*id).is_some_and(|c| t > c.start && t < c.end()) {
                                    p.split(*id, t)?;
                                }
                            }
                        }
                        Ok(())
                    });
                });
                self.notify(trn!(
                    n,
                    "{} scene added to the timeline.",
                    "{} scenes added to the timeline.",
                    n
                ));
            }
        }
    }
}

impl MediaScenes {
    /// Its window while the media is looked through; `false` once done or
    /// cancelled.
    pub(crate) fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let Some(progress) = app
            .engine
            .scene_progress()
            .filter(|p| p.media == Some(self.media))
        else {
            return false;
        };
        if progress.finished {
            app.engine.cancel_scene_detection();
            match progress.error {
                Some(e) => app.report_error(trf!("Scene detection: {}", e)),
                None => app.scenes_found(self, &progress.cuts),
            }
            return false;
        }
        let name = app
            .project()
            .media(self.media)
            .map(crate::i18n::media_name)
            .unwrap_or_default();
        let (mut open, mut cancel) = (true, false);
        crate::floating::Window::new(trf!("Detecting Scenes — {}", name))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let f = progress.done as f32 / progress.total.max(1) as f32;
                ui.add(
                    egui::ProgressBar::new(f)
                        .show_percentage()
                        .desired_width(300.0),
                );
                cancel = ui.button(tr("Cancel")).clicked();
            });
        if !open || cancel {
            app.engine.cancel_scene_detection();
            return false;
        }
        true
    }
}

/// Rename's box, at the place of the right-click: Enter (or a click
/// elsewhere) keeps the name, Esc leaves it; `false` once closed.
pub(crate) fn rename_box(
    ctx: &egui::Context,
    app: &mut BananaApp,
    (id, text, at): &mut (MediaId, String, egui::Pos2),
) -> bool {
    let mut done = None;
    egui::Area::new(egui::Id::new("renomear-midia"))
        .order(egui::Order::Foreground)
        .fixed_pos(*at)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let edit = ui.add(egui::TextEdit::singleline(text).desired_width(220.0));
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                } else if edit.lost_focus() {
                    done = Some(true);
                }
            });
        });
    match done {
        Some(keep) => {
            if keep {
                let (id, text) = (*id, text.clone());
                app.rename_media(id, &text);
            }
            false
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::Generator;

    /// A video file not found on disk: its name and picture, ten seconds.
    fn offline_video(app: &mut BananaApp, path: &Path) -> MediaId {
        let settings = app.project().settings;
        let mut info = MediaInfo::generated(Generator::ColorBars, &settings);
        info.source = MediaSource::File {
            path: path.to_string_lossy().into_owned(),
        };
        info.name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        info.duration = Tick::from_seconds(10);
        app.modify(|p| p.add_media(info))
    }

    /// Swap Video Files takes the files of the same names in a folder
    /// (another extension too), one undo; Rename names the media; the
    /// rotations turn its picture a quarter each way.
    #[test]
    fn swap_rename_and_rotate() {
        let dir = std::env::temp_dir().join(format!("bv-media-menu-{}", std::process::id()));
        let (own, other) = (dir.join("own"), dir.join("swap"));
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let mut app = BananaApp::new(egui::Context::default());
        let id = offline_video(&mut app, &own.join("take.mp4"));
        crate::tabs::trimmer::tests_wav(&other.join("take.wav"), 2);
        crate::tabs::trimmer::tests_wav(&other.join("other.wav"), 2);
        let edits = app.history.edits();
        assert_eq!(app.swap_video_files(&other), 1);
        assert_eq!(app.history.edits(), edits + 1, "one undo");
        let m = app.project().media(id).unwrap().clone();
        assert!(matches!(&m.source, MediaSource::File { path } if path.ends_with("take.wav")));
        assert_eq!(m.duration, Tick::from_seconds(2));
        // No video file of the project has a name there: none swapped.
        assert_eq!(app.swap_video_files(&own), 0);
        app.rename_media(id, "  Take one  ");
        assert_eq!(app.project().media(id).unwrap().name, "Take one");
        // A picture turned: its sides swap.
        let settings = app.project().settings;
        let bars =
            app.modify(|p| p.add_media(MediaInfo::generated(Generator::ColorBars, &settings)));
        let size = |app: &BananaApp| {
            let v = app.project().media(bars).unwrap().video.unwrap();
            (v.width, v.height, v.reading.rotation)
        };
        let (w, h, _) = size(&app);
        app.rotate_media(&[bars], 1);
        assert_eq!(size(&app), (h, w, MediaRotation::Clockwise));
        app.rotate_media(&[bars], -1);
        app.rotate_media(&[bars], -1);
        assert_eq!(size(&app), (h, w, MediaRotation::Counterclockwise));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The scenes found become events split where each starts (one undo),
    /// or subclips.
    #[test]
    fn scenes_become_events_or_subclips() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let bars = app.project().media[0].id;
        let job = |after| MediaScenes {
            media: bars,
            part: TimeRange::new(s(1), s(4)),
            after,
            streams: (true, true),
        };
        app.scenes_found(&job(AfterScenes::Subclips), &[s(2), s(4)]);
        let parts: Vec<TimeRange> = app.project().subclips.iter().map(|x| x.range).collect();
        assert_eq!(
            parts,
            [
                TimeRange::new(s(1), s(1)),
                TimeRange::new(s(2), s(2)),
                TimeRange::new(s(4), s(1))
            ]
        );
        let before = app.project().clips().count();
        app.scenes_found(&job(AfterScenes::AddAt(s(40))), &[s(2), s(4)]);
        let mut placed: Vec<(Tick, Tick)> = app
            .project()
            .clips()
            .map(|(_, c)| c)
            .filter(|c| c.start >= s(40))
            .map(|c| (c.start, c.source_in))
            .collect();
        placed.sort();
        assert_eq!(placed, [(s(40), s(1)), (s(41), s(2)), (s(43), s(4))]);
        assert_eq!(app.project().clips().count(), before + 3);
        // One undo takes them all back.
        app.undo();
        assert_eq!(app.project().clips().count(), before);
    }
}
