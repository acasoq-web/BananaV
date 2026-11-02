//! Dialog windows: rename marker, shortcuts and about (File › Properties
//! lives in `project_properties.rs`, File › Render As in `render`).

use crate::i18n::tr;

use bv_timeline::MarkerId;

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::theme;

#[derive(Debug, Default)]
pub struct Dialogs {
    /// File › Properties.
    pub project_properties: Option<crate::project_properties::ProjectPropertiesDialog>,
    /// File › Render As.
    pub render: Option<crate::render::RenderDialog>,
    /// The progress of a render.
    pub rendering: Option<crate::render::progress::ProgressWindow>,
    /// File › Import › Image Sequence.
    pub image_sequence: Option<crate::sequence_dialog::ImageSequenceDialog>,
    /// Project Media's Properties: a media file's.
    pub media_properties: Option<crate::media_properties::MediaPropertiesDialog>,
    /// An event's Properties (its shortcut menu's).
    pub event_properties: Option<crate::event_properties::EventPropertiesDialog>,
    pub about: bool,
    pub shortcuts: bool,
    pub rename: Option<(MarkerId, String)>,
}

impl Dialogs {
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        if let Some(d) = &mut self.project_properties
            && !d.show(ctx, app)
        {
            self.project_properties = None;
        }
        if let Some(d) = &mut self.render {
            match d.show(ctx, app) {
                crate::render::Outcome::Open => {}
                crate::render::Outcome::Closed => self.render = None,
                crate::render::Outcome::Started(file) => {
                    // Render to New Track: its file goes on new tracks once
                    // rendered.
                    app.new_track_render = d.new_track_at();
                    // The captions beside the file (not for a new track's).
                    let scc = d
                        .started_at()
                        .filter(|_| app.new_track_render.is_none())
                        .and_then(|from| {
                            crate::render::progress::captions_file(app.project(), from)
                        });
                    self.render = None;
                    self.rendering =
                        Some(crate::render::progress::ProgressWindow::new(file).with_captions(scc));
                }
            }
        }
        if let Some(w) = &mut self.rendering
            && !w.show(ctx, app)
        {
            self.rendering = None;
        }
        crate::render::place_new_track(app);
        app.follow_real_time_render();
        app.fx_automation_window(ctx);
        if let Some(d) = &mut self.image_sequence
            && !d.show(ctx, app)
        {
            self.image_sequence = None;
        }
        if let Some(d) = &mut self.media_properties
            && !d.show(ctx, app)
        {
            self.media_properties = None;
        }
        if let Some(d) = &mut self.event_properties
            && !d.show(ctx, app)
        {
            self.event_properties = None;
        }
        self.rename_window(ctx, app);
        if self.about {
            crate::floating::Window::new(tr("About BananaV"))
                .collapsible(false)
                .resizable(false)
                .open(&mut self.about)
                .show(ctx, |ui| {
                    ui.heading(RichText::new("BananaV 0.1").color(theme::banana()));
                    ui.label(tr("Video editor written in Rust."));
                    ui.label(tr("Compositing in linear light, 32-bit float."));
                    ui.label(tr("Time in ticks: 254,016,000,000 per second."));
                    ui.label(tr(
                        "Formats written from scratch from the public specifications.",
                    ));
                });
        }
        if self.shortcuts {
            let space_play_stop = app.settings.prefs.space_play_stop;
            crate::floating::Window::new(tr("Keyboard shortcuts"))
                .collapsible(false)
                .open(&mut self.shortcuts)
                .show(ctx, |ui| {
                    // Space does what Options › Preferences › General says,
                    // Enter the other.
                    let stop = tr("Play / stop (returns to where playback started)");
                    let pause = tr("Play / pause (stays where it stopped)");
                    let (space, enter) = if space_play_stop {
                        (stop, pause)
                    } else {
                        (pause, stop)
                    };
                    egui::Grid::new("atalhos").striped(true).show(ui, |ui| {
                        for (k, d) in [
                            (tr("Space"), space),
                            ("Enter", enter),
                            (tr("Shift+Space"), tr("Play from start")),
                            ("Q", tr("Loop on/off")),
                            ("← / →", tr("Previous / next frame")),
                            ("Ctrl+← / Ctrl+→", tr("Previous / next edit point")),
                            ("Home / End", tr("Project start / end")),
                            ("↑ / ↓", tr("Timeline zoom")),
                            ("S", tr("Split at cursor")),
                            ("Delete", tr("Delete selection")),
                            ("Alt+[ / Alt+]", tr("Trim start / end to cursor")),
                            ("Ctrl+T", tr("Trim to loop region")),
                            ("M / R", tr("Insert marker / region")),
                            ("G / U", tr("Group / ungroup")),
                            ("Ctrl+X / C / V", tr("Cut / copy / paste")),
                            ("Ctrl+Z / Ctrl+Y", tr("Undo / redo")),
                            ("Ctrl+L", "Auto ripple"),
                            ("F8", "Snapping"),
                            ("Ctrl+R", tr("Render As…")),
                            (tr("Mouse wheel"), tr("Zoom the timeline")),
                            (tr("Shift+wheel"), tr("Scroll the timeline")),
                        ] {
                            ui.label(RichText::new(k).monospace().color(theme::banana()));
                            ui.label(d);
                            ui.end_row();
                        }
                    });
                });
        }
    }

    fn rename_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((id, name)) = &mut self.rename else {
            return;
        };
        let mut done = false;
        let mut open = true;
        crate::floating::Window::new(tr("Rename"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let r = ui.text_edit_singleline(name);
                r.request_focus();
                if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = true;
                }
            });
        if done {
            let (id, name) = (*id, name.clone());
            app.modify(|p| p.rename_marker(id, name));
        }
        if done || !open {
            self.rename = None;
        }
    }
}
