//! Compositors: VEGAS's window of compositor plug-ins — what a video
//! track's Custom compositing mode runs. Each preset's picture is BananaV's
//! sample picture drawn as ripples guide it; under the pointer the ripples
//! spread. Double-click a preset (or its menu's Apply to Track) to give it
//! to the track — the one whose Custom mode asked for one, else the
//! selected track —: its compositing mode becomes Custom and its settings
//! open.

use std::collections::HashMap;

use bv_timeline::PluginFx;

use super::chooser::{self, Category, Plugin};
use crate::app::BananaApp;
use crate::egui::{
    self, Color32, ColorImage, Rect, TextureHandle, TextureId, TextureOptions, pos2,
};
use crate::i18n::tr;
use crate::icons::Icon;

/// The window's categories, as VEGAS's: BananaV's compositors are
/// Creative.
fn categories() -> [Category; 3] {
    let c = |name, ready| Category {
        name,
        ready,
        packages: false,
    };
    [
        c(tr("Creative"), true),
        c(tr("Utilities"), false),
        // OFX has no compositors of other makers.
        c(tr("Third Party"), false),
    ]
}

fn plugins(list: &[std::sync::Arc<bv_ofx::PluginInfo>]) -> Vec<Plugin> {
    list.iter()
        .map(|p| {
            let name = crate::ofx::plugin_name(p);
            Plugin {
                key: format!("ofx:{}", p.identifier),
                about: [
                    trf!(
                        "{}: BananaV, 32-bit floating point, group {}",
                        name,
                        tr("Creative")
                    ),
                    trf!("Description: {}", tr(&p.description)),
                ],
                name,
                category: 0,
                ready: true,
                provider: None,
            }
        })
        .collect()
}

/// A compositor's presets: its defaults, BananaV's and the user's.
fn presets(app: &mut BananaApp, p: &bv_ofx::PluginInfo) -> Vec<(String, PluginFx)> {
    let mut v = vec![(tr("(Default)").to_string(), crate::ofx::plugin_fx(p))];
    for (name, values) in crate::presets::builtin_plugin(&p.identifier) {
        let mut fx = crate::ofx::plugin_fx(p);
        fx.params = values;
        v.push((name, fx));
    }
    for (name, values) in app.clip_windows.presets().user_plugin(&p.identifier) {
        let mut fx = crate::ofx::plugin_fx(p);
        fx.params = values.clone();
        v.push((name.clone(), fx));
    }
    v
}

/// The steps of a preset's ripples while it plays.
const STEPS: u32 = 12;

/// The presets' pictures, made once each (and each step of their play).
#[derive(Default)]
pub struct Previews {
    map: HashMap<String, Option<TextureHandle>>,
}

impl std::fmt::Debug for Previews {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Previews")
            .field("n", &self.map.len())
            .finish()
    }
}

impl Previews {
    fn get(
        &mut self,
        ctx: &egui::Context,
        fx: &PluginFx,
        step: u32,
        w: u32,
        h: u32,
    ) -> Option<TextureId> {
        let key = format!("{w}x{h} {step} {fx:?}");
        self.map
            .entry(key)
            .or_insert_with(|| {
                let phase = step as f32 / STEPS as f32;
                let rgba = bv_engine::compositor_sample(fx, phase, w, h)?;
                Some(ctx.load_texture(
                    "compositor",
                    ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                    TextureOptions::LINEAR,
                ))
            })
            .as_ref()
            .map(|t| t.id())
    }
}

/// A preset's picture in `rect`; while it plays, the ripples spread.
fn paint_preset(
    painter: &egui::Painter,
    rect: Rect,
    previews: &mut Previews,
    fx: &PluginFx,
    play: Option<f32>,
) {
    let ctx = painter.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let (w, h) = (
        (rect.width() * ppp).round().max(1.0) as u32,
        (rect.height() * ppp).round().max(1.0) as u32,
    );
    let step = play.map_or(0, |t| (chooser::progress(t) * STEPS as f32) as u32 % STEPS);
    match previews.get(&ctx, fx, step, w, h) {
        Some(t) => {
            let all = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(t, rect, all, Color32::WHITE);
        }
        None => {
            painter.rect_filled(rect, 0.0, Color32::from_gray(40));
        }
    }
}

/// Gives the compositor to its track (see [`BananaApp::compositor_target`]).
fn apply(app: &mut BananaApp, fx: &PluginFx) {
    match app.compositor_target() {
        Some((track, parent)) => app.apply_compositor(track, parent, fx.clone()),
        None => app.notify(tr("Select a video track to give it the compositor.")),
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut state = std::mem::take(&mut app.choosers.compositors);
    let categories = categories();
    let list = crate::ofx::compositors();
    let plugins = plugins(&list);
    let frame = chooser::frame(
        ui,
        "compositors",
        &mut state,
        &mut app.settings.favorites,
        &categories,
        &plugins,
        Icon::Fx,
    );
    if frame.favorites_changed {
        let _ = app.settings.save();
    }
    if let Some(p) = frame.plugin.and_then(|i| list.get(i)) {
        let presets = presets(app, p);
        let names: Vec<String> = presets.iter().map(|(n, _)| n.clone()).collect();
        let previews = &mut app.compositor_previews;
        let cells = chooser::preset_grid(
            ui,
            frame.grid,
            "compositors",
            &mut state,
            &names,
            tr("Give it to the track and edit"),
            |painter, rect, i, play| paint_preset(painter, rect, previews, &presets[i].1, play),
        );
        for cell in cells {
            let fx = &presets[cell.i].1;
            if cell.double || cell.add {
                apply(app, fx);
            }
            cell.resp.context_menu(|ui| {
                let target = app.compositor_target();
                if ui
                    .add_enabled(target.is_some(), egui::Button::new(tr("Apply to Track")))
                    .clicked()
                {
                    apply(app, fx);
                    ui.close();
                }
            });
        }
    }
    app.choosers.compositors = state;
}
