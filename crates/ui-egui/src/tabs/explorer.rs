//! Explorer: VEGAS's file browser. The address bar (back, forward, up, and
//! as VEGAS 2026's a trail of the folders down to the one open — each a
//! click away, a click beside them to type a path — with its places at the
//! end), Refresh, Delete (to the Recycle Bin, asked first), Add to
//! Favorites, Start and Stop Preview, Auto Preview and Views; the folders' tree on the left and the open folder's contents
//! on the right, in a list, in details or — as VEGAS 2026's Help shows it —
//! in thumbnails (the first picture of each file, read in the background,
//! with + (Add to Project Media), Start Preview and Trim on the one under
//! the pointer); Views' Regions and Summary show the selected file's
//! regions (dragged to the timeline: an event of that part) and a short
//! description at the bottom. Double-click a file to add it (to the tracks
//! at the cursor, or to the Trimmer), or drag it to the timeline.

use crate::i18n::tr;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime};

use crate::app::{BananaApp, MediaDrag};
use crate::dock::Tab;
use crate::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets::{icon_button, icon_button_ex};
use bv_core::{Tick, TimeRange};
use bv_timeline::MediaInfo;

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
}

/// A file read in the background: its path and media information.
type Probed = (PathBuf, Option<MediaInfo>);

/// Views: how the folder's contents show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    List,
    Details,
    Thumbnails,
}

/// Views › Sort by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortBy {
    Name,
    Type,
    Date,
    Size,
}

#[derive(Debug)]
pub struct ExplorerState {
    dir: PathBuf,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    entries: Vec<Entry>,
    loaded: Option<PathBuf>,
    error: Option<String>,
    selected: Option<PathBuf>,
    /// Every file and folder picked (Ctrl and Shift add to them);
    /// `selected` is the one clicked last, which previews and panels follow.
    picked: Vec<PathBuf>,
    /// Where Shift's run of picks starts.
    anchor: Option<PathBuf>,
    path_text: String,
    /// The address bar shows the path as text to type in (a click beside
    /// the trail of folders), not the trail.
    typing: bool,
    /// Views › All Files: not only the media BananaV reads.
    all_files: bool,
    view: View,
    sort: SortBy,
    /// The tree: its root open, its first level, the folders open in it,
    /// the folders found in each (read when opened).
    root_open: bool,
    roots: TreeRoots,
    open: HashSet<PathBuf>,
    folders: HashMap<PathBuf, Vec<PathBuf>>,
    /// Views › Regions and Summary: panels about the selected file at the
    /// bottom.
    show_regions: bool,
    show_summary: bool,
    /// The files read (their media information, `None` when unreadable),
    /// and the reading of a folder's going on in the background.
    probes: HashMap<PathBuf, Option<MediaInfo>>,
    probing: Option<(PathBuf, std::sync::mpsc::Receiver<Probed>)>,
}

/// What stands for a file in the lists: an icon, or BananaV's logo for its own
/// projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
    Icon(Icon),
    Logo,
}

impl Glyph {
    /// Draws it in `rect`; the logo keeps its own colors.
    fn paint(self, painter: &egui::Painter, rect: Rect, color: Color32) {
        match self {
            Glyph::Icon(icon) => icons::paint(painter, rect, icon, color),
            Glyph::Logo => icons::paint_logo(painter, rect),
        }
    }
}

fn file_glyph(p: &Path) -> Glyph {
    use crate::media_files::FileKind;
    match crate::media_files::kind_of(p) {
        Some(FileKind::Audio) => Glyph::Icon(Icon::FileAudio),
        Some(FileKind::Image) => Glyph::Icon(Icon::FileImage),
        Some(FileKind::Video) => Glyph::Icon(Icon::FileVideo),
        Some(FileKind::Project) => Glyph::Logo,
        Some(FileKind::Timeline | FileKind::VegProject) | None => Glyph::Icon(Icon::File),
    }
}

fn human_size(b: u64) -> String {
    match b {
        0..=1023 => format!("{b} B"),
        1024..=1_048_575 => format!("{} KB", crate::fmt::num(b as f64 / 1024.0, 0)),
        1_048_576..=1_073_741_823 => format!("{} MB", crate::fmt::num(b as f64 / 1_048_576.0, 1)),
        _ => format!("{} GB", crate::fmt::num(b as f64 / 1_073_741_824.0, 2)),
    }
}

/// What kind of file it is, for the Type column.
fn type_text(e: &Entry) -> String {
    use crate::media_files::FileKind;
    if e.is_dir {
        return tr("Folder").into();
    }
    match crate::media_files::kind_of(&e.path) {
        Some(FileKind::Video) => tr("Video").into(),
        Some(FileKind::Audio) => tr("Audio").into(),
        Some(FileKind::Image) => tr("Image").into(),
        Some(FileKind::Project) => tr("BananaV project").into(),
        Some(FileKind::VegProject) => tr("VEG Project").into(),
        Some(FileKind::Timeline)
            if e.path
                .extension()
                .is_some_and(|x| x.to_string_lossy().to_lowercase().starts_with("otio")) =>
        {
            tr("OpenTimelineIO timeline").into()
        }
        Some(FileKind::Timeline) | None => e
            .path
            .extension()
            .map_or(tr("File").into(), |x| x.to_string_lossy().to_uppercase()),
    }
}

