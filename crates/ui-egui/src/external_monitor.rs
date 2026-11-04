//! Video Preview › Video Preview on External Monitor (Alt+Shift+4): the
//! preview full screen on a display of its own, as Options › Preferences ›
//! Preview Device sets it up (the graphics card's display; scaled to fit
//! or at its size; deinterlaced; studio levels expanded; the project's
//! output rotation or none; a sync offset). Esc or a double-click on it
//! closes it; Space plays and stops.

use std::time::{Duration, Instant};

use bv_core::{Tick, TimeRange};
use bv_timeline::OutputRotation;

use crate::app::BananaApp;
use crate::egui::{self, Color32, Key, Rect, TextureHandle, TextureOptions, pos2, vec2};
use crate::i18n::tr;
use crate::preferences::preview_display;

/// The external monitor while it is on.
#[derive(Default)]
pub(crate) struct ExternalMonitor {
    texture: Option<TextureHandle>,
    /// The frame shown (generation, time, size), so each is prepared once.
    shown: Option<(u64, Tick, u32, u32)>,
    /// The frame's size once prepared (rotated back, scaled).
    size: (u32, u32),
}

impl std::fmt::Debug for ExternalMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalMonitor")
            .field("shown", &self.shown)
            .finish()
    }
}

/// How the frames are prepared for the display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Prepare {
    /// The project's output rotation to take back (Use project output
    /// rotation setting off).
    pub unrotate: OutputRotation,
    pub deinterlace: bool,
    pub studio_rgb: bool,
    /// Scaled here, on the processor, to this size (Optimize GPU display
    /// performance off).
    pub scale_to: Option<(u32, u32)>,
}

/// Rotates an RGBA picture by `turns` quarter turns clockwise.
fn rotate(rgba: &[u8], w: u32, h: u32, turns: u32) -> (Vec<u8>, u32, u32) {
    let (w, h) = (w as usize, h as usize);
    match turns % 4 {
        0 => (rgba.to_vec(), w as u32, h as u32),
        2 => {
            let mut out = vec![0u8; rgba.len()];
            for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
                let j = w * h - 1 - i;
                out[j * 4..j * 4 + 4].copy_from_slice(px);
            }
            (out, w as u32, h as u32)
        }
        turns => {
            // A quarter turn: the picture is h wide, w tall.
            let mut out = vec![0u8; rgba.len()];
            for y in 0..h {
                for x in 0..w {
                    let (nx, ny) = if turns == 1 {
                        (h - 1 - y, x)
                    } else {
                        (y, w - 1 - x)
                    };
                    let src = (y * w + x) * 4;
                    let dst = (ny * h + nx) * 4;
                    out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
            (out, h as u32, w as u32)
        }
    }
}

/// Blends each pair of lines (the two fields of a frame) into both.
fn deinterlace(rgba: &mut [u8], w: u32) {
    let row = w as usize * 4;
    for pair in rgba.chunks_exact_mut(row * 2) {
        let (a, b) = pair.split_at_mut(row);
        for (x, y) in a.iter_mut().zip(b.iter_mut()) {
            let m = (*x as u16 + *y as u16).div_ceil(2) as u8;
            *x = m;
            *y = m;
        }
    }
}

/// Studio RGB (black 16, white 235) to the computer's full range.
fn studio_to_computer(rgba: &mut [u8]) {
    let lut: Vec<u8> = (0..256)
        .map(|v| (((v as f32 - 16.0) * 255.0 / 219.0).round()).clamp(0.0, 255.0) as u8)
        .collect();
    for px in rgba.as_chunks_mut::<4>().0 {
        for c in &mut px[..3] {
            *c = lut[*c as usize];
        }
    }
}

/// Scales an RGBA picture to `to` (bilinear).
fn scale(rgba: &[u8], w: u32, h: u32, to: (u32, u32)) -> Vec<u8> {
    let (tw, th) = (to.0.max(1) as usize, to.1.max(1) as usize);
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; tw * th * 4];
    for y in 0..th {
        let fy = ((y as f32 + 0.5) * h as f32 / th as f32 - 0.5).max(0.0);
        let (y0, ay) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(h - 1);
        for x in 0..tw {
            let fx = ((x as f32 + 0.5) * w as f32 / tw as f32 - 0.5).max(0.0);
            let (x0, ax) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(w - 1);
            for c in 0..4 {
                let at = |xx: usize, yy: usize| rgba[(yy * w + xx) * 4 + c] as f32;
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * ax;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * ax;
                out[(y * tw + x) * 4 + c] = (top + (bottom - top) * ay).round() as u8;
            }
        }
    }
    out
}

