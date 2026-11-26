//! The program's side of AI agents ([`crate::agents`]): a BananaV without a
//! window made safe for them (nothing of the user's written), the edits
//! they make as undo steps, their saves and renders, and — in the open
//! window, while Options › BananaV Mods › AI agents is on — the listener
//! whose calls run between frames, what waits for the user's yes, and the
//! status bar's notice.

use std::path::{Path, PathBuf};
use std::time::Instant;

use bv_engine::{RenderProgress, RenderSettings};
use bv_timeline::Project;

use super::BananaApp;
use crate::agents::link::{Call, Host};
use crate::egui;
use crate::i18n::tr;

/// What the agents have going on.
#[derive(Debug, Default)]
pub(crate) struct AgentState {
    /// A BananaV of the agents' own, without a window.
    pub headless: bool,
    /// The listener, while agents are allowed.
    host: Option<Host>,
    /// Why the listener could not start (until agents are turned off).
    pub failed: Option<String>,
    /// Calls waiting for the user's yes, the first one asked now.
    asks: Vec<(String, Call)>,
    /// The render an agent started: its number, its file, how it went.
    render: Option<(u64, PathBuf, Option<RenderProgress>)>,
    renders: u64,
    /// Where the listener writes its address, when not beside the
    /// settings (tests and screen captures: a file of their own).
    pub address: Option<PathBuf>,
}

impl BananaApp {
    /// Makes this BananaV one for agents without a window: the user's
    /// settings, keyboard map and plug-ins read but never written, no
    /// autosave, session, update check, proxies or peak files, the
    /// welcome screen closed; the OFX plug-ins the program found, from its
    /// cache.
    pub(crate) fn agent_headless(&mut self) {
        self.agents.headless = true;
        self.settings = self.settings.unsaved();
        self.keymap = crate::keymap::Keymap::in_memory();
        self.fx_packages_for_review();
        self.welcome_for_review("closed");
        let prefs = &mut self.settings.prefs;
        prefs.autosave = false;
        prefs.check_updates = false;
        prefs.auto_proxies = crate::settings::AutoProxies::Off;
        prefs.release_media_inactive = false;
        bv_engine::Engine::set_peak_files(false, false);
        // Not in the unit tests: the registry is the whole process's, and
        // the user's plug-ins would show in every other test's lists.
        if !cfg!(test) {
            self.ofx.known_from_cache();
        }
    }

    /// Whether work goes on between calls (a render).
    pub(crate) fn agent_busy(&self) -> bool {
        self.agent_render_running()
    }

    /// An edit as one undo step; its error (translated when it can be),
    /// the project as it was.
    pub(crate) fn agent_edit<R, E: std::fmt::Display>(
        &mut self,
        f: impl FnOnce(&mut Project) -> Result<R, E>,
    ) -> Result<R, String> {
        self.end_timeline_gesture();
        self.history
            .apply(f)
            .map_err(|e| tr(&e.to_string()).to_string())
    }

    /// An edit that does not fail, as one undo step.
    pub(crate) fn agent_modify<R>(&mut self, f: impl FnOnce(&mut Project) -> R) -> R {
        self.modify(f)
    }

    /// The project as it is now taken as the saved one (a new project
    /// whose settings were just given).
    pub(crate) fn agent_saved_now(&mut self) {
        let p = self.project().clone();
        self.set_saved(p);
    }

    /// The error the status bar got since `mark` (the interface's own
    /// calls report there).
    pub(crate) fn status_error_since(&self, mark: Instant) -> Option<String> {
        match &self.status {
            Some((msg, at, true)) if *at >= mark => Some(msg.clone()),
            _ => None,
        }
    }

    /// Saves the project to `path` as BananaV's own (its file from now on),
    /// as File › Save does.
    pub(crate) fn agent_save(&mut self, path: &Path) -> Result<(), String> {
        crate::safe_file::save(path, self.project().to_json().as_bytes())
            .map_err(|e| trf!("Could not save: {}", e))?;
        let p = self.project().clone();
        self.set_saved(p);
        self.autosave_forget_copy();
        self.notify(trf!("Project saved to {}", path.display()));
        self.settings.touch_recent(path);
        self.project_path = Some(path.to_path_buf());
        Ok(())
    }

    /// The engine given the project as it is now (a frame drawn at once).
    pub(crate) fn agent_sync_engine(&mut self) {
        let rev = self.history.revision();
        if self.engine_synced != Some(rev) {
            self.engine.set_project(self.history.current());
            self.engine_synced = Some(rev);
        }
    }

    pub(crate) fn agent_render_running(&self) -> bool {
        self.engine.render_progress().is_some_and(|p| !p.finished)
    }