fn home() -> PathBuf {
    bv_platform::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// The user's folders the tree starts with: (name, folder), where the
/// system keeps them (on Linux, in the system's language: `~/Vídeos`).
fn places() -> Vec<(&'static str, PathBuf)> {
    use bv_platform::UserDir;
    [
        (tr("Desktop"), UserDir::Desktop),
        (tr("Documents"), UserDir::Documents),
        ("Downloads", UserDir::Downloads),
        (tr("Music"), UserDir::Music),
        (tr("Images"), UserDir::Pictures),
        (tr("Videos"), UserDir::Videos),
    ]
    .into_iter()
    .filter_map(|(name, dir)| Some((name, bv_platform::user_dir(dir)?)))
    .filter(|(_, p)| p.is_dir())
    .collect()
}

/// The drives on Windows; on Linux the root and the removable media
/// mounted (memory cards, sticks, discs).
fn drives() -> Vec<PathBuf> {
    if cfg!(windows) {
        (b'A'..=b'Z')
            .map(|c| PathBuf::from(format!("{}:\\", c as char)))
            .filter(|p| p.exists())
            .collect()
    } else {
        std::iter::once(PathBuf::from("/"))
            .chain(bv_platform::removable_drives().into_iter().map(|d| d.root))
            .collect()
    }
}

/// A line of the tree's first level: its name, folder and icon.
type TreeRoot = (String, PathBuf, Icon);

/// The tree's first level: the favorites that exist, the user's folders and
/// the drives.
fn read_roots(favorites: &[PathBuf]) -> Vec<TreeRoot> {
    let mut nodes: Vec<TreeRoot> = favorites
        .iter()
        .filter(|p| p.is_dir())
        .map(|p| (node_name(p), p.clone(), Icon::StarFilled))
        .collect();
    nodes.extend(
        places()
            .into_iter()
            .map(|(n, p)| (n.to_string(), p, Icon::Folder)),
    );
    nodes.extend(
        drives()
            .into_iter()
            .map(|d| (d.display().to_string(), d, Icon::Disc)),
    );
    nodes
}

/// The tree's first level, read on a thread of its own every few seconds:
/// frames never touch the disks for it (a disk asleep or a network drive
/// would hold them up).
#[derive(Debug, Default)]
struct TreeRoots {
    nodes: Vec<TreeRoot>,
    /// The favorites they were read for, and when.
    favorites: Vec<PathBuf>,
    read_at: Option<Instant>,
    reading: Option<Receiver<Vec<TreeRoot>>>,
}

impl TreeRoots {
    const EVERY: Duration = Duration::from_secs(5);

    /// The first level as last read; a new reading starts when it is time
    /// (the interface repaints when it brings something new).
    fn nodes(&mut self, ctx: &egui::Context, favorites: &[PathBuf]) -> &[TreeRoot] {
        if let Some(reading) = &self.reading {
            match reading.try_recv() {
                Ok(nodes) => {
                    self.nodes = nodes;
                    self.reading = None;
                }
                Err(TryRecvError::Disconnected) => self.reading = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let due =
            self.favorites != favorites || self.read_at.is_none_or(|t| t.elapsed() >= Self::EVERY);
        if due && self.reading.is_none() {
            self.read_at = Some(Instant::now());
            self.favorites = favorites.to_vec();
            let (tx, rx) = channel();
            let (ctx, list, known) = (ctx.clone(), favorites.to_vec(), self.nodes.clone());
            let spawned = std::thread::Builder::new()
                .name("bv-explorer-roots".into())
                .spawn(move || {
                    let nodes = read_roots(&list);
                    let new = nodes != known;
                    let _ = tx.send(nodes);
                    if new {
                        ctx.request_repaint();
                    }
                });
            if spawned.is_ok() {
                // The first time, disks that answer quickly show at once.
                match rx.recv_timeout(Duration::from_millis(if self.nodes.is_empty() {
                    100
                } else {
                    0
                })) {
                    Ok(nodes) => self.nodes = nodes,
                    Err(_) => self.reading = Some(rx),
                }
            }
        }
        &self.nodes
    }
}

/// Folders and files to leave out: hidden and the system's.
fn hidden(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('$')
}

impl ExplorerState {
    pub fn new() -> ExplorerState {
        let dir = bv_platform::user_dir(bv_platform::UserDir::Videos)
            .filter(|v| v.is_dir())
            .unwrap_or_else(home);
        ExplorerState {
            path_text: dir.display().to_string(),
            typing: false,
            dir,
            back: Vec::new(),
            forward: Vec::new(),
            entries: Vec::new(),
            loaded: None,
            error: None,
            selected: None,
            picked: Vec::new(),
            anchor: None,
            all_files: false,
            view: View::Details,
            sort: SortBy::Name,
            root_open: true,
            roots: TreeRoots::default(),
            open: HashSet::new(),
            folders: HashMap::new(),
            show_regions: false,
            show_summary: false,
            probes: HashMap::new(),
            probing: None,
        }
    }

    /// The thumbnails' files read in the background, a folder at a time:
    /// what came in since the last frame kept; a folder not read yet
    /// started.
    fn read_files(&mut self) {
        if let Some((_, rx)) = &self.probing {
            while let Ok((path, info)) = rx.try_recv() {
                self.probes.insert(path, info);
            }
        }
        if self
            .probing
            .as_ref()
            .is_some_and(|(dir, _)| *dir == self.dir)
        {
            return;
        }
        let files: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| !e.is_dir && crate::media_files::kind_of(&e.path).is_some())
            .map(|e| e.path.clone())
            .filter(|p| !self.probes.contains_key(p))
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("explorer-probe".into())
            .spawn(move || {
                for path in files {
                    let info = bv_engine::Engine::probe_file(&path.to_string_lossy()).ok();
                    if tx.send((path, info)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        self.probing = Some((self.dir.clone(), rx));
    }

    fn load(&mut self) {
        self.error = None;
        self.entries.clear();
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let path = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if hidden(&name) {
                        continue;
                    }
                    let meta = e.metadata().ok();
                    let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
                    if !is_dir && !self.all_files && crate::media_files::kind_of(&path).is_none() {
                        continue;
                    }
                    self.entries.push(Entry {
                        size: meta.as_ref().map_or(0, |m| m.len()),
                        modified: meta.and_then(|m| m.modified().ok()),
                        path,
                        name,
                        is_dir,
                    });
                }
                self.sort_entries();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.loaded = Some(self.dir.clone());
        self.path_text = self.dir.display().to_string();
    }

    /// Folders first, then as Views › Sort by says.
    fn sort_entries(&mut self) {
        let sort = self.sort;
        self.entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| match sort {
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Type => type_text(a)
                    .cmp(&type_text(b))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                SortBy::Date => b.modified.cmp(&a.modified),
                SortBy::Size => b.size.cmp(&a.size),
            })
        });
    }

    fn go(&mut self, dir: PathBuf) {
        if dir != self.dir {
            self.back.push(std::mem::replace(&mut self.dir, dir));
            self.forward.clear();
            self.loaded = None;
            self.selected = None;
            self.picked.clear();
            self.anchor = None;
        }
    }

    /// A click on `path`: it alone, with Ctrl added to the picks (or taken
    /// from them), with Shift the run from the anchor to it.
    fn click(&mut self, path: &Path, mods: egui::Modifiers) {
        let place = |p: &Path| self.entries.iter().position(|e| e.path == p);
        if mods.shift
            && let Some(a) = self.anchor.as_deref().and_then(place)
            && let Some(b) = place(path)
        {
            let run = &self.entries[a.min(b)..=a.max(b)];
            self.picked = run.iter().map(|e| e.path.clone()).collect();
            self.selected = Some(path.to_path_buf());
            return;
        }
        if mods.command {
            if let Some(k) = self.picked.iter().position(|p| p == path) {
                self.picked.remove(k);
                self.selected = self.picked.last().cloned();
            } else {
                self.picked.push(path.to_path_buf());
                self.selected = Some(path.to_path_buf());
            }
        } else {
            self.picked = vec![path.to_path_buf()];
            self.selected = Some(path.to_path_buf());
        }
        self.anchor = Some(path.to_path_buf());
    }

    fn is_picked(&self, path: &Path) -> bool {
        self.picked.iter().any(|p| p == path)
    }

    /// What an action on `path` takes: every pick, in the folder's order,
    /// when `path` is one of several (only files BananaV reads, unless
    /// `any`), else `path` alone.
    fn targets(&self, path: &Path, any: bool) -> Vec<PathBuf> {
        if self.picked.len() < 2 || !self.is_picked(path) {
            return vec![path.to_path_buf()];
        }
        self.entries
            .iter()
            .filter(|e| self.is_picked(&e.path))
            .filter(|e| any || (!e.is_dir && crate::media_files::kind_of(&e.path).is_some()))
            .map(|e| e.path.clone())
            .collect()
    }

    /// Whether `p`, picked, is a file: as the folder's list read says, the
    /// disk asked only for what is not in it.
    fn is_file(&self, p: &Path) -> bool {
        self.entries
            .iter()
            .find(|e| e.path == p)
            .map_or_else(|| p.is_file(), |e| !e.is_dir)
    }

    /// The folders inside `dir` (read once, when its node opens).
    fn subfolders(&mut self, dir: &Path) -> &[PathBuf] {
        self.folders.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                        .filter(|e| !hidden(&e.file_name().to_string_lossy()))
                        .map(|e| e.path())
                        .collect()
                })
                .unwrap_or_default();
            v.sort_by_key(|p| node_name(p).to_lowercase());
            v
        })
    }
}

