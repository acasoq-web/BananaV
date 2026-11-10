//! Customize…: the settings of a template, saved as the user's own.
//!
//! Like VEGAS, BananaV's templates cannot be changed: OK keeps the changes
//! as an "Untitled" template of the format, and Save Template keeps them
//! under the typed name. The user's templates can be deleted.

use bv_core::FrameRate;
use bv_engine::{AudioParams, Category, Encoder, FileFormat, ProResProfile, RateMode, VideoParams};

use super::templates::{CustomTemplate, Template, channels_text, rate_text};
use crate::app::BananaApp;
use crate::egui::{self, Align, Layout, RichText, Stroke, Ui, vec2};
use crate::i18n::{tr, trc};
use crate::theme;

pub enum Outcome {
    Open,
    Closed,
    /// The template to select (saved or kept).
    Chosen(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Video,
    Audio,
    Project,
}

#[derive(Debug)]
pub struct CustomizeWindow {
    base: Template,
    name: String,
    notes: String,
    video: Option<VideoParams>,
    audio: Option<AudioParams>,
    tab: Tab,
}

const SIZES: [(u32, u32); 9] = [
    (3840, 2160),
    (2560, 1440),
    (1920, 1080),
    (1280, 720),
    (1080, 1920),
    (1080, 1350),
    (1080, 1080),
    (720, 480),
    (640, 360),
];

const RATES: [FrameRate; 8] = [
    FrameRate::FPS_23_976,
    FrameRate::FPS_24,
    FrameRate::FPS_25,
    FrameRate::FPS_29_97,
    FrameRate::FPS_30,
    FrameRate::FPS_50,
    FrameRate::FPS_59_94,
    FrameRate::FPS_60,
];

fn sample_rates(f: FileFormat) -> &'static [u32] {
    match f {
        FileFormat::AvcMp4 | FileFormat::HevcMp4 | FileFormat::Av1Mp4 | FileFormat::Aac => {
            &[44_100, 48_000]
        }
        FileFormat::Mp3 => &[32_000, 44_100, 48_000],
        FileFormat::Wma | FileFormat::Wmv => &[44_100, 48_000],
        _ => &[22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 192_000],
    }
}

fn bitrates(f: FileFormat) -> &'static [u32] {
    match f {
        FileFormat::Mp3 | FileFormat::Wma => {
            &[64_000, 96_000, 128_000, 160_000, 192_000, 256_000, 320_000]
        }
        FileFormat::Ac3 => &[192_000, 384_000, 448_000, 640_000],
        _ => &[96_000, 128_000, 160_000, 192_000],
    }
}

fn encoder_label(e: Encoder) -> &'static str {
    match e {
        Encoder::Software => tr("Processor (software)"),
        Encoder::Nvidia => "NVIDIA NVENC",
        Encoder::Amd => "AMD VCE/AMF",
        Encoder::Intel => "Intel Quick Sync Video",
        Encoder::Qualcomm => "Qualcomm Adreno",
        Encoder::Apple => "Apple VideoToolbox",
    }
}

/// A drop-down of `options` (value, label).
fn combo<T: PartialEq + Copy>(ui: &mut Ui, id: &str, value: &mut T, options: &[(T, String)]) {
    let text = options
        .iter()
        .find(|o| o.0 == *value)
        .map_or(String::new(), |o| o.1.clone());
    egui::ComboBox::from_id_salt(id)
        .width(220.0)
        .selected_text(text)
        .show_ui(ui, |ui| {
            for (v, label) in options {
                ui.selectable_value(value, *v, label);
            }
        });
}

/// A labelled row.
fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(240.0, 22.0), egui::Label::new(label));
        add(ui);
    });
}

