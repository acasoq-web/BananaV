//! Project Media's Media Tags panel, Quick Tags and Search Media Bins, as
//! VEGAS's Help describes them ("Using the Project Media window",
//! "Sorting your media with bins"): under the media, the selected media's
//! tags — a full tag on all of them, a partial one on some, its × taking
//! it off —, the Add Tags box (Enter tags them, Backspace in the empty box
//! takes the last tag off, Ctrl+Shift and a number keeps the text as that
//! Quick Tag) and the Quick Tags, Ctrl+1 to Ctrl+9, which tag the
//! selected media while Project Media has the keys; the Search Media Bins
//! dialog's conditions, its results a bin of the tree to keep as a bin or
//! a smart bin.

use bv_timeline::{
    BinId, MediaId, MediaKind, MediaSearch, SearchCondition, SearchField, SearchIn, SearchOp,
};

use crate::app::BananaApp;
use crate::egui::{
    self, Align2, Color32, FontId, Key, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use crate::i18n::tr;
use crate::theme;

use super::media::BinChoice;

/// How many Quick Tags there are (Ctrl+1 to Ctrl+9).
pub(crate) const QUICK_TAGS: usize = 9;

/// The panel's height: the tags' row and the Quick Tags' three.
pub(crate) const PANEL_H: f32 = 30.0 + 3.0 * 18.0 + 8.0;

/// The media the panel tags: those picked together, else the one selected.
pub(crate) fn selected_media(app: &BananaApp) -> Vec<MediaId> {
    let p = app.project();
    let known = |id: &MediaId| p.media(*id).is_some() || p.subclip(*id).is_some();
    if !app.media_window.picked.is_empty() {
        return app
            .media_window
            .picked
            .iter()
            .copied()
            .filter(known)
            .collect();
    }
    app.selected_media.filter(known).into_iter().collect()
}

/// Quick Tag `n` (0-based), when one is kept.
fn quick_tag(app: &BananaApp, n: usize) -> Option<String> {
    app.settings
        .quick_tags
        .get(n)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Tags `media` with `tag`.
fn tag_media(app: &mut BananaApp, media: &[MediaId], tag: &str) {
    let tag = tag.trim().to_string();
    if tag.is_empty() || media.is_empty() {
        return;
    }
    let media = media.to_vec();
    app.modify(|p| {
        for m in media {
            p.add_tag(m, &tag);
        }
    });
}

/// Takes `tag` off `media`.
fn untag_media(app: &mut BananaApp, media: &[MediaId], tag: &str) {
    let media = media.to_vec();
    let tag = tag.to_string();
    app.modify(|p| {
        for m in media {
            p.remove_tag(m, &tag);
        }
    });
}

/// The number (1–9) of a number key pressed with `ctrl` and `shift` as
/// asked, the main row's or the keypad's.
fn number_key(ui_ctx: &egui::Context, shift: bool) -> Option<usize> {
    const KEYS: [Key; QUICK_TAGS] = [
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ];
    ui_ctx.input_mut(|i| {
        let mut found = None;
        i.events.retain(|e| {
            let egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                modifiers,
                ..
            } = e
            else {
                return true;
            };
            if !(modifiers.command || modifiers.ctrl) || modifiers.shift != shift || modifiers.alt {
                return true;
            }
            let n = KEYS
                .iter()
                .position(|k| k == key || Some(*k) == *physical_key);
            match n {
                Some(n) if found.is_none() => {
                    found = Some(n);
                    false
                }
                _ => true,
            }
        });
        found
    })
}

impl BananaApp {
    /// Ctrl+1 to Ctrl+9 while Project Media has the keys: the Quick Tag
    /// on the selected media.
    pub(crate) fn media_tag_keys(&mut self, ctx: &egui::Context) {
        if self.keys_tab != Some(crate::dock::Tab::ProjectMedia) {
            return;
        }
        let Some(n) = number_key(ctx, false) else {
            return;
        };
        let media = selected_media(self);
        match quick_tag(self, n) {
            Some(tag) if !media.is_empty() => tag_media(self, &media, &tag),
            Some(_) => self.notify(tr("Select media to tag them.")),
            None => self.notify(trf!(
                "Ctrl+{} has no Quick Tag yet: type a tag in Add Tags and press Ctrl+Shift+{}.",
                n + 1,
                n + 1
            )),
        }
    }
}

/// A tag of the selected media: its name in a rounded box (outlined only
/// when only some of them have it) and its ×. Whether the × was clicked.
fn tag_pill(ui: &mut Ui, tag: &str, all: bool) -> bool {
    let font = FontId::proportional(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(tag.to_string(), font, theme::text());
    let size = vec2(galley.size().x + 28.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, tag));
    let painter = ui.painter();
    let fill = Color32::from_gray(0x5a);
    if all {
        painter.rect_filled(rect, 10.0, fill);
    } else {
        painter.rect_stroke(
            rect.shrink(0.5),
            10.0,
            Stroke::new(1.0, fill.gamma_multiply(1.6)),
            egui::StrokeKind::Inside,
        );
    }
    painter.galley(
        pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::text(),
    );
    let x = Rect::from_center_size(pos2(rect.right() - 11.0, rect.center().y), vec2(13.0, 13.0));
    let remove = ui.interact(x, ui.id().with(("remove-tag", tag)), Sense::click());
    remove.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, trf!("Remove tag {}", tag))
    });
    let circle = if remove.hovered() {
        theme::error()
    } else {
        Color32::from_gray(0x2c)
    };
    painter.circle_filled(x.center(), 6.5, circle);
    let st = Stroke::new(1.4, Color32::from_gray(0xe0));
    let c = x.center();
    painter.line_segment([c + vec2(-2.8, -2.8), c + vec2(2.8, 2.8)], st);
    painter.line_segment([c + vec2(-2.8, 2.8), c + vec2(2.8, -2.8)], st);
    remove.on_hover_text(tr("Remove the tag")).clicked()
}

