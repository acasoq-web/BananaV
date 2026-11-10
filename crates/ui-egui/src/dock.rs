//! Dockable windows: tabs can be dragged, regrouped, closed and reopened
//! from View › Window. Layouts can be saved and recalled (Alt+D). The
//! timeline is one of the layout's windows, fixed in its place as VEGAS's
//! (no tab, never closed or floated): the others dock around it. As VEGAS's
//! docking areas: F11 hides and shows every window docked around the
//! timeline, Alt+F11, Ctrl+Alt+F11 and Ctrl+Shift+F11 those above, below
//! and to its right (the layout kept to bring them back as they were),
//! and each window's tab bar has Maximize and Restore.

use crate::i18n::tr;
use egui_dock::{DockArea, DockState, Node, NodeIndex, Style, TabStyle, TabViewer};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::app::actions::LayoutPreset;
use crate::egui::{
    Color32, CornerRadius, Id, Margin, Rect, Sense, Stroke, StrokeKind, Ui, WidgetText, vec2,
};
use crate::tabs;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Explorer,
    ProjectMedia,
    Generators,
    Transitions,
    VideoFx,
    ColorGrading,
    Preview,
    Trimmer,
    Properties,
    Master,
    Notes,
    PluginManager,
    Scopes,
    /// View › Window › Surround Panner (5.1 projects).
    SurroundPanner,
    EditDetails,
    MixingConsole,
    LoudnessMeters,
    /// The compositor plug-ins (a track's Custom compositing mode).
    Compositors,
    /// The Transform window: the controls of the event the Video Preview's
    /// Transform mode works on (`tabs::transform`).
    Transform,
    /// The timeline: fixed in its place (no tab, never closed or floated),
    /// the other windows docked around it, as VEGAS's.
    Timeline,
}

impl Tab {
    pub const ALL: [Tab; 19] = [
        Tab::Explorer,
        Tab::ProjectMedia,
        Tab::Generators,
        Tab::Transitions,
        Tab::VideoFx,
        Tab::ColorGrading,
        Tab::Preview,
        Tab::Trimmer,
        Tab::Properties,
        Tab::Master,
        Tab::Notes,
        Tab::PluginManager,
        Tab::Scopes,
        Tab::SurroundPanner,
        Tab::EditDetails,
        Tab::MixingConsole,
        Tab::LoudnessMeters,
        Tab::Compositors,
        Tab::Transform,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Explorer => tr("Explorer"),
            Tab::ProjectMedia => tr("Project Media"),
            Tab::Generators => tr("Media Generators"),
            Tab::Transitions => tr("Transitions"),
            Tab::VideoFx => tr("Video FX"),
            Tab::ColorGrading => tr("Color Grading"),
            Tab::Preview => tr("Video Preview"),
            Tab::Trimmer => tr("Trimmer"),
            Tab::Properties => tr("Properties"),
            Tab::Master => tr("Master Bus"),
            Tab::Notes => tr("Project Notes"),
            Tab::PluginManager => tr("Plug-In Manager"),
            Tab::Scopes => tr("Video Scopes"),
            Tab::SurroundPanner => tr("Surround Panner"),
            Tab::EditDetails => tr("Edit Details"),
            Tab::MixingConsole => tr("Mixing Console"),
            Tab::LoudnessMeters => tr("Loudness Meters"),
            Tab::Compositors => tr("Compositors"),
            Tab::Transform => tr("Transform"),
            Tab::Timeline => tr("Timeline"),
        }
    }

    /// The name on the window's tab: VEGAS calls the generators' window
    /// "Media Generator" there (and "Media Generators" in View › Window).
    pub fn tab_label(self) -> &'static str {
        match self {
            Tab::Generators => tr("Media Generator"),
            t => t.title(),
        }
    }
}

/// The docking area's share of the main window above (or below) the
/// timeline in VEGAS's default layout (measured on its screenshot: 598 of
/// 944 points).
pub const DOCK_SHARE: f32 = 0.633;

/// Default layout, as VEGAS's: the docking area above the timeline — the
/// browsers on the left half (the Explorer open), the Video Preview and
/// the Trimmer beside them, the Master Bus at the right edge.
pub fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![
        Tab::Explorer,
        Tab::ProjectMedia,
        Tab::Generators,
        Tab::Transitions,
        Tab::VideoFx,
        Tab::ColorGrading,
    ]);
    let surface = dock.main_surface_mut();
    let [top, _] = surface.split_below(NodeIndex::root(), DOCK_SHARE, vec![Tab::Timeline]);
    let [_, right] = surface.split_right(top, 0.489, vec![Tab::Preview, Tab::Trimmer]);
    surface.split_right(right, 0.865, vec![Tab::Master]);
    focus(&mut dock, &[Tab::Explorer, Tab::Preview]);
    dock
}

/// A side of the timeline where windows dock: VEGAS's docking areas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Top,
    Bottom,
    Right,
    Left,
}

impl Area {
    pub const ALL: [Area; 4] = [Area::Top, Area::Bottom, Area::Right, Area::Left];

    /// Whether a window at `r` is on this side of the timeline at `t`.
    fn holds(self, r: Rect, t: Rect) -> bool {
        match self {
            Area::Top => r.bottom() <= t.top() + 1.0,
            Area::Bottom => r.top() >= t.bottom() - 1.0,
            Area::Right => r.left() >= t.right() - 1.0,
            Area::Left => r.right() <= t.left() + 1.0,
        }
    }
}

/// The docking areas hidden by F11 and its kin: the layout as it was, and
/// the sides hidden since.
#[derive(Clone, Debug)]
pub struct Hidden {
    full: DockState<Tab>,
    areas: Vec<Area>,
}

impl Hidden {
    /// The layout as it was before its areas were hidden.
    pub fn full(&self) -> &DockState<Tab> {
        &self.full
    }
}

/// A docked window maximized: its splits' shares as they were.
#[derive(Clone, Debug)]
pub struct Maximized {
    pub tab: Tab,
    shares: Vec<(usize, f32)>,
}

/// What a key or a tab bar's button asks of the layout (done where it is
/// drawn).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockRequest {
    /// One docking area hidden, or shown again.
    Area(Area),
    /// Every one (F11).
    All,
    Maximize(Tab),
    Restore,
    /// A window taken out into a window of the system's own (it goes to
    /// another monitor), and back into the layout.
    Float(Tab),
    Dock(Tab),
}

