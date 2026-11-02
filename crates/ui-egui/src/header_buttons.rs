//! More › Edit Visible Button Set, as VEGAS 23's (the user's prints): a
//! check box for each button the headers of a kind of track can show —
//! Mute and Solo among them, the only ones on at first — with Show All and
//! Hide All beside them and OK and Cancel at the bottom right
//! (`settings::HeaderButtons`). `timeline/headers.rs` lays the buttons out
//! (`header_button`), and its More menus have the commands of those a
//! header does not show.

use bv_timeline::TrackKind;

use crate::app::BananaApp;
use crate::egui::{self, vec2};
use crate::i18n::tr;
use crate::settings::HeaderButton;

/// The dialog: the kind it edits and the boxes ticked.
#[derive(Clone, Debug)]
pub(crate) struct ButtonsDialog {
    kind: TrackKind,
    shown: Vec<HeaderButton>,
}

/// Its buttons' size, as VEGAS's.
const BUTTON: egui::Vec2 = vec2(96.0, 26.0);

impl BananaApp {
    /// Opens Edit Visible Button Set for `kind`'s headers.
    pub(crate) fn open_header_buttons(&mut self, kind: TrackKind) {
        self.header_buttons_dialog = Some(ButtonsDialog {
            kind,
            shown: self.settings.header_buttons.of(kind),
        });
    }

    /// The dialog, while it is open.
    pub(crate) fn header_buttons_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.header_buttons_dialog.clone() else {
            return;
        };
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("Edit Visible Button Set…"))
            .id(egui::Id::new("header-buttons"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(456.0);
                ui.label(match d.kind {
                    TrackKind::Video => {
                        tr("Specify the controls you want to appear in all video track headers.")
                    }
                    TrackKind::Audio => {
                        tr("Specify the controls you want to appear in all audio track headers.")
                    }
                });
                ui.add_space(16.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for b in HeaderButton::of(d.kind) {
                            let mut on = d.shown.contains(b);
                            crate::widgets::checkbox(ui, &mut on, b.name());
                            if on && !d.shown.contains(b) {
                                d.shown.push(*b);
                            } else if !on {
                                d.shown.retain(|x| x != b);
                            }
                        }
                    });
                    // At the right, their names centered.
                    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                        if ui
                            .add_sized(BUTTON, egui::Button::new(tr("Show All")))
                            .clicked()
                        {
                            d.shown = HeaderButton::of(d.kind).to_vec();
                        }
                        ui.add_space(8.0);
                        if ui
                            .add_sized(BUTTON, egui::Button::new(tr("Hide All")))
                            .clicked()
                        {
                            d.shown.clear();
                        }
                    });
                });
                ui.add_space(12.0);
                // One row (a bare right-to-left layout takes all the
                // height there is).
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        cancel = ui
                            .add(egui::Button::new(tr("Cancel")).min_size(BUTTON))
                            .clicked();
                        ok = ui
                            .add(egui::Button::new(tr("OK")).min_size(BUTTON))
                            .clicked();
                    });
                });
            });
        if ok {
            self.settings.header_buttons.set(d.kind, &d.shown);
        }
        self.header_buttons_dialog = (open && !ok && !cancel).then_some(d);
    }

    /// Edit Visible Button Set open for video tracks, VEGAS's buttons
    /// shown (screen tests; the settings in memory).
    pub fn header_buttons_for_review(&mut self) -> bool {
        self.settings = crate::settings::UiSettings::in_memory();
        self.open_header_buttons(TrackKind::Video);
        true
    }

    /// The track list dragged to `width` (screen tests; the settings in
    /// memory).
    pub fn track_list_width_for_review(&mut self, width: f32) {
        if !self.settings.is_in_memory() {
            self.settings = crate::settings::UiSettings::in_memory();
        }
        self.settings.track_list_width = width;
    }
}

#[cfg(test)]
mod tests {
    use crate::egui::{Rect, pos2, vec2};
    use crate::settings::{HeaderButton, HeaderButtons};
    use crate::timeline::TimelineView;
    use bv_timeline::TrackKind;

    /// The track list's width moves the lanes' left edge (past the
    /// splitter); never past its least or its most.
    #[test]
    fn the_track_list_width_places_the_lanes() {
        use crate::timeline::{HEADER_W, SPLIT_W};
        let p = bv_timeline::Project::new(bv_timeline::ProjectSettings::default());
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 700.0));
        let mut view = TimelineView::default();
        let l = view.layout_for_test(rect, &p);
        assert!((l.0 - (HEADER_W + SPLIT_W)).abs() < 1e-3);
        assert!((l.1 - HEADER_W).abs() < 1e-3);
        view.header_w = 90.0;
        let l = view.layout_for_test(rect, &p);
        assert!((l.0 - (90.0 + SPLIT_W)).abs() < 1e-3);
    }

    /// Mute and Solo at first, as VEGAS's; what is ticked, in the
    /// dialog's order; settings saved before Mute and Solo were buttons
    /// keep them, Minimize or Maximize becoming Min/Max.
    #[test]
    fn the_headers_show_what_is_ticked() {
        let mut b = HeaderButtons::default();
        assert_eq!(b.of(TrackKind::Video), HeaderButton::DEFAULT);
        assert!(b.is_empty());
        b.set(
            TrackKind::Video,
            &[HeaderButton::Solo, HeaderButton::TrackFx],
        );
        assert_eq!(
            b.of(TrackKind::Video),
            [HeaderButton::TrackFx, HeaderButton::Solo]
        );
        assert_eq!(b.of(TrackKind::Audio), HeaderButton::DEFAULT);
        let text = serde_json::to_string(&b).unwrap();
        let back: HeaderButtons = serde_json::from_str(&text).unwrap();
        assert_eq!(back, b);
        b.set(TrackKind::Video, &[]);
        assert!(b.of(TrackKind::Video).is_empty());
        b.set(TrackKind::Video, &HeaderButton::DEFAULT);
        assert!(b.is_empty());

        let older: HeaderButtons = serde_json::from_str(
            r#"{"video":["Minimize","TrackMotion","Maximize"],"audio":["ArmForRecord"]}"#,
        )
        .unwrap();
        assert_eq!(
            older.of(TrackKind::Video),
            [
                HeaderButton::TrackMotion,
                HeaderButton::Mute,
                HeaderButton::Solo,
                HeaderButton::MinMax,
            ]
        );
        assert_eq!(
            older.of(TrackKind::Audio),
            [
                HeaderButton::ArmForRecord,
                HeaderButton::Mute,
                HeaderButton::Solo,
            ]
        );
    }
}
