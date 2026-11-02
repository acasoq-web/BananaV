//! VEGAS's plug-in windows (Media Generator, Transitions, Video FX): the
//! categories along the top ("All Plug-ins", with the search, and
//! "Favorites" first), the plug-ins of the chosen one on the left, the
//! chosen plug-in's presets as pictures on the right — only the one under
//! the pointer plays — and what the plug-in is at the bottom. Choosing the
//! Third Party category shows its Filter at the top right: one maker's
//! plug-ins.

use crate::egui::{
    self, Align2, Color32, FontId, Id, Painter, Rect, Response, RichText, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme;

/// A category of the strip along the top (after All Plug-ins and
/// Favorites).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Category {
    pub name: &'static str,
    /// BananaV has plug-ins of it; the others are greyed out.
    pub ready: bool,
    /// Filter Packages: chains the user saved, not plug-ins — All
    /// Plug-ins leaves them out — and its cell holds Refresh and Explore
    /// Effect Filters Folder while it is chosen.
    pub packages: bool,
}

/// A plug-in of the list.
#[derive(Clone, Debug)]
pub(crate) struct Plugin {
    /// What the favorites remember it by.
    pub key: String,
    pub name: String,
    /// Its category: an index in the window's categories.
    pub category: usize,
    /// BananaV has it; the others are greyed out.
    pub ready: bool,
    /// The two lines about it at the bottom of the window.
    pub about: [String; 2],
    /// Who made it, for another maker's (the Third Party category's
    /// Filter picks by it).
    pub provider: Option<String>,
}

/// The chosen category, plug-in and preset; the search.
#[derive(Debug, Default)]
pub(crate) struct ChooserState {
    /// 0: All Plug-ins, 1: Favorites, then the window's categories.
    pub category: usize,
    /// The chosen plug-in's key.
    pub plugin: Option<String>,
    pub preset: Option<usize>,
    /// The text searched for, while searching.
    pub search: Option<String>,
    /// The Third Party category's Filter: one maker's plug-ins.
    pub provider: Option<String>,
    /// The search was just opened: its field takes the keys.
    focus_search: bool,
    /// The preset under the pointer, and since when (it plays).
    hovered: Option<(usize, f64)>,
}

/// The windows' states.
#[derive(Debug, Default)]
pub(crate) struct Choosers {
    pub generators: ChooserState,
    pub transitions: ChooserState,
    pub effects: ChooserState,
    pub compositors: ChooserState,
}

/// What [`frame`] leaves to the window: where the presets go and the
/// chosen plug-in (an index in its plug-ins).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub grid: Rect,
    pub plugin: Option<usize>,
    /// A plug-in became, or stopped being, a favorite.
    pub favorites_changed: bool,
    /// Filter Packages' Refresh was clicked: read the folder again.
    pub refresh: bool,
    /// Its Explore Effect Filters Folder was clicked.
    pub explore: bool,
    /// The chosen plug-in was dragged out of the list: its first preset —
    /// (Default) — goes where it is dropped, as VEGAS's.
    pub dragged: bool,
}

const STRIP_H: f32 = 30.0;
const ABOUT_H: f32 = 40.0;
const LIST_W: f32 = 190.0;
const ROW_H: f32 = 26.0;
const SPLITTER: f32 = 4.0;
/// A preset's picture, as VEGAS's.
pub(crate) const THUMB: Vec2 = vec2(120.0, 72.0);
/// A preset's cell: its picture, the bar of its example and its name on up
/// to two lines; the space between cells, and around them.
const CELL: Vec2 = vec2(THUMB.x, THUMB.y + 38.0);
const GAP: Vec2 = vec2(10.0, 10.0);
const MARGIN: Vec2 = vec2(6.0, 8.0);
/// The squares under a see-through picture, as VEGAS's: their side and
/// their two grays.
const CHECKER: f32 = 10.0;
const CHECKER_GRAYS: [u8; 2] = [0x50, 0x62];
/// How long a preset plays, then holds its end, under the pointer.
const PLAY: f32 = 1.8;
const CYCLE: f32 = 2.5;