/// `area`'s windows (on that side of the timeline, as last drawn) taken
/// out of `dock`; the timeline takes their room.
pub fn hide_area(dock: &mut DockState<Tab>, area: Area) {
    let Some(t) = timeline_rect(dock) else {
        return;
    };
    for _ in 0..64 {
        let tree = dock.main_surface();
        let found = tree.iter().enumerate().find_map(|(i, n)| {
            let tabs = n.tabs()?;
            let r = n.rect()?;
            (!tabs.contains(&Tab::Timeline) && area.holds(r, t)).then_some(NodeIndex(i))
        });
        match found {
            Some(n) => dock.main_surface_mut().remove_leaf(n),
            None => break,
        }
    }
}

/// Alt+F11 and its kin: `area`'s windows hidden, or brought back as they
/// were (the other areas hidden stay so).
pub fn toggle_area(dock: &mut DockState<Tab>, hidden: &mut Option<Hidden>, area: Area) {
    match hidden {
        Some(h) if h.areas.contains(&area) => {
            h.areas.retain(|a| *a != area);
            *dock = h.full.clone();
            for a in h.areas.clone() {
                hide_area(dock, a);
            }
            if h.areas.is_empty() {
                *hidden = None;
            }
        }
        Some(h) => {
            h.areas.push(area);
            hide_area(dock, area);
        }
        None => {
            *hidden = Some(Hidden {
                full: dock.clone(),
                areas: vec![area],
            });
            hide_area(dock, area);
        }
    }
}

/// F11: every docking area hidden, or all of them brought back.
pub fn toggle_all(dock: &mut DockState<Tab>, hidden: &mut Option<Hidden>) {
    match hidden.take() {
        Some(h) => *dock = h.full,
        None => {
            *hidden = Some(Hidden {
                full: dock.clone(),
                areas: Area::ALL.to_vec(),
            });
            for a in Area::ALL {
                hide_area(dock, a);
            }
        }
    }
}

/// The share a maximized window's side of each split takes.
const MAXIMIZED_SHARE: f32 = 0.96;

/// `tab`'s window maximized: each split above it gives its side almost
/// all of its room (the timeline's too); `None` when it is not docked.
pub fn maximize(dock: &mut DockState<Tab>, tab: Tab) -> Option<Maximized> {
    let tree = dock.main_surface_mut();
    let (node, _) = tree.find_tab(&tab)?;
    let shares = tree
        .iter()
        .enumerate()
        .filter_map(|(i, n)| match n {
            Node::Vertical(s) | Node::Horizontal(s) => Some((i, s.fraction)),
            _ => None,
        })
        .collect();
    let mut n = node;
    while let Some(p) = n.parent() {
        if let Node::Vertical(s) | Node::Horizontal(s) = &mut tree[p] {
            s.fraction = if n.is_left() {
                MAXIMIZED_SHARE
            } else {
                1.0 - MAXIMIZED_SHARE
            };
        }
        n = p;
    }
    Some(Maximized { tab, shares })
}

/// The splits' shares back as they were before `m`.
pub fn restore(dock: &mut DockState<Tab>, m: &Maximized) {
    let tree = dock.main_surface_mut();
    for &(i, f) in &m.shares {
        if i < tree.len()
            && let Node::Vertical(s) | Node::Horizontal(s) = &mut tree[NodeIndex(i)]
        {
            s.fraction = f;
        }
    }
}

/// Does what `req` asks of `dock`.
fn apply(dock: &mut DockState<Tab>, app: &mut BananaApp, req: DockRequest) {
    // A maximized window is restored first: the areas count as they were.
    if let Some(m) = app.dock_maximized.take() {
        restore(dock, &m);
        if req == DockRequest::Restore {
            return;
        }
    }
    match req {
        DockRequest::Area(a) => toggle_area(dock, &mut app.docks_hidden, a),
        DockRequest::All => toggle_all(dock, &mut app.docks_hidden),
        DockRequest::Maximize(tab) => app.dock_maximized = maximize(dock, tab),
        DockRequest::Restore => {}
        DockRequest::Float(tab) => {
            if tab != Tab::Timeline
                && let Some(path) = dock.find_tab(&tab)
            {
                dock.remove_tab(path);
                if !app.floating_panels.contains(&tab) {
                    app.floating_panels.push(tab);
                }
            }
        }
        DockRequest::Dock(tab) => {
            app.floating_panels.retain(|t| *t != tab);
            if dock.find_tab(&tab).is_none() {
                open_tab(dock, tab);
            }
        }
    }
}

impl BananaApp {
    /// The window titled `title` floating in a window of its own (screen
    /// tests).
    pub fn float_for_review(&mut self, title: &str) {
        if let Some(tab) = Tab::ALL.into_iter().find(|t| t.title() == title) {
            self.dock_request = Some(DockRequest::Float(tab));
        }
    }
}

/// The windows taken out of the layout into windows of the system's own
/// (the tab's menu: Float in Its Own Window): each one resizable and
/// movable to any monitor, with Dock to put it back where windows open;
/// closing it closes the window, as a docked tab's close button does.
pub fn floating_windows(ctx: &crate::egui::Context, app: &mut BananaApp) {
    for tab in app.floating_panels.clone() {
        let mut open = true;
        let mut dock = false;
        crate::floating::Window::new(tab.tab_label())
            .id(Id::new(("floating-panel", tab)))
            .resizable(true)
            .default_size(vec2(560.0, 420.0))
            .min_size(vec2(240.0, 160.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Dock"))
                        .on_hover_text(tr("Puts this window back into the layout."))
                        .clicked()
                    {
                        dock = true;
                    }
                });
                ui.separator();
                let mut t = tab;
                Viewer { app: &mut *app }.ui(ui, &mut t);
            });
        if dock {
            app.dock_request = Some(DockRequest::Dock(tab));
        } else if !open {
            app.floating_panels.retain(|t| *t != tab);
        }
    }
}

/// Puts the timeline in a layout without it (saved before it was one of
/// the windows): below the docking area, or above it with `top`, with the
/// room of VEGAS's default layout.
pub fn ensure_timeline(dock: &mut DockState<Tab>, top: bool) {
    if dock.find_tab(&Tab::Timeline).is_some() {
        return;
    }
    let surface = dock.main_surface_mut();
    if surface.is_empty() {
        surface.push_to_first_leaf(Tab::Timeline);
    } else if top {
        // Above, the share given is the new pane's (egui_dock's).
        surface.split_above(NodeIndex::root(), 1.0 - DOCK_SHARE, vec![Tab::Timeline]);
    } else {
        surface.split_below(NodeIndex::root(), DOCK_SHARE, vec![Tab::Timeline]);
    }
}

