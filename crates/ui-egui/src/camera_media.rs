//! Media from cameras: File › Import › DVD Camcorder Disc and View › Window
//! › Device Explorer. Both copy media from a camera's disc or memory card
//! into a folder of the computer and add it to Project Media. (VEGAS keeps
//! them among its deprecated features; BananaV has them as usual ones.)

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::app::BananaApp;
use crate::egui::{self, vec2};
use crate::i18n::tr;
use crate::theme;

/// Files being copied, on a thread of their own.
#[derive(Debug)]
pub(crate) struct Copying {
    state: Arc<Mutex<CopyState>>,
}

#[derive(Debug, Default)]
struct CopyState {
    done: usize,
    total: usize,
    /// The copies, or what went wrong, once finished.
    result: Option<Result<Vec<PathBuf>, String>>,
}

impl Copying {
    /// Copies each `(from, to)`; a copy already there (as large) is kept.
    fn start(files: Vec<(PathBuf, PathBuf)>) -> Copying {
        let state = Arc::new(Mutex::new(CopyState {
            total: files.len(),
            ..CopyState::default()
        }));
        let shared = state.clone();
        std::thread::spawn(move || {
            let mut copies = Vec::new();
            let mut failed = None;
            for (from, to) in files {
                let same = |a: &Path, b: &Path| {
                    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
                    size(a).is_some() && size(a) == size(b)
                };
                if !same(&from, &to) {
                    let copied = to
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|_| std::fs::copy(&from, &to).map(|_| ()));
                    if let Err(e) = copied {
                        failed = Some(format!("{}: {e}", from.display()));
                        break;
                    }
                }
                copies.push(to);
                if let Ok(mut s) = shared.lock() {
                    s.done += 1;
                }
            }
            if let Ok(mut s) = shared.lock() {
                s.result = Some(match failed {
                    Some(e) => Err(e),
                    None => Ok(copies),
                });
            }
        });
        Copying { state }
    }

    /// How many files are copied of how many, and the result once done.
    fn poll(&self) -> (usize, usize, Option<Result<Vec<PathBuf>, String>>) {
        match self.state.lock() {
            Ok(mut s) => (s.done, s.total, s.result.take()),
            Err(_) => (0, 0, Some(Err("the copy stopped".into()))),
        }
    }
}

/// Where imported media goes: Documents\BananaV\Imported\`name`.
pub(crate) fn import_folder(name: &str) -> PathBuf {
    let name: String = name
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let name = if name.trim().is_empty() {
        tr("Device").to_string()
    } else {
        name.trim().to_string()
    };
    crate::project_properties::default_recorded_folder()
        .join("BananaV")
        .join(tr("Imported"))
        .join(name)
}

/// The titles of a DVD-Video disc (its VIDEO_TS folder): the VTS_nn_1.VOB
/// files onward, the menus (VTS_nn_0.VOB, VIDEO_TS.VOB) left out.
pub(crate) fn dvd_titles(root: &Path) -> Vec<PathBuf> {
    let folder = if root.ends_with("VIDEO_TS") {
        root.to_path_buf()
    } else {
        root.join("VIDEO_TS")
    };
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_uppercase())
                .unwrap_or_default();
            name.len() == 12
                && name.starts_with("VTS_")
                && name.ends_with(".VOB")
                && !name.ends_with("_0.VOB")
        })
        .collect();
    out.sort();
    out
}

/// A media file on a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeviceClip {
    pub path: PathBuf,
    pub size: u64,
}