/// How far a preset's example has played, 0–1, `seconds` after the
/// pointer came onto it (it starts over).
pub(crate) fn progress(seconds: f32) -> f32 {
    ((seconds % CYCLE) / PLAY).min(1.0)
}

/// Draws the window but for the presets.
pub(crate) fn frame(
    ui: &mut Ui,
    id: &str,
    state: &mut ChooserState,
    favorites: &mut Vec<String>,
    categories: &[Category],
    plugins: &[Plugin],
    icon: Icon,
) -> Frame {
    let full = ui.available_rect_before_wrap();
    let strip = Rect::from_min_size(full.min, vec2(full.width(), STRIP_H));
    let about_top = (full.bottom() - ABOUT_H).max(strip.bottom());
    let about = Rect::from_min_max(pos2(full.left(), about_top), full.max);
    let body = Rect::from_min_max(
        pos2(full.left(), strip.bottom()),
        pos2(full.right(), about_top),
    );
    ui.painter().rect_filled(full, 0.0, theme::panel());
    let w = crate::widgets::pane_width(ui, Id::new((id, "list-width")), body, LIST_W, 80.0, 140.0);
    let list_right = body.left() + w;
    let list = Rect::from_min_max(body.min, pos2(list_right, body.bottom()));
    let grid = Rect::from_min_max(pos2(list_right + SPLITTER, body.top()), body.max);
    let painter = ui.painter();
    // The dark lines between the parts, as VEGAS's.
    let dark = theme::panel_dark();
    painter.hline(full.x_range(), strip.bottom() - 0.5, Stroke::new(1.0, dark));
    painter.hline(full.x_range(), about.top() + 0.5, Stroke::new(1.0, dark));

    let (refresh, explore) = strip_ui(ui, strip, id, state, categories, plugins);
    let shown = shown(state, favorites, categories, plugins);
    // A plug-in is always chosen while any is listed.
    let chosen = state
        .plugin
        .as_deref()
        .and_then(|k| shown.iter().copied().find(|&i| plugins[i].key == k))
        .or_else(|| shown.iter().copied().find(|&i| plugins[i].ready));
    let key = chosen.map(|i| plugins[i].key.clone());
    if key != state.plugin {
        state.plugin = key;
        state.preset = None;
    }
    let (favorites_changed, dragged) =
        list_ui(ui, list, id, state, favorites, plugins, &shown, icon);
    // Dragged out of the list, a plug-in is chosen at once.
    let chosen = dragged.or(chosen);
    if let Some(i) = chosen {
        about_ui(ui, about, &plugins[i]);
    }
    ui.allocate_rect(full, Sense::hover());
    Frame {
        grid,
        plugin: chosen,
        favorites_changed,
        refresh,
        explore,
        dragged: dragged.is_some(),
    }
}

/// The plug-ins the chosen category and the search leave, by name.
fn shown(
    state: &ChooserState,
    favorites: &[String],
    categories: &[Category],
    plugins: &[Plugin],
) -> Vec<usize> {
    let query = state
        .search
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let mut v: Vec<usize> = (0..plugins.len())
        .filter(|&i| {
            let p = &plugins[i];
            let listed = match state.category {
                0 => !categories.get(p.category).is_some_and(|c| c.packages),
                1 => favorites.contains(&p.key),
                c => p.category == c - 2,
            };
            // The Third Party category's Filter.
            let made = state
                .provider
                .as_ref()
                .is_none_or(|w| p.provider.as_ref() == Some(w));
            // Search plug-ins: by name, description or group.
            let found = query.as_ref().is_none_or(|q| {
                p.name.to_lowercase().contains(q)
                    || p.about.iter().any(|a| a.to_lowercase().contains(q))
                    || categories
                        .get(p.category)
                        .is_some_and(|c| c.name.to_lowercase().contains(q))
            });
            listed && made && found
        })
        .collect();
    v.sort_by_key(|&i| plugins[i].name.to_lowercase());
    v
}

