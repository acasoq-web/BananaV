//! Plug-in Manager (View › Window): every effect, transition and generator
//! in a folder tree, OFX plug-ins included (with the folders searched for
//! them). Double-click adds an effect to the selected events; video
//! effects and transitions can also be dragged onto the timeline.

use crate::i18n::tr;
use bv_timeline::{Effect, EffectKind, TrackKind, Transition};

use crate::app::{BananaApp, MediaDrag};
use crate::egui::{self, RichText, Sense};
use crate::fx_window::categories;
use crate::theme;

fn add_to_selection(app: &mut BananaApp, kind: EffectKind) {
    add_effect_to_selection(app, Effect::new(kind));
}

fn add_effect_to_selection(app: &mut BananaApp, effect: Effect) {
    let name = crate::ofx::fx_name(&effect);
    let audio = effect.kind.is_audio();
    add_chain_to_selection(app, vec![effect], audio, &name);
}

/// Adds `chain` (an effect, or a filter package's) to the selected events
/// of its kind.
fn add_chain_to_selection(app: &mut BananaApp, chain: Vec<Effect>, audio: bool, name: &str) {
    let want = if audio {
        TrackKind::Audio
    } else {
        TrackKind::Video
    };
    let p = app.project();
    let ids: Vec<_> = app
        .selection
        .iter()
        .copied()
        .filter(|&id| p.clip_track(id).is_some_and(|t| t.kind == want))
        .collect();
    if ids.is_empty() {
        app.notify(if audio {
            tr("Select audio events to add the effect.")
        } else {
            tr("Select video events to add the effect.")
        });
        return;
    }
    let n = ids.len();
    app.modify(|p| {
        for id in ids {
            if let Some(c) = p.clip_mut(id) {
                c.effects.extend(chain.iter().cloned());
            }
        }
    });
    app.notify(trn!(
        n,
        "{} added to {} event.",
        "{} added to {} events.",
        name,
        n
    ));
}

/// What an OFX plug-in is used as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OfxKind {
    Effect,
    Transition,
    Generator,
}

/// The OFX plug-ins found, by group; the scan and its folders.
fn ofx_section(ui: &mut egui::Ui, app: &mut BananaApp) {
    match app.ofx.scan_state() {
        crate::ofx::ScanState::Scanning { done, total } => {
            ui.label(trf!("Looking for plug-ins: {} of {}…", done, total));
        }
        crate::ofx::ScanState::Done { failed, .. } if !failed.is_empty() => {
            egui::CollapsingHeader::new(
                RichText::new(trn!(
                    failed.len(),
                    "{} plug-in file could not be used",
                    "{} plug-in files could not be used",
                    failed.len()
                ))
                .color(theme::error()),
            )
            .id_salt("ofx-falhas")
            .show(ui, |ui| {
                for (file, why) in failed {
                    ui.label(format!("{}: {why}", file.display()));
                }
            });
        }
        _ => {}
    }
    // Effects, transitions and generators (a plug-in may be several).
    let list: Vec<(OfxKind, std::sync::Arc<bv_ofx::PluginInfo>)> = [
        (OfxKind::Effect, crate::ofx::filters()),
        (OfxKind::Transition, crate::ofx::transitions()),
        (OfxKind::Generator, crate::ofx::generators()),
    ]
    .into_iter()
    .flat_map(|(k, ps)| ps.into_iter().map(move |p| (k, p)))
    .collect();
    if list.is_empty() {
        ui.label(RichText::new(tr("No OFX plug-ins found.")).color(theme::text_dim()));
    }
    let mut groups: Vec<String> = list.iter().map(|(_, p)| crate::ofx::group_of(p)).collect();
    groups.sort();
    groups.dedup();
    for g in groups {
        egui::CollapsingHeader::new(&g)
            .id_salt(("plugins-ofx", &g))
            .show(ui, |ui| {
                for (kind, p) in list.iter().filter(|(_, p)| crate::ofx::group_of(p) == g) {
                    let (badge, tip) = match kind {
                        OfxKind::Effect | OfxKind::Transition => (
                            if *kind == OfxKind::Effect { "fx" } else { "tr" },
                            trf!(
                                "{} {}.{} — {}\nDouble-click: add to the selected events",
                                p.identifier,
                                p.version[0],
                                p.version[1],
                                p.file.display()
                            ),
                        ),
                        OfxKind::Generator => (
                            "gen",
                            trf!(
                                "{} {}.{} — {}\nDouble-click: add at the cursor",
                                p.identifier,
                                p.version[0],
                                p.version[1],
                                p.file.display()
                            ),
                        ),
                    };
                    let r = ui
                        .add(
                            egui::Label::new(format!("{badge}  {}", crate::ofx::plugin_name(p)))
                                .sense(Sense::click_and_drag()),
                        )
                        .on_hover_text(tip);
                    if r.double_clicked() {
                        match kind {
                            OfxKind::Effect => {
                                add_effect_to_selection(app, crate::ofx::new_effect(p));
                            }
                            OfxKind::Transition => {
                                let sel = app.selected();
                                if sel.is_empty() {
                                    app.notify(tr(
                                        "Select a clip on the timeline to apply the transition.",
                                    ));
                                } else {
                                    let fx = crate::ofx::plugin_fx(p);
                                    crate::tabs::transitions::apply_plugin(app, &sel, &fx);
                                }
                            }
                            OfxKind::Generator => {
                                let at = app.cursor;
                                app.add_generator(
                                    crate::ofx::new_generator(p),
                                    at,
                                    app.selected_track,
                                );
                            }
                        }
                    }
                    if r.drag_started() {
                        app.media_drag = Some(match kind {
                            OfxKind::Effect => MediaDrag::Effect(crate::ofx::new_effect(p)),
                            OfxKind::Transition => {
                                MediaDrag::PluginTransition(Box::new(crate::ofx::plugin_fx(p)))
                            }
                            OfxKind::Generator => {
                                MediaDrag::Generator(crate::ofx::new_generator(p))
                            }
                        });
                    }
                }
            });
    }
    ui.add_space(4.0);
    ui.label(RichText::new(tr("Folders searched for plug-ins:")).color(theme::text_dim()));
    for f in bv_ofx::plugin::standard_folders() {
        ui.label(RichText::new(f.display().to_string()).color(theme::text_dim()));
    }
    let mut remove = None;
    for (i, f) in app.settings.ofx_folders.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(f.display().to_string());
            if ui.small_button(tr("Remove")).clicked() {
                remove = Some(i);
            }
        });
    }
    let mut rescan = false;
    if let Some(i) = remove {
        app.settings.ofx_folders.remove(i);
        let _ = app.settings.save();
        rescan = true;
    }
    ui.horizontal(|ui| {
        if ui.button(tr("Add Folder…")).clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_title(tr("Folder with OFX plug-ins"))
                .pick_folder()
        {
            if !app.settings.ofx_folders.contains(&dir) {
                app.settings.ofx_folders.push(dir);
                let _ = app.settings.save();
            }
            rescan = true;
        }
        if ui.button(tr("Look Again")).clicked() {
            rescan = true;
        }
    });
    if rescan {
        let folders = app.settings.ofx_folders.clone();
        app.ofx.scan(&folders);
    }
}