/// A folder's name in the tree: its own, or a drive's letter.
fn node_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().to_string(),
    )
}

const ROW_H: f32 = 22.0;
const TREE_W: f32 = 255.0;

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    if app.explorer.loaded.as_ref() != Some(&app.explorer.dir) {
        app.explorer.load();
    }
    toolbar(ui, app);
    let st = &mut app.explorer;
    let area = ui.available_rect_before_wrap();
    let id = ui.id().with("explorer-tree");
    let w = crate::widgets::pane_width(ui, id, area, TREE_W, 80.0, 160.0);
    let tree_right = area.left() + w;
    let tree = Rect::from_min_max(area.min, pos2(tree_right, area.bottom()));
    let contents = Rect::from_min_max(pos2(tree_right + 4.0, area.top()), area.max);
    let favorites = app.settings.explorer_favorites.clone();
    let auto_preview = app.settings.explorer_auto_preview;
    ui.scope_builder(
        UiBuilder::new().max_rect(tree.shrink2(vec2(4.0, 4.0))),
        |ui| {
            egui::ScrollArea::both()
                .id_salt("explorer-tree")
                .auto_shrink([false, false])
                .show(ui, |ui| tree_ui(ui, st, &favorites));
        },
    );
    let thumbnails = st.view == View::Thumbnails;
    let panels = st.show_regions as u8 + st.show_summary as u8;
    let panel_h = 130.0;
    let list = Rect::from_min_max(
        contents.min,
        pos2(
            contents.right(),
            (contents.bottom() - panel_h * panels as f32).max(contents.top() + 60.0),
        ),
    );
    let mut act = None;
    ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
        act = if thumbnails {
            thumbnails_ui(ui, app, &favorites, auto_preview)
        } else {
            contents_ui(ui, &mut app.explorer, &favorites, auto_preview)
        };
    });
    let mut top = list.bottom();
    for (on, regions) in [
        (app.explorer.show_regions, true),
        (app.explorer.show_summary, false),
    ] {
        if !on {
            continue;
        }
        let r = Rect::from_min_max(
            pos2(contents.left(), top),
            pos2(contents.right(), top + panel_h),
        );
        top += panel_h;
        ui.scope_builder(UiBuilder::new().max_rect(r.shrink(2.0)), |ui| {
            if regions {
                regions_panel(ui, app);
            } else {
                summary_panel(ui, app);
            }
        });
    }
    ui.allocate_rect(area, Sense::hover());
    match act {
        Some(Act::Add(p)) => {
            let paths = app.explorer.targets(&p, false);
            add_files(app, paths);
        }
        Some(Act::Trim(p)) => add_file(app, p, true),
        Some(Act::Import(p)) => {
            let paths = app.explorer.targets(&p, false);
            app.import_paths(&paths);
        }
        Some(Act::Drag(p)) => {
            let mut paths = app.explorer.targets(&p, false);
            app.media_drag = Some(if paths.len() == 1 {
                MediaDrag::File(paths.remove(0))
            } else {
                MediaDrag::Files(paths)
            });
            app.media_drag_secondary = ui.input(|i| i.pointer.secondary_down());
        }
        Some(Act::Preview(p)) => app.preview_file(&p),
        Some(Act::Favorite(p, on)) => {
            let list = &mut app.settings.explorer_favorites;
            list.retain(|f| *f != p);
            if on {
                list.push(p);
            }
            let _ = app.settings.save();
        }
        Some(Act::EditSource(p)) => app.edit_source_project(&p),
        Some(Act::Recycle(p)) => {
            let paths = app.explorer.targets(&p, true);
            app.ask_recycle(paths);
        }
        Some(Act::Reveal(p)) => {
            if let Err(e) = crate::app::more_actions::open_externally(&p) {
                app.report_error(e.to_string());
            }
        }
        None => {}
    }
}

/// What the contents pane asks for. Add, Add to Project Media, a drag and
/// Delete on one of several picks take them all.
enum Act {
    /// Double-click or Add to Timeline: to the tracks at the cursor (or the
    /// Trimmer, as Options › Preferences › General says).
    Add(PathBuf),
    Trim(PathBuf),
    /// Add to Project Media.
    Import(PathBuf),
    Drag(PathBuf),
    /// Start Preview (the menu, or a click with Auto Preview on).
    Preview(PathBuf),
    Favorite(PathBuf, bool),
    /// Delete: to the Recycle Bin, asked first.
    Recycle(PathBuf),
    /// Open the folder in the system's file explorer.
    Reveal(PathBuf),
    /// Edit Source Project: the project the file was rendered from.
    EditSource(PathBuf),
}