/// The makers of the plug-ins of category `category` (an index of the
/// strip's: the window's own from 2), by name: the Third Party category's
/// Filter.
fn providers(plugins: &[Plugin], category: usize) -> Vec<String> {
    let mut v: Vec<String> = plugins
        .iter()
        .filter(|p| category >= 2 && p.category == category - 2)
        .filter_map(|p| p.provider.clone())
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

/// The room Filter Packages' Refresh and Explore Effect Filters Folder
/// take in its cell.
const PACKAGE_TOOLS_W: f32 = 44.0;

/// The categories along the top: All Plug-ins (its magnifier opens the
/// search), Favorites, the window's own; the Third Party category's
/// Filter and the views button at the right end. Whether Filter
/// Packages' Refresh and Explore Effect Filters Folder were clicked.
fn strip_ui(
    ui: &mut Ui,
    strip: Rect,
    id: &str,
    state: &mut ChooserState,
    categories: &[Category],
    plugins: &[Plugin],
) -> (bool, bool) {
    let font = FontId::proportional(13.0);
    let (top, bottom) = (strip.top() + 3.0, strip.bottom() - 1.0);
    let mut x = strip.left() + 8.0;
    let views = Rect::from_center_size(
        pos2(strip.right() - 18.0, strip.center().y),
        vec2(20.0, 20.0),
    );
    let (mut refresh, mut explore) = (false, false);
    // The Filter, left of the views button, while the chosen category has
    // other makers' plug-ins.
    let makers = providers(plugins, state.category);
    if state.provider.as_ref().is_some_and(|w| !makers.contains(w)) {
        state.provider = None;
    }
    let filter_w = if makers.is_empty() {
        0.0
    } else {
        let label = ui
            .painter()
            .layout_no_wrap(tr("Filter:").to_string(), font.clone(), theme::text())
            .size()
            .x;
        label + 8.0 + 150.0
    };
    let filter = Rect::from_min_max(
        pos2(views.left() - 8.0 - filter_w, top),
        pos2(views.left() - 8.0, bottom),
    );
    let packages_cell = |i: usize| i >= 2 && categories.get(i - 2).is_some_and(|c| c.packages);
    let cells: Vec<(&str, Option<Icon>, bool)> = [
        (tr("All Plug-ins"), Some(Icon::ZoomTool), true),
        (tr("Favorites"), Some(Icon::StarFilled), true),
    ]
    .into_iter()
    .chain(categories.iter().map(|c| (c.name, None, c.ready)))
    .collect();
    // The cells' widths; narrower when they do not fit, and those that
    // still do not go into the » menu at the end.
    let natural: Vec<f32> = cells
        .iter()
        .enumerate()
        .map(|(i, (label, icon, _))| {
            let text = if i == 0 && state.search.is_some() {
                150.0
            } else {
                ui.painter()
                    .layout_no_wrap(label.to_string(), font.clone(), theme::text())
                    .size()
                    .x
            };
            let tools = if packages_cell(i) && state.category == i {
                PACKAGE_TOOLS_W
            } else {
                0.0
            };
            text + tools + if icon.is_some() { 22.0 } else { 0.0 }
        })
        .collect();
    let right_end = if filter_w > 0.0 {
        filter.left()
    } else {
        views.left()
    };
    let room = right_end - 4.0 - x;
    let pad = if natural.iter().map(|w| w + 21.0).sum::<f32>() <= room {
        20.0
    } else {
        10.0
    };
    let mut fit = 0;
    let mut used = 0.0;
    for w in &natural {
        if used + w + pad + 1.0 > room - 26.0 {
            break;
        }
        used += w + pad + 1.0;
        fit += 1;
    }
    if natural.iter().map(|w| w + pad + 1.0).sum::<f32>() <= room {
        fit = cells.len();
    }
    for (i, &(label, icon, ready)) in cells.iter().enumerate().take(fit) {
        let searching = i == 0 && state.search.is_some();
        // Colored when painted: selected, ready or greyed out.
        let galley =
            ui.painter()
                .layout_no_wrap(label.to_string(), font.clone(), Color32::PLACEHOLDER);
        let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
        let r = Rect::from_min_max(pos2(x, top), pos2(x + natural[i] + pad, bottom));
        let left_pad = pad / 2.0;
        let sense = if ready {
            Sense::click()
        } else {
            Sense::hover()
        };
        let resp = ui.interact(r, Id::new((id, "category", i)), sense);
        resp.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ready,
                state.category == i,
                label,
            )
        });
        let selected = state.category == i;
        if selected {
            ui.painter().rect_filled(r, 0.0, theme::selection());
        } else if resp.hovered() && ready {
            ui.painter().rect_filled(r, 0.0, theme::hover());
        }
        ui.painter().vline(
            r.right() + 0.5,
            r.y_range(),
            Stroke::new(1.0, theme::panel_dark()),
        );
        let color = if selected {
            Color32::WHITE
        } else if ready {
            theme::text()
        } else {
            theme::text_disabled()
        };
        let mut left = r.left() + left_pad;
        if let Some(icon) = icon {
            let ir = Rect::from_center_size(pos2(left + 8.0, r.center().y), vec2(16.0, 16.0));
            icons::paint(ui.painter(), ir, icon, color);
            left += icon_w;
        }
        if searching {
            // The search field in the tab's place.
            let field = Rect::from_min_max(
                pos2(left, r.top() + 3.0),
                pos2(r.right() - 6.0, r.bottom() - 3.0),
            );
            let text = state.search.get_or_insert_with(String::new);
            let edit = ui.put(
                field,
                egui::TextEdit::singleline(text)
                    .hint_text(tr("Search plug-ins"))
                    .text_color(color)
                    .frame(egui::Frame::NONE),
            );
            if edit.gained_focus() || resp.clicked() {
                state.category = 0;
            }
            if std::mem::take(&mut state.focus_search) {
                edit.request_focus();
            }
            // Escape closes it.
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                state.search = None;
            }
        } else {
            let text_right = left + galley.size().x;
            ui.painter().galley(
                pos2(left, r.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
            // Filter Packages chosen: Refresh and Explore Effect Filters
            // Folder after its name.
            if packages_cell(i) && selected {
                let tools = [
                    (Icon::Refresh, tr("Refresh")),
                    (Icon::Folder, tr("Explore Effect Filters Folder")),
                ];
                for (k, (icon, tip)) in tools.into_iter().enumerate() {
                    let ir = Rect::from_center_size(
                        pos2(text_right + 14.0 + 20.0 * k as f32, r.center().y),
                        vec2(18.0, 18.0),
                    );
                    let tool = ui.interact(ir, Id::new((id, "package-tool", k)), Sense::click());
                    tool.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip)
                    });
                    if tool.hovered() {
                        ui.painter().rect_filled(ir, 2.0, theme::hover());
                    }
                    icons::paint(ui.painter(), ir.shrink(2.0), icon, Color32::WHITE);
                    if tool.clicked() {
                        if k == 0 {
                            refresh = true;
                        } else {
                            explore = true;
                        }
                    }
                    tool.on_hover_text(tip);
                }
            }
        }
        if resp.clicked() {
            // All Plug-ins' magnifier opens (or closes) the search.
            let on_icon = i == 0
                && resp
                    .interact_pointer_pos()
                    .is_some_and(|p| p.x < r.left() + left_pad + icon_w);
            if on_icon {
                state.search = match state.search {
                    Some(_) => None,
                    None => Some(String::new()),
                };
                state.focus_search = state.search.is_some();
            }
            if state.category != i {
                state.provider = None;
            }
            state.category = i;
        }
        let tip = match (i, ready) {
            (0, _) => tr("All plug-ins; the magnifier searches them"),
            (1, _) => tr("The plug-ins marked with a star"),
            (_, true) => "",
            (_, false) => tr("Coming soon to BananaV"),
        };
        if !tip.is_empty() {
            resp.on_hover_text(tip);
        }
        x = r.right() + 1.0;
    }
    // The categories that did not fit: a menu.
    if fit < cells.len() {
        let r = Rect::from_min_max(pos2(x, top), pos2(x + 24.0, bottom));
        let resp = ui.interact(r, Id::new((id, "more-categories")), Sense::click());
        let chosen_hidden = state.category >= fit;
        if chosen_hidden {
            ui.painter().rect_filled(r, 0.0, theme::selection());
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, theme::hover());
        }
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            "»",
            font.clone(),
            if chosen_hidden {
                Color32::WHITE
            } else {
                theme::text()
            },
        );
        egui::Popup::menu(&resp).show(|ui| {
            for (i, &(label, _, ready)) in cells.iter().enumerate().skip(fit) {
                let item =
                    ui.add_enabled(ready, egui::Button::selectable(state.category == i, label));
                if item.clicked() {
                    if state.category != i {
                        state.provider = None;
                    }
                    state.category = i;
                    ui.close();
                }
                item.on_disabled_hover_text(tr("Coming soon to BananaV"));
            }
        });
    }
    // The Third Party category's Filter: all makers, or one.
    if filter_w > 0.0 {
        ui.scope_builder(UiBuilder::new().max_rect(filter), |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new(tr("Filter:")).font(font.clone()));
                let all = tr("All");
                egui::ComboBox::from_id_salt((id, "provider"))
                    .width(150.0)
                    .selected_text(state.provider.as_deref().unwrap_or(all))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.provider, None, all);
                        for m in &makers {
                            ui.selectable_value(&mut state.provider, Some(m.clone()), m);
                        }
                    });
            });
        });
    }
    // Views: not in BananaV yet.
    let resp = ui.interact(views, Id::new((id, "views")), Sense::hover());
    icons::paint(
        ui.painter(),
        views.shrink(2.0),
        Icon::Views,
        theme::text_disabled(),
    );
    resp.on_hover_text(tr("Coming soon to BananaV"));
    (refresh, explore)
}