impl CustomizeWindow {
    pub fn new(t: Template, _app: &BananaApp) -> CustomizeWindow {
        CustomizeWindow {
            name: t.name(),
            notes: t.notes(),
            video: t.video,
            audio: t.audio,
            tab: if t.format.has_video() {
                Tab::Video
            } else {
                Tab::Audio
            },
            base: t,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> Outcome {
        let mut open = true;
        let mut outcome = Outcome::Open;
        let format = self.base.format;
        crate::floating::Window::new(trf!("Custom Settings - {}", tr(format.name())))
            .id(egui::Id::new("render-customize"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0, theme::separator());
                ui.horizontal(|ui| {
                    ui.label(tr("Template:"));
                    ui.add_sized(
                        vec2(330.0, 22.0),
                        egui::TextEdit::singleline(&mut self.name),
                    );
                    if crate::widgets::icon_button(
                        ui,
                        crate::icons::Icon::Save,
                        tr("Save Template"),
                    )
                    .clicked()
                    {
                        outcome = Outcome::Chosen(self.save(app, false));
                    }
                    let can_delete = self.base.is_custom();
                    if crate::widgets::icon_button_ex(
                        ui,
                        crate::icons::Icon::Delete,
                        tr("Delete Template"),
                        false,
                        can_delete,
                    )
                    .clicked()
                    {
                        let id = self.base.id.clone();
                        app.settings.render.custom.retain(|c| c.id != id);
                        app.settings.render.favorites.retain(|f| f != &id);
                        let _ = app.settings.save();
                        outcome = Outcome::Closed;
                    }
                });
                ui.label(tr("Notes:"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.notes)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    for (tab, label, enabled) in [
                        (Tab::Video, tr("Video"), self.video.is_some()),
                        (Tab::Audio, tr("Audio"), self.audio.is_some()),
                        (Tab::Project, tr("Project"), true),
                    ] {
                        if crate::widgets::tab_button(ui, label, self.tab == tab, enabled, 80.0)
                            .clicked()
                        {
                            self.tab = tab;
                        }
                    }
                });
                egui::Frame::new()
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0, theme::separator()))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_min_size(vec2(540.0, 280.0));
                        match self.tab {
                            Tab::Video => self.video_tab(ui, app),
                            Tab::Audio => self.audio_tab(ui),
                            Tab::Project => self.project_tab(ui, app),
                        }
                    });
                ui.add_space(6.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(tr("Cancel")).min_size(vec2(80.0, 0.0)))
                        .clicked()
                    {
                        outcome = Outcome::Closed;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("OK").strong())
                                .min_size(vec2(80.0, 0.0)),
                        )
                        .clicked()
                    {
                        outcome = Outcome::Chosen(self.save(app, true));
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    outcome = Outcome::Closed;
                }
            });
        if !open {
            return Outcome::Closed;
        }
        outcome
    }

    /// Keeps the settings as a template of the user's; returns its id.
    /// `ok`: OK was pressed (unsaved changes go to "Untitled").
    fn save(&mut self, app: &mut BananaApp, ok: bool) -> String {
        let unchanged = self.video == self.base.video
            && self.audio == self.base.audio
            && self.notes == self.base.notes()
            && self.name == self.base.name();
        if ok && unchanged {
            return self.base.id.clone();
        }
        let format = self.base.format;
        let name = self.name.trim();
        let typed = !name.is_empty() && name != self.base.name();
        let (id, name) = if typed || (self.base.is_custom() && !name.is_empty()) {
            // The user's template of that name (new or replaced).
            let existing = app
                .settings
                .render
                .custom
                .iter()
                .find(|c| c.format == format && c.name == name)
                .map(|c| c.id.clone());
            let id = existing.unwrap_or_else(|| {
                let n = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                format!("custom:{}:{n}", format.extension())
            });
            (id, name.to_string())
        } else {
            (
                format!("custom:untitled:{}", format.extension()),
                tr("Untitled").to_string(),
            )
        };
        let t = CustomTemplate {
            id: id.clone(),
            format,
            name,
            notes: self.notes.clone(),
            video: self.video,
            audio: self.audio,
        };
        let list = &mut app.settings.render.custom;
        match list.iter_mut().find(|c| c.id == id) {
            Some(c) => *c = t,
            None => list.push(t),
        }
        let _ = app.settings.save();
        id
    }

    fn video_tab(&mut self, ui: &mut Ui, app: &BananaApp) {
        let format = self.base.format;
        let p = app.project().settings;
        let Some(v) = &mut self.video else {
            ui.label(tr("This format has no video."));
            return;
        };
        let (pw, ph) = p.output_size();
        let mut sizes: Vec<((u32, u32), String)> = vec![((0, 0), trf!("Project ({}x{})", pw, ph))];
        sizes.extend(SIZES.iter().map(|&(w, h)| ((w, h), format!("{w}x{h}"))));
        let mut size = (v.width, v.height);
        let custom = size != (0, 0) && !SIZES.contains(&size);
        if custom {
            sizes.push((size, trf!("Custom ({}x{})", size.0, size.1)));
        }
        row(ui, tr("Frame size:"), |ui| {
            combo(ui, "cs-size", &mut size, &sizes);
        });
        if size != (v.width, v.height) {
            v.width = size.0;
            v.height = size.1;
        }
        if v.width != 0 {
            row(ui, "", |ui| {
                ui.label(tr("Width:"));
                ui.add(egui::DragValue::new(&mut v.width).range(16..=8192));
                ui.label(tr("Height:"));
                ui.add(egui::DragValue::new(&mut v.height).range(16..=8192));
            });
        }
        let mut rates: Vec<(FrameRate, String)> = vec![(
            super::templates::PROJECT_RATE,
            trf!("Project ({} fps)", rate_text(p.frame_rate)),
        )];
        rates.extend(RATES.iter().map(|r| (*r, crate::fmt::fps(*r))));
        row(ui, tr("Frame rate:"), |ui| {
            combo(ui, "cs-rate", &mut v.frame_rate, &rates);
        });
        // Interlaced renders: two fields a frame, the first at the
        // frame's time, the second half a frame later.
        use bv_core::FieldOrder;
        row(ui, tr("Field order:"), |ui| {
            combo(
                ui,
                "cs-fields",
                &mut v.field_order,
                &[
                    (
                        FieldOrder::Progressive,
                        tr("None (progressive scan)").to_string(),
                    ),
                    (FieldOrder::UpperFirst, tr("Upper field first").to_string()),
                    (FieldOrder::LowerFirst, tr("Lower field first").to_string()),
                ],
            );
        });
        if format.category() == Category::Video
            && format != FileFormat::ProRes
            && format != FileFormat::Mpeg1
            && format != FileFormat::Mpeg2
        {
            let encoders: Vec<(Encoder, String)> = format
                .video_encoders()
                .into_iter()
                .map(|e| (e, encoder_label(e).to_string()))
                .collect();
            if !encoders.is_empty() {
                row(ui, tr("Encode mode:"), |ui| {
                    combo(ui, "cs-encoder", &mut v.encoder, &encoders);
                });
            }
            if matches!(format, FileFormat::HevcMp4 | FileFormat::Av1Mp4) {
                let ten = format.ten_bit(v.encoder);
                if !ten {
                    v.bit_depth = 8;
                }
                row(ui, tr("Bit depth:"), |ui| {
                    ui.add_enabled_ui(ten, |ui| {
                        combo(
                            ui,
                            "cs-depth",
                            &mut v.bit_depth,
                            &[(8, "8 bit".into()), (10, "10 bit".into())],
                        );
                    });
                });
            }
            row(ui, tr("Bit rate control:"), |ui| {
                combo(
                    ui,
                    "cs-mode",
                    &mut v.rate_mode,
                    &[
                        (RateMode::Cbr, tr("Constant bit rate (CBR)").to_string()),
                        (RateMode::Vbr, tr("Variable bit rate (VBR)").to_string()),
                        (RateMode::Quality, tr("Constant quality").to_string()),
                    ],
                );
            });
            if v.rate_mode == RateMode::Quality {
                row(ui, tr("Quality:"), |ui| {
                    ui.add(egui::Slider::new(&mut v.quality, 1..=100));
                });
            } else {
                let mut avg = v.bitrate as f32 / 1e6;
                row(ui, tr("Average bit rate (Mb/s):"), |ui| {
                    ui.add(
                        crate::fmt::drag(&mut avg, 1, "")
                            .range(0.1..=400.0)
                            .speed(0.1),
                    );
                });
                v.bitrate = (avg * 1e6) as u32;
                if v.rate_mode == RateMode::Vbr {
                    let mut max = v.max_bitrate.max(v.bitrate) as f32 / 1e6;
                    row(ui, tr("Maximum bit rate (Mb/s):"), |ui| {
                        ui.add(
                            crate::fmt::drag(&mut max, 1, "")
                                .range(0.1..=600.0)
                                .speed(0.1),
                        );
                    });
                    v.max_bitrate = (max * 1e6).max(v.bitrate as f32) as u32;
                }
            }
            row(ui, tr("Keyframe interval (frames):"), |ui| {
                ui.add(egui::DragValue::new(&mut v.keyframe_interval).range(0..=600))
                    .on_hover_text(tr("0: one per second"));
            });
        }
        if format == FileFormat::ProRes {
            let profiles: Vec<(ProResProfile, String)> = ProResProfile::ALL
                .iter()
                .map(|p| (*p, format!("ProRes {}", p.name())))
                .collect();
            row(ui, tr("Profile:"), |ui| {
                combo(ui, "cs-prores", &mut v.prores, &profiles);
            });
            if !v.prores.is_444() {
                v.alpha = false;
            }
        }
        if format.supports_alpha() {
            let can = format != FileFormat::ProRes || v.prores.is_444();
            row(ui, "", |ui| {
                ui.add_enabled_ui(can, |ui| {
                    crate::widgets::checkbox(ui, &mut v.alpha, tr("Include alpha channel"));
                });
            });
        }
        match format {
            FileFormat::Jpeg | FileFormat::Heic | FileFormat::WmPhoto => {
                row(ui, tr("Quality:"), |ui| {
                    ui.add(egui::Slider::new(&mut v.quality, 1..=100));
                });
            }
            FileFormat::Png | FileFormat::Tiff => {
                row(ui, tr("Bit depth:"), |ui| {
                    combo(
                        ui,
                        "cs-imgdepth",
                        &mut v.bit_depth,
                        &[(8, "8 bit".into()), (16, "16 bit".into())],
                    );
                });
            }
            FileFormat::Exr => {
                row(ui, tr("Pixel type:"), |ui| {
                    combo(
                        ui,
                        "cs-exr",
                        &mut v.bit_depth,
                        &[
                            (16, tr("Half float (16 bit)").to_string()),
                            (32, tr("Float (32 bit)").to_string()),
                        ],
                    );
                });
            }
            _ => {}
        }
    }

    fn audio_tab(&mut self, ui: &mut Ui) {
        let format = self.base.format;
        let Some(a) = &mut self.audio else {
            ui.label(tr("This format has no audio."));
            return;
        };
        let mut rates: Vec<(u32, String)> = vec![(0, tr("Project").to_string())];
        rates.extend(
            sample_rates(format)
                .iter()
                .map(|r| (*r, format!("{} Hz", crate::fmt::grouped(*r as u64)))),
        );
        row(ui, tr("Sample rate (Hz):"), |ui| {
            combo(ui, "cs-srate", &mut a.sample_rate, &rates);
        });
        let channels: Vec<(u16, String)> = match format {
            FileFormat::Mp3 | FileFormat::Wma | FileFormat::Wmv => vec![1, 2],
            FileFormat::AvcMp4 | FileFormat::HevcMp4 | FileFormat::Av1Mp4 | FileFormat::Aac => {
                vec![1, 2, 6]
            }
            _ => vec![1, 2, 6, 8],
        }
        .into_iter()
        .map(|c| (c, channels_text(c)))
        .collect();
        row(ui, tr("Channels:"), |ui| {
            combo(ui, "cs-channels", &mut a.channels, &channels);
        });
        if format.compressed_audio() {
            let options: Vec<(u32, String)> = bitrates(format)
                .iter()
                .map(|b| (*b, format!("{} kb/s", b / 1000)))
                .collect();
            row(ui, tr("Bit rate:"), |ui| {
                combo(ui, "cs-abitrate", &mut a.bitrate, &options);
            });
        } else {
            // (bits, float)
            let depths: Vec<((u8, bool), String)> = match format {
                FileFormat::Flac => vec![
                    ((16, false), "16 bit".into()),
                    ((24, false), "24 bit".into()),
                ],
                FileFormat::Aiff => vec![
                    ((8, false), "8 bit".into()),
                    ((16, false), "16 bit".into()),
                    ((24, false), "24 bit".into()),
                    ((32, false), "32 bit".into()),
                ],
                FileFormat::ProRes => vec![
                    ((16, false), "16 bit".into()),
                    ((24, false), "24 bit".into()),
                ],
                _ => vec![
                    ((8, false), "8 bit".into()),
                    ((16, false), "16 bit".into()),
                    ((24, false), "24 bit".into()),
                    ((32, false), "32 bit".into()),
                    ((32, true), tr("32 bit (IEEE float)").to_string()),
                ],
            };
            let mut d = (a.bit_depth, a.float);
            row(ui, tr("Bit depth:"), |ui| {
                combo(ui, "cs-adepth", &mut d, &depths);
            });
            a.bit_depth = d.0;
            a.float = d.1;
        }
    }
}