/// The media files on a device: its folders a few deep, as cameras lay
/// them out (DCIM, AVCHD, M4ROOT, …).
pub(crate) fn device_clips(root: &Path) -> Vec<DeviceClip> {
    fn walk(dir: &Path, depth: usize, budget: &mut usize, out: &mut Vec<DeviceClip>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if depth < 8 {
                    walk(&path, depth + 1, budget, out);
                }
                continue;
            }
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let media = bv_engine::VIDEO_EXTENSIONS
                .iter()
                .chain(bv_engine::AUDIO_EXTENSIONS)
                .chain(bv_engine::IMAGE_EXTENSIONS)
                .any(|x| x.eq_ignore_ascii_case(&ext));
            if media {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(DeviceClip { path, size });
            }
        }
    }
    let mut out = Vec::new();
    let mut budget = 20_000;
    walk(root, 0, &mut budget, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// File › Import › DVD Camcorder Disc's window.
#[derive(Debug, Default)]
pub(crate) struct DvdImport {
    pub open: bool,
    /// The drives with a DVD's titles, found when the window opens (or on
    /// Refresh).
    discs: Option<Vec<bv_platform::Drive>>,
    source: Option<PathBuf>,
    /// The source's titles, and which source they are of.
    titles: Option<(PathBuf, Vec<PathBuf>)>,
    dest: Option<PathBuf>,
    copying: Option<Copying>,
}

/// View › Window › Device Explorer.
#[derive(Debug, Default)]
pub(crate) struct DeviceExplorer {
    pub open: bool,
    drives: Option<Vec<bv_platform::Drive>>,
    device: Option<PathBuf>,
    clips: Vec<DeviceClip>,
    chosen: HashSet<PathBuf>,
    dest: Option<PathBuf>,
    copying: Option<Copying>,
}

/// A size in MB or GB.
fn size_text(bytes: u64) -> String {
    let mb = bytes as f64 / (1u64 << 20) as f64;
    if mb >= 1024.0 {
        format!("{} GB", crate::fmt::num(mb / 1024.0, 2))
    } else {
        format!("{} MB", crate::fmt::num(mb, 1))
    }
}

/// A folder, as a line with a Browse… button.
fn folder_line(ui: &mut egui::Ui, value: &mut PathBuf, title: &str) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Browse…")).clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_title(title)
                    .set_directory(value.as_path())
                    .pick_folder()
            {
                *value = dir;
            }
            let mut text = value.display().to_string();
            ui.add_enabled(
                false,
                egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()),
            );
        });
    });
}

impl BananaApp {
    /// The files copied: added to Project Media.
    fn finish_copy(&mut self, result: Result<Vec<PathBuf>, String>) {
        match result {
            Ok(copies) => {
                let ids = self.import_paths(&copies);
                self.notify(trn!(
                    ids.len(),
                    "{} file imported into Project Media.",
                    "{} files imported into Project Media.",
                    ids.len()
                ));
            }
            Err(e) => self.report_error(trf!("The copy stopped: {}", e)),
        }
    }