/// The plug-ins on the left: a star in front of each favorite (and of the
/// one under the pointer: a click on it marks or unmarks it), the
/// window's icon and the name. Whether the favorites changed.
#[allow(clippy::too_many_arguments)]
fn list_ui(
    ui: &mut Ui,
    list: Rect,
    id: &str,
    state: &mut ChooserState,
    favorites: &mut Vec<String>,
    plugins: &[Plugin],
    shown: &[usize],
    icon: Icon,
) -> (bool, Option<usize>) {
    let mut changed = false;
    let mut dragged = None;
    let font = FontId::proportional(13.0);
    let inner = list.shrink2(vec2(6.0, 5.0));
    ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
        egui::ScrollArea::vertical()
            .id_salt((id, "plugins"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for &i in shown {
                    let p = &plugins[i];
                    let sense = if p.ready {
                        Sense::click_and_drag()
                    } else {
                        Sense::hover()
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), sense);
                    let selected = state.plugin.as_deref() == Some(p.key.as_str());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            p.ready,
                            selected,
                            &p.name,
                        )
                    });
                    let painter = ui.painter();
                    if selected {
                        painter.rect_filled(rect, 0.0, theme::selection());
                    } else if resp.hovered() && p.ready {
                        painter.rect_filled(rect, 0.0, theme::hover());
                    }
                    let color = if selected {
                        Color32::WHITE
                    } else if p.ready {
                        theme::text()
                    } else {
                        theme::text_disabled()
                    };
                    let star = Rect::from_center_size(
                        pos2(rect.left() + 9.0, rect.center().y),
                        vec2(13.0, 13.0),
                    );
                    let favorite = favorites.contains(&p.key);
                    if favorite {
                        icons::paint(painter, star, Icon::StarFilled, theme::banana());
                    } else if resp.hovered() && p.ready {
                        icons::paint(painter, star, Icon::Star, color);
                    }
                    let ir = Rect::from_center_size(
                        pos2(rect.left() + 28.0, rect.center().y),
                        vec2(16.0, 16.0),
                    );
                    icons::paint(painter, ir, icon, color);
                    crate::widgets::fit_text(
                        painter,
                        pos2(rect.left() + 44.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        &p.name,
                        font.clone(),
                        color,
                        rect.width() - 48.0,
                    );
                    if resp.clicked() {
                        let on_star = resp
                            .interact_pointer_pos()
                            .is_some_and(|q| q.x < star.right() + 2.0);
                        if on_star {
                            if favorite {
                                favorites.retain(|k| *k != p.key);
                            } else {
                                favorites.push(p.key.clone());
                            }
                            changed = true;
                        } else {
                            state.plugin = Some(p.key.clone());
                            state.preset = None;
                        }
                    }
                    // Dragged (not by its star): chosen, its first preset
                    // dragged by the window.
                    if resp.drag_started()
                        && resp
                            .interact_pointer_pos()
                            .is_none_or(|q| q.x >= star.right() + 2.0)
                    {
                        state.plugin = Some(p.key.clone());
                        state.preset = None;
                        dragged = Some(i);
                    }
                    if !p.ready {
                        resp.on_hover_text(tr("Coming soon to BananaV"));
                    }
                }
            });
    });
    (changed, dragged)
}

