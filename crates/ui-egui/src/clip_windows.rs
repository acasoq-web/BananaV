//! Janelas abertas pelos botões do clipe (Pan/Crop, FX e "⋯"): enquadramento
//! visual, cadeias de efeitos, velocidade, duração, colar atributos,
//! detecção de cenas e o menu de extras.

use crate::i18n::tr;
use bv_core::{Tick, timecode};
use bv_timeline::{ClipId, MediaId, TrackId, TrackKind};

use crate::app::BananaApp;
use crate::dock::Tab;
use crate::egui::{self, RichText, vec2};
use crate::fx_window::{Choice, Chooser, FxTarget, FxWindow, Node};
use crate::presets::Presets;
use crate::theme;

/// Botões mostrados no canto dos clipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipButtons {
    pub pan_crop: bool,
    pub fx: bool,
    pub more: bool,
}

impl Default for ClipButtons {
    fn default() -> ClipButtons {
        ClipButtons {
            pan_crop: true,
            fx: true,
            more: true,
        }
    }
}

/// O que "colar atributos" copia do clipe da área de transferência.
#[derive(Clone, Copy, Debug)]
pub struct PasteOptions {
    effects: bool,
    framing: bool,
    fades: bool,
    level: bool,
    rate: bool,
}

#[derive(Debug, Default)]
pub struct ClipWindows {
    // Pedidos de abertura, atendidos no próximo quadro.
    pub pan_crop: Option<ClipId>,
    /// The Pan/Crop window opened on its Mask row, with a mask and an
    /// anchor picked, the mask's box showing or not (screen and interface
    /// tests).
    pub(crate) mask_row: Option<(Option<usize>, Option<usize>, bool)>,
    pub clip_fx: Option<ClipId>,
    /// An event's FX window on its last effect (the one just added).
    pub clip_fx_last: Option<ClipId>,
    /// Mídia e cadeia (`true` = áudio).
    pub media_fx: Option<(MediaId, bool)>,
    pub track_fx: Option<TrackId>,
    /// Tools › Video › Video Output FX.
    pub output_fx: bool,
    /// A bus's, an assignable effects chain's or the Master's effects to
    /// open (the Mixing Console's FX buttons).
    pub fx_target: Option<FxTarget>,
    /// A chain's effect to show (the Mixing Console's Insert FX buttons).
    pub fx_node: Option<(FxTarget, usize)>,
    pub rate: Option<(ClipId, f32)>,
    pub duration: Option<(ClipId, String, bool)>,
    pub paste: Option<PasteOptions>,
    pub buttons_editor: bool,
    pub scenes: Option<(ClipId, f32, bool)>,
    fx: Option<FxWindow>,
    chooser: Option<Chooser>,
    presets: Option<Presets>,
}

fn clip_name(app: &BananaApp, id: ClipId) -> String {
    let p = app.project();
    p.clip(id)
        .and_then(|c| p.media(c.media))
        .map_or_else(|| tr("clip").into(), crate::i18n::media_name)
}

impl ClipWindows {
    /// The effects' presets saved by the user (read once).
    pub(crate) fn presets(&mut self) -> &Presets {
        self.presets.get_or_insert_with(Presets::load)
    }

    pub fn open_duration(&mut self, app: &BananaApp, id: ClipId) {
        if let Some(c) = app.project().clip(id) {
            let tc = timecode::format(c.duration, app.frame_rate());
            self.duration = Some((id, tc, false));
        }
    }

    pub fn open_rate(&mut self, app: &BananaApp, id: ClipId) {
        if let Some(c) = app.project().clip(id) {
            self.rate = Some((id, c.rate as f32));
        }
    }

