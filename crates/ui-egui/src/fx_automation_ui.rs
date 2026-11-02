//! FX automation's interface (VEGAS's FX Automation Chooser): which of a
//! track's — or a bus's or a chain's — audio effects' parameters (and
//! their Bypass; a VST 3 effect's as its plug-in describes them, those it
//! shows and lets be changed) have envelopes; Insert › Audio Envelopes › FX
//! Automation… and a track header's FX Automation Envelopes open it. OK
//! adds an envelope for each parameter ticked (holding its value now) and
//! takes away those cleared, in one undo.

use bv_timeline::fx_automation::{Automated, automatable};
use bv_timeline::{EnvelopeKind, EnvelopeOwner};

use crate::app::BananaApp;
use crate::egui::{self, vec2};
use crate::i18n::tr;

/// The FX Automation Chooser while it is open.
#[derive(Debug)]
pub(crate) struct FxAutomationChooser {
    owner: EnvelopeOwner,
    /// The effect whose parameters show.
    picked: usize,
    /// What of each effect is ticked.
    wanted: Vec<Vec<Automated>>,
}

/// The rows the chooser and a header's menu offer for effect `e`: its
/// Bypass, then its parameters (a VST 3 effect's by id, titled by its
/// plug-in).
fn rows_of(e: &bv_timeline::Effect) -> Vec<(Automated, String)> {
    let mut rows: Vec<(Automated, String)> = vec![(None, tr("Bypass").to_string())];
    match e.vst.as_deref() {
        Some(v) => rows.extend(
            crate::vst_ui::param_infos(v)
                .into_iter()
                .filter(|p| p.shown() && !p.read_only() && !p.bypass())
                .map(|p| (Some(p.id), p.title)),
        ),
        None => rows.extend(
            e.kind
                .params()
                .iter()
                .enumerate()
                .map(|(k, s)| (Some(k as u32), tr(s.name).to_string())),
        ),
    }
    rows
}

/// Before effect `i` of `owner` is automated as `wanted`: a VST 3
/// effect's automated parameters get their names, and those it never set
/// their plug-in's defaults, so the envelopes start where they are.
fn name_vst_params(
    p: &mut bv_timeline::Project,
    owner: EnvelopeOwner,
    i: usize,
    wanted: &[Automated],
) {
    let Some(v) = p
        .fx_chain_and_envelopes_mut(owner)
        .and_then(|(effects, _)| effects.get_mut(i))
        .and_then(|e| e.vst.as_deref_mut())
    else {
        return;
    };
    let infos = crate::vst_ui::param_infos(v);
    for id in wanted.iter().flatten() {
        if let Some(info) = infos.iter().find(|p| p.id == *id) {
            v.param_names.insert(*id, info.title.clone());
            v.params.entry(*id).or_insert(info.default);
        }
    }
}

impl BananaApp {
    /// Opens the FX Automation Chooser on `owner`'s effects.
    pub(crate) fn open_fx_automation(&mut self, owner: EnvelopeOwner) {
        let Some((effects, _)) = self.project().fx_chain_and_envelopes(owner) else {
            return;
        };
        let wanted = (0..effects.len())
            .map(|i| self.project().fx_automation(owner, i))
            .collect();
        let picked = effects.iter().position(automatable).unwrap_or(0);
        self.fx_automation = Some(FxAutomationChooser {
            owner,
            picked,
            wanted,
        });
    }

    /// Whether `owner` has effects FX automation can work on.
    pub(crate) fn has_automatable_fx(&self, owner: EnvelopeOwner) -> bool {
        self.project()
            .fx_chain_and_envelopes(owner)
            .is_some_and(|(fx, _)| fx.iter().any(automatable))
    }

    /// Ticks or clears `what` of effect `i` of `owner` at once (a track
    /// header's FX Automation Envelopes).
    pub(crate) fn toggle_fx_automation(&mut self, owner: EnvelopeOwner, i: usize, what: Automated) {
        let mut wanted = self.project().fx_automation(owner, i);
        match wanted.iter().position(|w| *w == what) {
            Some(k) => {
                wanted.remove(k);
            }
            None => wanted.push(what),
        }
        let curve = self
            .settings
            .prefs
            .new_envelope_curve(EnvelopeKind::FxParam { fx: 0, param: 0 });
        self.modify(|p| {
            name_vst_params(p, owner, i, &wanted);
            p.set_fx_automation(owner, i, &wanted, curve)
        });
        self.timeline.show_envelopes = true;
    }