/// The two lines about the chosen plug-in at the bottom.
fn about_ui(ui: &Ui, about: Rect, plugin: &Plugin) {
    let painter = ui.painter_at(about);
    let font = FontId::proportional(12.0);
    for (k, line) in plugin.about.iter().enumerate() {
        crate::widgets::fit_text(
            &painter,
            pos2(about.left() + 8.0, about.top() + 5.0 + 15.0 * k as f32),
            Align2::LEFT_TOP,
            line,
            font.clone(),
            theme::text(),
            about.width() - 16.0,
        );
    }
}

/// A preset of the grid: its response (dragging, its menu) and what was
/// done to it.
pub(crate) struct Cell {
    pub(crate) i: usize,
    pub(crate) resp: Response,
    /// Its + clicked: added, to edit it.
    pub(crate) add: bool,
    /// Double-clicked (not on its +).
    pub(crate) double: bool,
}

/// A see-through picture's squares, as VEGAS draws them under it.
fn checkerboard(painter: &Painter, rect: Rect) {
    let [light, dark] = CHECKER_GRAYS.map(Color32::from_gray);
    painter.rect_filled(rect, 0.0, light);
    let (cols, rows) = (
        (rect.width() / CHECKER).ceil() as usize,
        (rect.height() / CHECKER).ceil() as usize,
    );
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    for row in 0..rows {
        for col in (row % 2..cols).step_by(2) {
            let min = rect.min + vec2(col as f32 * CHECKER, row as f32 * CHECKER);
            painter.rect_filled(Rect::from_min_size(min, Vec2::splat(CHECKER)), 0.0, dark);
        }
    }
}