/// The filter packages, the video ones then the audio ones: double-click
/// adds one's chain to the selected events of its kind, a video one drags
/// onto an event.
fn packages_section(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut any = false;
    for audio in [false, true] {
        for p in app.fx_packages.list(audio) {
            any = true;
            let names: Vec<String> = p.chain.iter().map(crate::ofx::fx_name).collect();
            let r = ui
                .add(egui::Label::new(format!("fx  {}", p.name)).sense(Sense::click_and_drag()))
                .on_hover_text(trf!(
                    "{}\nDouble-click: add to the selected events",
                    names.join(" → ")
                ));
            if r.double_clicked() {
                add_chain_to_selection(app, p.chain.clone(), audio, &p.name);
            }
            if r.drag_started() && !audio {
                app.media_drag = Some(MediaDrag::Package(p.name.clone(), p.chain.clone()));
            }
        }
    }
    if !any {
        ui.label(RichText::new(tr("No filter packages yet.")).color(theme::text_dim()));
    }
}

fn effect_row(ui: &mut egui::Ui, app: &mut BananaApp, kind: EffectKind) {
    let r = ui
        .add(egui::Label::new(format!("fx  {}", tr(kind.name()))).sense(Sense::click_and_drag()))
        .on_hover_text(tr("Double-click: add to the selected events"));
    if r.double_clicked() {
        add_to_selection(app, kind);
    }
    if r.drag_started() && !kind.is_audio() {
        app.media_drag = Some(MediaDrag::Effect(bv_timeline::Effect::new(kind)));
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr("Video FX")).strong())
                .default_open(true)
                .show(ui, |ui| {
                    for cat in categories(&EffectKind::VIDEO) {
                        egui::CollapsingHeader::new(tr(cat))
                            .id_salt(("plugins-v", cat))
                            .show(ui, |ui| {
                                for k in EffectKind::VIDEO
                                    .into_iter()
                                    .filter(|k| k.category() == cat)
                                {
                                    effect_row(ui, app, k);
                                }
                            });
                    }
                });
            egui::CollapsingHeader::new(RichText::new(tr("Audio FX")).strong())
                .default_open(true)
                .show(ui, |ui| {
                    for cat in categories(&EffectKind::AUDIO) {
                        egui::CollapsingHeader::new(tr(cat))
                            .id_salt(("plugins-a", cat))
                            .show(ui, |ui| {
                                for k in EffectKind::AUDIO
                                    .into_iter()
                                    .filter(|k| k.category() == cat)
                                {
                                    effect_row(ui, app, k);
                                }
                            });
                    }
                });
            egui::CollapsingHeader::new(RichText::new(tr("OFX Plug-Ins")).strong())
                .default_open(true)
                .show(ui, |ui| ofx_section(ui, app));
            egui::CollapsingHeader::new(RichText::new(tr("Filter Packages")).strong())
                .show(ui, |ui| packages_section(ui, app));
            egui::CollapsingHeader::new(RichText::new(tr("Transitions")).strong()).show(ui, |ui| {
                for t in Transition::ALL {
                    let r = ui.add(egui::Label::new(tr(t.name())).sense(Sense::drag()));
                    if r.drag_started() {
                        app.media_drag = Some(MediaDrag::Transition(t));
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr(
                    "Drag video effects and transitions onto the timeline events.",
                ))
                .color(theme::text_dim()),
            );
        });
}