/// Where the timeline's window is, as last drawn.
pub fn timeline_rect(dock: &DockState<Tab>) -> Option<Rect> {
    let tree = dock.main_surface();
    let (node, _) = tree.find_tab(&Tab::Timeline)?;
    tree[node].rect()
}

/// Opens `tab` in the first window that is not the timeline's (its pane
/// takes no other), else above the timeline.
pub fn open_tab(dock: &mut DockState<Tab>, tab: Tab) {
    let tree = dock.main_surface_mut();
    let pane = tree.iter().enumerate().find_map(|(i, node)| {
        let tabs = node.tabs()?;
        (!tabs.contains(&Tab::Timeline)).then_some(NodeIndex(i))
    });
    match pane {
        Some(pane) => {
            tree[pane].append_tab(tab);
            tree.set_focused_node(pane);
        }
        None => match tree.find_tab(&Tab::Timeline) {
            Some((node, _)) => {
                tree.split_above(node, 0.6, vec![tab]);
            }
            None => tree.push_to_first_leaf(tab),
        },
    }
}

/// `tab` opened in the window of the Explorer and the other browsers
/// (Project Media, Media Generators, Transitions, Video FX, Color Grading):
/// a tab beside theirs, wherever they are; where none is, as
/// [`open_tab`].
pub fn open_beside_browsers(dock: &mut DockState<Tab>, tab: Tab) {
    const BROWSERS: [Tab; 6] = [
        Tab::Explorer,
        Tab::ProjectMedia,
        Tab::Generators,
        Tab::Transitions,
        Tab::VideoFx,
        Tab::ColorGrading,
    ];
    let place = BROWSERS.iter().find_map(|b| dock.find_tab(b));
    match place {
        Some(path) => {
            if let Some(tree) = dock
                .get_surface_mut(path.surface)
                .and_then(|s| s.node_tree_mut())
            {
                tree[path.node].append_tab(tab);
                tree.set_focused_node(path.node);
            }
            if let Some(path) = dock.find_tab(&tab) {
                let _ = dock.set_active_tab(path);
            }
        }
        None => open_tab(dock, tab),
    }
}

fn focus(dock: &mut DockState<Tab>, tabs: &[Tab]) {
    for tab in tabs {
        if let Some(path) = dock.find_tab(tab) {
            let _ = dock.set_active_tab(path);
        }
    }
}

/// The built-in layouts of View › Window Layouts, as VEGAS 23's
/// screenshots show them (the windows, their order, the open one and the
/// room each takes, measured on 1920 × 1080 ones).
pub fn preset_layout(preset: LayoutPreset) -> DockState<Tab> {
    match preset {
        LayoutPreset::Default => default_layout(),
        // The Mixing Console open in a wide window with the browsers, the
        // Video Preview beside it; a taller timeline.
        LayoutPreset::AudioMixing => {
            let mut dock = DockState::new(vec![
                Tab::MixingConsole,
                Tab::Explorer,
                Tab::ProjectMedia,
                Tab::Transitions,
                Tab::VideoFx,
                Tab::Generators,
            ]);
            let surface = dock.main_surface_mut();
            let [top, _] = surface.split_below(NodeIndex::root(), 0.49, vec![Tab::Timeline]);
            surface.split_right(top, 0.699, vec![Tab::Preview]);
            focus(&mut dock, &[Tab::MixingConsole]);
            dock
        }
        // The timeline at the top left over Color Grading; the Video
        // Preview over the Video Scopes down the right side.
        LayoutPreset::ColorCorrection => {
            let mut dock = DockState::new(vec![Tab::Timeline]);
            let surface = dock.main_surface_mut();
            let [left, right] = surface.split_right(NodeIndex::root(), 0.716, vec![Tab::Preview]);
            surface.split_below(left, 0.656, vec![Tab::ColorGrading]);
            surface.split_below(right, 0.442, vec![Tab::Scopes]);
            dock
        }
        // The browsers, the Trimmer, the Video Preview and the Master Bus
        // side by side above the timeline.
        LayoutPreset::SourceWindow => {
            let mut dock = DockState::new(vec![
                Tab::Explorer,
                Tab::ProjectMedia,
                Tab::VideoFx,
                Tab::Generators,
                Tab::Transitions,
                Tab::Notes,
            ]);
            let surface = dock.main_surface_mut();
            let [top, _] = surface.split_below(NodeIndex::root(), 0.597, vec![Tab::Timeline]);
            let [_, rest] = surface.split_right(top, 0.340, vec![Tab::Trimmer]);
            let [_, right] = surface.split_right(rest, 0.454, vec![Tab::Preview]);
            surface.split_right(right, 0.8, vec![Tab::Master]);
            focus(&mut dock, &[Tab::Explorer]);
            dock
        }
        // The timeline on top, the docking area below it.
        LayoutPreset::TimelineTop => {
            let mut dock = DockState::new(vec![
                Tab::Explorer,
                Tab::ProjectMedia,
                Tab::Generators,
                Tab::VideoFx,
                Tab::Transitions,
                Tab::ColorGrading,
            ]);
            let surface = dock.main_surface_mut();
            // Above, the share given is the new pane's (egui_dock's).
            let [bottom, _] = surface.split_above(NodeIndex::root(), 0.347, vec![Tab::Timeline]);
            let [_, right] = surface.split_right(bottom, 0.477, vec![Tab::Preview, Tab::Trimmer]);
            surface.split_right(right, 0.87, vec![Tab::Master]);
            focus(&mut dock, &[Tab::Explorer, Tab::Preview]);
            dock
        }
    }
}