/// A preset's name under its picture, centred on `top`, on up to two lines
/// (cut short with … past them).
fn name_text(painter: &Painter, top: egui::Pos2, name: &str, font: &FontId) {
    let mut job = egui::text::LayoutJob::single_section(
        name.to_string(),
        egui::TextFormat::simple(font.clone(), theme::text()),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: CELL.x,
        max_rows: 2,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job.halign = egui::Align::Center;
    let galley = painter.layout_job(job);
    painter.galley(top, galley, theme::text());
}

/// Where a preset's + is on its picture.
fn plus_rect(pic: Rect) -> Rect {
    Rect::from_min_size(pos2(pic.right() - 21.0, pic.top() + 3.0), vec2(18.0, 18.0))
}

/// The chosen plug-in's presets in `area`: each a picture, with the bar of
/// its example under it and its name. `paint` draws preset `i` in its
/// picture; `play` is how long the pointer has been on it (only that one
/// plays, and shows its +, `add_tip`). Each preset's cell.
#[allow(clippy::too_many_arguments)]
pub(crate) fn preset_grid(
    ui: &mut Ui,
    area: Rect,
    id: &str,
    state: &mut ChooserState,
    names: &[String],
    add_tip: &str,
    mut paint: impl FnMut(&Painter, Rect, usize, Option<f32>),
) -> Vec<Cell> {
    let mut out = Vec::new();
    let now = ui.input(|i| i.time);
    let mut under = None;
    let font = FontId::proportional(12.0);
    ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
        egui::ScrollArea::vertical()
            .id_salt((id, "presets"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // In columns, every cell under the one above, as VEGAS's.
                let width = ui.available_width();
                let cols =
                    (((width - 2.0 * MARGIN.x + GAP.x) / (CELL.x + GAP.x)).floor() as usize).max(1);
                let rows = names.len().div_ceil(cols);
                let (grid, _) = ui.allocate_exact_size(
                    vec2(
                        width,
                        2.0 * MARGIN.y + (rows as f32 * (CELL.y + GAP.y) - GAP.y).max(0.0),
                    ),
                    Sense::hover(),
                );
                {
                    for (i, name) in names.iter().enumerate() {
                        let (col, row) = (i % cols, i / cols);
                        let rect = Rect::from_min_size(
                            grid.min
                                + MARGIN
                                + vec2(
                                    col as f32 * (CELL.x + GAP.x),
                                    row as f32 * (CELL.y + GAP.y),
                                ),
                            CELL,
                        );
                        let resp = ui.interact(
                            rect,
                            ui.id().with((id, "preset", i)),
                            Sense::click_and_drag(),
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name)
                        });
                        if ui.is_rect_visible(rect) {
                            let pic = Rect::from_min_size(rect.min, THUMB);
                            let painter = ui.painter_at(rect);
                            checkerboard(&painter, pic);
                            let play = resp.hovered().then(|| {
                                under = Some(i);
                                state
                                    .hovered
                                    .filter(|(h, _)| *h == i)
                                    .map_or(0.0, |(_, t0)| (now - t0) as f32)
                            });
                            paint(&painter, pic, i, play);
                            // The example's bar: how far it has played.
                            let bar = Rect::from_min_size(
                                pos2(pic.left(), pic.bottom() + 2.0),
                                vec2(pic.width(), 3.0),
                            );
                            painter.rect_filled(bar, 0.0, Color32::from_gray(0x64));
                            if let Some(t) = play {
                                let done = Rect::from_min_size(
                                    bar.min,
                                    vec2(bar.width() * progress(t), bar.height()),
                                );
                                painter.rect_filled(done, 0.0, theme::selection());
                            }
                            if state.preset == Some(i) || resp.hovered() {
                                let c = if state.preset == Some(i) {
                                    theme::selection()
                                } else {
                                    theme::text_dim()
                                };
                                painter.rect_stroke(
                                    pic,
                                    0.0,
                                    Stroke::new(1.0, c),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            name_text(
                                &painter,
                                pos2(rect.center().x, pic.bottom() + 8.0),
                                name,
                                &font,
                            );
                            // Under the pointer, the + that adds it to edit
                            // it, as VEGAS's.
                            if play.is_some() {
                                let plus = plus_rect(pic);
                                let lit = ui
                                    .input(|i| i.pointer.hover_pos())
                                    .is_some_and(|p| plus.contains(p));
                                let fill = if lit {
                                    theme::selection()
                                } else {
                                    Color32::from_black_alpha(170)
                                };
                                painter.rect_filled(plus, 2.0, fill);
                                let c = plus.center();
                                let st = Stroke::new(2.0, Color32::WHITE);
                                painter.line_segment([c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], st);
                                painter.line_segment([c - vec2(0.0, 5.0), c + vec2(0.0, 5.0)], st);
                            }
                        }
                        if resp.clicked() || resp.drag_started() {
                            state.preset = Some(i);
                        }
                        let plus = plus_rect(Rect::from_min_size(rect.min, THUMB));
                        let on_plus = resp
                            .interact_pointer_pos()
                            .or_else(|| ui.input(|i| i.pointer.hover_pos()))
                            .is_some_and(|p| plus.contains(p));
                        let add = resp.clicked() && on_plus;
                        let double = resp.double_clicked() && !on_plus;
                        let resp = if on_plus && resp.hovered() {
                            resp.on_hover_text(add_tip)
                        } else {
                            resp
                        };
                        out.push(Cell {
                            i,
                            resp,
                            add,
                            double,
                        });
                    }
                }
            });
    });
    // The clock of the preset that plays.
    match (under, state.hovered) {
        (Some(i), Some((h, _))) if h == i => {}
        (Some(i), _) => state.hovered = Some((i, now)),
        (None, _) => state.hovered = None,
    }
    if under.is_some() {
        ui.ctx().request_repaint();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(key: &str, name: &str, category: usize) -> Plugin {
        Plugin {
            key: key.into(),
            name: name.into(),
            category,
            ready: true,
            about: [format!("{name}: about"), String::new()],
            provider: None,
        }
    }

    #[test]
    fn categories_favorites_and_search_choose_what_is_listed() {
        let categories = [
            Category {
                name: "Color",
                ready: true,
                packages: false,
            },
            Category {
                name: "Blur",
                ready: true,
                packages: false,
            },
        ];
        let plugins = [
            plugin("sepia", "Sepia", 0),
            plugin("blur", "Gaussian Blur", 1),
            plugin("bw", "Black and White", 0),
        ];
        let mut state = ChooserState::default();
        let favorites = vec!["blur".to_string()];
        // All Plug-ins, by name.
        assert_eq!(shown(&state, &favorites, &categories, &plugins), [2, 1, 0]);
        state.category = 1;
        assert_eq!(shown(&state, &favorites, &categories, &plugins), [1]);
        state.category = 2;
        assert_eq!(shown(&state, &favorites, &categories, &plugins), [2, 0]);
        // The search: names, descriptions and groups.
        state.category = 0;
        state.search = Some("blur".into());
        assert_eq!(shown(&state, &favorites, &categories, &plugins), [1]);
        state.search = Some("COLOR".into());
        assert_eq!(shown(&state, &favorites, &categories, &plugins), [2, 0]);
        state.search = Some("  ".into());
        assert_eq!(shown(&state, &favorites, &categories, &plugins).len(), 3);
    }

    #[test]
    fn third_parties_are_filtered_by_maker_and_packages_kept_apart() {
        let c = |name, packages| Category {
            name,
            ready: true,
            packages,
        };
        let categories = [
            c("Color", false),
            c("Third Party", false),
            c("Filter Packages", true),
        ];
        let maker = |key: &str, name: &str, by: &str| Plugin {
            provider: Some(by.into()),
            ..plugin(key, name, 1)
        };
        let plugins = [
            plugin("sepia", "Sepia", 0),
            maker("glow", "S_Glow", "Sapphire"),
            maker("blur", "BCC Blur", "Continuum"),
            maker("rays", "S_Rays", "Sapphire"),
            plugin("pkg:Warm", "Warm", 2),
        ];
        let mut state = ChooserState::default();
        // All Plug-ins leaves the packages out; their category has them.
        assert_eq!(shown(&state, &[], &categories, &plugins), [2, 1, 3, 0]);
        state.category = 4;
        assert_eq!(shown(&state, &[], &categories, &plugins), [4]);
        // Third Party: its makers, each the Filter's choice.
        state.category = 3;
        assert_eq!(providers(&plugins, 3), ["Continuum", "Sapphire"]);
        assert!(providers(&plugins, 2).is_empty(), "Color has none");
        assert_eq!(shown(&state, &[], &categories, &plugins), [2, 1, 3]);
        state.provider = Some("Sapphire".into());
        assert_eq!(shown(&state, &[], &categories, &plugins), [1, 3]);
    }

    #[test]
    fn a_preset_plays_then_holds_its_end_and_starts_over() {
        assert_eq!(progress(0.0), 0.0);
        assert!((progress(0.9) - 0.5).abs() < 1e-6);
        assert_eq!(progress(2.0), 1.0, "holds its end");
        assert!(progress(2.6) < 0.1, "starts over");
    }
}