/// VEGAS's toolbar: the address bar with back, forward and up in it and
/// its places at the end; Refresh, Auto Preview and Views.
fn toolbar(ui: &mut Ui, app: &mut BananaApp) {
    // Previews: the selected file alone through the preview bus.
    let file = app
        .explorer
        .selected
        .clone()
        .filter(|p| app.explorer.is_file(p));
    let previewing = app.previewing();
    let auto = app.settings.explorer_auto_preview;
    let favorites = app.settings.explorer_favorites.clone();
    let (mut start, mut stop, mut toggle) = (false, false, false);
    let (mut recycle, mut favorite) = (None, None);
    let st = &mut app.explorer;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Room for the seven buttons after it.
        let bar_w = (ui.available_width() - 206.0).max(120.0);
        let (bar, _) = ui.allocate_exact_size(vec2(bar_w, 26.0), Sense::hover());
        ui.painter().rect_filled(bar, 2.0, theme::panel_dark());
        ui.scope_builder(
            UiBuilder::new().max_rect(bar.shrink2(vec2(2.0, 1.0))),
            |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if icon_button_ex(ui, Icon::Back, tr("Back"), false, !st.back.is_empty())
                        .clicked()
                        && let Some(d) = st.back.pop()
                    {
                        st.forward.push(std::mem::replace(&mut st.dir, d));
                        st.loaded = None;
                    }
                    if icon_button_ex(
                        ui,
                        Icon::Forward,
                        tr("Forward"),
                        false,
                        !st.forward.is_empty(),
                    )
                    .clicked()
                        && let Some(d) = st.forward.pop()
                    {
                        st.back.push(std::mem::replace(&mut st.dir, d));
                        st.loaded = None;
                    }
                    let up = st.dir.parent().map(Path::to_path_buf);
                    if icon_button_ex(ui, Icon::Up, tr("Up One Folder"), false, up.is_some())
                        .clicked()
                        && let Some(parent) = up
                    {
                        st.go(parent);
                    }
                    ui.add_space(6.0);
                    let field = (ui.available_width() - 24.0).max(40.0);
                    if st.typing {
                        let r = ui.add_sized(
                            vec2(field, 22.0),
                            egui::TextEdit::singleline(&mut st.path_text)
                                .frame(egui::Frame::NONE)
                                .vertical_align(egui::Align::Center),
                        );
                        if !r.has_focus() && !r.lost_focus() {
                            r.request_focus();
                        }
                        if r.lost_focus() {
                            let p = PathBuf::from(st.path_text.trim());
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && p.is_dir() {
                                st.go(p);
                            }
                            st.path_text = st.dir.display().to_string();
                            st.typing = false;
                        }
                    } else if let Some(to) = trail(ui, &st.dir, field) {
                        match to {
                            Some(dir) => st.go(dir),
                            None => st.typing = true,
                        }
                    }
                    // Its places: the user's folders and the drives.
                    egui::ComboBox::from_id_salt("explorer-places")
                        .width(18.0)
                        .selected_text("")
                        .show_ui(ui, |ui| {
                            for (name, p) in places() {
                                if ui.selectable_label(false, name).clicked() {
                                    st.go(p);
                                }
                            }
                            ui.separator();
                            for d in drives() {
                                if ui
                                    .selectable_label(false, d.display().to_string())
                                    .clicked()
                                {
                                    st.go(d);
                                }
                            }
                        });
                });
            },
        );
        if icon_button(ui, Icon::Refresh, tr("Refresh")).clicked() {
            st.loaded = None;
            st.folders.clear();
        }
        let picked = st.selected.clone();
        if icon_button_ex(ui, Icon::Delete, tr("Delete"), false, picked.is_some()).clicked()
            && let Some(p) = picked.clone()
        {
            recycle = Some(st.targets(&p, true));
        }
        // Add to Favorites: the folder picked, or the one open.
        let folder = picked
            .filter(|p| !st.is_file(p))
            .unwrap_or_else(|| st.dir.clone());
        let fav = favorites.contains(&folder);
        if crate::widgets::icon_toggle(ui, Icon::Star, fav, tr("Add to Favorites")).clicked() {
            favorite = Some((folder, !fav));
        }
        start =
            icon_button_ex(ui, Icon::Play, tr("Start Preview"), false, file.is_some()).clicked();
        stop = icon_button_ex(ui, Icon::Stop, tr("Stop Preview"), false, previewing).clicked();
        toggle =
            crate::widgets::icon_toggle(ui, Icon::AutoPreview, auto, tr("Auto Preview")).clicked();
        views_menu(ui, st);
    });
    if start && let Some(path) = &file {
        app.preview_file(path);
    }
    if stop {
        app.stop_preview(false);
    }
    if toggle {
        app.settings.explorer_auto_preview = !auto;
        let _ = app.settings.save();
    }
    if let Some(paths) = recycle {
        app.ask_recycle(paths);
    }
    if let Some((p, on)) = favorite {
        let list = &mut app.settings.explorer_favorites;
        list.retain(|f| *f != p);
        if on {
            list.push(p);
        }
        let _ = app.settings.save();
    }
}

/// The address bar's trail: the folders from the drive (or `/`) down to
/// `dir`, each a button, those that do not fit in `width` behind a «
/// menu. Returns where a click goes: `Some(Some(folder))`, or
/// `Some(None)` for a click beside them (typing a path).
fn trail(ui: &mut Ui, dir: &Path, width: f32) -> Option<Option<PathBuf>> {
    let crumbs: Vec<(String, PathBuf)> = dir
        .ancestors()
        .map(|p| (crumb_name(p), p.to_path_buf()))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let font = egui::TextStyle::Button.resolve(ui.style());
    let room = |name: &str| {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        }) + 30.0
    };
    // As many of the last folders as fit; the others behind «.
    let mut used = 26.0;
    let mut first = crumbs.len();
    while first > 0 {
        let w = room(&crumbs[first - 1].0);
        if used + w > width && first < crumbs.len() {
            break;
        }
        used += w;
        first -= 1;
    }
    let (rect, back) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    let mut out = back.clicked().then_some(None);
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 0.0;
    if first > 0 {
        let more = row.add(egui::Button::new("«").frame(false));
        egui::Popup::menu(&more).show(|ui| {
            for (name, p) in crumbs[..first].iter().rev() {
                if ui.button(name).clicked() {
                    out = Some(Some(p.clone()));
                    ui.close();
                }
            }
        });
    }
    for (i, (name, p)) in crumbs.iter().enumerate().skip(first) {
        if i > first || first > 0 {
            row.label(egui::RichText::new(" › ").color(theme::text_dim()));
        }
        let (icon, _) = row.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        icons::paint(row.painter(), icon, Icon::Folder, theme::text_dim());
        row.add_space(2.0);
        if row
            .add(egui::Button::new(name.as_str()).frame(false))
            .clicked()
        {
            out = Some(Some(p.clone()));
        }
    }
    out
}

/// A folder's name in the trail: its own, or the drive's (`C:`) or `/`.
fn crumb_name(p: &Path) -> String {
    match p.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => {
            let s = p.display().to_string();
            let s = s.trim_end_matches(['\\', '/']);
            if s.is_empty() {
                "/".to_string()
            } else {
                s.to_string()
            }
        }
    }
}

/// Views: List or Details, All Files, Sort by.
fn views_menu(ui: &mut Ui, st: &mut ExplorerState) {
    let resp = icon_button(ui, Icon::Views, tr("Views"));
    egui::Popup::menu(&resp).show(|ui| {
        for (view, name) in [(View::List, tr("List")), (View::Details, tr("Details"))] {
            if crate::widgets::radio(ui, st.view == view, name).clicked() {
                st.view = view;
                ui.close();
            }
        }
        if crate::widgets::radio(ui, st.view == View::Thumbnails, tr("Thumbnails")).clicked() {
            st.view = View::Thumbnails;
            ui.close();
        }
        ui.separator();
        if crate::widgets::checkbox(ui, &mut st.show_regions, tr("Regions")).changed() {
            ui.close();
        }
        if crate::widgets::checkbox(ui, &mut st.show_summary, tr("Summary")).changed() {
            ui.close();
        }
        ui.separator();
        if crate::widgets::checkbox(ui, &mut st.all_files, tr("All Files")).changed() {
            st.loaded = None;
            ui.close();
        }
        ui.menu_button(tr("Sort by"), |ui| {
            for (sort, name) in [
                (SortBy::Name, tr("Name")),
                (SortBy::Type, tr("Type")),
                (SortBy::Date, tr("Date")),
                (SortBy::Size, tr("Size")),
            ] {
                if crate::widgets::radio(ui, st.sort == sort, name).clicked() {
                    st.sort = sort;
                    st.sort_entries();
                    ui.close();
                }
            }
        });
    });
}

