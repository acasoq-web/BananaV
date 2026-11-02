//! Propriedades do clipe selecionado (ou da trilha dele): tempo, nível,
//! fades, transição, enquadramento (pan/crop) e cadeia de efeitos.

use crate::i18n::tr;
use bv_core::{Tick, timecode};
use bv_timeline::{
    Clip, ClipId, Effect, EffectKind, FadeCurve, ParamKind, Project, TrackKind, Transform,
    Transition,
};

use crate::app::BananaApp;
use crate::dock::Tab;
use crate::egui::{self, Color32, RichText};
use crate::theme;

const CURVES: [(FadeCurve, &str); 5] = [
    (FadeCurve::Linear, "Linear"),
    (FadeCurve::Fast, "Fast"),
    (FadeCurve::Slow, "Slow"),
    (FadeCurve::Smooth, "Smooth"),
    (FadeCurve::Sharp, "Sharp"),
];

fn curve_name(c: FadeCurve) -> &'static str {
    CURVES.iter().find(|x| x.0 == c).map_or("?", |x| tr(x.1))
}

/// Edita uma cópia do clipe; se mudou, grava (agrupando arrastos).
pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let Some(id) = app.primary_clip() else {
        super::hint(
            ui,
            tr("Select a clip on the timeline to see and edit its properties."),
        );
        return;
    };
    let project = app.project().clone();
    let (Some(clip), Some(track)) = (project.clip(id), project.clip_track(id)) else {
        return;
    };
    let rate = project.settings.frame_rate;
    let media_name = project
        .media(clip.media)
        .map_or("?".into(), crate::i18n::media_name);
    let is_video = track.kind == TrackKind::Video;
    let mut c = clip.clone();
    // O enquadramento é editado no instante do cursor (com quadros-chave).
    let at = if app.cursor >= clip.start && app.cursor < clip.end() {
        app.cursor
    } else {
        clip.start
    };
    let framing = clip.transform_at(at);
    let mut tf = framing;
    let mut conform = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&media_name).strong().color(theme::banana()));
                ui.label(
                    RichText::new(trf!("at {}", crate::i18n::track_name(track)))
                        .color(theme::text_dim()),
                );
            });
            ui.label(
                RichText::new(trf!(
                    "Start {}   Length {}   Media from {}",
                    timecode::format(c.start, rate),
                    timecode::format(c.duration, rate),
                    timecode::format(c.source_in, rate)
                ))
                .monospace()
                .color(theme::text_dim()),
            );
            ui.horizontal(|ui| {
                crate::widgets::checkbox(ui, &mut c.muted, tr("Disabled"));
                crate::widgets::checkbox(ui, &mut c.locked, tr("Locked"));
            });
            if !is_video {
                // Stretched (Ctrl+drag of an edge, Playback Rate), the event
                // keeps its pitch.
                crate::widgets::checkbox(
                    ui,
                    &mut c.preserve_pitch,
                    tr("Preserve pitch when stretched"),
                );
                // Its pitch change, its length kept (= and − on the
                // timeline).
                ui.horizontal(|ui| {
                    ui.label(tr("Pitch change (semitones):"));
                    ui.add(
                        egui::DragValue::new(&mut c.pitch)
                            .range(-48.0..=48.0)
                            .speed(0.05)
                            .fixed_decimals(2),
                    );
                });
                crate::widgets::checkbox(ui, &mut c.auto_normalize, "Auto Normalize");
            } else {
                ui.horizontal(|ui| {
                    // Its length follows (kept after each edit).
                    if crate::widgets::checkbox(
                        ui,
                        &mut c.all_frames,
                        tr("Trim event to include all frames"),
                    )
                    .changed()
                        && c.all_frames
                    {
                        c.looped = false;
                        c.hold_last = false;
                    }
                    crate::widgets::checkbox(
                        ui,
                        &mut c.reduce_flicker,
                        tr("Reduce interlace flicker"),
                    );
                });
                // A high-frame-rate clip at the project's rate (VEGAS's
                // Video Event tab).
                let media_rate = project
                    .media(clip.media)
                    .and_then(|m| m.video)
                    .map(|v| v.frame_rate);
                conform = ui
                    .add_enabled(
                        media_rate.is_some_and(|r| r != rate),
                        egui::Button::new(tr("Conform to Project Frame Rate")),
                    )
                    .clicked();
                // The share of the media's frames it shows (VEGAS's Video
                // Event tab).
                ui.horizontal(|ui| {
                    ui.label(tr("Undersample rate:"));
                    ui.add(
                        egui::DragValue::new(&mut c.undersample)
                            .range(0.01..=1.0)
                            .speed(0.005)
                            .fixed_decimals(3),
                    );
                });
            }
            ui.separator();
            egui::Grid::new("clip-geral").num_columns(2).show(ui, |ui| {
                if is_video {
                    ui.label(tr("Opacity"));
                    let mut pct = c.level * 100.0;
                    ui.add(crate::fmt::slider(&mut pct, 0.0..=100.0, 0, " %"));
                    c.level = pct / 100.0;
                } else {
                    ui.label(tr("Gain"));
                    let mut db = bv_mixer_gain_db(c.level);
                    ui.add(crate::fmt::slider(&mut db, -60.0..=12.0, 1, " dB"));
                    c.level = if db <= -59.9 {
                        0.0
                    } else {
                        10f32.powf(db / 20.0)
                    };
                }
                ui.end_row();
                let max = c.duration.as_seconds_f64();
                for (label, fade, curve) in [
                    (tr("Fade in"), &mut c.fade_in, &mut c.fade_in_curve),
                    (tr("Fade out"), &mut c.fade_out, &mut c.fade_out_curve),
                ] {
                    ui.label(label);
                    ui.horizontal(|ui| {
                        let mut s = fade.as_seconds_f64() as f32;
                        ui.add(crate::fmt::slider(
                            &mut s,
                            0.0..=max.min(10.0) as f32,
                            2,
                            " s",
                        ));
                        *fade = rate.snap_nearest(Tick::from_seconds_f64(s as f64));
                        egui::ComboBox::from_id_salt(label)
                            .width(90.0)
                            .selected_text(curve_name(*curve))
                            .show_ui(ui, |ui| {
                                for (cv, name) in CURVES {
                                    ui.selectable_value(curve, cv, tr(name));
                                }
                            });
                    });
                    ui.end_row();
                }
                if is_video {
                    ui.label(tr("Transition in"));
                    transition_choice(ui, &mut c);
                    ui.end_row();
                }
            });
            // An OFX plug-in's transition: its settings, as it describes them.
            if is_video && let Some(fx) = c.transition_fx.clone() {
                ui.add_space(4.0);
                ui.label(RichText::new(trf!("Transition: {}", fx.name)).strong());
                let key = crate::ofx::EditorKey {
                    owner: crate::ofx::EditorOwner::Transition(id),
                    index: 0,
                    id: fx.id.clone(),
                };
                if let Some(place) = crate::ofx::transition_place(&project, id, app.cursor)
                    && let Some(new) = crate::ofx::params(ui, &mut app.ofx, key, &fx, &place, 1)
                {
                    c.transition_fx = Some(Box::new(new));
                }
            }
            // Fades não podem passar da duração.
            c.fade_in = c.fade_in.min(c.duration);
            c.fade_out = c.fade_out.min(c.duration - c.fade_in);

            if is_video {
                ui.separator();
                pan_crop(ui, &mut tf);
                ui.horizontal(|ui| {
                    if ui.small_button(tr("Open Pan/Crop…")).clicked() {
                        app.clip_windows.pan_crop = Some(id);
                    }
                    if !clip.transform_keys.is_empty() {
                        super::hint(ui, tr("Animated: edits the keyframe at the cursor."));
                    }
                });
            }
            ui.separator();
            effects_chain(ui, &mut c, app, !is_video);
        });

    if conform {
        app.edit(|p| p.conform_to_project_rate(id, false));
        return;
    }
    if c != *clip {
        let key = ui.id().with(("clipe", id.0));
        // Only effect parameters changed: an undo, or none (Options ›
        // Preferences › General: "Create undos for FX parameter changes").
        let mut same = c.clone();
        for (e, old) in same.effects.iter_mut().zip(&clip.effects) {
            e.params.clone_from(&old.params);
        }
        if same == *clip {
            let chain = move |p: &Project| p.clip(id).map(|c| c.effects.clone());
            let set = move |p: &mut Project, effects| {
                if let Some(c) = p.clip_mut(id) {
                    c.effects = effects;
                }
            };
            app.set_fx_params(key, c.effects, chain, set);
        } else {
            apply_clip(app, key, id, c);
        }
    }
    if tf != framing {
        let key = ui.id().with(("enquadramento", id.0));
        let new = app.settings.prefs.new_motion_keys();
        app.modify_merged(key, |p| {
            let _ = p.set_clip_transform_at_as(id, at, tf, new);
        });
    }
}