struct Viewer<'a> {
    app: &'a mut BananaApp,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> Id {
        Id::new(("aba", *tab))
    }

    fn title(&mut self, tab: &mut Tab) -> WidgetText {
        tab.tab_label().into()
    }

    fn leaf_buttons_width(&self) -> f32 {
        18.0
    }

    /// Maximize, or Restore on the window maximized.
    fn leaf_buttons(&mut self, ui: &mut Ui, rect: Rect, tabs: &mut [Tab], active: usize) {
        let maxed = self
            .app
            .dock_maximized
            .as_ref()
            .is_some_and(|m| tabs.contains(&m.tab));
        let r = Rect::from_center_size(rect.center(), vec2(16.0, 16.0));
        let id = Id::new(("dock-maximizar", tabs.first().copied()));
        let resp = ui.interact(r, id, Sense::click());
        let color = if resp.hovered() {
            theme::text_strong()
        } else {
            theme::text_dim()
        };
        if resp.hovered() {
            ui.painter().rect_filled(r, 2.0, theme::hover());
        }
        let st = Stroke::new(1.0, color);
        let p = ui.painter();
        if maxed {
            // Restore: two windows, one behind the other.
            let back = Rect::from_min_size(r.min + vec2(6.0, 4.0), vec2(8.0, 7.0));
            let front = Rect::from_min_size(r.min + vec2(3.5, 7.0), vec2(8.0, 7.0));
            p.rect_stroke(back, 0.0, st, StrokeKind::Middle);
            p.rect_filled(front, 0.0, theme::panel_dark());
            p.rect_stroke(front, 0.0, st, StrokeKind::Middle);
        } else {
            // Maximize: a window, its title bar thick.
            let w = Rect::from_center_size(r.center(), vec2(10.0, 9.0));
            p.rect_stroke(w, 0.0, st, StrokeKind::Middle);
            p.hline(w.x_range(), w.top() + 1.0, Stroke::new(2.0, color));
        }
        let tip = if maxed { tr("Restore") } else { tr("Maximize") };
        if resp.on_hover_text(tip).clicked() {
            self.app.dock_request = Some(if maxed {
                DockRequest::Restore
            } else {
                DockRequest::Maximize(tabs[active.min(tabs.len().saturating_sub(1))])
            });
        }
    }

    fn fixed(&self, tab: &Tab) -> bool {
        *tab == Tab::Timeline
    }

    fn is_closeable(&self, tab: &Tab) -> bool {
        *tab != Tab::Timeline
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        *tab != Tab::Timeline
    }

    /// The tab's menu (with Close and Undock): a window of the system's
    /// own, which goes to another monitor.
    fn context_menu(&mut self, ui: &mut Ui, tab: &mut Tab, _path: egui_dock::NodePath) {
        if *tab != Tab::Timeline && ui.button(tr("Float in Its Own Window")).clicked() {
            self.app.dock_request = Some(DockRequest::Float(*tab));
            ui.close();
        }
    }

    /// The timeline fills its window: no margin, the timeline's color. The
    /// Video Scopes' black reaches their window's sides, as VEGAS's.
    fn tab_style_override(&self, tab: &Tab, global: &TabStyle) -> Option<TabStyle> {
        match tab {
            Tab::Timeline => {
                let mut style = global.clone();
                style.tab_body.inner_margin = Margin::ZERO;
                style.tab_body.bg_fill = theme::bg();
                Some(style)
            }
            Tab::Scopes => {
                let mut style = global.clone();
                style.tab_body.inner_margin = Margin::same(2);
                Some(style)
            }
            _ => None,
        }
    }

    /// The windows laid out over their whole pane scroll only their parts
    /// (lists, trees), never as a whole: a pane a fraction of a point
    /// narrower than they measured would otherwise get scroll bars.
    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        let whole = matches!(
            tab,
            Tab::Explorer
                | Tab::ProjectMedia
                | Tab::Generators
                | Tab::Transitions
                | Tab::VideoFx
                | Tab::Compositors
                | Tab::Preview
                | Tab::Trimmer
                | Tab::Master
                | Tab::Timeline
                | Tab::Scopes
                | Tab::SurroundPanner
        );
        [!whole; 2]
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Tab) {
        let app = &mut *self.app;
        // A click in a window gives it the keys: windows with their own
        // keyboard context (Explorer, Trimmer, Mixer, Edit Details, Mixing
        // Console) take them; the others leave them to the timeline.
        let pressed = ui.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if pressed.is_some_and(|p| {
            ui.max_rect().contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
        }) {
            use crate::keymap::Context;
            if *tab != Tab::Timeline {
                app.timeline.marker_tool.active = false;
            }
            app.keys_tab = Some(*tab);
            app.key_focus = match tab {
                Tab::Explorer => Context::Explorer,
                Tab::Trimmer => Context::Trimmer,
                Tab::Master => Context::MixingDesk,
                Tab::EditDetails => Context::EditDetails,
                Tab::MixingConsole => Context::MixingConsole,
                _ => Context::TrackView,
            };
        }
        match tab {
            Tab::Explorer => tabs::explorer::ui(ui, app),
            Tab::ProjectMedia => tabs::media::ui(ui, app),
            Tab::Generators => tabs::generators::ui(ui, app),
            Tab::Transitions => tabs::transitions::ui(ui, app),
            Tab::VideoFx => tabs::effects::ui(ui, app),
            Tab::ColorGrading => tabs::color::ui(ui, app),
            Tab::Preview => tabs::preview::ui(ui, app),
            Tab::Trimmer => tabs::trimmer::ui(ui, app),
            Tab::Properties => tabs::properties::ui(ui, app),
            Tab::Master => tabs::master::ui(ui, app),
            Tab::Notes => tabs::notes::ui(ui, app),
            Tab::PluginManager => tabs::plugins::ui(ui, app),
            Tab::Scopes => tabs::scopes::ui(ui, app),
            Tab::SurroundPanner => tabs::surround::ui(ui, app),
            Tab::Transform => tabs::transform::ui(ui, app),
            Tab::EditDetails => tabs::details::ui(ui, app),
            Tab::MixingConsole => tabs::console::ui(ui, app),
            Tab::LoudnessMeters => tabs::loudness::ui(ui, app),
            Tab::Compositors => tabs::compositors::ui(ui, app),
            // While it draws, `app.timeline` is a stand-in.
            Tab::Timeline => {
                let mut timeline = std::mem::take(&mut app.timeline);
                timeline.ui(ui, app);
                app.timeline = timeline;
            }
        }
        // Something dragged (media, an effect, a transition…) over a
        // window that takes none: the arrow saying it can't go there. Files
        // dragged in from the system have the system's pointer.
        if app.media_drag.is_some()
            && app.os_drag.is_none()
            && !takes_drops(*tab)
            && ui.rect_contains_pointer(ui.max_rect())
        {
            crate::cursors::set(ui.ctx(), crate::cursors::Cursor::ArrowNo);
        }
    }
}

/// Whether a window takes something dragged onto it: the timeline (media,
/// effects, transitions…) and Project Media (media into its bins and
/// storyboards, which say where themselves); the others take nothing.
fn takes_drops(tab: Tab) -> bool {
    matches!(tab, Tab::Timeline | Tab::ProjectMedia)
}

