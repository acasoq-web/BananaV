//! Copies of generated media events, as VEGAS's Help ("Adding generated
//! media to your project") describes them: events pasted, copied by the
//! right-drag's Copy Here or dragged from Project Media while another
//! event shows their generated media get a question — a new copy of the
//! source media for each (edited on its own) or a reference to the
//! original (a change to either event changes both).

use bv_timeline::{ClipId, MediaSource, Project};

use crate::app::BananaApp;
use crate::egui;
use crate::i18n::tr;

/// The question, for the events just placed.
#[derive(Debug)]
pub(crate) struct Question {
    clips: Vec<ClipId>,
    /// The answer picked: a new copy (`true`) or a reference.
    copy: bool,
    /// The history's edits when asked: the answer joins that edit's undo
    /// step while nothing came after it.
    edits: u64,
}

/// The events from id `mark` on that show generated media an older event
/// shows too.
fn shared_copies(p: &Project, mark: u64) -> Vec<ClipId> {
    let generated = |id| {
        p.media(id)
            .is_some_and(|m| matches!(m.source, MediaSource::Generated(_)) && !m.is_adjustment())
    };
    p.clips()
        .map(|(_, c)| c)
        .filter(|c| {
            c.id.0 >= mark
                && generated(c.media)
                && p.clips().any(|(_, o)| o.id.0 < mark && o.media == c.media)
        })
        .map(|c| c.id)
        .collect()
}

impl BananaApp {
    /// After an edit that placed events (their ids from `mark` on, the
    /// project's id counter before it): asks about those showing
    /// generated media another event shows too.
    pub(crate) fn ask_generated_copies(&mut self, mark: u64) {
        let clips = shared_copies(self.project(), mark);
        if !clips.is_empty() {
            self.generated_copies = Some(Question {
                clips,
                copy: true,
                edits: self.history.edits(),
            });
        }
    }

    /// The question while it is asked; OK gives each event a copy of its
    /// own when asked to.
    pub(crate) fn generated_copies_dialog(&mut self, ctx: &egui::Context) {
        let Some(q) = &mut self.generated_copies else {
            return;
        };
        let mut done = false;
        egui::Modal::new(egui::Id::new("generated-copies")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.heading(tr("Copying Generated Media"));
            ui.add_space(4.0);
            ui.label(tr(
                "The events placed show generated media another event shows too.",
            ));
            ui.add_space(6.0);
            let _ = ui
                .radio_value(
                    &mut q.copy,
                    true,
                    tr("Create a new copy of the source media"),
                )
                .on_hover_text(tr("Each event can be edited on its own."));
            let _ = ui
                .radio_value(
                    &mut q.copy,
                    false,
                    tr("Create a reference to the original media"),
                )
                .on_hover_text(tr("A change to either event changes both."));
            ui.add_space(8.0);
            if ui
                .add(egui::Button::new(tr("OK")).min_size(egui::vec2(72.0, 0.0)))
                .clicked()
            {
                done = true;
            }
        });
        if !done {
            return;
        }
        let Some(q) = self.generated_copies.take() else {
            return;
        };
        if !q.copy {
            return;
        }
        let copy = |p: &mut Project| {
            for &c in &q.clips {
                p.copy_generated_media(c);
            }
        };
        if self.history.edits() == q.edits {
            self.history.amend(copy);
        } else {
            self.modify(copy);
        }
    }
}