/// The folders' tree: Explorer, then the favorites, the user's folders and
/// the drives, each opening onto the folders in it.
fn tree_ui(ui: &mut Ui, st: &mut ExplorerState, favorites: &[PathBuf]) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let root = tree_row(
        ui,
        0,
        tr("Explorer"),
        Icon::Folder,
        st.root_open,
        true,
        false,
    );
    if root.toggled {
        st.root_open = !st.root_open;
    }
    if !st.root_open {
        return;
    }
    let nodes = st.roots.nodes(ui.ctx(), favorites).to_vec();
    for (name, path, icon) in nodes {
        folder_node(ui, st, 1, name, path, icon);
    }
}

/// A folder of the tree, and the folders in it while it is open.
fn folder_node(
    ui: &mut Ui,
    st: &mut ExplorerState,
    depth: usize,
    name: String,
    path: PathBuf,
    icon: Icon,
) {
    let open = st.open.contains(&path);
    let leaf = open && st.subfolders(&path).is_empty();
    let row = tree_row(ui, depth, &name, icon, open, !leaf, st.dir == path);
    if row.toggled {
        if open {
            st.open.remove(&path);
        } else {
            st.open.insert(path.clone());
        }
    }
    if row.clicked {
        st.go(path.clone());
    }
    if open && depth < 24 {
        let children = st.subfolders(&path).to_vec();
        for child in children {
            folder_node(ui, st, depth + 1, node_name(&child), child, Icon::Folder);
        }
    }
}

struct TreeRow {
    toggled: bool,
    clicked: bool,
}

/// A line of the tree: its triangle (when it can open), icon and name.
fn tree_row(
    ui: &mut Ui,
    depth: usize,
    name: &str,
    icon: Icon,
    open: bool,
    can_open: bool,
    selected: bool,
) -> TreeRow {
    let indent = 4.0 + 16.0 * depth as f32;
    let font = FontId::proportional(13.0);
    let width = (indent + 44.0 + name.chars().count() as f32 * 7.5).max(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, theme::selection());
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::hover());
    }
    let color = if selected {
        Color32::WHITE
    } else {
        theme::text()
    };
    let arrow = Rect::from_center_size(
        pos2(rect.left() + indent + 6.0, rect.center().y),
        vec2(12.0, 12.0),
    );
    if can_open {
        let c = arrow.center();
        let pts = if open {
            vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ]
        } else {
            vec![
                c + vec2(-2.0, -4.0),
                c + vec2(3.0, 0.0),
                c + vec2(-2.0, 4.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(
            pts,
            theme::text_dim(),
            Stroke::NONE,
        ));
    }
    let ir = Rect::from_center_size(
        pos2(arrow.right() + 12.0, rect.center().y),
        vec2(16.0, 16.0),
    );
    let icon_color = match icon {
        Icon::Folder => theme::banana(),
        Icon::StarFilled => theme::banana(),
        _ => color,
    };
    icons::paint(painter, ir, icon, icon_color);
    painter.text(
        pos2(ir.right() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        font,
        color,
    );
    let on_arrow = resp
        .interact_pointer_pos()
        .is_some_and(|p| p.x <= arrow.right() + 2.0);
    TreeRow {
        toggled: can_open && (resp.double_clicked() || (resp.clicked() && on_arrow)),
        clicked: resp.clicked() && !on_arrow,
    }
}

/// The open folder's contents: a list, or details with sortable columns.
fn contents_ui(
    ui: &mut Ui,
    st: &mut ExplorerState,
    favorites: &[PathBuf],
    auto_preview: bool,
) -> Option<Act> {
    if let Some(e) = &st.error {
        ui.add_space(6.0);
        ui.colored_label(theme::error(), e);
        return None;
    }
    let mut act = None;
    let mut navigate = None;
    if st.view == View::Details {
        header(ui, st);
    }
    let details = st.view == View::Details;
    egui::ScrollArea::vertical()
        .id_salt("explorer-contents")
        .auto_shrink([false, false])
        .show_rows(ui, ROW_H, st.entries.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let rows = st.entries[range].to_vec();
            for e in &rows {
                let (rect, resp) = ui.allocate_exact_size(
                    vec2(ui.available_width(), ROW_H),
                    Sense::click_and_drag(),
                );
                let painter = ui.painter_at(rect);
                let selected = st.is_picked(&e.path);
                if selected {
                    painter.rect_filled(rect, 0.0, theme::selection());
                } else if resp.hovered() {
                    painter.rect_filled(rect, 0.0, theme::hover());
                }
                let supported = e.is_dir || crate::media_files::kind_of(&e.path).is_some();
                let color = if selected {
                    Color32::WHITE
                } else if supported {
                    theme::text()
                } else {
                    theme::text_dim()
                };
                let glyph = if e.is_dir {
                    Glyph::Icon(Icon::Folder)
                } else {
                    file_glyph(&e.path)
                };
                glyph.paint(
                    &painter,
                    Rect::from_min_size(rect.min + vec2(6.0, 3.0), vec2(16.0, 16.0)),
                    if e.is_dir { theme::banana() } else { color },
                );
                let columns = columns(rect.width());
                let name_room = if details {
                    columns[0] - 30.0
                } else {
                    rect.width() - 34.0
                };
                crate::widgets::fit_text(
                    &painter,
                    rect.left_center() + vec2(28.0, 0.0),
                    Align2::LEFT_CENTER,
                    &e.name,
                    FontId::proportional(13.0),
                    color,
                    name_room,
                );
                if details {
                    let small = FontId::proportional(12.0);
                    let mut x = rect.left() + columns[0];
                    let size = if e.is_dir {
                        String::new()
                    } else {
                        human_size(e.size)
                    };
                    let date = e.modified.map(crate::fmt::date_time).unwrap_or_default();
                    for (text, w) in [
                        (size, columns[1]),
                        (type_text(e), columns[2]),
                        (date, columns[3]),
                    ] {
                        crate::widgets::fit_text(
                            &painter,
                            pos2(x + 6.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            &text,
                            small.clone(),
                            if selected {
                                Color32::WHITE
                            } else {
                                theme::text_dim()
                            },
                            w - 10.0,
                        );
                        x += w;
                    }
                }
                if let Some(a) = row_acts(st, e, &resp, supported, auto_preview) {
                    act = Some(a);
                }
                if resp.double_clicked() && e.is_dir {
                    navigate = Some(e.path.clone());
                }
                let resp = if supported {
                    resp
                } else {
                    resp.on_hover_text(tr("Format not supported by BananaV yet"))
                };
                let path = e.path.clone();
                let is_dir = e.is_dir;
                resp.context_menu(|ui| {
                    if let Some(a) = entry_menu(ui, &path, is_dir, supported, favorites) {
                        act = Some(a);
                    }
                });
            }
        });
    if let Some(d) = navigate {
        st.go(d);
    }
    act
}

/// The details' columns: name, size, type, date modified.
fn columns(width: f32) -> [f32; 4] {
    let (size, kind, date) = (90.0, 110.0, 150.0);
    [(width - size - kind - date).max(120.0), size, kind, date]
}

/// The details' header: a click sorts by the column.
fn header(ui: &mut Ui, st: &mut ExplorerState) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::panel_dark());
    let mut x = rect.left();
    let cols = columns(rect.width());
    for (k, (name, sort)) in [
        (tr("Name"), SortBy::Name),
        (tr("Size"), SortBy::Size),
        (tr("Type"), SortBy::Type),
        (tr("Date Modified"), SortBy::Date),
    ]
    .into_iter()
    .enumerate()
    {
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(cols[k], rect.height()));
        let resp = ui.interact(r, ui.id().with(("explorer-column", k)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, theme::hover());
        }
        let color = if st.sort == sort {
            theme::text_strong()
        } else {
            theme::text_dim()
        };
        crate::widgets::fit_text(
            ui.painter(),
            pos2(r.left() + 6.0, r.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(12.0),
            color,
            r.width() - 10.0,
        );
        ui.painter().vline(
            r.right() - 0.5,
            r.y_range(),
            Stroke::new(1.0, theme::separator()),
        );
        if resp.clicked() {
            st.sort = sort;
            st.sort_entries();
        }
        x += cols[k];
    }
}