    pub fn open_paste(&mut self) {
        self.paste = Some(PasteOptions {
            effects: true,
            framing: true,
            fades: false,
            level: false,
            rate: false,
        });
    }

    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        self.take_requests(app);
        self.fx_windows(ctx, app);
        self.rate_window(ctx, app);
        self.duration_window(ctx, app);
        self.paste_window(ctx, app);
        self.scenes_window(ctx, app);
        if self.buttons_editor {
            let mut open = true;
            crate::floating::Window::new(tr("Clip Buttons"))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    let b = &mut app.timeline.buttons;
                    crate::widgets::checkbox(ui, &mut b.pan_crop, tr("Pan/Crop (video only)"));
                    crate::widgets::checkbox(ui, &mut b.fx, tr("Clip Effects"));
                    crate::widgets::checkbox(ui, &mut b.more, "Extras (⋯)");
                    crate::tabs::hint(
                        ui,
                        tr("The extras are still on the clip's right-click menu."),
                    );
                });
            self.buttons_editor = open;
        }
    }

    // ---- FX e Pan/Crop --------------------------------------------------------

    /// Transforma os pedidos de abertura (botões, menus, testes) em janelas.
    fn take_requests(&mut self, app: &BananaApp) {
        let p = app.project();
        if let Some(id) = self.pan_crop.take()
            && p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Video)
        {
            self.open_fx(FxTarget::Clip(id), Node::PanCrop);
            if let Some((path, anchor, boxed)) = self.mask_row.take()
                && let Some(w) = &mut self.fx
            {
                w.masks_for_review(path, anchor, boxed);
            }
        }
        if let Some(id) = self.clip_fx_last.take() {
            let target = FxTarget::Clip(id);
            if let Some(n) = target.chain(p).map(|c| c.len()).filter(|&n| n > 0) {
                self.open_fx(target, Node::Effect(n - 1));
            }
        }
        if let Some((target, i)) = self.fx_node.take()
            && target.chain(p).is_some_and(|c| i < c.len())
        {
            self.open_fx(target, Node::Effect(i));
        }
        let chains = [
            self.clip_fx.take().map(FxTarget::Clip),
            self.media_fx
                .take()
                .map(|(m, audio)| FxTarget::Media(m, audio)),
            self.track_fx.take().map(FxTarget::Track),
            std::mem::take(&mut self.output_fx).then_some(FxTarget::VideoOutput),
            self.fx_target.take(),
        ];
        for target in chains.into_iter().flatten() {
            // Como no VEGAS: sem efeitos, abre o seletor de plug-ins.
            match target.chain(p) {
                Some(c) if c.is_empty() => self.chooser = Some(Chooser::new(target, c)),
                Some(_) => self.open_fx(target, Node::Effect(0)),
                None => {}
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn fx_window(&self) -> Option<&FxWindow> {
        self.fx.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn fx_window_mut(&mut self) -> Option<&mut FxWindow> {
        self.fx.as_mut()
    }

    /// Whose effects the FX window shows, while it is open (screen
    /// tests).
    pub(crate) fn fx_target_shown(&self) -> Option<FxTarget> {
        self.fx.as_ref().map(|w| w.target)
    }

    /// The size the FX window's plug-in page takes (for screen tests).
    pub(crate) fn fx_page_size(&self) -> Option<egui::Vec2> {
        self.fx.as_ref()?.natural()
    }

    /// Where the Pan/Crop window drew its box's point (`sx`, `sy` from −1
    /// to 1 across and down; 0, 0 its middle).
    pub(crate) fn pan_crop_spot(&self, sx: f32, sy: f32) -> Option<egui::Pos2> {
        self.fx.as_ref()?.pan_spot(sx, sy)
    }

    pub(crate) fn pan_crop_key_spot(&self, mask: bool, i: usize) -> Option<egui::Pos2> {
        self.fx.as_ref()?.pan_key_spot(mask, i)
    }

    pub(crate) fn pan_crop_media_spot(&self, fx: f32, fy: f32) -> Option<egui::Pos2> {
        self.fx.as_ref()?.pan_media_spot(fx, fy)
    }

    pub(crate) fn pan_crop_lane_spot(&self, mask: bool, f: f32) -> Option<egui::Pos2> {
        self.fx.as_ref()?.pan_lane_spot(mask, f)
    }

    fn open_fx(&mut self, target: FxTarget, node: Node) {
        match &mut self.fx {
            Some(w) if w.target == target => w.select(node),
            _ => self.fx = Some(FxWindow::new(target, node)),
        }
    }

    fn fx_windows(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let presets = self.presets.get_or_insert_with(Presets::load);
        if let Some(mut w) = self.fx.take() {
            let mut chooser = None;
            if w.show(ctx, app, presets, &mut chooser) {
                self.fx = Some(w);
            }
            if chooser.is_some() {
                self.chooser = chooser;
            }
        }
        // VST 3 plug-ins' pages no longer shown close.
        app.vst.frame_end(ctx);
        let Some(mut c) = self.chooser.take() else {
            return;
        };
        let prefs = &app.settings.prefs;
        let (folders, left_out) = (prefs.vst_folders.clone(), prefs.vst_left_out.clone());
        let vst = app.vst.effects(&folders, &left_out);
        match c.show(ctx, app, &vst) {
            Choice::Open => self.chooser = Some(c),
            Choice::Cancel => {}
            Choice::Ok(sel) => {
                let mut target = c.target;
                let n = c.chain.len();
                if target == FxTarget::NewChain {
                    // The chain is made with the effects picked, as
                    // VEGAS's: none, none.
                    let made = (n > 0)
                        .then(|| app.modify(|p| p.add_fx_chain(c.chain)))
                        .flatten();
                    let Some(id) = made else {
                        return;
                    };
                    target = FxTarget::Chain(id);
                } else if target.chain(app.project()).as_ref() != Some(&c.chain) {
                    app.modify(|p| target.set_chain(p, c.chain));
                }
                if n > 0 || target.has_pan_crop(app.project()) {
                    let node = match sel {
                        Some(i) if i < n => Node::Effect(i),
                        _ if n > 0 => Node::Effect(n - 1),
                        _ => Node::PanCrop,
                    };
                    self.open_fx(target, node);
                }
            }
        }
    }

    // ---- Velocidade, duração, colar atributos -------------------------------

    fn rate_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((id, mut rate)) = self.rate else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        crate::floating::Window::new(trf!("Playback Rate — {}", clip_name(app, id)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (label, r) in [("¼×", 0.25), ("½×", 0.5), ("1×", 1.0), ("1½×", 1.5), ("2×", 2.0), ("4×", 4.0)] {
                        if ui.selectable_label((rate - r).abs() < 1e-3, label).clicked() {
                            rate = r;
                        }
                    }
                });
                ui.add(crate::fmt::slider(&mut rate, 0.1..=8.0, 2, "×").logarithmic(true));
                crate::tabs::hint(
                    ui,
                    tr("The clip keeps showing the same part of the media: it gets shorter when sped up and longer when slowed down. The audio follows, its pitch kept if Preserve pitch when stretched is on in its Properties."),
                );
                if ui.button(RichText::new(tr("Apply")).strong()).clicked() {
                    apply = true;
                }
            });
        if apply {
            app.edit(|p| p.set_clip_rate(id, rate as f64));
            open = false;
        }
        self.rate = open.then_some((id, rate));
    }

    fn duration_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((id, mut text, mut error)) = self.duration.clone() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let rate = app.frame_rate();
        crate::floating::Window::new(trf!("Clip Length — {}", clip_name(app, id)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("New length (timecode or seconds):"));
                let r =
                    ui.add(egui::TextEdit::singleline(&mut text).font(egui::TextStyle::Monospace));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                if error {
                    ui.colored_label(
                        theme::error(),
                        tr("Use HH:MM:SS;FF or a number of seconds."),
                    );
                }
                if ui.button(tr("Apply")).clicked() {
                    apply = true;
                }
            });
        if apply {
            match timecode::parse(&text, rate) {
                Some(d) if d > Tick::ZERO => {
                    app.edit(|p| p.set_clip_duration(id, d));
                    open = false;
                }
                _ => error = true,
            }
        }
        self.duration = open.then_some((id, text, error));
    }

    fn paste_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some(mut o) = self.paste else { return };
        let mut open = true;
        let mut apply = false;
        crate::floating::Window::new(tr("Paste Clip Attributes"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                crate::tabs::hint(
                    ui,
                    tr("Copies from the clip on the clipboard to the selected ones:"),
                );
                crate::widgets::checkbox(ui, &mut o.effects, tr("Effects"));
                crate::widgets::checkbox(ui, &mut o.framing, tr("Framing (pan/crop)"));
                crate::widgets::checkbox(ui, &mut o.fades, tr("Fades and transition"));
                crate::widgets::checkbox(ui, &mut o.level, tr("Level (opacity/gain)"));
                crate::widgets::checkbox(ui, &mut o.rate, tr("Playback rate"));
                if ui.button(tr("Paste")).clicked() {
                    apply = true;
                }
            });
        if apply {
            let targets = app.selected();
            let Some(data) = app.clipboard.clone() else {
                app.notify(tr("Copy a clip first (Ctrl+C)."));
                return;
            };
            let mut n = 0;
            app.modify(|p| {
                for id in targets {
                    let Some(kind) = p.clip_track(id).map(|t| t.kind) else {
                        continue;
                    };
                    let Some(src) = data.first_of(kind).cloned() else {
                        continue;
                    };
                    if o.rate {
                        let _ = p.set_clip_rate(id, src.rate);
                    }
                    if let Some(c) = p.clip_mut(id) {
                        if o.effects {
                            c.effects = src.effects.clone();
                            c.pan_crop_at = src.pan_crop_at;
                        }
                        if o.framing && kind == TrackKind::Video {
                            c.pan_crop = src.pan_crop.clone();
                        }
                        if o.fades {
                            c.fade_in = src.fade_in.min(c.duration);
                            c.fade_out = src.fade_out.min(c.duration - c.fade_in);
                            c.fade_in_curve = src.fade_in_curve;
                            c.fade_out_curve = src.fade_out_curve;
                            c.transition = src.transition;
                        }
                        if o.level {
                            c.level = src.level;
                        }
                        n += 1;
                    }
                }
            });
            app.notify(trn!(
                n,
                "Attributes pasted into {} clip.",
                "Attributes pasted into {} clips.",
                n
            ));
            open = false;
        }
        self.paste = open.then_some(o);
    }

    // ---- Detecção de cenas ---------------------------------------------------

    fn scenes_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some((id, mut sensitivity, started)) = self.scenes else {
            return;
        };
        let mut open = true;
        let progress = app.engine.scene_progress().filter(|p| p.clip == Some(id));
        let mut start = false;
        crate::floating::Window::new(trf!("Detect Scenes and Split — {}", clip_name(app, id)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!started, |ui| {
                    ui.label(tr("Sensitivity"));
                    ui.add(crate::fmt::slider(&mut sensitivity, 0.0..=1.0, 2, ""));
                    crate::tabs::hint(ui, tr("Higher: also splits on soft scene changes."));
                    if ui
                        .button(RichText::new(tr("Detect and Split")).strong())
                        .clicked()
                    {
                        start = true;
                    }
                });
                if let Some(p) = &progress {
                    let f = p.done as f32 / p.total.max(1) as f32;
                    ui.add(egui::ProgressBar::new(f).show_percentage());
                }
            });
        if start {
            app.engine.set_project(app.history.current());
            if app.engine.start_scene_detection(id, sensitivity) {
                self.scenes = Some((id, sensitivity, true));
            } else {
                app.report_error(tr("This clip has no video to analyze."));
                self.scenes = None;
            }
            return;
        }
        if let Some(p) = progress
            && started
            && p.finished
        {
            if let Some(e) = p.error {
                app.report_error(trf!("Scene detection: {}", e));
            } else {
                let cuts = p.cuts.clone();
                let n = cuts.len();
                app.edit(|proj| -> Result<(), bv_timeline::EditError> {
                    // Do fim para o começo: o clipe original continua sendo a
                    // parte da esquerda, que contém os cortes anteriores.
                    for t in cuts.into_iter().rev() {
                        if proj.clip(id).is_some_and(|c| t > c.start && t < c.end()) {
                            proj.split(id, t)?;
                        }
                    }
                    Ok(())
                });
                app.notify(if n == 0 {
                    tr("No scene cuts found.").to_string()
                } else {
                    trn!(
                        n,
                        "{} scenes: clip split at {} point.",
                        "{} scenes: clip split at {} points.",
                        n + 1,
                        n
                    )
                });
            }
            app.engine.cancel_scene_detection();
            open = false;
        }
        if !open {
            app.engine.cancel_scene_detection();
        }
        self.scenes = open.then_some((id, sensitivity, started));
    }
}