    /// Starts the render `s` as the Render As window does (in the window,
    /// its progress window shows): its number.
    pub(crate) fn agent_start_render(&mut self, s: RenderSettings) -> u64 {
        self.agent_sync_engine();
        crate::logs::log(
            crate::logs::LogKind::Renders,
            &crate::logs::render_lines(&s),
        );
        let path = s.path.clone();
        self.engine.start_render(s);
        if !self.agents.headless {
            self.dialogs.rendering =
                Some(crate::render::progress::ProgressWindow::new(path.clone()));
        }
        self.agents.renders += 1;
        let job = self.agents.renders;
        self.agents.render = Some((job, path, None));
        job
    }

    /// The agent's render: its number, its file, how it goes.
    pub(crate) fn agent_render_status(&mut self) -> Option<(u64, PathBuf, Option<RenderProgress>)> {
        self.keep_render_state();
        self.agents.render.clone()
    }

    /// The render's state kept before its window forgets it.
    fn keep_render_state(&mut self) {
        if let Some(r) = &mut self.agents.render
            && let Some(p) = self.engine.render_progress()
        {
            r.2 = Some(p);
        }
    }

    /// At every frame of the open window: the listener on or off with the
    /// mod, the calls waiting run (or asked about), the question shown.
    pub(crate) fn serve_agents(&mut self, ctx: &egui::Context) {
        if self.agents.headless {
            return;
        }
        self.keep_render_state();
        let wanted = self.settings.mods.agents;
        if wanted && self.agents.host.is_none() && self.agents.failed.is_none() {
            let wake = ctx.clone();
            let wake = move || wake.request_repaint();
            let started = match self.agents.address.clone() {
                Some(file) => Host::start_at(wake, Some(file)),
                None => Host::start(wake),
            };
            match started {
                Ok(h) => self.agents.host = Some(h),
                Err(e) => self.agents.failed = Some(e.to_string()),
            }
        }
        if !wanted {
            self.agents.failed = None;
            if self.agents.host.take().is_some() {
                for (_, call) in self.agents.asks.drain(..) {
                    let _ = call
                        .reply
                        .send(Err(tr("AI agents were turned off in BananaV.").into()));
                }
            }
        }
        let mut calls = Vec::new();
        if let Some(h) = &self.agents.host {
            while let Some(c) = h.take() {
                calls.push(c);
            }
        }
        for call in calls {
            match crate::agents::tools::question(self, &call.tool, &call.args) {
                Some(q) => self.agents.asks.push((q, call)),
                None => {
                    let out = crate::agents::tools::call(self, &call.tool, &call.args, true);
                    let _ = call.reply.send(out);
                }
            }
        }
        self.agent_question(ctx);
    }

    /// The first call waiting for the user's yes: what it asks, Allow and
    /// Deny (Esc).
    fn agent_question(&mut self, ctx: &egui::Context) {
        let Some((question, _)) = self.agents.asks.first() else {
            return;
        };
        let question = question.clone();
        let mut answer = None;
        crate::floating::Window::new(tr("AI agent"))
            .id(egui::Id::new("agent-question"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.add(egui::Label::new(question.as_str()).wrap());
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Deny")).clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        answer = Some(false);
                    }
                    if ui.button(tr("Allow")).clicked() {
                        answer = Some(true);
                    }
                });
            });
        if let Some(yes) = answer {
            let (_, call) = self.agents.asks.remove(0);
            let out = if yes {
                crate::agents::tools::call(self, &call.tool, &call.args, true)
            } else {
                Err(tr("The user said no.").into())
            };
            let _ = call.reply.send(out);
        }
    }

    /// The status bar's notice while an agent is connected.
    pub(crate) fn agents_status(&self, ui: &mut egui::Ui) {
        let Some(h) = &self.agents.host else {
            return;
        };
        let n = h.clients();
        if n > 0 {
            ui.label(trn!(
                n,
                "{} AI agent connected",
                "{} AI agents connected",
                n
            ));
            ui.separator();
        }
    }

    /// Whether the window listens for agents now, and why not when it
    /// could not (the Mods window).
    pub(crate) fn agents_listening(&self) -> (bool, Option<&str>) {
        (self.agents.host.is_some(), self.agents.failed.as_deref())
    }

    /// For screen tests: Options › BananaV Mods open with AI agents on,
    /// the listener's address in `file` (never the user's settings
    /// folder), the settings never saved.
    pub fn agents_for_review(&mut self, file: PathBuf) {
        self.settings = self.settings.unsaved();
        self.agents.address = Some(file);
        self.settings.mods.agents = true;
        self.open_mods();
    }

    /// For tests: the calls waiting for the user's yes.
    #[cfg(test)]
    pub(crate) fn agent_asks(&self) -> usize {
        self.agents.asks.len()
    }

    /// For tests: the first question answered.
    #[cfg(test)]
    pub(crate) fn agent_answer(&mut self, yes: bool) {
        if self.agents.asks.is_empty() {
            return;
        }
        let (_, call) = self.agents.asks.remove(0);
        let out = if yes {
            crate::agents::tools::call(self, &call.tool, &call.args, true)
        } else {
            Err(tr("The user said no.").into())
        };
        let _ = call.reply.send(out);
    }
}