/// A file double-clicked or added: a project opens; media goes into the
/// project and to the tracks at the cursor, or into the Trimmer (`trim`, or
/// as Options › Preferences › General says).
/// A row's or a thumbnail's clicks: picking it (with Ctrl and Shift; a
/// drag or a right-click on one not picked picks it alone), Auto Preview,
/// a double-click's Add and a drag.
fn row_acts(
    st: &mut ExplorerState,
    e: &Entry,
    resp: &egui::Response,
    supported: bool,
    auto_preview: bool,
) -> Option<Act> {
    let mut act = None;
    if resp.clicked() {
        st.click(&e.path, resp.ctx.input(|i| i.modifiers));
        if auto_preview && !e.is_dir && supported && st.is_picked(&e.path) {
            act = Some(Act::Preview(e.path.clone()));
        }
    }
    if (resp.drag_started() || resp.secondary_clicked()) && !st.is_picked(&e.path) {
        st.click(&e.path, egui::Modifiers::NONE);
    }
    if resp.double_clicked() && !e.is_dir && supported {
        act = Some(Act::Add(e.path.clone()));
    }
    if resp.drag_started() && !e.is_dir && supported {
        act = Some(Act::Drag(e.path.clone()));
    }
    act
}

/// Add to Timeline of several files: imported and placed one after
/// another at the cursor (one undo); one alone as a double-click adds it.
fn add_files(app: &mut BananaApp, paths: Vec<PathBuf>) {
    if let [one] = paths.as_slice() {
        add_file(app, one.clone(), false);
        return;
    }
    let at = app.cursor;
    app.as_one_undo(|app| {
        let ids = app.import_paths(&paths);
        let arrange = crate::timeline::Arrange::AcrossTime;
        app.add_dropped(ids, at, None, None, arrange, (true, true));
    });
}

fn add_file(app: &mut BananaApp, p: PathBuf, trim: bool) {
    use crate::media_files::FileKind;
    if matches!(
        crate::media_files::kind_of(&p),
        Some(FileKind::Project | FileKind::Timeline)
    ) {
        app.open_path(p);
    } else if let Some(&id) = app.import_paths(&[p]).first() {
        app.selected_media = Some(id);
        // To the Trimmer, or to the tracks at the cursor (Options ›
        // Preferences › General: "Double click on media file loads into
        // Trimmer instead of tracks").
        if trim || app.settings.prefs.media_to_trimmer {
            app.trimmer.open(id);
            app.show_tab(Tab::Trimmer, false);
        } else {
            let at = app.cursor;
            app.place_media(id, at, None);
        }
    }
}

/// A thumbnail's picture size.
const THUMB: egui::Vec2 = vec2(144.0, 81.0);

/// The open folder's contents as thumbnails (VEGAS 2026's Explorer): each
/// file's first picture (a sound's or an unread file's icon), its name
/// under it in two lines; on the one under the pointer, +, Start Preview
/// and Trim.
fn thumbnails_ui(
    ui: &mut Ui,
    app: &mut BananaApp,
    favorites: &[PathBuf],
    auto_preview: bool,
) -> Option<Act> {
    app.explorer.read_files();
    if let Some(e) = &app.explorer.error {
        ui.add_space(6.0);
        ui.colored_label(theme::error(), e);
        return None;
    }
    let entries = app.explorer.entries.clone();
    let mut act = None;
    let mut navigate = None;
    egui::ScrollArea::vertical()
        .id_salt("explorer-thumbnails")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                ui.add_space(6.0);
                for e in &entries {
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(THUMB.x, THUMB.y + 34.0),
                        Sense::click_and_drag(),
                    );
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let pic = Rect::from_min_size(rect.min, THUMB);
                    let selected = app.explorer.is_picked(&e.path);
                    let current = app.explorer.selected.as_ref() == Some(&e.path);
                    let painter = ui.painter_at(rect.expand(3.0));
                    if selected {
                        painter.rect_filled(
                            rect.expand(3.0),
                            2.0,
                            theme::selection().gamma_multiply(0.5),
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect.expand(3.0), 2.0, theme::hover());
                    }
                    painter.rect_filled(pic, 0.0, Color32::BLACK);
                    let info = app.explorer.probes.get(&e.path).cloned().flatten();
                    let picture = info.as_ref().filter(|i| i.video.is_some()).and_then(|i| {
                        let h = (pic.height() * ui.ctx().pixels_per_point()).round() as u32;
                        app.engine.thumbnail(i, Tick::ZERO, h)
                    });
                    match picture {
                        Some(t) => {
                            let id = app.textures.get(ui.ctx(), &t);
                            let aspect = t.width as f32 / t.height.max(1) as f32;
                            let w = (pic.height() * aspect).min(pic.width());
                            painter.image(
                                id,
                                Rect::from_center_size(pic.center(), vec2(w, w / aspect)),
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            let glyph = if e.is_dir {
                                Glyph::Icon(Icon::Folder)
                            } else {
                                file_glyph(&e.path)
                            };
                            glyph.paint(
                                &painter,
                                Rect::from_center_size(pic.center(), vec2(32.0, 32.0)),
                                if e.is_dir {
                                    theme::banana()
                                } else {
                                    theme::text_dim()
                                },
                            );
                        }
                    }
                    // The name, in two lines at most.
                    let mut job = egui::text::LayoutJob::simple(
                        e.name.clone(),
                        FontId::proportional(12.0),
                        if selected {
                            Color32::WHITE
                        } else {
                            theme::text()
                        },
                        THUMB.x,
                    );
                    job.wrap.max_rows = 2;
                    job.halign = egui::Align::Center;
                    let galley = ui.fonts_mut(|f| f.layout_job(job));
                    painter.galley(
                        pos2(pic.center().x, pic.bottom() + 3.0),
                        galley,
                        theme::text(),
                    );
                    let supported = e.is_dir || crate::media_files::kind_of(&e.path).is_some();
                    // The one under the pointer: +, Start Preview, Trim.
                    if !e.is_dir && supported && (resp.hovered() || current) {
                        for (k, (icon, tip, what)) in [
                            (Icon::Plus, tr("Add to Project Media"), 0),
                            (Icon::Play, tr("Start Preview"), 1),
                            (Icon::Trim, tr("Trim"), 2),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let b = Rect::from_min_size(
                                pos2(
                                    pic.right() - 22.0 * (3 - k) as f32 - 2.0,
                                    pic.bottom() - 22.0,
                                ),
                                vec2(20.0, 20.0),
                            );
                            let r = ui
                                .interact(
                                    b,
                                    ui.id().with(("explorer-thumb", &e.path, what)),
                                    Sense::click(),
                                )
                                .on_hover_text(tip);
                            painter.rect_filled(
                                b,
                                3.0,
                                if r.hovered() {
                                    theme::selection()
                                } else {
                                    Color32::from_black_alpha(170)
                                },
                            );
                            icons::paint(&painter, b.shrink(3.0), icon, Color32::WHITE);
                            if r.clicked() {
                                act = Some(match what {
                                    0 => Act::Import(e.path.clone()),
                                    1 => Act::Preview(e.path.clone()),
                                    _ => Act::Trim(e.path.clone()),
                                });
                            }
                        }
                    }
                    if let Some(a) = row_acts(&mut app.explorer, e, &resp, supported, auto_preview)
                    {
                        act = Some(a);
                    }
                    if resp.double_clicked() && e.is_dir {
                        navigate = Some(e.path.clone());
                    }
                    let path = e.path.clone();
                    let is_dir = e.is_dir;
                    resp.context_menu(|ui| {
                        if let Some(a) = entry_menu(ui, &path, is_dir, supported, favorites) {
                            act = Some(a);
                        }
                    });
                }
            });
        });
    if let Some(d) = navigate {
        app.explorer.go(d);
    }
    act
}