/// The Media Tags panel in `rect`.
pub(crate) fn tags_panel(ui: &mut Ui, app: &mut BananaApp, rect: Rect) {
    let painter = ui.painter();
    painter.hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0, theme::panel_dark()),
    );
    let media = selected_media(app);
    // The tags of the selected media, each with whether all have it.
    let mut tags: Vec<(String, bool)> = Vec::new();
    {
        let p = app.project();
        for &m in &media {
            for t in p.tags_of(m) {
                if !tags
                    .iter()
                    .any(|(x, _)| x.to_lowercase() == t.to_lowercase())
                {
                    let all = media.iter().all(|&o| {
                        p.tags_of(o)
                            .iter()
                            .any(|y| y.to_lowercase() == t.to_lowercase())
                    });
                    tags.push((t.clone(), all));
                }
            }
        }
    }
    let row = Rect::from_min_size(rect.min + vec2(6.0, 5.0), vec2(rect.width() - 12.0, 22.0));
    let mut removed = None;
    let mut add = None;
    let mut quick = None;
    let was_empty = app.media_window.tag_text.is_empty();
    ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (tag, all) in &tags {
                if tag_pill(ui, tag, *all) {
                    removed = Some(tag.clone());
                }
            }
            let hint = if media.is_empty() {
                tr("Select files to edit tags")
            } else {
                tr("Add tags")
            };
            let edit = ui.add_enabled(
                !media.is_empty(),
                egui::TextEdit::singleline(&mut app.media_window.tag_text)
                    // Its own id: the tags before it come and go.
                    .id_salt("media-tags-add")
                    .hint_text(RichText::new(hint).italics())
                    .frame(egui::Frame::NONE)
                    .desired_width(ui.available_width().max(60.0)),
            );
            if edit.has_focus() {
                // Ctrl+Shift and a number: the text becomes that Quick Tag.
                if let Some(n) = number_key(ui.ctx(), true) {
                    quick = Some(n);
                }
                if was_empty && ui.input(|i| i.key_pressed(Key::Backspace)) {
                    removed = tags.last().map(|(t, _)| t.clone());
                }
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                add = Some(app.media_window.tag_text.trim().to_string());
                edit.request_focus();
            }
        });
    });
    if let Some(n) = quick {
        let text = app.media_window.tag_text.trim().to_string();
        if !text.is_empty() {
            let tags = &mut app.settings.quick_tags;
            tags.resize(QUICK_TAGS.max(tags.len()), String::new());
            tags[n] = text.clone();
            let _ = app.settings.save();
            app.notify(trf!("Ctrl+{} now tags media {}.", n + 1, text));
        }
    }
    if let Some(tag) = add.filter(|t| !t.is_empty()) {
        tag_media(app, &media, &tag);
        app.media_window.tag_text.clear();
    }
    if let Some(tag) = removed {
        untag_media(app, &media, &tag);
    }
    // The Quick Tags: three columns of three; a click tags the selected
    // media too; the menu edits them.
    let grid = Rect::from_min_max(pos2(rect.left() + 6.0, row.bottom() + 4.0), rect.max);
    let grid_resp = ui.interact(grid, ui.id().with("quick-tags"), Sense::click());
    let col_w = (grid.width() / 3.0).max(60.0);
    let font = FontId::proportional(12.0);
    for n in 0..QUICK_TAGS {
        let (col, line) = (n / 3, n % 3);
        let cell = Rect::from_min_size(
            grid.min + vec2(col as f32 * col_w, line as f32 * 18.0),
            vec2(col_w - 6.0, 18.0),
        );
        let resp = ui.interact(cell, ui.id().with(("quick-tag", n)), Sense::click());
        let tag = quick_tag(app, n);
        let label = format!("Ctrl+{}", n + 1);
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("{label} {}", tag.as_deref().unwrap_or("")),
            )
        });
        let painter = ui.painter();
        if resp.hovered() && tag.is_some() {
            painter.rect_filled(cell, 2.0, theme::hover());
        }
        painter.text(
            pos2(cell.left() + 2.0, cell.center().y),
            Align2::LEFT_CENTER,
            &label,
            font.clone(),
            theme::text(),
        );
        if let Some(t) = &tag {
            crate::widgets::fit_text(
                painter,
                pos2(cell.left() + 52.0, cell.center().y),
                Align2::LEFT_CENTER,
                t,
                font.clone(),
                theme::text(),
                cell.width() - 56.0,
            );
        }
        if resp.clicked()
            && let Some(t) = tag
        {
            tag_media(app, &media, &t);
        }
        resp.context_menu(|ui| quick_tags_menu(ui, app));
    }
    grid_resp.context_menu(|ui| quick_tags_menu(ui, app));
}