/// O menu "⋯" de extras do clipe. Devolve `true` quando deve fechar.
pub fn extras_menu(ui: &mut egui::Ui, app: &mut BananaApp, id: ClipId) -> bool {
    // Itens de menu: sem moldura, ocupando a largura toda.
    ui.style_mut().visuals.button_frame = false;
    ui.spacing_mut().button_padding = vec2(8.0, 3.0);
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
        extras_items(ui, app, id)
    })
    .inner
}

fn extras_items(ui: &mut egui::Ui, app: &mut BananaApp, id: ClipId) -> bool {
    let Some(clip) = app.project().clip(id).cloned() else {
        return true;
    };
    let is_video = app
        .project()
        .clip_track(id)
        .is_some_and(|t| t.kind == TrackKind::Video);
    let mut close = false;
    ui.set_min_width(250.0);
    if crate::widgets::checkbox(
        ui,
        &mut app.timeline.show_take_info,
        tr("Active Media Information"),
    )
    .clicked()
    {
        close = true;
    }
    if ui.button(tr("Playback Rate…")).clicked() {
        let mut w = std::mem::take(&mut app.clip_windows);
        w.open_rate(app, id);
        app.clip_windows = w;
        close = true;
    }
    let frozen = clip.freeze.is_some();
    if ui
        .add_enabled(
            is_video,
            egui::Button::new(if frozen {
                tr("Unfreeze Frame")
            } else {
                tr("Freeze Frame at Cursor")
            }),
        )
        .clicked()
    {
        let at = app.cursor;
        if frozen {
            app.edit(|p| p.set_clip_freeze(id, None));
        } else {
            app.edit(|p| p.freeze_frame_at(id, at));
        }
        close = true;
    }
    if ui
        .add_enabled(
            app.clipboard.is_some(),
            egui::Button::new(tr("Paste Clip Attributes…")),
        )
        .clicked()
    {
        if !app.selection.contains(&id) {
            app.select_clip(id, false);
        }
        app.clip_windows.open_paste();
        close = true;
    }
    if crate::widgets::checkbox(ui, &mut app.timeline.show_clip_headers, tr("Clip Headers"))
        .clicked()
    {
        close = true;
    }
    if ui.button(tr("Clip Length…")).clicked() {
        let mut w = std::mem::take(&mut app.clip_windows);
        w.open_duration(app, id);
        app.clip_windows = w;
        close = true;
    }
    ui.separator();
    if ui
        .add_enabled(is_video, egui::Button::new(tr("Color Grading")))
        .clicked()
    {
        app.select_clip(id, false);
        crate::tabs::color::open_at(app, crate::tabs::color::Level::Event);
        close = true;
    }
    if ui.button(tr("Media FX…")).clicked() {
        app.clip_windows.media_fx = Some((clip.media, !is_video));
        close = true;
    }
    if ui.button(tr("Clip Controls")).clicked() {
        app.select_clip(id, false);
        app.show_tab(Tab::Properties, false);
        close = true;
    }
    if ui
        .add_enabled(is_video, egui::Button::new(tr("Motion Tracking")))
        .clicked()
    {
        app.open_motion_tracking(Some(id));
        close = true;
    }
    if ui
        .add_enabled(is_video, egui::Button::new(tr("Detect Scenes and Split…")))
        .clicked()
    {
        app.clip_windows.scenes = Some((id, 0.5, false));
        close = true;
    }
    ui.separator();
    if ui.button(tr("Edit Visible Buttons…")).clicked() {
        app.clip_windows.buttons_editor = true;
        close = true;
    }
    close
}