/// The event's transition: one of BananaV's, or an OFX plug-in's.
fn transition_choice(ui: &mut egui::Ui, c: &mut Clip) {
    let ofx = crate::ofx::transitions();
    let shown = match &c.transition_fx {
        Some(fx) => fx.name.clone(),
        None => tr(c.transition.name()).to_string(),
    };
    egui::ComboBox::from_id_salt("transicao")
        .width(200.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for t in Transition::ALL {
                let on = c.transition_fx.is_none() && c.transition == t;
                if ui.selectable_label(on, tr(t.name())).clicked() {
                    c.transition = t;
                    c.transition_fx = None;
                }
            }
            if !ofx.is_empty() {
                ui.separator();
            }
            for p in &ofx {
                let on = c
                    .transition_fx
                    .as_ref()
                    .is_some_and(|fx| fx.id == p.identifier);
                if ui
                    .selectable_label(on, crate::ofx::plugin_name(p))
                    .clicked()
                    && !on
                {
                    c.transition_fx = Some(Box::new(crate::ofx::plugin_fx(p)));
                }
            }
        });
}

fn bv_mixer_gain_db(g: f32) -> f32 {
    if g <= 1e-4 { -60.0 } else { 20.0 * g.log10() }
}

fn apply_clip(app: &mut BananaApp, key: egui::Id, id: ClipId, c: Clip) {
    // Propriedades que valem para o grupo inteiro (como no VEGAS): fades e travamento.
    let (fade_in, fade_out, locked) = (c.fade_in, c.fade_out, c.locked);
    app.modify_merged(key, |p: &mut Project| {
        let group = p.group_members(id);
        if let Some(slot) = p.clip_mut(id) {
            *slot = c;
        }
        for m in group.into_iter().filter(|&m| m != id) {
            if let Some(o) = p.clip_mut(m) {
                o.fade_in = fade_in.min(o.duration);
                o.fade_out = fade_out.min(o.duration - o.fade_in);
                o.locked = locked;
            }
        }
    });
}