/// A file's or folder's shortcut menu (the thumbnails').
fn entry_menu(
    ui: &mut Ui,
    path: &Path,
    is_dir: bool,
    supported: bool,
    favorites: &[PathBuf],
) -> Option<Act> {
    let mut act = None;
    if is_dir {
        let fav = favorites.iter().any(|f| f == path);
        let (label, on) = if fav {
            (tr("Remove from Favorites"), false)
        } else {
            (tr("Add to Favorites"), true)
        };
        if ui.button(label).clicked() {
            act = Some(Act::Favorite(path.to_path_buf(), on));
            ui.close();
        }
    } else if supported {
        for (label, what) in [
            (tr("Add to Project Media"), 0),
            (tr("Add to Timeline"), 1),
            (tr("Trim"), 2),
            (tr("Start Preview"), 3),
        ] {
            if ui.button(label).clicked() {
                let p = path.to_path_buf();
                act = Some(match what {
                    0 => Act::Import(p),
                    1 => Act::Add(p),
                    2 => Act::Trim(p),
                    _ => Act::Preview(p),
                });
                ui.close();
            }
        }
    }
    // A file rendered with a path to its project.
    if !is_dir
        && crate::archive::keeps_reference(path)
        && ui.button(tr("Edit Source Project")).clicked()
    {
        act = Some(Act::EditSource(path.to_path_buf()));
        ui.close();
    }
    if ui.button(tr("Delete")).clicked() {
        act = Some(Act::Recycle(path.to_path_buf()));
        ui.close();
    }
    ui.separator();
    if ui.button(tr("Explorer…")).clicked() {
        let folder = if is_dir {
            path.to_path_buf()
        } else {
            path.parent().map_or(path.to_path_buf(), Path::to_path_buf)
        };
        act = Some(Act::Reveal(folder));
        ui.close();
    }
    act
}

impl BananaApp {
    /// The Explorer's Delete: asks, then sends `paths` to the Recycle Bin.
    pub(crate) fn ask_recycle(&mut self, paths: Vec<PathBuf>) {
        let text = match paths.as_slice() {
            [] => return,
            [path] => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                trf!("Move \"{}\" to the Recycle Bin?", name)
            }
            _ => trn!(
                paths.len(),
                "Move these {} item to the Recycle Bin?",
                "Move these {} items to the Recycle Bin?",
                paths.len()
            ),
        };
        self.ask_user(crate::ask::Ask {
            title: tr("Delete").to_string(),
            text,
            what: crate::ask::Asked::RecycleFiles(paths),
            check: None,
        });
    }

    /// `paths` to the Recycle Bin, the Explorer read again.
    pub(crate) fn recycle_files(&mut self, paths: &[PathBuf]) {
        let mut error = None;
        for path in paths {
            match bv_platform::to_recycle_bin(path) {
                Ok(()) => {
                    let st = &mut self.explorer;
                    st.picked.retain(|p| p != path);
                    if st.selected.as_ref() == Some(path) {
                        st.selected = None;
                    }
                }
                Err(e) => error = Some(e),
            }
        }
        let st = &mut self.explorer;
        st.loaded = None;
        st.folders.clear();
        if let Some(e) = error {
            self.report_error(trf!("Could not move it to the Recycle Bin: {}", e));
        }
    }
}

/// The selected file's media information (read once).
fn selected_info(app: &mut BananaApp) -> Option<(PathBuf, MediaInfo)> {
    let path = app
        .explorer
        .selected
        .clone()
        .filter(|p| app.explorer.is_file(p))?;
    if let Some(info) = app.explorer.probes.get(&path) {
        return info.clone().map(|i| (path, i));
    }
    let info = app.probe_cached(&path);
    app.explorer.probes.insert(path.clone(), info.clone());
    info.map(|i| (path, i))
}