fn quick_tags_menu(ui: &mut Ui, app: &mut BananaApp) {
    if ui.button(tr("Edit Quick Tags…")).clicked() {
        let mut tags = app.settings.quick_tags.clone();
        tags.resize(QUICK_TAGS, String::new());
        app.media_window.quick_tags_dialog = Some(tags);
        ui.close();
    }
}

/// The Edit Quick Tags dialog, while open: each Ctrl+number's text.
pub(crate) fn quick_tags_dialog(ctx: &egui::Context, app: &mut BananaApp) {
    let Some(mut tags) = app.media_window.quick_tags_dialog.take() else {
        return;
    };
    let (mut open, mut ok, mut cancel) = (true, false, false);
    crate::floating::Window::new(tr("Edit Quick Tags"))
        .id(egui::Id::new("edit-quick-tags"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            egui::Grid::new("quick-tags-grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for (n, tag) in tags.iter_mut().enumerate() {
                        ui.label(format!("Ctrl+{}", n + 1));
                        ui.add(egui::TextEdit::singleline(tag).desired_width(200.0));
                        ui.end_row();
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("OK").strong()).clicked() {
                    ok = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if ok {
        app.settings.quick_tags = tags.iter().map(|t| t.trim().to_string()).collect();
        let _ = app.settings.save();
    } else if open && !cancel {
        app.media_window.quick_tags_dialog = Some(tags);
    }
}

/// The Search Media Bins dialog: the search, and the smart bin it edits
/// (Edit Search).
#[derive(Clone, Debug)]
pub(crate) struct SearchDialog {
    pub search: MediaSearch,
    pub editing: Option<BinId>,
}

/// Opens Search Media Bins on `within` (its last search's conditions
/// kept).
pub(crate) fn open_search(app: &mut BananaApp, within: SearchIn) {
    let mut search = app.media_window.results.clone().unwrap_or_default();
    search.within = within;
    app.media_window.search_dialog = Some(SearchDialog {
        search,
        editing: None,
    });
}

/// Edit Search: smart bin `id`'s search in the dialog.
pub(crate) fn edit_smart_bin(app: &mut BananaApp, id: BinId) {
    if let Some(b) = app.project().smart_bin(id) {
        let search = b.search.clone();
        app.media_window.search_dialog = Some(SearchDialog {
            search,
            editing: Some(id),
        });
    }
}

/// The values a condition's box offers: the kinds for Type, the tags for
/// Tag (typed values too).
fn choices(app: &BananaApp, field: SearchField) -> Vec<String> {
    match field {
        SearchField::Type => [MediaKind::Video, MediaKind::Audio, MediaKind::Image]
            .iter()
            .map(|k| k.name().to_string())
            .collect(),
        SearchField::Tag => app.project().all_tags(),
        _ => Vec::new(),
    }
}

/// A Type condition's value shown in the interface's language.
fn value_name(field: SearchField, v: &str) -> String {
    match field {
        SearchField::Type => match v {
            "Video" => tr("Video").to_string(),
            "Audio" => tr("Audio").to_string(),
            "Image" => tr("Image").to_string(),
            _ => v.to_string(),
        },
        _ => v.to_string(),
    }
}

/// A field's name in the interface's language.
pub(crate) fn field_name(f: SearchField) -> &'static str {
    match f {
        SearchField::Name => tr("Name"),
        SearchField::Path => tr("Path"),
        SearchField::Type => tr("Type"),
        SearchField::Tag => tr("Tag"),
        SearchField::Length => tr("Length (seconds)"),
        SearchField::Width => tr("Width"),
        SearchField::Height => tr("Height"),
        SearchField::FrameRate => tr("Frame Rate"),
        SearchField::SampleRate => tr("Sample Rate"),
        SearchField::Channels => tr("Channels"),
        SearchField::TapeName => tr("Tape Name"),
    }
}

/// A comparison's name in the interface's language.
fn op_name(o: SearchOp) -> &'static str {
    match o {
        SearchOp::Contains => tr("contains"),
        SearchOp::DoesNotContain => tr("does not contain"),
        SearchOp::Is => tr("is"),
        SearchOp::IsNot => tr("is not"),
        SearchOp::StartsWith => tr("starts with"),
        SearchOp::EndsWith => tr("ends with"),
        SearchOp::Less => tr("is less than"),
        SearchOp::Greater => tr("is greater than"),
    }
}

/// Where a search looks, named.
fn within_name(app: &BananaApp, within: SearchIn) -> String {
    match within {
        SearchIn::All => tr("All Media").to_string(),
        SearchIn::Bin(b) => app
            .project()
            .bin(b)
            .map_or_else(|| tr("All Media").to_string(), |b| b.name.clone()),
    }
}

/// The Search Media Bins dialog, while open: Search shows the results (or
/// changes the smart bin it edits).
pub(crate) fn search_dialog(ctx: &egui::Context, app: &mut BananaApp) {
    let Some(mut d) = app.media_window.search_dialog.take() else {
        return;
    };
    let (mut open, mut go, mut cancel) = (true, false, false);
    let title = if d.editing.is_some() {
        tr("Edit Search")
    } else {
        tr("Search Media Bins")
    };
    let heading = match d.search.within {
        SearchIn::All => tr("Search all the media for those that meet:").to_string(),
        SearchIn::Bin(_) => trf!(
            "Search in {} and the bins in it for media that meet:",
            within_name(app, d.search.within)
        ),
    };
    crate::floating::Window::new(title)
        .id(egui::Id::new("search-media-bins"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(heading);
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.search.all, true, tr("All the conditions"));
                ui.radio_value(&mut d.search.all, false, tr("Any condition"));
            });
            ui.add_space(4.0);
            let mut remove = None;
            egui::Grid::new("search-conditions")
                .num_columns(4)
                .spacing([6.0, 4.0])
                .show(ui, |ui| {
                    let n = d.search.conditions.len();
                    for (i, c) in d.search.conditions.iter_mut().enumerate() {
                        condition_row(ui, app, i, c);
                        if ui
                            .add_enabled(n > 1, egui::Button::new("−"))
                            .on_hover_text(tr("Remove the condition"))
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
            if let Some(i) = remove {
                d.search.conditions.remove(i);
            }
            if ui.button(tr("Add Condition")).clicked() {
                d.search.conditions.push(SearchCondition::new(
                    SearchField::Name,
                    SearchOp::Contains,
                    "",
                ));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new(tr("Search")).strong()).clicked() {
                    go = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if go {
        match d.editing {
            Some(id) => {
                let search = d.search.clone();
                app.modify(|p| {
                    if let Some(b) = p.smart_bin_mut(id) {
                        b.search = search;
                    }
                });
                app.media_window.bin = BinChoice::Smart(id);
            }
            None => {
                app.media_window.results = Some(d.search.clone());
                app.media_window.bin = BinChoice::Results;
            }
        }
    } else if open && !cancel {
        app.media_window.search_dialog = Some(d);
    }
}

/// A condition's field, comparison and value.
fn condition_row(ui: &mut Ui, app: &BananaApp, i: usize, c: &mut SearchCondition) {
    let before = c.field;
    egui::ComboBox::from_id_salt(("search-field", i))
        .width(130.0)
        .selected_text(field_name(c.field))
        .show_ui(ui, |ui| {
            for f in SearchField::ALL {
                ui.selectable_value(&mut c.field, f, field_name(f));
            }
        });
    if c.field != before {
        // A comparison the new field takes, and a value that suits it.
        if !c.field.ops().contains(&c.op) {
            c.op = c.field.ops()[0];
        }
        c.value = choices(app, c.field).into_iter().next().unwrap_or_default();
    }
    egui::ComboBox::from_id_salt(("search-op", i))
        .width(120.0)
        .selected_text(op_name(c.op))
        .show_ui(ui, |ui| {
            for &o in c.field.ops() {
                ui.selectable_value(&mut c.op, o, op_name(o));
            }
        });
    // Type picks a kind, Tag one of the project's tags; the others take
    // what is typed.
    if matches!(c.field, SearchField::Type | SearchField::Tag) {
        let offered = choices(app, c.field);
        egui::ComboBox::from_id_salt(("search-value", i))
            .width(150.0)
            .selected_text(value_name(c.field, &c.value))
            .show_ui(ui, |ui| {
                if offered.is_empty() {
                    ui.label(RichText::new(tr("The project has no tags yet")).weak());
                }
                for v in &offered {
                    ui.selectable_value(&mut c.value, v.clone(), value_name(c.field, v));
                }
            });
    } else {
        ui.add(egui::TextEdit::singleline(&mut c.value).desired_width(150.0));
    }
}

/// The text a search is shown by (its first condition, and how many more).
pub(crate) fn search_summary(search: &MediaSearch) -> String {
    let Some(c) = search.conditions.first() else {
        return tr("Search Results").to_string();
    };
    let first = format!(
        "{} {} {}",
        field_name(c.field),
        op_name(c.op),
        value_name(c.field, &c.value)
    );
    match search.conditions.len() {
        1 => first,
        n => trn!(n - 1, "{} and {} more", "{} and {} more", first, n - 1),
    }
}

impl BananaApp {
    /// Project Media with its Media Tags panel, the project's first two
    /// media picked and tagged ("Friends" both, "Beach" the first), the
    /// Quick Tags Ctrl+1 "Friends" and Ctrl+2 "Beach" (screen tests; these
    /// settings are never saved).
    pub fn media_tags_for_review(&mut self) {
        self.settings = self.settings.unsaved();
        self.settings.media_tags_panel = true;
        self.settings.quick_tags = vec!["Friends".into(), "Beach".into()];
        let ids: Vec<MediaId> = self.project().media.iter().take(2).map(|m| m.id).collect();
        let picked = ids.clone();
        self.modify(|p| {
            for &m in &picked {
                p.add_tag(m, "Friends");
            }
            if let Some(&m) = picked.first() {
                p.add_tag(m, "Beach");
            }
        });
        self.selected_media = ids.first().copied();
        self.media_window.picked = ids;
    }

    /// The Search Media Bins dialog open on all the media: a Tag condition
    /// and a Name one, any of them (screen tests).
    pub fn media_search_for_review(&mut self) {
        self.media_window.search_dialog = Some(SearchDialog {
            search: MediaSearch {
                conditions: vec![
                    SearchCondition::new(SearchField::Tag, SearchOp::Is, "Beach"),
                    SearchCondition::new(SearchField::Name, SearchOp::Contains, "tone"),
                ],
                all: false,
                within: SearchIn::All,
            },
            editing: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::{Event, Modifiers, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    fn key(h: &mut Harness<'_, BananaApp>, key: Key, modifiers: Modifiers) {
        for pressed in [true, false] {
            h.event(Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            });
        }
        h.run_steps(2);
    }

    #[test]
    fn media_are_tagged_quick_tagged_found_and_kept_in_a_smart_bin() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let ids: Vec<MediaId> = app.project().media.iter().map(|m| m.id).collect();
        let (first, second) = (ids[0], ids[1]);
        app.settings.media_tags_panel = true;
        app.selected_media = Some(first);
        app.media_window.picked = vec![first, second];
        app.show_tab(crate::dock::Tab::ProjectMedia, false);
        let mut h = Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        // The Add Tags box (Project Media is docked on the left): a tag
        // and Enter tags both picked media.
        let field = |h: &Harness<'_, BananaApp>| {
            h.query_all_by(|n| n.role() == egui::accesskit::Role::TextInput)
                .find(|n| n.rect().left() < 800.0)
                .expect("the Add Tags box")
                .rect()
                .center()
        };
        let at = field(&h);
        h.event(Event::PointerMoved(at));
        for pressed in [true, false] {
            h.event(Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::default(),
            });
            h.step();
        }
        h.event(Event::Text("Friends".into()));
        h.run_steps(1);
        key(&mut h, Key::Enter, Modifiers::default());
        let tags = |h: &Harness<'_, BananaApp>, m: MediaId| h.state().project().tags_of(m).to_vec();
        assert_eq!(tags(&h, first), ["Friends"]);
        assert_eq!(tags(&h, second), ["Friends"]);
        // Ctrl+Shift+1 keeps the text as Ctrl+1's Quick Tag; Enter tags.
        h.event(Event::Text("Dresden".into()));
        h.run_steps(1);
        key(&mut h, Key::Num1, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(h.state().settings.quick_tags[0], "Dresden");
        key(&mut h, Key::Enter, Modifiers::default());
        assert_eq!(tags(&h, second), ["Friends", "Dresden"]);
        // Backspace in the empty box takes the last tag off.
        key(&mut h, Key::Backspace, Modifiers::default());
        assert_eq!(tags(&h, first), ["Friends"]);
        // Ctrl+2 while Project Media has the keys: its Quick Tag on the
        // media selected.
        {
            let app = h.state_mut();
            app.settings.quick_tags[1] = "Beach".into();
            app.media_window.picked.clear();
            app.selected_media = Some(second);
            app.keys_tab = Some(crate::dock::Tab::ProjectMedia);
        }
        // The box lets the keys go.
        h.ctx.memory_mut(|m| m.stop_text_input());
        h.run_steps(2);
        key(&mut h, Key::Num2, Modifiers::COMMAND);
        assert_eq!(tags(&h, second), ["Friends", "Beach"]);
        // A tag's × takes it off; the tag's bin under Tagged Media.
        h.get_by_label("Remove tag Friends").click();
        h.run_steps(2);
        assert_eq!(tags(&h, second), ["Beach"]);
        h.get_all_by_label("Beach")
            .find(|n| n.accesskit_node().role() != egui::accesskit::Role::Label)
            .expect("the tag's bin")
            .click();
        h.run_steps(2);
        assert_eq!(h.state().media_window.bin, BinChoice::Tag("Beach".into()));
        // Search Media Bins: Tag is Beach finds the second media; the
        // results kept as a smart bin.
        h.state_mut().media_window.search_dialog = Some(SearchDialog {
            search: MediaSearch {
                conditions: vec![SearchCondition::new(
                    SearchField::Tag,
                    SearchOp::Is,
                    "beach",
                )],
                all: true,
                within: SearchIn::All,
            },
            editing: None,
        });
        // The dialog takes its size and place first.
        h.run_steps(6);
        h.get_by_label("Search").click();
        h.run_steps(2);
        assert_eq!(h.state().media_window.bin, BinChoice::Results);
        let results = h.state().media_window.results.clone().unwrap();
        assert_eq!(h.state().project().search_media(&results), [second]);
        h.get_by_label("Search Results").click_secondary();
        h.run_steps(2);
        h.get_by_label("Save As Smart Bin").click();
        h.run_steps(2);
        let smart = h.state().project().smart_bins.clone();
        assert_eq!(smart.len(), 1);
        assert_eq!(h.state().media_window.bin, BinChoice::Smart(smart[0].id));
        assert_eq!(smart[0].search, results);
    }
}