/// The frame as the display shows it.
pub(crate) fn prepare(rgba: &[u8], w: u32, h: u32, how: Prepare) -> (Vec<u8>, u32, u32) {
    let back = match how.unrotate {
        OutputRotation::None => 0,
        OutputRotation::Clockwise90 => 3,
        OutputRotation::Inverted180 => 2,
        OutputRotation::Counterclockwise90 => 1,
    };
    let (mut img, w, h) = rotate(rgba, w, h, back);
    if how.deinterlace {
        deinterlace(&mut img, w);
    }
    if how.studio_rgb {
        studio_to_computer(&mut img);
    }
    match how.scale_to {
        Some(to) if to != (w, h) => {
            let out = scale(&img, w, h, to);
            (out, to.0, to.1)
        }
        _ => (img, w, h),
    }
}

/// `size` fitted inside `room`, keeping its shape.
fn fit(size: (u32, u32), room: (u32, u32)) -> (u32, u32) {
    let k = (room.0 as f32 / size.0.max(1) as f32).min(room.1 as f32 / size.1.max(1) as f32);
    (
        ((size.0 as f32 * k).round() as u32).max(1),
        ((size.1 as f32 * k).round() as u32).max(1),
    )
}

impl BananaApp {
    /// Video Preview on External Monitor, on or off.
    pub(crate) fn toggle_external_monitor(&mut self) {
        if self.external.take().is_some() {
            return;
        }
        let displays = bv_platform::displays();
        if preview_display(&displays, self.settings.prefs.preview_display.as_deref()).is_none() {
            self.notify(tr("No display found for the external monitor."));
            return;
        }
        self.external = Some(ExternalMonitor::default());
    }

    /// Options › Preferences › Preview Device › Identify Displays: each
    /// display's number on it for a few seconds.
    pub(crate) fn identify_displays(&mut self) {
        self.identify_until = Some(Instant::now() + Duration::from_secs(3));
    }