    /// The FX Automation Chooser, while it is open.
    pub(crate) fn fx_automation_window(&mut self, ctx: &egui::Context) {
        let Some(mut c) = self.fx_automation.take() else {
            return;
        };
        let Some((effects, _)) = self.project().fx_chain_and_envelopes(c.owner) else {
            return;
        };
        let effects = effects.to_vec();
        c.wanted.resize(effects.len(), Vec::new());
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("FX Automation Chooser"))
            .id(egui::Id::new("fx-automation-chooser"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                if !effects.iter().any(automatable) {
                    ui.label(tr(
                        "There are no effects in this chain that can be automated.",
                    ));
                } else {
                    // The chain's effects, as buttons.
                    ui.horizontal_wrapped(|ui| {
                        for (i, e) in effects.iter().enumerate() {
                            if !automatable(e) {
                                continue;
                            }
                            let name = crate::ofx::fx_name(e);
                            if ui
                                .add(egui::Button::selectable(c.picked == i, name))
                                .clicked()
                            {
                                c.picked = i;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(tr("Parameters to control with envelopes:"));
                    if let Some(e) = effects.get(c.picked).filter(|e| automatable(e)) {
                        let wanted = &mut c.wanted[c.picked];
                        egui::Frame::new()
                            .stroke(egui::Stroke::new(1.0, crate::theme::separator()))
                            .inner_margin(6.0)
                            .show(ui, |ui| {
                                ui.set_min_width(400.0);
                                for (what, name) in rows_of(e) {
                                    let mut on = wanted.contains(&what);
                                    if crate::widgets::checkbox(ui, &mut on, name).changed() {
                                        if on {
                                            wanted.push(what);
                                        } else {
                                            wanted.retain(|w| *w != what);
                                        }
                                    }
                                }
                            });
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ok = ui.button(tr("OK")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
                ui.allocate_exact_size(vec2(0.0, 0.0), egui::Sense::hover());
            });
        if ok {
            let owner = c.owner;
            let wanted = c.wanted.clone();
            let curve = self
                .settings
                .prefs
                .new_envelope_curve(EnvelopeKind::FxParam { fx: 0, param: 0 });
            self.modify(|p| {
                let mut changed = false;
                for (i, w) in wanted.iter().enumerate() {
                    name_vst_params(p, owner, i, w);
                    changed |= p.set_fx_automation(owner, i, w, curve);
                }
                changed
            });
            self.timeline.show_envelopes = true;
        }
        if open && !ok && !cancel {
            self.fx_automation = Some(c);
        }
    }

    /// A track header's FX Automation Envelopes: each automatable effect's
    /// Bypass and parameters, ticked where they have envelopes, and the
    /// chooser.
    pub(crate) fn fx_automation_items(&mut self, ui: &mut egui::Ui, owner: EnvelopeOwner) {
        let Some((effects, _)) = self.project().fx_chain_and_envelopes(owner) else {
            return;
        };
        let effects = effects.to_vec();
        let mut any = false;
        for (i, e) in effects.iter().enumerate() {
            if !automatable(e) {
                continue;
            }
            any = true;
            let have = self.project().fx_automation(owner, i);
            let name = crate::ofx::fx_name(e);
            crate::menus::sub(ui, &name, true, |ui| {
                for (what, label) in rows_of(e) {
                    if crate::menus::check_item(ui, &label, have.contains(&what), false, "") {
                        self.toggle_fx_automation(owner, i, what);
                        ui.close();
                    }
                }
            });
        }
        if any {
            ui.separator();
        }
        if ui
            .add_enabled(any, egui::Button::new(tr("FX Automation…")))
            .clicked()
        {
            self.open_fx_automation(owner);
            ui.close();
        }
    }

    /// The FX Automation Chooser on the second audio track's 3-band EQ,
    /// its Low and Mid gains and its Bypass ticked (screen tests).
    pub fn fx_automation_for_review(&mut self) {
        use bv_timeline::{Effect, EffectKind, TrackKind};
        let made = self.modify(|p| {
            let t = p.tracks.iter().find(|t| t.kind == TrackKind::Audio)?.id;
            let track = p.track_by_id_mut(t)?;
            track.effects.push(Effect::new(EffectKind::Eq3));
            track.effects.push(Effect::new(EffectKind::Compressor));
            let owner = EnvelopeOwner::Track(t);
            p.set_fx_automation(
                owner,
                0,
                &[None, Some(0), Some(2)],
                bv_timeline::KeyCurve::Linear,
            );
            Some(owner)
        });
        if let Some(owner) = made {
            self.timeline.show_envelopes = true;
            self.open_fx_automation(owner);
        }
    }
}

/// Text for a parameter's value as the FX window and the timeline show it.
pub(crate) fn param_text(spec: &bv_timeline::fx::ParamSpec, value: f32) -> String {
    match spec.kind {
        bv_timeline::fx::ParamKind::Toggle if value >= 0.5 => tr("On").into(),
        bv_timeline::fx::ParamKind::Toggle => tr("Off").into(),
        _ => crate::fmt::num(value as f64, 2),
    }
}