/// A drop-down of `options` (value, label, available): the ones BananaV
/// cannot do yet greyed out, the list as wide as its longest label.
fn choice<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[(T, String, bool)],
) {
    let text = options
        .iter()
        .find(|o| o.0 == *value)
        .map_or(String::new(), |o| o.1.clone());
    egui::ComboBox::from_id_salt(id)
        .width(220.0)
        .height(640.0)
        .selected_text(text)
        .show_ui(ui, |ui| {
            for (v, label, ready) in options {
                if *ready {
                    ui.selectable_value(value, *v, label);
                } else {
                    crate::widgets::soon_item(ui, label);
                }
            }
        });
}

impl CustomizeWindow {
    /// VEGAS's Project tab: the video rendering quality (the project's or
    /// the template's own) and, under ACES, the color space frames leave
    /// in and the range of the YCbCr written.
    fn project_tab(&mut self, ui: &mut Ui, app: &BananaApp) {
        use bv_timeline::{ColorSpace, RenderQuality};
        let aces = app.project().settings.color_managed();
        let mut none = VideoParams::default();
        let has_video = self.video.is_some();
        let v = self.video.as_mut().unwrap_or(&mut none);
        ui.add_enabled_ui(has_video, |ui| {
            let qualities = [
                (None, tr("Use Project Settings").to_string(), true),
                (Some(RenderQuality::Draft), tr("Draft").to_string(), true),
                (
                    Some(RenderQuality::Preview),
                    tr("Preview").to_string(),
                    true,
                ),
                (Some(RenderQuality::Good), tr("Good").to_string(), true),
                (Some(RenderQuality::Best), tr("Best").to_string(), true),
            ];
            row(ui, tr("Video rendering quality:"), |ui| {
                choice(
                    ui,
                    "cs-rendering-quality",
                    &mut v.rendering_quality,
                    &qualities,
                );
            });
            ui.add_space(60.0);
            // VEGAS's list: Default, then the ACES and camera spaces.
            let mut spaces = vec![(None, tr("Default").to_string(), true)];
            spaces.extend(
                ColorSpace::ALL
                    .iter()
                    .map(|&c| (Some(c), c.name().to_string(), c.drawn())),
            );
            row(ui, tr("Color space:"), |ui| {
                ui.add_enabled_ui(aces, |ui| {
                    choice(ui, "cs-color-space", &mut v.color_space, &spaces);
                })
                .response
                .on_disabled_hover_text(tr(
                    "The color space applies with ACES (File › Properties › Video).",
                ));
            });
            let mut range = 0;
            row(ui, tr("Color range (YCbCr output):"), |ui| {
                choice(
                    ui,
                    "cs-color-range",
                    &mut range,
                    &[
                        (0, tr("Limited (default)").to_string(), true),
                        (1, trc("color range", "Full").to_string(), false),
                    ],
                );
            });
        });
    }
}