    /// The numbers of Identify Displays, each in a small window in the middle
    /// of its display.
    fn identify_windows(&mut self, ctx: &egui::Context) {
        let Some(until) = self.identify_until else {
            return;
        };
        if Instant::now() >= until || ctx.embed_viewports() {
            self.identify_until = None;
            return;
        }
        for (i, d) in bv_platform::displays().iter().enumerate() {
            let number = crate::preferences::display_number(i, d);
            let size = vec2(200.0, 160.0);
            let center = pos2(
                (d.x as f32 + d.width as f32 / 2.0) / d.scale,
                (d.y as f32 + d.height as f32 / 2.0) / d.scale,
            );
            let builder = egui::ViewportBuilder::default()
                .with_title(trf!("Display {}", number))
                .with_decorations(false)
                .with_taskbar(false)
                .with_always_on_top()
                .with_inner_size(size)
                .with_position(center - size / 2.0);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("identify-display", i)),
                builder,
                |ui, _| {
                    let rect = ui.max_rect();
                    ui.painter()
                        .rect_filled(rect, 0.0, Color32::from_black_alpha(230));
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        number.to_string(),
                        egui::FontId::proportional(110.0),
                        Color32::WHITE,
                    );
                },
            );
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    /// The external monitor's window, full screen on its display.
    pub(crate) fn external_monitor_window(&mut self, ctx: &egui::Context) {
        self.identify_windows(ctx);
        let Some(mut ext) = self.external.take() else {
            return;
        };
        let displays = bv_platform::displays();
        let prefs = self.settings.prefs.clone();
        let Some(index) = preview_display(&displays, prefs.preview_display.as_deref()) else {
            self.notify(tr("No display found for the external monitor."));
            return;
        };
        let display = &displays[index];
        let settings = self.project().settings;
        // The frame: at the display's size, or the project's own.
        let (out_w, out_h) = settings.output_size();
        let room = (display.width, display.height);
        let (max_w, max_h) = if prefs.preview_scale_to_fit {
            fit((out_w, out_h), room)
        } else {
            (out_w, out_h)
        };
        // Options › Preferences › Preview Device › "Sync Offset (frames)":
        // ahead (or behind) while playing.
        let rate = settings.frame_rate;
        let mut t = self.display_time();
        if self.is_playing() {
            t = (t + Tick(rate.frame_duration().0 * prefs.preview_sync_offset as i64))
                .max(Tick::ZERO);
        }
        // Multicamera editing: the takes in tiles, as in the Video
        // Preview, unless Tools › Multicamera › Show Full Frame on External
        // Monitor.
        let tiles = if self.settings.multicam_full_frame {
            None
        } else {
            self.multicam_view()
        };
        // Trimmer on External Monitor: the Trimmer's picture while it has
        // the focus (VEGAS's Help: the window clicked last decides).
        let trimmer = (self.trimmer.external && self.keys_tab == Some(crate::dock::Tab::Trimmer))
            .then(|| self.trimmer_picture())
            .flatten();
        let frame = match (&trimmer, &tiles) {
            (Some((info, t)), _) => {
                // Asked for here only when the Trimmer and the Video
                // Preview do not show it themselves.
                let s = &self.settings;
                if s.trimmer_no_monitor && s.trimmer_no_preview_window {
                    self.engine
                        .request_trimmer(info, *t, max_w.max(2), max_h.max(2));
                }
                self.engine.trimmer_frame()
            }
            (None, Some((project, _, _))) => {
                self.engine
                    .request_multicam(project.clone(), t, max_w.max(2), max_h.max(2));
                self.engine.multicam_frame()
            }
            (None, None) => {
                // At the Video Preview's quality, without its split screen
                // or channels.
                self.engine.set_preview_look(bv_engine::PreviewLook {
                    quality: Some(self.settings.preview.quality.level),
                    ..Default::default()
                });
                self.engine.request_preview(t, max_w.max(2), max_h.max(2));
                self.engine.preview_frame()
            }
        };
        if let Some(f) = frame {
            let key = (f.generation, f.time, f.width, f.height);
            // "Recompress edited frames" off: frames the project draws
            // (effects, transitions, compositing) are not sent, only plain
            // media.
            let edited = !prefs.preview_recompress && tiles.is_none() && trimmer.is_none() && {
                let range = TimeRange::new(f.time, rate.frame_duration());
                let prerendered = self.engine.prerendered().iter().any(|r| r.contains(f.time));
                self.project().needs_prerender(range) && !prerendered
            };
            if ext.shown != Some(key) && !edited {
                let unrotate = if prefs.preview_rotation {
                    OutputRotation::None
                } else {
                    settings.rotation
                };
                let rotated = matches!(
                    unrotate,
                    OutputRotation::Clockwise90 | OutputRotation::Counterclockwise90
                );
                let (fw, fh) = if rotated {
                    (f.height, f.width)
                } else {
                    (f.width, f.height)
                };
                let how = Prepare {
                    unrotate,
                    deinterlace: prefs.preview_deinterlace,
                    studio_rgb: prefs.preview_studio_rgb,
                    scale_to: (!prefs.preview_optimize_gpu && prefs.preview_scale_to_fit)
                        .then(|| fit((fw, fh), room)),
                };
                let (rgba, w, h) = prepare(&f.rgba, f.width, f.height, how);
                let image = crate::tabs::preview::frame_image([w as usize, h as usize], &rgba);
                match &mut ext.texture {
                    Some(tex) => tex.set(image, TextureOptions::LINEAR),
                    None => {
                        ext.texture = Some(ctx.load_texture(
                            "external-monitor",
                            image,
                            TextureOptions::LINEAR,
                        ));
                    }
                }
                ext.shown = Some(key);
                ext.size = (w, h);
            }
        }
        let builder = egui::ViewportBuilder::default()
            .with_title(tr("External Monitor"))
            .with_decorations(false)
            .with_taskbar(false)
            .with_monitor(index);
        let mut close = false;
        let mut play = false;
        let embedded = ctx.embed_viewports();
        let texture = ext.texture.clone();
        let size = ext.size;
        let scale_to_fit = prefs.preview_scale_to_fit;
        let overlay = tiles
            .filter(|_| trimmer.is_none())
            .map(|(_, tiles, look)| (tiles, look));
        let draw = |ui: &mut egui::Ui, area: Rect| {
            ui.painter().rect_filled(area, 0.0, Color32::BLACK);
            if let Some(tex) = &texture {
                let ppp = ui.ctx().pixels_per_point();
                let picture = vec2(size.0 as f32, size.1 as f32);
                let shown = if scale_to_fit {
                    let k = (area.width() / picture.x).min(area.height() / picture.y);
                    picture * k
                } else {
                    // Its own pixels, one for one.
                    picture / ppp
                };
                let picture = Rect::from_center_size(area.center(), shown);
                ui.painter().image(
                    tex.id(),
                    picture,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                if let Some((tiles, look)) = &overlay {
                    crate::multicam::paint(ui.painter(), picture, tiles, look);
                }
            }
        };
        if embedded {
            // Without windows of their own (the tests): a window in the
            // main one.
            egui::Window::new(tr("External Monitor"))
                .id(egui::Id::new("external-monitor"))
                .default_size(vec2(480.0, 270.0))
                .show(ctx, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(480.0, 270.0), egui::Sense::hover());
                    draw(ui, rect);
                });
        } else {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("external-monitor"),
                builder,
                |ui, _| {
                    let area = ui.max_rect();
                    let resp = ui.interact(area, ui.id().with("external"), egui::Sense::click());
                    draw(ui, area);
                    let input = ui.ctx().input(|i| {
                        (
                            i.key_pressed(Key::Escape) || i.viewport().close_requested(),
                            i.key_pressed(Key::Space),
                        )
                    });
                    close = input.0 || resp.double_clicked();
                    play = input.1;
                },
            );
        }
        if play {
            self.space_bar();
        }
        if !close {
            self.external = Some(ext);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 picture: red, green / blue, white.
    fn picture() -> Vec<u8> {
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 255,
        ]
    }

    #[test]
    fn frames_are_turned_back_blended_expanded_and_scaled() {
        let none = Prepare {
            unrotate: OutputRotation::None,
            deinterlace: false,
            studio_rgb: false,
            scale_to: None,
        };
        assert_eq!(prepare(&picture(), 2, 2, none).0, picture());
        // Shown turned a quarter clockwise, taken back: the top right
        // (green) goes to the top left.
        let (back, w, h) = prepare(
            &picture(),
            2,
            2,
            Prepare {
                unrotate: OutputRotation::Clockwise90,
                ..none
            },
        );
        assert_eq!((w, h), (2, 2));
        assert_eq!(&back[..4], &[0, 255, 0, 255]);
        // A turn and its opposite give the picture back, on a wide one too.
        let wide: Vec<u8> = (0..6 * 4 * 3).map(|i| i as u8).collect();
        let (a, aw, ah) = rotate(&wide, 6, 3, 1);
        assert_eq!((aw, ah), (3, 6));
        assert_eq!(rotate(&a, aw, ah, 3).0, wide);
        // The two lines blended.
        let (blended, _, _) = prepare(
            &picture(),
            2,
            2,
            Prepare {
                deinterlace: true,
                ..none
            },
        );
        assert_eq!(&blended[..4], &blended[8..12]);
        assert_eq!(&blended[..4], &[128, 0, 128, 255]);
        // Studio black and white become 0 and 255.
        let mut studio = vec![16, 235, 126, 200];
        studio_to_computer(&mut studio);
        assert_eq!(&studio[..2], &[0, 255]);
        assert_eq!(studio[3], 200, "alpha as it was");
        // Scaled on the processor.
        let (big, w, h) = prepare(
            &picture(),
            2,
            2,
            Prepare {
                scale_to: Some((4, 4)),
                ..none
            },
        );
        assert_eq!((w, h, big.len()), (4, 4, 64));
        assert_eq!(&big[..4], &[255, 0, 0, 255], "the corner keeps its color");
        assert_eq!(fit((1920, 1080), (1280, 1024)), (1280, 720));
    }
}
