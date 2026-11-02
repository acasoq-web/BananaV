//! Folders that take no files, explained so the user can act: above all
//! Windows Security's Controlled folder access, which keeps apps it does not
//! know from writing to Documents, Pictures, Videos, Music and Desktop (they
//! only get "file not found" or "access denied").

use std::path::Path;

use crate::egui::{self, Align, Layout, Ui, vec2};
use crate::i18n::tr;

/// Why files cannot be written in a folder.
#[derive(Clone, Debug)]
pub struct FolderProblem {
    text: String,
    /// Controlled folder access refused BananaV.
    protected: bool,
}

impl FolderProblem {
    /// What keeps files out of `dir`; `None` when it takes them.
    pub fn of(dir: &Path) -> Option<FolderProblem> {
        let e = bv_platform::try_write(dir).err()?;
        Some(FolderProblem::new(dir, &e))
    }

    pub fn new(dir: &Path, e: &std::io::Error) -> FolderProblem {
        if bv_platform::blocked_by_folder_protection(dir, e) {
            FolderProblem::blocked(dir)
        } else {
            FolderProblem {
                text: trf!("BananaV cannot save files in {}: {}", dir.display(), e),
                protected: false,
            }
        }
    }

    /// Controlled folder access keeping BananaV out of `dir` (for screen
    /// tests, which cannot turn the protection on).
    pub fn blocked(dir: &Path) -> FolderProblem {
        FolderProblem {
            text: trf!(
                "Windows Security's Controlled folder access does not let BananaV save files in {}.",
                dir.display()
            ),
            protected: true,
        }
    }

    /// Controlled folder access refused BananaV.
    pub fn protected(&self) -> bool {
        self.protected
    }

    /// How to allow BananaV (for the protection).
    fn how(&self) -> Option<&'static str> {
        self.protected.then(|| {
            tr(
                "To save there, allow BananaV in Windows Security › Virus & threat protection › Manage ransomware protection › Allow an app through Controlled folder access, or choose another folder.",
            )
        })
    }

    /// The explanation and how to act, in one text (the status bar's).
    pub fn message(&self) -> String {
        match self.how() {
            Some(how) => format!("{} {how}", self.text),
            None => self.text.clone(),
        }
    }

    /// The explanation and, for the protection, how to allow BananaV.
    pub fn explain(&self, ui: &mut Ui) {
        ui.add(egui::Label::new(&self.text).wrap());
        if let Some(how) = self.how() {
            ui.add_space(6.0);
            ui.add(egui::Label::new(how).wrap());
        }
    }

    /// "Open Windows Security", at its ransomware protection page (only for
    /// the protection).
    pub fn security_button(&self, ui: &mut Ui) {
        if self.protected
            && ui
                .add(egui::Button::new(tr("Open Windows Security")).min_size(vec2(96.0, 24.0)))
                .clicked()
        {
            let _ = std::process::Command::new("explorer")
                .arg("windowsdefender://ransomwareprotection/")
                .spawn();
        }
    }

    /// The explanation in a window of its own; false once OK closes it.
    pub fn window(&self, ctx: &egui::Context, id: &str) -> bool {
        let mut open = true;
        crate::floating::Window::new(tr("Render"))
            .id(egui::Id::new(id))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(420.0);
                self.explain(ui);
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("OK").min_size(vec2(96.0, 24.0)))
                            .clicked()
                        {
                            open = false;
                        }
                        self.security_button(ui);
                    });
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    open = false;
                }
            });
        open
    }
}