pub(crate) fn pan_crop(ui: &mut egui::Ui, t: &mut Transform) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Framing (pan/crop)")).strong());
        if ui.small_button(tr("Reset")).clicked() {
            *t = Transform::default();
        }
    });
    egui::Grid::new("pancrop").num_columns(2).show(ui, |ui| {
        let row = |ui: &mut egui::Ui,
                   label: &str,
                   v: &mut f32,
                   range: std::ops::RangeInclusive<f32>,
                   default: f32,
                   pct: bool| {
            ui.label(label);
            let mut shown = if pct { *v * 100.0 } else { *v };
            let r = if pct {
                (*range.start() * 100.0)..=(*range.end() * 100.0)
            } else {
                range
            };
            let resp = ui.add(crate::fmt::slider(
                &mut shown,
                r,
                1,
                if pct { " %" } else { "°" },
            ));
            *v = if pct { shown / 100.0 } else { shown };
            if resp.double_clicked() {
                *v = default;
            }
            ui.end_row();
        };
        row(ui, tr("Position X"), &mut t.x, -1.0..=1.0, 0.0, true);
        row(ui, tr("Position Y"), &mut t.y, -1.0..=1.0, 0.0, true);
        row(ui, tr("Scale"), &mut t.scale, 0.05..=4.0, 1.0, true);
        row(
            ui,
            tr("Rotation"),
            &mut t.rotation_deg,
            -180.0..=180.0,
            0.0,
            false,
        );
        row(ui, tr("Crop left"), &mut t.crop_left, 0.0..=0.95, 0.0, true);
        row(
            ui,
            tr("Crop right"),
            &mut t.crop_right,
            0.0..=0.95,
            0.0,
            true,
        );
        row(ui, tr("Crop top"), &mut t.crop_top, 0.0..=0.95, 0.0, true);
        row(
            ui,
            tr("Crop bottom"),
            &mut t.crop_bottom,
            0.0..=0.95,
            0.0,
            true,
        );
    });
}