/// Views › Regions: the selected file's regions, each dragged to the
/// timeline as an event of that part.
fn regions_panel(ui: &mut Ui, app: &mut BananaApp) {
    ui.painter()
        .rect_filled(ui.max_rect(), 2.0, theme::panel_dark());
    ui.label(egui::RichText::new(tr("Regions")).strong());
    let Some((path, info)) = selected_info(app) else {
        ui.label(
            egui::RichText::new(tr("Select a media file to see its regions."))
                .color(theme::text_dim()),
        );
        return;
    };
    let regions: Vec<_> = info
        .markers
        .iter()
        .filter(|m| m.is_region())
        .cloned()
        .collect();
    if regions.is_empty() {
        ui.label(egui::RichText::new(tr("This file has no regions.")).color(theme::text_dim()));
        return;
    }
    let rate = info.video.map_or(app.frame_rate(), |v| v.frame_rate);
    egui::ScrollArea::vertical()
        .id_salt("explorer-regions")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (k, m) in regions.iter().enumerate() {
                let name = if m.name.is_empty() {
                    trf!("Region {}", k + 1)
                } else {
                    m.name.clone()
                };
                let text = format!(
                    "{name}   {} – {}",
                    bv_core::timecode::format(m.time, rate),
                    bv_core::timecode::format(m.end(), rate)
                );
                let r = ui
                    .add(egui::Label::new(text).sense(Sense::click_and_drag()))
                    .on_hover_text(tr("Drag to the timeline: an event of this part"));
                if r.drag_started() {
                    app.media_drag = Some(MediaDrag::FilePart(
                        path.clone(),
                        TimeRange::new(m.time, m.length),
                    ));
                }
            }
        });
}

/// Views › Summary: a short description of the selected file.
fn summary_panel(ui: &mut Ui, app: &mut BananaApp) {
    ui.painter()
        .rect_filled(ui.max_rect(), 2.0, theme::panel_dark());
    ui.label(egui::RichText::new(tr("Summary")).strong());
    let Some((path, info)) = selected_info(app) else {
        ui.label(
            egui::RichText::new(tr("Select a media file to see its summary."))
                .color(theme::text_dim()),
        );
        return;
    };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let rate = info.video.map_or(app.frame_rate(), |v| v.frame_rate);
    let mut lines = vec![
        crate::i18n::media_name(&info),
        trf!(
            "{}, {}",
            human_size(size),
            bv_core::timecode::format(info.duration, rate)
        ),
    ];
    if let Some(v) = info.video {
        lines.push(trf!(
            "Video: {}×{}, {}",
            v.width,
            v.height,
            crate::fmt::fps(v.frame_rate)
        ));
    }
    if let Some(a) = info.audio {
        lines.push(trf!(
            "Audio: {}",
            trn!(
                a.channels,
                "{} Hz, {} channel",
                "{} Hz, {} channels",
                a.sample_rate.0,
                a.channels
            )
        ));
    }
    if let Some(when) = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
    {
        lines.push(trf!("Modified {}", crate::fmt::date_time(when)));
    }
    for l in lines {
        ui.label(egui::RichText::new(l).color(theme::text()));
    }
}

impl BananaApp {
    /// The Explorer on `dir` in Thumbnails with Regions and Summary shown
    /// for `selected`, its files read at once (screen tests).
    pub fn explorer_for_review(&mut self, dir: &Path, selected: &Path) {
        let st = &mut self.explorer;
        st.dir = dir.to_path_buf();
        st.load();
        st.view = View::Thumbnails;
        st.show_regions = true;
        st.show_summary = true;
        st.selected = Some(selected.to_path_buf());
        st.picked = vec![selected.to_path_buf()];
        let files: Vec<PathBuf> = st
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        for f in files {
            let info = bv_engine::Engine::probe_file(&f.to_string_lossy()).ok();
            self.explorer.probes.insert(f, info);
        }
        // Read already: no reading of the folder in the background.
        let (_tx, rx) = std::sync::mpsc::channel();
        self.explorer.probing = Some((dir.to_path_buf(), rx));
        self.show_tab(Tab::Explorer, false);
    }

    /// The Explorer's files all picked, in Details (screen tests).
    pub fn explorer_pick_all_for_review(&mut self) {
        let st = &mut self.explorer;
        st.view = View::Details;
        st.picked = st
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ctrl adds to the picks and takes from them, Shift picks a run from
    /// the last click; actions on one of several take them all.
    #[test]
    fn several_files_are_picked_with_ctrl_and_shift() {
        let mut st = ExplorerState::new();
        st.entries = ["a.wav", "b.wav", "c.txt", "d.wav", "e.wav"]
            .iter()
            .map(|n| Entry {
                path: PathBuf::from("/media").join(n),
                name: (*n).into(),
                is_dir: false,
                size: 0,
                modified: None,
            })
            .collect();
        let p = |n: &str| PathBuf::from("/media").join(n);
        let ctrl = egui::Modifiers::COMMAND;
        st.click(&p("b.wav"), egui::Modifiers::NONE);
        st.click(&p("e.wav"), ctrl);
        assert_eq!(st.picked, [p("b.wav"), p("e.wav")]);
        assert_eq!(st.selected, Some(p("e.wav")));
        st.click(&p("b.wav"), ctrl);
        assert_eq!(st.picked, [p("e.wav")], "Ctrl takes a pick back");
        st.click(&p("e.wav"), egui::Modifiers::NONE);
        st.click(&p("a.wav"), egui::Modifiers::SHIFT);
        assert_eq!(st.picked.len(), 5, "the run from e to a");
        // The text file stays out of what is dragged or added.
        assert_eq!(
            st.targets(&p("d.wav"), false),
            [p("a.wav"), p("b.wav"), p("d.wav"), p("e.wav")]
        );
        assert_eq!(st.targets(&p("c.txt"), true).len(), 5);
        st.click(&p("d.wav"), egui::Modifiers::NONE);
        assert_eq!(st.targets(&p("d.wav"), false), [p("d.wav")]);
        assert_eq!(st.targets(&p("a.wav"), false), [p("a.wav")], "not picked");
    }

    /// The address bar's trail names each folder, the drive by its letter.
    #[test]
    fn the_trail_names_folders_and_drives() {
        assert_eq!(crumb_name(Path::new("/")), "/");
        assert_eq!(crumb_name(Path::new("/home/me/Videos")), "Videos");
        #[cfg(windows)]
        {
            assert_eq!(crumb_name(Path::new(r"C:\")), "C:");
            assert_eq!(crumb_name(Path::new(r"C:\Users\me")), "me");
        }
    }

    #[test]
    fn thumbnails_read_the_folder_and_regions_come_with_the_files() {
        let dir = std::env::temp_dir().join(format!("bv-explorer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        crate::tabs::trimmer::tests_wav(&wav, 4);
        let s = Tick::from_seconds;
        let marks = vec![bv_timeline::MediaMarker {
            time: s(1),
            length: s(2),
            name: "Chorus".into(),
        }];
        bv_engine::save_markers(&wav.to_string_lossy(), &marks).unwrap();
        let mut st = ExplorerState::new();
        st.dir = dir.clone();
        st.load();
        st.view = View::Thumbnails;
        // Read in the background: wait for it.
        let started = std::time::Instant::now();
        while !st.probes.contains_key(&wav) && started.elapsed().as_secs() < 30 {
            st.read_files();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let info = st.probes.get(&wav).cloned().flatten().expect("read");
        let regions: Vec<_> = info.markers.iter().filter(|m| m.is_region()).collect();
        assert_eq!(regions.len(), 1);
        assert_eq!((regions[0].time, regions[0].length), (s(1), s(2)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
