//! Create Subclip (the Trimmer's button and an event's menu), as VEGAS's
//! Help describes it: a name (the file's, a number appended) and Reverse;
//! OK puts the subclip in Project Media (`bv_timeline::subclips`).

use crate::i18n::tr;
use bv_core::{Tick, TimeRange};
use bv_timeline::MediaId;

use crate::app::BananaApp;
use crate::egui::{self, vec2};

/// The Create Subclip dialog.
#[derive(Debug)]
pub struct SubclipDialog {
    parent: MediaId,
    range: TimeRange,
    name: String,
    reverse: bool,
}

impl SubclipDialog {
    /// A subclip of `parent` over `range` (its time).
    pub fn new(app: &BananaApp, parent: MediaId, range: TimeRange) -> SubclipDialog {
        SubclipDialog {
            parent,
            range,
            name: app.project().next_subclip_name(parent),
            reverse: false,
        }
    }

    /// Draws the dialog; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        crate::floating::Window::new(tr("Create Subclip"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Name:"));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(260.0));
                    ok |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                ui.checkbox(&mut self.reverse, tr("Reverse"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ok |= ui
                        .add(egui::Button::new("OK").min_size(vec2(72.0, 0.0)))
                        .clicked();
                    cancel = ui
                        .add(egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)))
                        .clicked();
                });
            });
        if ok {
            let (parent, range, reverse) = (self.parent, self.range, self.reverse);
            let name = if self.name.trim().is_empty() {
                app.project().next_subclip_name(parent)
            } else {
                self.name.clone()
            };
            match app.modify(|p| p.add_subclip(parent, range, reverse, &name)) {
                Some(_) => app.notify(trf!("Subclip {} added to Project Media.", name.trim())),
                None => app.notify(tr(
                    "A subclip needs a part of its media at least a frame long.",
                )),
            }
            return false;
        }
        open && !cancel
    }
}

impl BananaApp {
    /// Create Subclip from the Trimmer's selection or an event's part.
    pub(crate) fn open_subclip_dialog(&mut self, parent: MediaId, range: TimeRange) {
        if range.duration <= Tick::ZERO {
            return;
        }
        self.menu_dialogs.more.subclip = Some(SubclipDialog::new(self, parent, range));
    }

    /// Opens media `id` in the Trimmer; a subclip opens alone, its part
    /// the whole of what it shows (VEGAS's Open Parent Media in Trimmer
    /// and the Trimmer's Select Parent Media open its parent).
    pub(crate) fn open_in_trimmer(&mut self, id: MediaId) {
        self.trimmer.open(id);
    }
}

impl BananaApp {
    /// Project Media with a subclip of the first media (after it) and
    /// Create Subclip open on another part (screen tests).
    pub fn subclips_for_review(&mut self) {
        let s = Tick::from_seconds;
        let Some(parent) = self.project().media.first().map(|m| m.id) else {
            return;
        };
        let name = self.project().next_subclip_name(parent);
        self.modify(|p| p.add_subclip(parent, TimeRange::new(s(1), s(2)), false, &name));
        self.open_subclip_dialog(parent, TimeRange::new(s(3), s(1)));
        self.show_tab(crate::dock::Tab::ProjectMedia, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reversed_subclip_places_its_part_backward() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let parent = app.project().media[0].id;
        let sub = app
            .modify(|p| p.add_subclip(parent, TimeRange::new(s(2), s(3)), true, "Part"))
            .unwrap();
        let before = app.project().clips().count();
        app.place_media(sub, s(40), None);
        assert_eq!(app.project().clips().count(), before + 1);
        let c = app
            .project()
            .clips()
            .map(|(_, c)| c.clone())
            .find(|c| c.start == s(40))
            .unwrap();
        assert_eq!((c.media, c.source_in, c.duration), (parent, s(2), s(3)));
        assert!(c.reverse.is_some(), "backward");
        // Opened in the Trimmer: alone, from its start.
        app.open_in_trimmer(sub);
        assert_eq!(app.trimmer.media_id(), Some(sub));
        assert_eq!(app.trimmer.marks(), (s(0), None, None));
        // Removed alone.
        app.request_remove_media(sub);
        assert!(app.project().subclip(sub).is_none());
        assert!(app.project().media(parent).is_some());
    }
}