/// The Master Bus's width, in points: VEGAS's narrow strip at the right.
pub(crate) const MASTER_WIDTH: f32 = 114.0;

/// The split the Master Bus is the right side of, when it is alone in its
/// window there.
fn master_split(dock: &DockState<Tab>) -> Option<NodeIndex> {
    let tree = dock.main_surface();
    let (leaf, _) = tree.find_tab(&Tab::Master)?;
    let parent = leaf.parent()?;
    let alone = tree[leaf].get_leaf().is_some_and(|l| l.tabs.len() == 1);
    (alone && leaf.is_right() && matches!(tree[parent], Node::Horizontal(_))).then_some(parent)
}

/// The Master Bus keeps its width in points whatever the main window's:
/// the windows left of it take the difference. Returns its split's width,
/// to tell afterwards a main window resized from an edge dragged.
fn keep_master_width(dock: &mut DockState<Tab>, width: f32, separator: f32) -> Option<f32> {
    let split = master_split(dock)?;
    let Node::Horizontal(s) = &mut dock.main_surface_mut()[split] else {
        return None;
    };
    let w = s.rect.width();
    if w.is_finite() && w > width * 2.0 {
        s.fraction = (1.0 - (width + separator / 2.0) / w).clamp(0.05, 0.98);
    }
    Some(w)
}

/// The Master Bus's width now, and its split's.
fn master_width(dock: &DockState<Tab>) -> Option<(f32, f32)> {
    let split = master_split(dock)?;
    let tree = dock.main_surface();
    let master = tree[split.right()].rect()?.width();
    let whole = tree[split].rect()?.width();
    (master > 0.0).then_some((master, whole))
}

pub fn show(ui: &mut Ui, dock: &mut DockState<Tab>, app: &mut BananaApp) {
    let prefs = &app.settings.prefs;
    // Options › Preferences › Display: the tabs below the windows unless
    // "Position tabs at top of docked windows"; a window dragged out of a
    // floating one docks with "Allow floating windows to dock", Ctrl
    // turning it around.
    let at_bottom = !prefs.tabs_at_top;
    let floating_dock = prefs.floating_dock != ui.input(|i| i.modifiers.command);
    let style = style(ui, at_bottom);
    // Two strips side by side with the Preview fader.
    let strips = if app.settings.preview_fader { 2.0 } else { 1.0 };
    let target = app.master_width.unwrap_or(MASTER_WIDTH) * strips;
    let before = keep_master_width(dock, target, style.separator.width);
    DockArea::new(dock)
        .style(style)
        .show_close_buttons(true)
        .show_leaf_close_all_buttons(false)
        .show_leaf_collapse_buttons(false)
        .floating_tabs_dock(floating_dock)
        .show_inside(ui, &mut Viewer { app });
    // Its edge dragged (not the main window resized): the new width stays.
    if let (Some((now, whole)), Some(before)) = (master_width(dock), before)
        && (whole - before).abs() < 0.5
        && (now - target).abs() > 1.0
    {
        app.master_width = Some(now / strips);
    }
    if let Some(req) = app.dock_request.take() {
        apply(dock, app, req);
    }
    // A layout recalled with a window that floats on its own: the layout's.
    app.floating_panels.retain(|t| dock.find_tab(t).is_none());
    let menu = &mut dock.translations.tab_context_menu;
    menu.close_button = tr("Close").into();
    menu.eject_button = tr("Undock").into();
}