fn effects_chain(ui: &mut egui::Ui, c: &mut Clip, app: &mut BananaApp, audio: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Clip Effects")).strong());
        if !audio && ui.small_button(tr("Catalog…")).clicked() {
            app.show_tab(Tab::VideoFx, false);
        }
    });
    chain_editor(ui, &mut c.effects, audio, "props");
}

/// Editor de uma cadeia de efeitos: adicionar, ligar/desligar, reordenar,
/// remover e ajustar parâmetros. `audio` escolhe o catálogo.
pub(crate) fn chain_editor(ui: &mut egui::Ui, effects: &mut Vec<Effect>, audio: bool, salt: &str) {
    ui.menu_button(tr("+ Add Effect"), |ui| {
        let list: &[EffectKind] = if audio {
            &EffectKind::AUDIO
        } else {
            &EffectKind::VIDEO
        };
        for cat in crate::fx_window::categories(list) {
            ui.label(RichText::new(tr(cat)).small().color(theme::text_dim()));
            for &k in list.iter().filter(|k| k.category() == cat) {
                if ui.button(tr(k.name())).clicked() {
                    effects.push(Effect::new(k));
                    ui.close();
                }
            }
        }
    });
    if effects.is_empty() {
        super::hint(ui, tr("No effects yet."));
        return;
    }
    let mut remove = None;
    let mut swap = None;
    let n = effects.len();
    for (i, e) in effects.iter_mut().enumerate() {
        let title = RichText::new(crate::ofx::fx_name(e)).color(if e.enabled {
            theme::text()
        } else {
            theme::text_dim()
        });
        egui::CollapsingHeader::new(title)
            .id_salt((salt, "efeito", i))
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::widgets::checkbox(ui, &mut e.enabled, tr("Active"));
                    if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                        swap = Some((i, i - 1));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("↓")).clicked() {
                        swap = Some((i, i + 1));
                    }
                    if ui.button(tr("Remove")).clicked() {
                        remove = Some(i);
                    }
                    if ui.button(tr("Defaults")).clicked() {
                        *e = e.reset();
                    }
                });
                if e.plugin.is_some() {
                    super::hint(ui, tr("Its settings are in the FX window."));
                } else {
                    params(ui, e);
                }
            });
    }
    if let Some((a, b)) = swap {
        effects.swap(a, b);
    }
    if let Some(i) = remove {
        effects.remove(i);
    }
}

/// Controles genéricos a partir da descrição dos parâmetros.
pub fn params(ui: &mut egui::Ui, e: &mut Effect) {
    let specs = e.kind.params();
    egui::Grid::new(("params", e.kind))
        .num_columns(2)
        .show(ui, |ui| {
            let mut i = 0;
            while i < specs.len() {
                let spec = specs[i];
                match spec.kind {
                    ParamKind::Color => {
                        ui.label(tr(spec.name));
                        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                        let mut c = Color32::from_rgb(
                            to8(e.param(i)),
                            to8(e.param(i + 1)),
                            to8(e.param(i + 2)),
                        );
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            e.set_param(i, c.r() as f32 / 255.0);
                            e.set_param(i + 1, c.g() as f32 / 255.0);
                            e.set_param(i + 2, c.b() as f32 / 255.0);
                        }
                        ui.end_row();
                        i += 3;
                        continue;
                    }
                    ParamKind::Toggle => {
                        ui.label(tr(spec.name));
                        let mut on = e.param(i) >= 0.5;
                        if crate::widgets::checkbox(ui, &mut on, "").changed() {
                            e.set_param(i, on as u8 as f32);
                        }
                    }
                    ParamKind::Slider => {
                        ui.label(tr(spec.name));
                        let mut v = e.param(i);
                        let r = ui.add(crate::fmt::slider(&mut v, spec.min..=spec.max, 2, ""));
                        if r.double_clicked() {
                            v = spec.default;
                        }
                        if v != e.param(i) {
                            e.set_param(i, v);
                        }
                    }
                }
                ui.end_row();
                i += 1;
            }
        });
}