    /// File › Import › DVD Camcorder Disc: the titles of a finalized disc
    /// (or a folder holding its VIDEO_TS) copied and added to Project Media.
    pub(crate) fn dvd_import_window(&mut self, ctx: &egui::Context) {
        if !self.dvd_import.open {
            return;
        }
        if let Some(c) = &self.dvd_import.copying {
            let (_, _, result) = c.poll();
            if let Some(r) = result {
                self.dvd_import = DvdImport::default();
                self.finish_copy(r);
                return;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut open = true;
        let mut start = None;
        let mut refresh = false;
        let w = &mut self.dvd_import;
        let discs = w.discs.get_or_insert_with(|| {
            bv_platform::removable_drives()
                .into_iter()
                .filter(|d| !dvd_titles(&d.root).is_empty())
                .collect()
        });
        if w.source.is_none() {
            w.source = discs.first().map(|d| d.root.clone());
        }
        if let Some(s) = &w.source
            && w.titles.as_ref().is_none_or(|(of, _)| of != s)
        {
            w.titles = Some((s.clone(), dvd_titles(s)));
        }
        let titles = w.titles.as_ref().map_or_else(Vec::new, |t| t.1.clone());
        let label = discs
            .iter()
            .find(|d| Some(&d.root) == w.source.as_ref())
            .map_or_else(|| "DVD".to_string(), |d| d.label.clone());
        let dest = w.dest.get_or_insert_with(|| import_folder(&label));
        let busy = w.copying.as_ref().map(|c| {
            let (done, total, _) = c.poll();
            (done, total)
        });
        crate::floating::Window::new(tr("Import DVD Camcorder Disc"))
            .id(egui::Id::new("importar-dvd"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(tr("Disc:"));
                ui.horizontal(|ui| {
                    let shown = w.source.as_ref().map_or_else(
                        || tr("No disc found").to_string(),
                        |s| s.display().to_string(),
                    );
                    egui::ComboBox::from_id_salt("importar-dvd-disco")
                        .width(330.0)
                        .truncate()
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for d in discs.iter() {
                                let text = format!("{} {}", d.root.display(), d.label);
                                if ui
                                    .selectable_label(w.source.as_ref() == Some(&d.root), text)
                                    .clicked()
                                {
                                    w.source = Some(d.root.clone());
                                }
                            }
                        });
                    if ui.button(tr("Browse…")).clicked()
                        && let Some(dir) = rfd::FileDialog::new()
                            .set_title(tr("Import DVD Camcorder Disc"))
                            .pick_folder()
                    {
                        w.source = Some(dir);
                    }
                    if ui.button(tr("Refresh")).clicked() {
                        refresh = true;
                    }
                });
                ui.label(trn!(
                    titles.len(),
                    "{} title on the disc.",
                    "{} titles on the disc.",
                    titles.len()
                ));
                ui.label(tr("Copy to:"));
                folder_line(ui, dest, tr("Copy to:"));
                if let Some((done, total)) = busy {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(trf!(
                            "Copying {} of {}",
                            done + 1,
                            total
                        )),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = busy.is_none() && !titles.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(tr("Import")))
                        .clicked()
                    {
                        start = Some(
                            titles
                                .iter()
                                .filter_map(|t| Some((t.clone(), dest.join(t.file_name()?))))
                                .collect::<Vec<_>>(),
                        );
                    }
                });
            });
        if refresh {
            let w = &mut self.dvd_import;
            (w.discs, w.titles) = (None, None);
        }
        if let Some(files) = start {
            self.dvd_import.copying = Some(Copying::start(files));
        }
        if !open && self.dvd_import.copying.is_none() {
            self.dvd_import = DvdImport::default();
        }
    }

    /// View › Window › Device Explorer: the media of the cameras and cards
    /// connected, copied (the ones chosen) and added to Project Media.
    pub(crate) fn device_explorer_window(&mut self, ctx: &egui::Context) {
        if !self.device_explorer.open {
            return;
        }
        if let Some(c) = &self.device_explorer.copying {
            let (_, _, result) = c.poll();
            if let Some(r) = result {
                self.device_explorer.copying = None;
                self.finish_copy(r);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let mut open = true;
        let mut start = None;
        let w = &mut self.device_explorer;
        let drives = w.drives.get_or_insert_with(bv_platform::removable_drives);
        let mut refresh = false;
        let busy = w.copying.as_ref().map(|c| {
            let (done, total, _) = c.poll();
            (done, total)
        });
        crate::floating::Window::new(tr("Device Explorer"))
            .id(egui::Id::new("explorador-dispositivos"))
            .collapsible(false)
            .default_size(vec2(720.0, 420.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr("Refresh")).clicked() {
                        refresh = true;
                    }
                    if drives.is_empty() {
                        crate::tabs::hint(ui, tr("No camera or memory card is connected."));
                    }
                });
                ui.columns(2, |c| {
                    let (left, right) = c.split_at_mut(1);
                    let left = &mut left[0];
                    let right = &mut right[0];
                    left.label(egui::RichText::new(tr("Devices")).strong());
                    for d in drives.iter() {
                        let text = if d.label.is_empty() {
                            d.root.display().to_string()
                        } else {
                            format!("{} ({})", d.label, d.root.display())
                        };
                        let on = w.device.as_ref() == Some(&d.root);
                        if left.selectable_label(on, text).clicked() && !on {
                            w.device = Some(d.root.clone());
                            w.clips = device_clips(&d.root);
                            w.chosen.clear();
                            w.dest = Some(import_folder(&d.label));
                        }
                    }
                    right.label(egui::RichText::new(tr("Media")).strong());
                    let dest = w.dest.clone();
                    egui::ScrollArea::vertical()
                        .id_salt("explorador-midia")
                        .max_height(260.0)
                        .auto_shrink([false, false])
                        .show(right, |ui| {
                            if w.device.is_some() && w.clips.is_empty() {
                                crate::tabs::hint(ui, tr("No media on this device."));
                            }
                            for clip in &w.clips {
                                let name = clip
                                    .path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let there = dest.as_ref().is_some_and(|d| {
                                    std::fs::metadata(d.join(&name))
                                        .is_ok_and(|m| m.len() == clip.size)
                                });
                                ui.horizontal(|ui| {
                                    let mut on = w.chosen.contains(&clip.path);
                                    if crate::widgets::checkbox(ui, &mut on, &name).changed() {
                                        if on {
                                            w.chosen.insert(clip.path.clone());
                                        } else {
                                            w.chosen.remove(&clip.path);
                                        }
                                    }
                                    ui.label(
                                        egui::RichText::new(size_text(clip.size))
                                            .color(theme::text_dim()),
                                    );
                                    if there {
                                        ui.label(
                                            egui::RichText::new(tr("Imported"))
                                                .color(theme::region()),
                                        );
                                    }
                                });
                            }
                        });
                    right.horizontal(|ui| {
                        if ui.button(tr("Select All")).clicked() {
                            w.chosen = w.clips.iter().map(|c| c.path.clone()).collect();
                        }
                        if ui.button(tr("Select None")).clicked() {
                            w.chosen.clear();
                        }
                    });
                });
                ui.separator();
                if let Some(dest) = w.dest.as_mut() {
                    ui.label(tr("Import to:"));
                    folder_line(ui, dest, tr("Import to:"));
                }
                if let Some((done, total)) = busy {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(trf!(
                            "Copying {} of {}",
                            (done + 1).min(total),
                            total
                        )),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = busy.is_none() && !w.chosen.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(tr("Import")))
                        .clicked()
                        && let Some(dest) = &w.dest
                    {
                        start = Some(
                            w.clips
                                .iter()
                                .filter(|c| w.chosen.contains(&c.path))
                                .filter_map(|c| {
                                    Some((c.path.clone(), dest.join(c.path.file_name()?)))
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                });
            });
        if refresh {
            let w = &mut self.device_explorer;
            w.drives = None;
            if let Some(d) = w.device.clone() {
                w.clips = device_clips(&d);
            }
        }
        if let Some(files) = start {
            self.device_explorer.copying = Some(Copying::start(files));
        }
        if !open {
            self.device_explorer.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dvds_titles_and_a_cards_media() {
        let dir = std::env::temp_dir().join(format!("bananav-camera-{}", std::process::id()));
        let ts = dir.join("VIDEO_TS");
        std::fs::create_dir_all(&ts).unwrap();
        for name in [
            "VIDEO_TS.VOB",
            "VTS_01_0.VOB",
            "VTS_01_1.VOB",
            "VTS_01_2.VOB",
            "VTS_01_0.IFO",
        ] {
            std::fs::write(ts.join(name), b"x").unwrap();
        }
        let titles: Vec<String> = dvd_titles(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(titles, ["VTS_01_1.VOB", "VTS_01_2.VOB"]);
        // A card: its clips, folders deep, and nothing else.
        let clip = dir.join("DCIM").join("100CANON");
        std::fs::create_dir_all(&clip).unwrap();
        std::fs::write(clip.join("MVI_0001.MP4"), b"video").unwrap();
        std::fs::write(clip.join("notes.txt"), b"text").unwrap();
        let clips = device_clips(&dir.join("DCIM"));
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].size, 5);
        // Copied once: a second time the copy is kept.
        let to = dir.join("out").join("MVI_0001.MP4");
        let c = Copying::start(vec![(clips[0].path.clone(), to.clone())]);
        let result = loop {
            if let (_, _, Some(r)) = c.poll() {
                break r;
            }
            std::thread::yield_now();
        };
        assert_eq!(result, Ok(vec![to.clone()]));
        assert_eq!(std::fs::read(&to).unwrap(), b"video");
        assert!(import_folder("CAM:1").ends_with("CAM_1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