/// VEGAS's tab bars: flat, on the dark bars between the windows; the open
/// tab's name bright and bold, marked by a line on its window's side, with
/// its close button; the others dim.
fn style(ui: &Ui, at_bottom: bool) -> Style {
    let mut style = Style::from_egui(ui.style());
    let bar = theme::panel_dark();
    style.tab_bar.at_bottom = at_bottom;
    style.tab_bar.shrink_titles_on_overflow = true;
    style.tab_bar.bg_fill = bar;
    style.tab_bar.hline_color = bar;
    let t = &mut style.tab;
    for s in [
        &mut t.active,
        &mut t.inactive,
        &mut t.focused,
        &mut t.hovered,
        &mut t.inactive_with_kb_focus,
        &mut t.active_with_kb_focus,
        &mut t.focused_with_kb_focus,
    ] {
        s.bg_fill = bar;
        s.outline_color = Color32::TRANSPARENT;
        s.corner_radius = CornerRadius::ZERO;
        s.text_color = theme::text_strong();
    }
    t.inactive.text_color = theme::text_dim();
    t.inactive_with_kb_focus.text_color = theme::text_dim();
    t.hovered.text_color = theme::text();
    t.active_strong = true;
    t.active_line = Stroke::new(1.5, theme::text_strong());
    t.close_on_active_only = true;
    t.spacing = 10.0;
    t.tab_body.bg_fill = theme::panel();
    t.tab_body.stroke = Stroke::NONE;
    style.buttons.close_tab_color = theme::text_dim();
    style.buttons.close_tab_active_color = theme::text_strong();
    style.buttons.close_tab_bg_fill = theme::hover();
    style.separator.color_idle = bar;
    style.separator.color_hovered = theme::banana_dim();
    style.separator.width = 4.0;
    // A tab dragged, as Premiere's and Resolve's windows dock: the room it
    // would take lit up as the pointer moves (the middle of a window: a tab
    // beside its own; a side: a window of its own there), the window under
    // the pointer outlined.
    let o = &mut style.overlay;
    o.overlay_type = egui_dock::OverlayType::HighlightedAreas;
    o.selection_color = theme::selection().gamma_multiply(0.45);
    o.selection_stroke_width = 2.0;
    o.hovered_leaf_highlight.color = theme::selection().gamma_multiply(0.12);
    o.hovered_leaf_highlight.stroke = Stroke::new(1.5, theme::selection());
    o.hovered_leaf_highlight.expansion = 0.0;
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Float in Its Own Window takes a window out of the layout; Dock puts
    /// it back; the timeline never floats.
    #[test]
    fn a_window_floats_on_its_own_and_docks_back() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        let mut dock = default_layout();
        assert!(dock.find_tab(&Tab::Explorer).is_some());
        apply(&mut dock, &mut app, DockRequest::Float(Tab::Explorer));
        assert!(dock.find_tab(&Tab::Explorer).is_none());
        assert_eq!(app.floating_panels, vec![Tab::Explorer]);
        apply(&mut dock, &mut app, DockRequest::Dock(Tab::Explorer));
        assert!(dock.find_tab(&Tab::Explorer).is_some());
        assert!(app.floating_panels.is_empty());
        apply(&mut dock, &mut app, DockRequest::Float(Tab::Timeline));
        assert!(dock.find_tab(&Tab::Timeline).is_some());
        assert!(app.floating_panels.is_empty());
    }
    use crate::egui::{self, pos2, vec2};
    use egui_kittest::Harness;

    /// The default layout drawn once (its windows' places known).
    fn drawn() -> DockState<Tab> {
        struct Tabs;
        impl TabViewer for Tabs {
            type Tab = Tab;
            fn id(&mut self, tab: &mut Tab) -> Id {
                Id::new(*tab)
            }
            fn title(&mut self, tab: &mut Tab) -> WidgetText {
                tab.title().into()
            }
            fn ui(&mut self, _ui: &mut Ui, _tab: &mut Tab) {}
            fn fixed(&self, tab: &Tab) -> bool {
                *tab == Tab::Timeline
            }
        }
        let mut h = Harness::builder()
            .with_size(vec2(1400.0, 900.0))
            .build_ui_state(
                |ui, dock: &mut DockState<Tab>| {
                    DockArea::new(dock).show_inside(ui, &mut Tabs);
                },
                default_layout(),
            );
        h.run_steps(2);
        h.state().clone()
    }

    fn tabs(dock: &DockState<Tab>) -> Vec<Tab> {
        dock.main_surface()
            .iter()
            .filter_map(|n| n.tabs())
            .flatten()
            .copied()
            .collect()
    }

    /// F11's kin: the windows above the timeline hidden and brought back
    /// as they were, nothing below it in the default layout, every area
    /// with F11; a window maximized gets almost all the room, and is
    /// restored.
    #[test]
    fn docking_areas_hide_and_windows_maximize() {
        let full = drawn();
        let mut dock = full.clone();
        let mut hidden = None;
        toggle_area(&mut dock, &mut hidden, Area::Top);
        assert_eq!(tabs(&dock), vec![Tab::Timeline]);
        toggle_area(&mut dock, &mut hidden, Area::Top);
        assert!(hidden.is_none());
        assert_eq!(tabs(&dock), tabs(&full));
        toggle_area(&mut dock, &mut hidden, Area::Bottom);
        assert_eq!(tabs(&dock), tabs(&full), "nothing docks below it");
        toggle_area(&mut dock, &mut hidden, Area::Bottom);
        toggle_all(&mut dock, &mut hidden);
        assert_eq!(tabs(&dock), vec![Tab::Timeline]);
        toggle_all(&mut dock, &mut hidden);
        assert_eq!(tabs(&dock), tabs(&full));
        // The Video Preview maximized: each split above it gives it almost
        // all; restored, as it was.
        let m = maximize(&mut dock, Tab::Preview).expect("docked");
        let tree = dock.main_surface();
        let (node, _) = tree.find_tab(&Tab::Preview).unwrap();
        let mut n = node;
        while let Some(p) = n.parent() {
            if let Node::Vertical(s) | Node::Horizontal(s) = &tree[p] {
                let share = if n.is_left() {
                    s.fraction
                } else {
                    1.0 - s.fraction
                };
                assert!(share > 0.9, "{share}");
            }
            n = p;
        }
        restore(&mut dock, &m);
        let shares = |d: &DockState<Tab>| -> Vec<f32> {
            d.main_surface()
                .iter()
                .filter_map(|n| match n {
                    Node::Vertical(s) | Node::Horizontal(s) => Some(s.fraction),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(shares(&dock), shares(&full));
    }

    /// "Display timeline at bottom of main window": the timeline taken out
    /// comes back above the docking area, or below it.
    #[test]
    fn the_timeline_goes_above_or_below_the_docking_area() {
        let mut dock = default_layout();
        for (top, child) in [(true, NodeIndex(1)), (false, NodeIndex(2))] {
            let at = dock.find_tab(&Tab::Timeline).unwrap();
            dock.remove_tab(at);
            ensure_timeline(&mut dock, top);
            let (node, _) = dock.main_surface().find_tab(&Tab::Timeline).unwrap();
            assert_eq!(node, child, "top: {top}");
            // The docking area's windows are all still there.
            assert!(dock.find_tab(&Tab::Preview).is_some());
            assert!(dock.find_tab(&Tab::Master).is_some());
        }
    }

    struct Plain;

    impl TabViewer for Plain {
        type Tab = &'static str;

        fn id(&mut self, tab: &mut &'static str) -> Id {
            Id::new(*tab)
        }

        fn title(&mut self, tab: &mut &'static str) -> WidgetText {
            (*tab).into()
        }

        fn ui(&mut self, ui: &mut Ui, tab: &mut &'static str) {
            ui.label(*tab);
        }
    }

    /// Drags a floating window's tab onto the middle of the docked window,
    /// Ctrl held or not; whether it docked there.
    fn drop_floating_tab(floating_dock: bool, ctrl: bool) -> bool {
        let mut dock = DockState::new(vec!["Docked"]);
        let window = dock.add_window(vec!["Floating"]);
        dock.get_window_state_mut(window)
            .unwrap()
            .set_position(pos2(40.0, 40.0))
            .set_size(vec2(220.0, 160.0));
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 640.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                move |ui, dock: &mut DockState<&'static str>| {
                    let ctrl = ui.input(|i| i.modifiers.command);
                    DockArea::new(dock)
                        .floating_tabs_dock(floating_dock != ctrl)
                        .show_inside(ui, &mut Plain);
                },
                dock,
            );
        h.run_steps(3);
        let bar = h.state()[window][NodeIndex::root()].rect().unwrap();
        let from = pos2(bar.left() + 24.0, bar.top() + 12.0);
        let to = h.state().main_surface()[NodeIndex::root()]
            .rect()
            .unwrap()
            .center();
        // Ctrl as Windows gives it (the command key: Cmd on a Mac).
        let modifiers = if ctrl {
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND
        } else {
            egui::Modifiers::NONE
        };
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.event(egui::Event::PointerMoved(from));
        h.step();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        h.event(button(from, true));
        h.step();
        for k in 1..=12 {
            h.event(egui::Event::PointerMoved(from.lerp(to, k as f32 / 12.0)));
            h.step();
        }
        // egui_dock keeps the window under the pointer preferred for a
        // moment (0.3 s): the pointer rests on the destination first.
        h.run_steps(30);
        h.event(button(to, false));
        h.run_steps(3);
        h.state()
            .find_tab(&"Floating")
            .is_some_and(|p| p.surface.is_main())
    }

    #[test]
    fn floating_windows_dock_as_the_preference_says() {
        // "Allow floating windows to dock": on, they dock unless Ctrl is
        // held; off (VEGAS's default), only with Ctrl.
        assert!(drop_floating_tab(true, false), "docks");
        assert!(!drop_floating_tab(true, true), "Ctrl keeps it floating");
        assert!(!drop_floating_tab(false, false), "stays floating");
        assert!(drop_floating_tab(false, true), "Ctrl docks it");
    }

    /// A floating window dragged by its tab follows the pointer from the
    /// first move, as an OS window by its title (VEGAS's floating windows
    /// are), and stays where it is let go.
    #[test]
    fn a_floating_window_follows_its_tab_at_once() {
        let mut dock = DockState::new(vec!["Docked"]);
        let window = dock.add_window(vec!["Floating"]);
        dock.get_window_state_mut(window)
            .unwrap()
            .set_position(pos2(40.0, 40.0))
            .set_size(vec2(220.0, 160.0));
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 640.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, dock: &mut DockState<&'static str>| {
                    DockArea::new(dock)
                        .floating_tabs_dock(false)
                        .show_inside(ui, &mut Plain);
                },
                dock,
            );
        h.run_steps(3);
        let corner = |h: &Harness<'_, DockState<&'static str>>| {
            h.state()[window][NodeIndex::root()].rect().unwrap().min
        };
        let start = corner(&h);
        let from = start + vec2(24.0, 12.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        h.event(egui::Event::PointerMoved(from));
        h.step();
        h.event(button(from, true));
        h.step();
        // Three points: egui's drag starts six away, the tab's farther.
        let nudge = from + vec2(3.0, 2.0);
        h.event(egui::Event::PointerMoved(nudge));
        h.run_steps(2);
        assert_eq!(corner(&h), start + vec2(3.0, 2.0), "it moved at once");
        let far = from + vec2(250.0, 120.0);
        for k in 1..=10 {
            h.event(egui::Event::PointerMoved(nudge.lerp(far, k as f32 / 10.0)));
            h.step();
            // To the pixel (egui rounds windows' places).
            let followed = corner(&h) - (start + (nudge.lerp(far, k as f32 / 10.0) - from));
            assert!(
                followed.x.abs() <= 0.5 && followed.y.abs() <= 0.5,
                "step {k}: {followed:?} off"
            );
        }
        h.event(button(far, false));
        h.run_steps(3);
        let off = corner(&h) - (start + vec2(250.0, 120.0));
        assert!(off.length() < 0.5, "it stays where it was let go: {off:?}");
        assert!(
            !h.state().find_tab(&"Floating").unwrap().surface.is_main(),
            "still floating"
        );
    }

    /// A viewer with a fixed window, as BananaV's timeline is.
    struct WithFixed;

    impl TabViewer for WithFixed {
        type Tab = &'static str;

        fn id(&mut self, tab: &mut &'static str) -> Id {
            Id::new(*tab)
        }

        fn title(&mut self, tab: &mut &'static str) -> WidgetText {
            (*tab).into()
        }

        fn ui(&mut self, ui: &mut Ui, tab: &mut &'static str) {
            ui.label(*tab);
        }

        fn fixed(&self, tab: &&'static str) -> bool {
            *tab == "Fixed"
        }
    }

    /// A window dropped on the middle of a fixed one docks beside it,
    /// never in its pane (VEGAS's timeline takes no tab).
    #[test]
    fn nothing_docks_in_a_fixed_window_s_pane() {
        let mut dock = DockState::new(vec!["Docked"]);
        dock.main_surface_mut()
            .split_below(NodeIndex::root(), 0.5, vec!["Fixed"]);
        let window = dock.add_window(vec!["Floating"]);
        dock.get_window_state_mut(window)
            .unwrap()
            .set_position(pos2(40.0, 40.0))
            .set_size(vec2(220.0, 160.0));
        let mut h = Harness::builder()
            .with_size(vec2(900.0, 640.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, dock: &mut DockState<&'static str>| {
                    DockArea::new(dock)
                        .floating_tabs_dock(true)
                        .show_inside(ui, &mut WithFixed);
                },
                dock,
            );
        h.run_steps(3);
        let pane = |h: &Harness<'_, DockState<&'static str>>, tab| {
            let (node, _) = h.state().main_surface().find_tab(&tab).unwrap();
            h.state().main_surface()[node].rect().unwrap()
        };
        let bar = h.state()[window][NodeIndex::root()].rect().unwrap();
        let from = pos2(bar.left() + 24.0, bar.top() + 12.0);
        // Its upper part: above it.
        let fixed = pane(&h, "Fixed");
        let to = pos2(fixed.center().x, fixed.top() + fixed.height() * 0.15);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        h.event(egui::Event::PointerMoved(from));
        h.step();
        h.event(button(from, true));
        h.step();
        for k in 1..=12 {
            h.event(egui::Event::PointerMoved(from.lerp(to, k as f32 / 12.0)));
            h.step();
        }
        h.run_steps(30);
        h.event(button(to, false));
        h.run_steps(3);
        let tree = h.state().main_surface();
        let (fixed, _) = tree.find_tab(&"Fixed").unwrap();
        assert_eq!(tree[fixed].tabs_count(), 1, "the fixed window stays alone");
        let floating = h.state().find_tab(&"Floating").unwrap();
        assert!(
            floating.surface.is_main(),
            "it docked, around the fixed one"
        );
        let placed = pane(&h, "Floating");
        assert!(placed.bottom() <= pane(&h, "Fixed").top() + 1.0, "above it");
    }

    /// VEGAS 23's five layouts, as its screenshots show them: the windows,
    /// their places and the open ones.
    #[test]
    fn the_layouts_place_the_windows_as_vegas_s() {
        use crate::app::actions::LayoutPreset as P;
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        let mut h = Harness::builder()
            .with_size(vec2(1920.0, 1040.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        let rect = |h: &Harness<'_, BananaApp>, tab: Tab| {
            let tree = h.state().dock.as_ref().unwrap().main_surface();
            let (node, _) = tree.find_tab(&tab).unwrap_or_else(|| panic!("{tab:?}"));
            tree[node].rect().unwrap()
        };
        let open = |h: &Harness<'_, BananaApp>, tab: Tab| {
            let tree = h.state().dock.as_ref().unwrap().main_surface();
            let (node, index) = tree.find_tab(&tab).unwrap();
            tree[node].get_leaf().unwrap().active == index
        };
        let near = |a: f32, b: f32| (a - b).abs() < 8.0;
        for p in P::ALL {
            h.state_mut().apply_layout(p);
            h.run_steps(3);
            let t = rect(&h, Tab::Timeline);
            match p {
                P::Default | P::TimelineTop => {
                    let (e, v, m) = (
                        rect(&h, Tab::Explorer),
                        rect(&h, Tab::Preview),
                        rect(&h, Tab::Master),
                    );
                    assert_eq!(rect(&h, Tab::Trimmer), v, "{p:?}: with the preview");
                    assert!(e.right() < v.left() && v.right() < m.left(), "{p:?}");
                    if p == P::Default {
                        assert!(t.top() > e.bottom() && t.top() > v.bottom(), "below");
                    } else {
                        assert!(t.bottom() < e.top() && t.bottom() < v.top(), "on top");
                    }
                    assert!(open(&h, Tab::Explorer) && open(&h, Tab::Preview), "{p:?}");
                }
                P::AudioMixing => {
                    let (c, v) = (rect(&h, Tab::MixingConsole), rect(&h, Tab::Preview));
                    assert_eq!(rect(&h, Tab::Explorer), c, "with the console");
                    assert!(c.right() < v.left() && c.width() > 2.0 * v.width());
                    assert!(t.top() > c.bottom() && t.top() > v.bottom());
                    assert!(open(&h, Tab::MixingConsole));
                    let dock = h.state().dock.as_ref().unwrap();
                    assert!(dock.find_tab(&Tab::Master).is_none());
                }
                P::ColorCorrection => {
                    let (g, v, s) = (
                        rect(&h, Tab::ColorGrading),
                        rect(&h, Tab::Preview),
                        rect(&h, Tab::Scopes),
                    );
                    // The timeline over Color Grading on the left, the
                    // preview over the scopes down the right side.
                    assert!(near(t.left(), g.left()) && t.bottom() < g.top());
                    assert!(t.right() < v.left() && v.bottom() < s.top());
                    assert!(near(v.top(), t.top()) && near(s.bottom(), g.bottom()));
                }
                P::SourceWindow => {
                    let x = |tab| rect(&h, tab).left();
                    assert!(x(Tab::Explorer) < x(Tab::Trimmer));
                    assert!(x(Tab::Trimmer) < x(Tab::Preview));
                    assert!(x(Tab::Preview) < x(Tab::Master));
                    assert_eq!(rect(&h, Tab::Notes), rect(&h, Tab::Explorer));
                    assert!(t.top() > rect(&h, Tab::Trimmer).bottom());
                    assert!(open(&h, Tab::Explorer));
                }
            }
        }
    }

    /// A window opened goes to a pane of the docking area, never the
    /// timeline's.
    #[test]
    fn windows_open_beside_the_timeline_never_in_its_pane() {
        use crate::app::actions::LayoutPreset as P;
        let mut dock = preset_layout(P::ColorCorrection);
        open_tab(&mut dock, Tab::Notes);
        let tree = dock.main_surface();
        let (timeline, _) = tree.find_tab(&Tab::Timeline).unwrap();
        assert_eq!(tree[timeline].tabs_count(), 1);
        assert!(dock.find_tab(&Tab::Notes).is_some());
        // Only the timeline left: above it.
        let mut alone = DockState::new(vec![Tab::Timeline]);
        open_tab(&mut alone, Tab::Explorer);
        let tree = alone.main_surface();
        let (timeline, _) = tree.find_tab(&Tab::Timeline).unwrap();
        assert_eq!(tree[timeline].tabs_count(), 1);
        assert!(alone.find_tab(&Tab::Explorer).is_some());
    }

    #[test]
    fn docked_tabs_sit_below_their_windows_unless_asked_on_top() {
        for at_bottom in [true, false] {
            let mut h = Harness::builder()
                .with_size(vec2(400.0, 300.0))
                .build_ui_state(
                    move |ui, dock: &mut DockState<&'static str>| {
                        let mut style = Style::from_egui(ui.style());
                        style.tab_bar.at_bottom = at_bottom;
                        DockArea::new(dock).style(style).show_inside(ui, &mut Plain);
                    },
                    DockState::new(vec!["A", "B"]),
                );
            h.run_steps(2);
            let leaf = h.state().main_surface()[NodeIndex::root()]
                .get_leaf()
                .unwrap();
            let (whole, body) = (leaf.rect, leaf.viewport);
            if at_bottom {
                assert_eq!(body.top(), whole.top(), "{body:?} in {whole:?}");
                assert!(body.bottom() < whole.bottom() - 10.0, "the tabs below");
            } else {
                assert!(body.top() > whole.top() + 10.0, "the tabs above");
                assert_eq!(body.bottom(), whole.bottom());
            }
        }
    }

    #[test]
    fn tabs_that_do_not_fit_get_shorter_titles() {
        // The window's height: a scroll bar under overflowing tabs would
        // take part of it.
        let body_height = |names: Vec<&'static str>, shrink: bool| {
            let mut h = Harness::builder()
                .with_size(vec2(260.0, 200.0))
                .build_ui_state(
                    move |ui, dock: &mut DockState<&'static str>| {
                        let mut style = Style::from_egui(ui.style());
                        style.tab_bar.shrink_titles_on_overflow = shrink;
                        DockArea::new(dock).style(style).show_inside(ui, &mut Plain);
                    },
                    DockState::new(names),
                );
            h.run_steps(2);
            h.state().main_surface()[NodeIndex::root()]
                .get_leaf()
                .unwrap()
                .viewport
                .height()
        };
        let long = vec!["A rather long window name", "Another long one", "Third"];
        let fits = body_height(vec!["A", "B"], true);
        assert_eq!(body_height(long.clone(), true), fits, "shortened titles");
        assert!(body_height(long, false) < fits, "egui_dock's scroll bar");
    }

    /// What is dragged (media, effects, transitions…) goes onto the
    /// timeline or into Project Media; over the other windows, which take
    /// none of it, the pointer says it can't go there.
    #[test]
    fn only_the_timeline_and_project_media_take_what_is_dragged() {
        assert!(takes_drops(Tab::Timeline));
        for tab in Tab::ALL {
            assert_eq!(takes_drops(tab), tab == Tab::ProjectMedia, "{tab:?}");
        }
    }
}
