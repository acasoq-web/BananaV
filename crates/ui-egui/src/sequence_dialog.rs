//! File › Import › Image Sequence: numbered pictures (a 3D render's
//! `shot_0001.png`…) taken as one video event, as VEGAS's Open dialog does
//! with "Open sequence": the first picture is chosen, then the last one's
//! number and the rate the pictures run at.

use std::path::PathBuf;

use bv_core::FrameRate;
use bv_engine::{Engine, IMAGE_EXTENSIONS};
use bv_timeline::ImageSequence;

use crate::app::BananaApp;
use crate::egui::{self, Align, Layout, RichText, vec2};
use crate::i18n::tr;

/// The rates offered (the project's first).
const RATES: [FrameRate; 9] = [
    FrameRate::FPS_23_976,
    FrameRate::FPS_24,
    FrameRate::FPS_25,
    FrameRate::FPS_29_97,
    FrameRate::FPS_30,
    FrameRate::new(48, 1),
    FrameRate::FPS_50,
    FrameRate::FPS_59_94,
    FrameRate::FPS_60,
];

#[derive(Debug)]
pub struct ImageSequenceDialog {
    first: PathBuf,
    /// The first picture's number.
    from: u64,
    /// How many pictures follow it without a gap.
    found: u32,
    /// The last picture's number.
    last: u64,
    rate: FrameRate,
}

/// Asks for the first picture and opens the dialog (nothing when the name
/// has no number).
pub fn open(app: &mut BananaApp) {
    let Some(first) = rfd::FileDialog::new()
        .set_title(tr("Import Image Sequence"))
        .add_filter(tr("Image Files"), IMAGE_EXTENSIONS)
        .add_filter(tr("All Files"), &["*"])
        .pick_file()
    else {
        return;
    };
    open_with(app, first);
}

/// Opens the dialog for the run that starts at `first`.
pub fn open_with(app: &mut BananaApp, first: PathBuf) {
    let path = first.to_string_lossy().to_string();
    let Some((_, _, from)) = ImageSequence::number_of(&path) else {
        app.report_error(tr(
            "The picture's name has no number: an image sequence's pictures are numbered (shot_0001.png, shot_0002.png…).",
        ));
        return;
    };
    let found = Engine::sequence_length(&path).max(1);
    app.dialogs.image_sequence = Some(ImageSequenceDialog {
        first,
        from,
        found,
        last: from + found as u64 - 1,
        rate: app.project().settings.frame_rate,
    });
}

impl ImageSequenceDialog {
    /// Shows the dialog; false once it closes.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut ok = false;
        let mut cancel = false;
        let count = (self.last.saturating_sub(self.from) + 1).min(u32::MAX as u64) as u32;
        crate::floating::Window::new(tr("Image Sequence"))
            .id(egui::Id::new("image-sequence"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                egui::Grid::new("image-sequence-grid")
                    .num_columns(2)
                    .spacing(vec2(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.label(tr("First image:"));
                        let name = self
                            .first
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().to_string());
                        ui.label(RichText::new(name).strong());
                        ui.end_row();
                        ui.label(tr("Last image:"));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.last)
                                    .range(self.from..=self.from + 9_999_999),
                            );
                            ui.label(trn!(
                                self.found,
                                "({} picture found in a row)",
                                "({} pictures found in a row)",
                                self.found
                            ));
                        });
                        ui.end_row();
                        ui.label(tr("Frame rate:"));
                        let project = app.project().settings.frame_rate;
                        let text = |r: FrameRate| {
                            format!("{} fps", crate::render::templates::rate_text(r))
                        };
                        egui::ComboBox::from_id_salt("image-sequence-rate")
                            .selected_text(text(self.rate))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.rate,
                                    project,
                                    trf!("{} (project)", text(project)),
                                );
                                for r in RATES {
                                    if r != project {
                                        ui.selectable_value(&mut self.rate, r, text(r));
                                    }
                                }
                            });
                        ui.end_row();
                    });
                ui.add_space(6.0);
                let length = self.rate.frame_start(count as i64);
                ui.label(
                    RichText::new(trn!(
                        count,
                        "{} picture, {} long. Missing ones show black.",
                        "{} pictures, {} long. Missing ones show black.",
                        count,
                        crate::render::progress::clock(length.as_seconds_f64())
                    ))
                    .color(crate::theme::text_dim()),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(tr("Cancel")).min_size(vec2(80.0, 24.0)))
                            .clicked()
                        {
                            cancel = true;
                        }
                        if ui
                            .add(egui::Button::new("OK").min_size(vec2(80.0, 24.0)))
                            .clicked()
                        {
                            ok = true;
                        }
                    });
                });
            });
        if ok {
            let first = self.first.to_string_lossy().to_string();
            match Engine::probe_sequence(&first, count, self.rate) {
                Ok(info) => {
                    let id = app.modify(|p| p.add_media(info));
                    app.place_media(id, app.cursor, None);
                    app.notify(trn!(
                        count,
                        "Image sequence of {} picture imported.",
                        "Image sequence of {} pictures imported.",
                        count
                    ));
                }
                Err(e) => app.report_error(format!(
                    "{}: {}",
                    self.first
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().to_string()),
                    crate::media_files::error_text(&e)
                )),
            }
            return false;
        }
        open && !cancel
    }
}
