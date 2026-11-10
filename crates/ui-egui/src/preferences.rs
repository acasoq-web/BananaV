//! Options › Preferences: VEGAS's tabbed window with OK, Cancel and Apply.
//! Changes are made on a draft and only reach the app on OK/Apply.
//!
//! The tabs sit in VEGAS's three rows, the row of the open tab next to the
//! page, as Windows lays out tabs in rows; settings BananaV does not have
//! yet show greyed out with VEGAS's usual value. The Language tab is
//! BananaV's own (VEGAS has none): English is the main language and the
//! others are translations of it.

use std::path::PathBuf;

use bv_engine::{DecodeChoice, GpuChoice, GpuStatus};
use bv_timeline::{FadeCurve, KeyCurve};

use crate::app::BananaApp;
use crate::color_picker::{self, ColorPicker};
use crate::egui::{self, Align2, Color32, Rect, RichText, Stroke, Ui, Vec2, pos2, vec2};
use crate::i18n::{Language, tr, trc};
use crate::settings::{
    AudioDeviceType, AutoProxies, ControlDevice, CutAlign, EnvelopeColor, FadeAlign, FrameNumbers,
    JklSpeed, LocateMessages, MidiRoute, MtcRate, Prefs, PreviewBackground, RecordWaveforms,
    SnapKind, SurfaceKind, Thumbnails, track_color_name,
};
use crate::theme::Interface;
use crate::{theme, vst};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefTab {
    General,
    Video,
    PreviewDevice,
    Audio,
    AudioDevice,
    Midi,
    VstEffects,
    Editing,
    Display,
    CdSettings,
    Sync,
    ExternalControl,
    Deprecated,
    FileIo,
    Language,
}

impl PrefTab {
    /// VEGAS's rows of tabs, top to bottom as they stand with the General
    /// tab open (Language, BananaV's own, ends the middle row).
    const ROWS: [&'static [PrefTab]; 3] = [
        &[
            PrefTab::Editing,
            PrefTab::Display,
            PrefTab::CdSettings,
            PrefTab::Sync,
        ],
        &[
            PrefTab::ExternalControl,
            PrefTab::Deprecated,
            PrefTab::FileIo,
            PrefTab::Language,
        ],
        &[
            PrefTab::General,
            PrefTab::Video,
            PrefTab::PreviewDevice,
            PrefTab::Audio,
            PrefTab::AudioDevice,
            PrefTab::Midi,
            PrefTab::VstEffects,
        ],
    ];

    fn label(self) -> &'static str {
        match self {
            PrefTab::General => tr("General"),
            PrefTab::Video => tr("Video"),
            PrefTab::PreviewDevice => tr("Preview Device"),
            PrefTab::Audio => tr("Audio"),
            PrefTab::AudioDevice => tr("Audio Device"),
            PrefTab::Midi => tr("MIDI"),
            PrefTab::VstEffects => tr("VST Effects"),
            PrefTab::Editing => tr("Editing"),
            PrefTab::Display => tr("Display"),
            PrefTab::CdSettings => tr("CD Settings"),
            PrefTab::Sync => tr("Sync"),
            PrefTab::ExternalControl => tr("External Control & Automation"),
            PrefTab::Deprecated => tr("Deprecated Features"),
            PrefTab::FileIo => tr("File I/O"),
            PrefTab::Language => tr("Language"),
        }
    }
}

/// The colors swatches set (each opens the color picker): the Video
/// page's and the Display page's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorSetting {
    TakeIndicator,
    FadeTop,
    FadeBottom,
    Track(usize),
    Envelope(EnvelopeColor),
    Snap(SnapKind),
    ButtonHighlight,
}

/// The values the window edits (Options › Internal applies its own the
/// same way).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draft {
    pub(crate) show_welcome: bool,
    pub(crate) language: Language,
    pub(crate) gpu: GpuChoice,
    pub(crate) decoding: DecodeChoice,
    pub(crate) prefs: Prefs,
}

impl Draft {
    fn color_mut(&mut self, which: ColorSetting) -> &mut [u8; 4] {
        match which {
            ColorSetting::TakeIndicator => &mut self.prefs.take_indicator,
            ColorSetting::FadeTop => &mut self.prefs.fade_top,
            ColorSetting::FadeBottom => &mut self.prefs.fade_bottom,
            ColorSetting::Track(i) => &mut self.prefs.track_colors[i % 8],
            ColorSetting::Envelope(e) => self.prefs.envelope_color_mut(e),
            ColorSetting::Snap(k) => self.prefs.snap_color_mut(k),
            ColorSetting::ButtonHighlight => &mut self.prefs.button_highlight,
        }
    }

    fn of(app: &BananaApp) -> Draft {
        Draft {
            show_welcome: app.settings.show_welcome,
            language: app.settings.language,
            gpu: app.settings.video_gpu.clone(),
            decoding: app.settings.decoding.clone(),
            prefs: app.settings.prefs.clone(),
        }
    }

    pub(crate) fn apply(&self, app: &mut BananaApp) {
        app.settings.show_welcome = self.show_welcome;
        app.set_language(self.language);
        app.settings.video_gpu = self.gpu.clone();
        app.engine.set_gpu(self.gpu.clone());
        app.settings.decoding = self.decoding.clone();
        app.engine.set_hardware_decoding(self.decoding.clone());
        // Display timeline at bottom of main window: the timeline moves.
        let moved = app.settings.prefs.timeline_at_bottom != self.prefs.timeline_at_bottom;
        app.settings.prefs = self.prefs.clone();
        if moved {
            app.place_timeline(self.prefs.timeline_at_bottom);
        }
        crate::logs::set_levels(
            self.prefs.log_plugins,
            self.prefs.log_reads,
            self.prefs.log_renders,
        );
        bv_engine::Engine::set_render_threads(self.prefs.render_threads as usize);
        bv_engine::Engine::set_dim_db(self.prefs.dim_db as f32);
        bv_engine::Engine::set_peak_files(!self.prefs.no_peak_files, self.prefs.hide_peak_files);
        bv_engine::Engine::set_keep_bypassed_fx(self.prefs.keep_bypassed_fx);
        bv_engine::Engine::set_aces_on_gpu(self.prefs.aces_on_gpu);
        bv_bridge::set_enabled(self.prefs.vst_32bit);
        app.engine
            .set_ram_preview((self.prefs.ram_preview_mb as usize) << 20);
        app.apply_audio_prefs();
        let _ = app.settings.save();
    }
}

/// What a line of the General page's list changes.
#[derive(Clone, Copy)]
enum Item {
    /// A preference that works.
    On(fn(&mut Prefs) -> &mut bool),
    /// "Always show Welcome Screen when program starts" (kept apart).
    Welcome,
    /// A preference for a feature BananaV does not have yet: greyed out,
    /// its value kept.
    Later(fn(&mut Prefs) -> &mut bool),
}

/// The General page's list, in VEGAS's order (without its maker's online
/// services: BananaV has none).
fn general_items() -> [(&'static str, Item); 35] {
    use Item::{Later, On, Welcome};
    [
        (
            tr("Automatically open last project on startup"),
            On(|p| &mut p.open_last_project),
        ),
        (
            tr("Confirm media file deletion when still in use"),
            On(|p| &mut p.confirm_media_removal),
        ),
        (
            tr("Save active prerenders on project close"),
            On(|p| &mut p.keep_prerenders),
        ),
        (
            tr("Close media files when not the active application"),
            On(|p| &mut p.release_media_inactive),
        ),
        (
            tr("Close audio and MIDI ports when not the active application"),
            On(|p| &mut p.release_audio_inactive),
        ),
        (
            tr("Prompt to adjust project settings to match first media added to timeline"),
            On(|p| &mut p.match_first_media),
        ),
        (
            tr("Check project file type associations at startup"),
            On(|p| &mut p.check_associations),
        ),
        (
            tr("Check for updates when BananaV starts"),
            On(|p| &mut p.check_updates),
        ),
        (tr("Enable joystick support"), On(|p| &mut p.joystick)),
        (
            tr("Allow pulldown removal when opening 24p DV"),
            Later(|p| &mut p.dv_pulldown_removal),
        ),
        (
            tr("AAF Export - Use frame unit for audio"),
            Later(|p| &mut p.aaf_frame_units),
        ),
        (
            tr("AAF Export - Use clip-based audio envelope"),
            Later(|p| &mut p.aaf_clip_envelopes),
        ),
        (
            tr("Import stereo as dual mono"),
            On(|p| &mut p.stereo_as_dual_mono),
        ),
        (
            tr("Use GPU processing for ACES color management"),
            On(|p| &mut p.aces_on_gpu),
        ),
        (
            tr("Prompt to keep files after recording"),
            On(|p| &mut p.ask_keep_recordings),
        ),
        (
            tr("Create undos for FX parameter changes"),
            On(|p| &mut p.fx_undo),
        ),
        (
            tr("Keep bypassed FX running (to avoid pause on bypass/enable)"),
            On(|p| &mut p.keep_bypassed_fx),
        ),
        (
            tr("Automatically name regions and markers if not playing"),
            On(|p| &mut p.name_markers),
        ),
        (tr("Use linear scrub range"), On(|p| &mut p.linear_scrub)),
        (
            tr("Allow Ctrl+drag cursor style scrub over events"),
            On(|p| &mut p.ctrl_scrub_over_events),
        ),
        (
            tr("Make spacebar and F12 Play/Stop instead of Play/Pause"),
            On(|p| &mut p.space_play_stop),
        ),
        (tr("Always draw marker lines"), On(|p| &mut p.marker_lines)),
        (
            tr("Allow edit cursor to be dragged"),
            On(|p| &mut p.cursor_drag),
        ),
        (
            tr("Double click on media file loads into Trimmer instead of tracks"),
            On(|p| &mut p.media_to_trimmer),
        ),
        (
            tr("Show trimmer history with filename first, then folder"),
            On(|p| &mut p.trimmer_name_first),
        ),
        (
            tr("Automatically save trimmer markers and regions with media file"),
            On(|p| &mut p.save_trimmer_markers),
        ),
        (
            tr("Provide a warning if a track being deleted holds at least one event"),
            On(|p| &mut p.warn_track_delete),
        ),
        (
            tr("Warn if creating a nested timeline may adversely affect the project"),
            On(|p| &mut p.warn_nesting),
        ),
        (
            tr("Warn to save modified nested timeline prior to opening parent timeline"),
            On(|p| &mut p.warn_nested_save),
        ),
        (
            tr("Always show Welcome Screen when program starts"),
            Welcome,
        ),
        (tr("Enable autosave"), On(|p| &mut p.autosave)),
        (
            tr("Double Click event edge to toggle Expanded Edit Mode"),
            On(|p| &mut p.edge_expanded_edit),
        ),
        (
            tr("Enable Automatic FX Window Resizing"),
            On(|p| &mut p.fx_window_resize),
        ),
        (
            tr("Do not create .bvpk files to store peak data"),
            On(|p| &mut p.no_peak_files),
        ),
        (tr("Hide new .bvpk files"), On(|p| &mut p.hide_peak_files)),
    ]
}

#[derive(Debug)]
pub struct PreferencesDialog {
    pub tab: PrefTab,
    draft: Draft,
    /// Free space of the temporary files folder, and the computer's memory,
    /// looked up when the window opens.
    free_space: Option<u64>,
    memory: Option<u64>,
    /// The color picker a swatch opened, and the color it sets.
    picker: Option<(ColorSetting, ColorPicker)>,
    /// The displays (Preview Device), found when the window opens.
    displays: Vec<bv_platform::Display>,
    /// The audio devices (Audio Device), found when the tab is first shown:
    /// outputs, inputs and ASIO drivers.
    audio_devices: Option<AudioDevices>,
    /// The MIDI ports (MIDI), found when the tab is first shown.
    midi_ports: Option<MidiPorts>,
    vst: VstPage,
    /// External Control & Automation: the control surface whose window is
    /// open; Sync: its Advanced… window is open.
    device_dialog: Option<usize>,
    /// The Available devices list's choice, and the Configure Generic
    /// Control dialog's state.
    surface_pick: SurfaceKind,
    generic_ui: crate::generic_surface::GenericUi,
    /// The Configure Mackie Control dialog's state.
    mackie_ui: crate::mackie_config::MackieUi,
    advanced_sync: bool,
    display: DisplayPage,
}

/// The audio devices of this computer.
#[derive(Debug, Default)]
struct AudioDevices {
    outputs: Vec<(String, bool)>,
    inputs: Vec<bv_engine::InputDevice>,
    asio: Vec<String>,
}

impl AudioDevices {
    fn find() -> AudioDevices {
        AudioDevices {
            outputs: bv_engine::Engine::output_devices(),
            inputs: bv_engine::input_devices(),
            asio: bv_platform::asio_drivers(),
        }
    }
}

/// The Preview Device's devices: the external hardware ones BananaV does
/// not have yet, then the graphics card's display.
const PREVIEW_DEVICES: [&str; 4] = [
    "AJA Video Device",
    "Blackmagic Design DeckLink",
    "OHCI Compliant IEEE 1394/DV",
    "Windows Graphics Card",
];

/// The display the external monitor uses: the one chosen, else the first
/// that is not the main display, else the main one.
pub(crate) fn preview_display(
    displays: &[bv_platform::Display],
    chosen: Option<&str>,
) -> Option<usize> {
    chosen
        .and_then(|name| displays.iter().position(|d| d.name == name))
        .or_else(|| displays.iter().position(|d| !d.primary))
        .or_else(|| (!displays.is_empty()).then_some(0))
}

/// A display's number, as the system's display settings give it (from its
/// name, `\\.\DISPLAY2`); its place in the list otherwise.
pub(crate) fn display_number(index: usize, d: &bv_platform::Display) -> usize {
    let digits: String = d
        .name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().unwrap_or(index + 1)
}

/// How the display list names a display: "2. 1920 x 1080 (180)".
pub(crate) fn display_label(index: usize, d: &bv_platform::Display) -> String {
    format!(
        "{}. {} x {} ({})",
        display_number(index, d),
        d.width,
        d.height,
        d.hz
    )
}

/// A text field with a visible box, as VEGAS's.
fn boxed(ui: &mut Ui, enabled: bool, edit: egui::TextEdit<'_>) -> egui::Response {
    let resp = ui.add_enabled(enabled, edit);
    ui.painter().rect_stroke(
        resp.rect,
        2.0,
        Stroke::new(1.0, theme::separator()),
        egui::StrokeKind::Inside,
    );
    resp
}

/// A check box that cannot change, with why.
fn fixed_check(ui: &mut Ui, label: &str, on: bool, why: &str) {
    ui.add_enabled_ui(false, |ui| crate::widgets::checkbox(ui, &mut { on }, label))
        .inner
        .on_disabled_hover_text(why);
}

/// A slider of seconds with its value written after it, as VEGAS's.
fn seconds_slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> bool {
    let changed = ui
        .add_sized(
            vec2(150.0, 18.0),
            egui::Slider::new(value, range).show_value(false),
        )
        .changed();
    ui.label(crate::fmt::num(*value as f64, 2));
    changed
}

/// The pages' height: the same on every tab, as VEGAS's.
const PAGE_HEIGHT: f32 = 560.0;
/// Room between a label and its control.
const GAP: f32 = 8.0;
/// Widths of the Video page's lists, as VEGAS's.
const WIDE_LIST: f32 = 200.0;
const LIST: f32 = 140.0;
const SMALL_LIST: f32 = 58.0;
const SPIN: f32 = 56.0;
const SWATCH: Vec2 = vec2(58.0, 20.0);

/// A greyed-out list showing `value`.
fn soon_list(ui: &mut Ui, id: &str, value: &str, width: f32) {
    ui.add_enabled_ui(false, |ui| {
        egui::ComboBox::from_id_salt(("pref-soon", id))
            .width(width)
            .selected_text(value)
            .show_ui(ui, |_| {})
            .response
    })
    .inner
    .on_disabled_hover_text(tr("Coming soon to BananaV"));
}

/// A greyed-out check box.
fn soon_check(ui: &mut Ui, label: &str, on: bool) {
    ui.add_enabled_ui(false, |ui| crate::widgets::checkbox(ui, &mut { on }, label))
        .inner
        .on_disabled_hover_text(tr("Coming soon to BananaV"));
}

/// A color swatch named `name` (for screen readers); a click opens the
/// color picker.
pub(crate) fn swatch(ui: &mut Ui, rgba: [u8; 4], name: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(SWATCH, egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    let lit = resp.hovered() || resp.has_focus();
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, if lit { theme::hover() } else { theme::panel() });
    let inner = rect.shrink(3.0);
    if rgba[3] < 255 {
        color_picker::checkerboard(painter, inner);
    }
    let [r, g, b, a] = rgba;
    let color = Color32::from_rgba_unmultiplied(r, g, b, a);
    let color = if ui.is_enabled() {
        color
    } else {
        color.gamma_multiply(0.35)
    };
    painter.rect_filled(inner, 1.0, color);
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(
            1.0,
            if lit {
                theme::banana()
            } else {
                theme::separator()
            },
        ),
        egui::StrokeKind::Inside,
    );
    resp
}

/// A label without its colon: the name of what it sets.
fn label_name(label: &str) -> &str {
    label.trim_end().trim_end_matches(':').trim_end()
}

/// A spin box of a whole number.
fn spin(ui: &mut Ui, value: &mut u32, range: std::ops::RangeInclusive<u32>) -> bool {
    let mut v = *value as f64;
    let r = crate::widgets::spin_box(
        ui,
        &mut v,
        *range.start() as f64..=*range.end() as f64,
        1.0,
        0,
        SPIN,
    );
    let new = v.round() as u32;
    let changed = new != *value;
    *value = new;
    r.changed() || changed
}

/// A line of a page: `label` on the left, wrapped when it does not fit
/// beside a control `width` wide, and the control against the right edge,
/// as VEGAS lays its pages out.
fn row<R>(ui: &mut Ui, label: &str, width: f32, control: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let room = (ui.available_width() - width - GAP).max(60.0);
        ui.allocate_ui_with_layout(
            vec2(room, ui.spacing().interact_size.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_max_width(room);
                ui.add(egui::Label::new(label).wrap());
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control)
            .inner
    })
    .inner
}

/// A group's title with a line after it, and its contents indented, as
/// VEGAS's group boxes.
fn group(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(title);
        let (rect, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), ui.spacing().interact_size.y),
            egui::Sense::hover(),
        );
        ui.painter().hline(
            rect.x_range(),
            rect.center().y,
            Stroke::new(1.0, theme::separator()),
        );
    });
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 12,
            ..Default::default()
        })
        .show(ui, contents);
}

/// VEGAS's "Default All" at the bottom right of a page, under a line.
fn default_all(ui: &mut Ui) -> bool {
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
        let clicked = ui.button(tr("Default All")).clicked();
        ui.separator();
        clicked
    })
    .inner
}

impl PreferencesDialog {
    pub fn new(app: &BananaApp) -> PreferencesDialog {
        // The graphics cards are looked up off the interface's thread.
        std::thread::spawn(bv_engine::gpu_adapters);
        PreferencesDialog {
            tab: PrefTab::General,
            draft: Draft::of(app),
            free_space: bv_platform::free_space(&app.settings.prefs.temp_dir()),
            memory: bv_platform::total_memory(),
            picker: None,
            displays: bv_platform::displays(),
            audio_devices: None,
            midi_ports: None,
            vst: VstPage::default(),
            device_dialog: None,
            surface_pick: SurfaceKind::MackieControl,
            generic_ui: Default::default(),
            mackie_ui: Default::default(),
            advanced_sync: false,
            display: DisplayPage::default(),
        }
    }

    /// Draws the window; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut close = false;
        // The interface brightness shows as its slider moves.
        app.look_preview = Some(self.draft.prefs.interface_brightness);
        crate::floating::Window::new(tr("Preferences"))
            .id(egui::Id::new("preferences"))
            .collapsible(false)
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                let width = self.tab_strip(ui);
                egui::Frame::new()
                    .fill(theme::panel_dark())
                    .stroke(Stroke::new(1.0, theme::separator()))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_width(width - 26.0);
                        ui.set_height(PAGE_HEIGHT);
                        match self.tab {
                            PrefTab::General => self.general(ui),
                            PrefTab::Video => self.video(ui, app),
                            PrefTab::PreviewDevice => self.preview_device(ui, app),
                            PrefTab::Audio => self.audio(ui),
                            PrefTab::AudioDevice => self.audio_device(ui, app),
                            PrefTab::Midi => self.midi(ui),
                            PrefTab::VstEffects => self.vst_effects(ui),
                            PrefTab::Editing => self.editing(ui, app),
                            PrefTab::Display => self.display(ui),
                            PrefTab::CdSettings => self.cd_settings(ui),
                            PrefTab::Sync => self.sync(ui),
                            PrefTab::ExternalControl => self.external_control(ui, app),
                            PrefTab::Deprecated => self.deprecated(ui),
                            PrefTab::FileIo => self.file_io(ui),
                            PrefTab::Language => self.language(ui),
                        }
                    });
                ui.add_space(6.0);
                let buttons = vec2(width, ui.spacing().interact_size.y);
                let layout = egui::Layout::right_to_left(egui::Align::Center);
                ui.allocate_ui_with_layout(buttons, layout, |ui| {
                    let changed = self.draft != Draft::of(app);
                    if ui
                        .add_enabled(changed, egui::Button::new(tr("Apply")))
                        .clicked()
                    {
                        self.draft.apply(app);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                    if ui.button(RichText::new("OK").strong()).clicked() {
                        self.draft.apply(app);
                        close = true;
                    }
                });
            });
        self.device_window(ctx, app);
        self.advanced_sync_window(ctx);
        // The color picker a swatch opened: OK sets the color in the draft.
        match self.picker.as_mut().map(|(_, p)| p.show(ctx)) {
            Some(color_picker::Outcome::Chosen(rgba)) => {
                if let Some((which, _)) = self.picker.take() {
                    *self.draft.color_mut(which) = rgba;
                }
            }
            Some(color_picker::Outcome::Cancelled) => self.picker = None,
            _ => {}
        }
        if !open || close {
            app.look_preview = None;
        }
        open && !close
    }

    /// VEGAS's three rows of tabs, each filling the width; the row of the
    /// open tab goes next to the page. The window's width.
    fn tab_strip(&mut self, ui: &mut Ui) -> f32 {
        const GAP: f32 = 2.0;
        const PAD: f32 = 18.0;
        let font = egui::TextStyle::Button.resolve(ui.style());
        let painter = ui.painter().clone();
        let natural = |t: PrefTab| {
            painter
                .layout_no_wrap(t.label().to_string(), font.clone(), theme::text())
                .size()
                .x
                + PAD
        };
        let row_width = |row: &[PrefTab]| {
            row.iter().map(|&t| natural(t)).sum::<f32>() + GAP * (row.len() - 1) as f32
        };
        let width = PrefTab::ROWS
            .iter()
            .map(|r| row_width(r))
            .fold(430.0, f32::max)
            .ceil();
        let open = PrefTab::ROWS
            .iter()
            .position(|r| r.contains(&self.tab))
            .unwrap_or(2);
        for k in 1..=PrefTab::ROWS.len() {
            let row = PrefTab::ROWS[(open + k) % PrefTab::ROWS.len()];
            let extra = (width - row_width(row)) / row.len() as f32;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for &tab in row {
                    let w = natural(tab) + extra;
                    if crate::widgets::tab_button(ui, tab.label(), self.tab == tab, true, w)
                        .clicked()
                    {
                        self.tab = tab;
                    }
                }
            });
        }
        ui.add_space(2.0);
        width
    }

    /// VEGAS's General page: the list of switches, the recent projects and
    /// the temporary files folder.
    fn general(&mut self, ui: &mut Ui) {
        ui.label(tr("General preferences:"));
        egui::Frame::new()
            .fill(theme::panel())
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(4.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("pref-general-list")
                    .max_height(390.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Long lines run out of sight, as in VEGAS's list.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let d = &mut self.draft;
                        for (text, item) in general_items() {
                            match item {
                                Item::On(value) => {
                                    crate::widgets::checkbox(ui, value(&mut d.prefs), text);
                                }
                                Item::Welcome => {
                                    crate::widgets::checkbox(ui, &mut d.show_welcome, text);
                                }
                                Item::Later(value) => soon_check(ui, text, *value(&mut d.prefs)),
                            }
                        }
                    });
            });
        ui.add_space(6.0);
        let p = &mut self.draft.prefs;
        ui.horizontal(|ui| {
            crate::widgets::checkbox(ui, &mut p.recent_list, tr("Recently used project list:"));
            ui.add_enabled_ui(p.recent_list, |ui| spin(ui, &mut p.recent_count, 1..=12));
        });
        ui.add_space(4.0);
        // Where copies of media from removable drives go (and other
        // temporary files).
        ui.label(tr("Temporary files folder:"));
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Browse…")).clicked()
                    && let Some(dir) = rfd::FileDialog::new()
                        .set_title(tr("Temporary files folder:"))
                        .set_directory(self.draft.prefs.temp_dir())
                        .pick_folder()
                {
                    self.free_space = bv_platform::free_space(&dir);
                    self.draft.prefs.temp_folder = Some(dir);
                }
                let mut path = self.draft.prefs.temp_dir().display().to_string();
                boxed(
                    ui,
                    false,
                    egui::TextEdit::singleline(&mut path).desired_width(ui.available_width()),
                );
            });
        });
        ui.horizontal(|ui| {
            ui.label(tr("Free storage space in selected folder:"));
            ui.add_space(24.0);
            if let Some(free) = self.free_space {
                let gb = free as f64 / (1u64 << 30) as f64;
                ui.label(trf!("{} Gigabytes", crate::fmt::num(gb, 1)));
            }
        });
        if default_all(ui) {
            let mut defaults = Prefs::default();
            let d = &mut self.draft;
            d.show_welcome = false;
            for (_, item) in general_items() {
                if let Item::On(value) | Item::Later(value) = item {
                    *value(&mut d.prefs) = *value(&mut defaults);
                }
            }
            d.prefs.recent_list = defaults.recent_list;
            d.prefs.recent_count = defaults.recent_count;
            d.prefs.temp_folder = defaults.temp_folder;
        }
    }

    /// VEGAS's Video page.
    fn video(&mut self, ui: &mut Ui, app: &BananaApp) {
        let cards = bv_engine::gpu_adapters();
        ui.spacing_mut().item_spacing.y = 6.0;
        // Memory for frames drawn ahead (Tools › Build Dynamic RAM
        // Preview, Shift+B) and kept to play again at once: up to half the
        // computer's.
        let total = self
            .memory
            .map_or(8192, |b| (b as f64 / (1u64 << 20) as f64).round() as u32);
        let mb = &mut self.draft.prefs.ram_preview_mb;
        row(ui, tr("Dynamic RAM Preview max:"), 280.0, |ui| {
            let percent = (*mb as f64 * 100.0 / total.max(1) as f64).round();
            ui.label(trf!("{}% of {} MB", percent, total));
            ui.add_sized(
                vec2(96.0, 18.0),
                egui::Slider::new(mb, 0..=total / 2).show_value(false),
            );
            let mut v = *mb as f64;
            if ui
                .add_sized(
                    vec2(64.0, ui.spacing().interact_size.y),
                    egui::DragValue::new(&mut v)
                        .range(0.0..=(total / 2) as f64)
                        .speed(16.0)
                        .suffix(" MB"),
                )
                .changed()
            {
                *mb = v.round() as u32;
            }
        });

        let d = &mut self.draft;
        // The number box lines up with the lists below, as in VEGAS.
        row(
            ui,
            tr("Maximum number of rendering threads:"),
            WIDE_LIST,
            |ui| {
                ui.add_space(WIDE_LIST - SPIN - 12.0);
                // From Options › Internal's least to its most, as VEGAS's.
                let lo = d.prefs.min_render_threads.max(1);
                let hi = d.prefs.render_threads_limit.max(lo);
                spin(ui, &mut d.prefs.render_threads, lo..=hi);
            },
        );

        let name_of = |c: &GpuChoice| match c {
            GpuChoice::Off => tr("Off").to_string(),
            GpuChoice::Auto => match cards.first() {
                Some(a) => trf!("Optimal - {}", a.name),
                None => tr("Optimal").to_string(),
            },
            GpuChoice::Named(n) => n.clone(),
        };
        let status = match app.engine.gpu_status() {
            GpuStatus::Off => tr("The processor composites video.").to_string(),
            GpuStatus::Starting => tr("Starting the graphics card…").to_string(),
            GpuStatus::Ready(name) => trf!("In use: {}", name),
            GpuStatus::Failed(e) => trf!(
                "The graphics card failed ({}); the processor composites video.",
                e
            ),
        };
        row(
            ui,
            tr("GPU acceleration of video processing:"),
            WIDE_LIST,
            |ui| {
                egui::ComboBox::from_id_salt("pref-gpu")
                    .width(WIDE_LIST)
                    .truncate()
                    .selected_text(name_of(&d.gpu))
                    .show_ui(ui, |ui| {
                        let mut options = vec![GpuChoice::Off, GpuChoice::Auto];
                        options.extend(cards.iter().map(|a| GpuChoice::Named(a.name.clone())));
                        for o in options {
                            let text = name_of(&o);
                            ui.selectable_value(&mut d.gpu, o, text);
                        }
                    })
                    .response
                    .on_hover_text(status);
            },
        );

        let numbers = |f: FrameNumbers| match f {
            FrameNumbers::None => tr("None"),
            FrameNumbers::Frames => tr("Frame Numbers"),
            FrameNumbers::Time => tr("Time"),
            FrameNumbers::Timecode => tr("Timecode"),
        };
        row(
            ui,
            tr("Show source frame numbers on event thumbnails as:"),
            LIST,
            |ui| {
                egui::ComboBox::from_id_salt("pref-frame-numbers")
                    .width(LIST)
                    .selected_text(numbers(d.prefs.frame_numbers))
                    .show_ui(ui, |ui| {
                        for f in FrameNumbers::ALL {
                            ui.selectable_value(&mut d.prefs.frame_numbers, f, numbers(f));
                        }
                    });
            },
        );

        let thumbs = |t: Thumbnails| match t {
            Thumbnails::None => tr("None"),
            Thumbnails::Head => tr("Head"),
            Thumbnails::HeadTail => tr("Head, Tail"),
            Thumbnails::HeadCenterTail => tr("Head, Center, Tail"),
            Thumbnails::All => tr("All"),
        };
        row(ui, tr("Thumbnails to show in video events:"), LIST, |ui| {
            egui::ComboBox::from_id_salt("pref-thumbnails")
                .width(LIST)
                .selected_text(thumbs(d.prefs.thumbnails))
                .show_ui(ui, |ui| {
                    for t in Thumbnails::ALL {
                        ui.selectable_value(&mut d.prefs.thumbnails, t, thumbs(t));
                    }
                });
        });

        // Proxies of large media, which previews read (Project Media ›
        // Create Video Proxy makes others).
        let proxies = |a: AutoProxies| match a {
            AutoProxies::On => tr("On"),
            AutoProxies::Off => tr("Off"),
            AutoProxies::Auto => tr("Auto"),
        };
        row(
            ui,
            tr("Automatically create video proxies for greater than 4K resolution media:"),
            SMALL_LIST,
            |ui| {
                egui::ComboBox::from_id_salt("pref-proxies")
                    .width(SMALL_LIST)
                    .selected_text(proxies(d.prefs.auto_proxies))
                    .show_ui(ui, |ui| {
                        for a in AutoProxies::ALL {
                            ui.selectable_value(&mut d.prefs.auto_proxies, a, proxies(a));
                        }
                    });
            },
        );

        ui.add_space(16.0);
        group(ui, tr("Video Preview display"), |ui| {
            ui.columns(2, |c| {
                row(&mut c[0], tr("Action safe area (%):"), SPIN, |ui| {
                    spin(ui, &mut d.prefs.action_safe, 0..=50)
                });
                row(&mut c[0], tr("Title safe area (%):"), SPIN, |ui| {
                    spin(ui, &mut d.prefs.title_safe, 0..=50)
                });
                crate::widgets::checkbox(
                    &mut c[0],
                    &mut d.prefs.display_at_project_size,
                    tr("Display at project size"),
                );
                row(
                    &mut c[1],
                    tr("Horizontal grid divisions (#):"),
                    SPIN,
                    |ui| spin(ui, &mut d.prefs.grid_horizontal, 1..=64),
                );
                row(&mut c[1], tr("Vertical grid divisions (#):"), SPIN, |ui| {
                    spin(ui, &mut d.prefs.grid_vertical, 1..=64)
                });
                crate::widgets::checkbox(
                    &mut c[1],
                    &mut d.prefs.simulate_device_aspect,
                    tr("Simulate device aspect ratio"),
                );
            });
            let background = |b: PreviewBackground| match b {
                PreviewBackground::Default => tr("Default"),
                PreviewBackground::White => tr("White"),
                PreviewBackground::Black => tr("Black"),
            };
            ui.horizontal(|ui| {
                ui.label(tr("Background color:"));
                egui::ComboBox::from_id_salt("pref-preview-background")
                    .width(ui.available_width())
                    .selected_text(background(d.prefs.preview_background))
                    .show_ui(ui, |ui| {
                        for b in PreviewBackground::ALL {
                            ui.selectable_value(&mut d.prefs.preview_background, b, background(b));
                        }
                    });
            });
        });

        // Track fades to a color are not in BananaV yet: their colors are
        // kept for them.
        let (mut take_names, mut take_numbers) = (d.prefs.take_names, d.prefs.take_numbers);
        let mut pick = None;
        let mut color_row = |ui: &mut Ui, label: &'static str, which: ColorSetting| {
            let rgba = *d.color_mut(which);
            row(ui, label, SWATCH.x, |ui| {
                if swatch(ui, rgba, label_name(label)).clicked() {
                    pick = Some((which, label));
                }
            });
        };
        group(ui, tr("Multicamera editing"), |ui| {
            ui.columns(2, |c| {
                crate::widgets::checkbox(&mut c[0], &mut take_names, tr("Display take names"));
                crate::widgets::checkbox(&mut c[0], &mut take_numbers, tr("Display take numbers"));
                color_row(
                    &mut c[1],
                    tr("Active take indicator:"),
                    ColorSetting::TakeIndicator,
                );
            });
        });
        group(ui, tr("Default track fade colors"), |ui| {
            ui.columns(2, |c| {
                color_row(&mut c[0], tr("Track fade top:"), ColorSetting::FadeTop);
                color_row(
                    &mut c[1],
                    tr("Track fade bottom:"),
                    ColorSetting::FadeBottom,
                );
            });
        });
        self.draft.prefs.take_names = take_names;
        self.draft.prefs.take_numbers = take_numbers;
        if let Some((which, label)) = pick {
            let rgba = *self.draft.color_mut(which);
            self.picker = Some((which, ColorPicker::new(label_name(label), rgba)));
        }

        if default_all(ui) {
            let defaults = Prefs::default();
            let d = &mut self.draft;
            d.gpu = GpuChoice::Auto;
            d.prefs.render_threads = defaults.render_threads;
            d.prefs.frame_numbers = defaults.frame_numbers;
            d.prefs.thumbnails = defaults.thumbnails;
            d.prefs.action_safe = defaults.action_safe;
            d.prefs.title_safe = defaults.title_safe;
            d.prefs.grid_horizontal = defaults.grid_horizontal;
            d.prefs.grid_vertical = defaults.grid_vertical;
            d.prefs.display_at_project_size = defaults.display_at_project_size;
            d.prefs.preview_background = defaults.preview_background;
            d.prefs.take_indicator = defaults.take_indicator;
            d.prefs.fade_top = defaults.fade_top;
            d.prefs.fade_bottom = defaults.fade_bottom;
        }
    }

    /// VEGAS's File I/O tab: 360° media detected, decoding on the graphics
    /// card, and the settings BananaV does not have yet.
    fn file_io(&mut self, ui: &mut Ui) {
        crate::widgets::checkbox(
            ui,
            &mut self.draft.prefs.detect_360,
            tr("Enable auto-detection of 360° media"),
        );
        crate::widgets::checkbox(
            ui,
            &mut self.draft.prefs.no_recompress,
            tr("Enable no-recompress rendering"),
        );
        for label in [
            tr("Render large Wave files as Wave64"),
            tr("Allow Wave renders up to 4 GB"),
            tr("Ignore fact chunk when opening compressed WAV files"),
            tr("Import MXF as multichannel"),
            tr("Enable QSV Encoding and Decoding (where available)"),
            tr("Enable OpenCL/GL Interop for Intel GPU"),
            tr("Enable direct GPU import path"),
            tr("Favor audio sync when playing Variable Frame rate (VFR) media"),
        ] {
            soon_check(ui, label, false);
        }
        ui.add_space(6.0);
        let mut on = self.draft.decoding != DecodeChoice::Off;
        if crate::widgets::checkbox(
            ui,
            &mut on,
            tr("Enable Hardware Decoding for supported formats"),
        )
        .changed()
        {
            self.draft.decoding = if on {
                DecodeChoice::Auto
            } else {
                DecodeChoice::Off
            };
        }
        let cards = bv_engine::Engine::decode_adapters();
        ui.add_enabled_ui(on, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                ui.label(tr("Hardware decoder to use:"));
                let name_of = |c: &DecodeChoice| match c {
                    DecodeChoice::Card(n) => n.clone(),
                    _ => match cards.first() {
                        Some(n) => trf!("Auto ({})", n),
                        None => tr("Auto").to_string(),
                    },
                };
                egui::ComboBox::from_id_salt("pref-decoder")
                    .width(ui.available_width() - 8.0)
                    .selected_text(name_of(&self.draft.decoding))
                    .show_ui(ui, |ui| {
                        let mut options = vec![DecodeChoice::Auto];
                        options.extend(cards.iter().map(|n| DecodeChoice::Card(n.clone())));
                        for o in options {
                            let text = name_of(&o);
                            ui.selectable_value(&mut self.draft.decoding, o, text);
                        }
                    });
            });
        });
        crate::tabs::hint(
            ui,
            tr(
                "Windows' decoders use the card's video engine (NVIDIA, AMD or Intel) for H.264, HEVC, VP9 and AV1, and the processor for the rest.",
            ),
        );
        ui.add_space(6.0);
        soon_check(ui, tr("Enable MKV Reader (experimental)"), false);
        ui.add_space(6.0);
        egui::Grid::new("pref-file-io")
            .num_columns(2)
            .spacing(vec2(10.0, 7.0))
            .show(ui, |ui| {
                ui.label(tr("RAW Processor:"));
                soon_list(ui, "raw", tr("Off"), 180.0);
                ui.end_row();
                let p = &mut self.draft.prefs;
                for (label, id, level) in [
                    (tr("Log Plugin loading:"), "log-plugins", &mut p.log_plugins),
                    (tr("Log Reads:"), "log-reads", &mut p.log_reads),
                    (tr("Log Renders:"), "log-renders", &mut p.log_renders),
                ] {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(("pref", id))
                        .width(180.0)
                        .selected_text(level.name())
                        .show_ui(ui, |ui| {
                            for l in crate::logs::LogLevel::ALL {
                                ui.selectable_value(level, l, l.name());
                            }
                        });
                    ui.end_row();
                }
            });
        if let Some(dir) = crate::logs::folder() {
            crate::tabs::hint(ui, &trf!("The logs are text files in {}.", dir.display()));
        }
    }

    fn language(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(tr("Interface language")).strong());
        ui.add_space(4.0);
        for &lang in Language::ALL {
            // Each language is listed by its own name, like system settings
            // do; the partial ones say so.
            ui.horizontal(|ui| {
                crate::widgets::radio_value(ui, &mut self.draft.language, lang, lang.native_name());
                if lang.partial() {
                    ui.label(RichText::new(tr("(partial translation)")).color(theme::text_dim()));
                }
            });
        }
        ui.add_space(10.0);
        crate::tabs::hint(
            ui,
            tr(
                "English is BananaV's main language; other languages are translations of it. Texts not translated yet show in English.",
            ),
        );
        crate::tabs::hint(
            ui,
            tr("Numbers use the language's decimal separator (1.5 or 1,5)."),
        );
    }
}

impl PreferencesDialog {
    /// VEGAS's Preview Device page: the external monitor (the graphics
    /// card's display; the external hardware devices are to come).
    fn preview_device(&mut self, ui: &mut Ui, app: &mut BananaApp) {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.label(tr("Device:"));
            egui::ComboBox::from_id_salt("pref-preview-device")
                .width(ui.available_width())
                .selected_text(tr("Windows Graphics Card"))
                .show_ui(ui, |ui| {
                    for (i, name) in PREVIEW_DEVICES.iter().enumerate() {
                        let graphics = i == PREVIEW_DEVICES.len() - 1;
                        let text = if graphics {
                            tr("Windows Graphics Card")
                        } else {
                            *name
                        };
                        ui.add_enabled(graphics, egui::Button::selectable(graphics, text))
                            .on_disabled_hover_text(tr("Coming soon to BananaV"));
                    }
                });
        });
        let d = &mut self.draft;
        let chosen = preview_display(&self.displays, d.prefs.preview_display.as_deref());
        ui.label(tr("Details:"));
        egui::Frame::new()
            .fill(theme::panel())
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(54.0);
                ui.label(match chosen {
                    Some(_) => tr("Device connected"),
                    None => tr("No display found"),
                });
            });
        ui.add_space(4.0);
        egui::Frame::new()
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(tr("Display adapter:"));
                ui.horizontal(|ui| {
                    let selected = chosen
                        .and_then(|i| self.displays.get(i).map(|x| display_label(i, x)))
                        .unwrap_or_default();
                    let identify = ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("pref-preview-display")
                            .width(ui.available_width() - 110.0)
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                // In the system's order of numbers.
                                let mut order: Vec<usize> = (0..self.displays.len()).collect();
                                order.sort_by_key(|&i| display_number(i, &self.displays[i]));
                                for i in order {
                                    let x = &self.displays[i];
                                    if ui
                                        .selectable_label(chosen == Some(i), display_label(i, x))
                                        .clicked()
                                    {
                                        d.prefs.preview_display = Some(x.name.clone());
                                    }
                                }
                            });
                        ui.button(tr("Identify Displays")).clicked()
                    });
                    if identify.inner {
                        app.identify_displays();
                    }
                });
                crate::widgets::checkbox(
                    ui,
                    &mut d.prefs.preview_scale_to_fit,
                    tr("Scale output to fit display"),
                );
                crate::widgets::checkbox(
                    ui,
                    &mut d.prefs.preview_deinterlace,
                    tr("Apply deinterlace filter"),
                );
                crate::widgets::checkbox(
                    ui,
                    &mut d.prefs.preview_studio_rgb,
                    tr("Adjust levels from studio RGB to computer RGB"),
                );
                // BananaV's windows always show their frames in step with
                // the display's refresh.
                fixed_check(
                    ui,
                    tr("Wait for vertical sync"),
                    true,
                    tr("BananaV's windows always wait for the display's refresh: no tearing."),
                );
                crate::widgets::checkbox(
                    ui,
                    &mut d.prefs.preview_optimize_gpu,
                    tr("Optimize GPU display performance"),
                );
                ui.label(tr("Sync Offset (frames)"));
                ui.horizontal(|ui| {
                    ui.add_space(52.0);
                    let mut v = d.prefs.preview_sync_offset as f64;
                    crate::widgets::spin_box(ui, &mut v, -12.0..=12.0, 1.0, 0, 120.0);
                    d.prefs.preview_sync_offset = v.round() as i32;
                });
            });
        ui.add_space(6.0);
        crate::widgets::checkbox(
            ui,
            &mut d.prefs.preview_recompress,
            tr("Recompress edited frames"),
        );
        crate::widgets::checkbox(
            ui,
            &mut d.prefs.preview_in_window,
            tr("Display frames in video preview window during playback"),
        );
        crate::widgets::checkbox(
            ui,
            &mut d.prefs.preview_rotation,
            tr("Use project output rotation setting"),
        );
        let (device, all) = ui
            .with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                let (device, all) = ui
                    .horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let all = ui.button(tr("Default All")).clicked();
                            let device = ui.button(tr("Device Defaults")).clicked();
                            (device, all)
                        })
                        .inner
                    })
                    .inner;
                ui.separator();
                (device, all)
            })
            .inner;
        let defaults = Prefs::default();
        let p = &mut self.draft.prefs;
        if device || all {
            p.preview_display = defaults.preview_display.clone();
            p.preview_scale_to_fit = defaults.preview_scale_to_fit;
            p.preview_deinterlace = defaults.preview_deinterlace;
            p.preview_studio_rgb = defaults.preview_studio_rgb;
            p.preview_optimize_gpu = defaults.preview_optimize_gpu;
            p.preview_sync_offset = defaults.preview_sync_offset;
        }
        if all {
            p.preview_recompress = defaults.preview_recompress;
            p.preview_in_window = defaults.preview_in_window;
            p.preview_rotation = defaults.preview_rotation;
        }
    }

    /// VEGAS's Audio page.
    fn audio(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let p = &mut self.draft.prefs;
        let waveforms = |w: RecordWaveforms| match w {
            RecordWaveforms::None => tr("Don't Show Waveforms"),
            RecordWaveforms::Single => tr("Show Waveform For a Single Track"),
            RecordWaveforms::All => tr("Show All Waveforms"),
        };
        row(
            ui,
            tr("Waveform display while recording:"),
            WIDE_LIST,
            |ui| {
                egui::ComboBox::from_id_salt("pref-record-waveforms")
                    .width(WIDE_LIST)
                    .selected_text(waveforms(p.record_waveforms))
                    .show_ui(ui, |ui| {
                        for w in RecordWaveforms::ALL {
                            ui.selectable_value(&mut p.record_waveforms, w, waveforms(w));
                        }
                    });
            },
        );
        row(ui, tr("Normalize peak level (dB):"), 80.0, |ui| {
            let mut v = p.normalize_peak_db as f64;
            crate::widgets::spin_box(ui, &mut v, -60.0..=0.0, 0.1, 1, 80.0);
            p.normalize_peak_db = v as f32;
        });
        // Tempo-stretched loops, older sends' gain and 32-bit plug-ins are
        // to come.
        soon_check(ui, tr("Import audio at project tempo"), false);
        crate::widgets::checkbox(ui, &mut p.record_bwf, tr("Record to Broadcast Wave Format"));
        crate::widgets::checkbox(
            ui,
            &mut p.prefader_sends_listen_to_mute,
            tr("Track prefader sends listen to mute"),
        );
        soon_check(ui, tr("Use legacy track send gain"), false);
        crate::widgets::checkbox(
            ui,
            &mut p.ask_no_armed_track,
            tr("Prompt user when no track has been armed for record"),
        );
        soon_check(
            ui,
            tr("Enable support for 32-bit VST plug-ins (experimental)"),
            false,
        );
        ui.label(tr("Preferred audio editor:"));
        let pick = |current: &Option<std::path::PathBuf>, title: &str, programs: bool| {
            let mut d = rfd::FileDialog::new().set_title(title);
            if programs {
                d = d.add_filter(tr("Programs"), &["exe"]);
            } else {
                d = d.add_filter(tr("Sound files"), bv_engine::AUDIO_EXTENSIONS);
            }
            if let Some(dir) = current.as_ref().and_then(|c| c.parent()) {
                d = d.set_directory(dir);
            }
            d.pick_file()
        };
        let path_row = |ui: &mut Ui,
                        value: &mut Option<std::path::PathBuf>,
                        title: &str,
                        programs: bool,
                        enabled: bool| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(enabled, egui::Button::new(tr("Browse…")))
                        .clicked()
                        && let Some(f) = pick(value, title, programs)
                    {
                        *value = Some(f);
                    }
                    let mut text = value
                        .as_ref()
                        .map_or(String::new(), |v| v.display().to_string());
                    let edit = boxed(
                        ui,
                        enabled,
                        egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()),
                    );
                    if edit.changed() {
                        *value = (!text.trim().is_empty())
                            .then(|| std::path::PathBuf::from(text.trim()));
                    }
                });
            });
        };
        path_row(
            ui,
            &mut p.audio_editor,
            tr("Preferred audio editor:"),
            true,
            true,
        );
        ui.add_space(4.0);
        group(ui, tr("Metronome"), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            crate::widgets::radio_value(
                ui,
                &mut p.metronome_custom,
                false,
                tr("Use default metronome sounds"),
            );
            crate::widgets::radio_value(
                ui,
                &mut p.metronome_custom,
                true,
                tr("Use custom metronome sounds"),
            );
            let custom = p.metronome_custom;
            for (label, value) in [
                (tr("Normal:"), &mut p.metronome_normal),
                (tr("Accent:"), &mut p.metronome_accent),
            ] {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.add_enabled(custom, egui::Label::new(label));
                    ui.add_space(8.0);
                    path_row(ui, value, label_name(label), false, custom);
                });
            }
        });
        group(ui, tr("Loudness configuration"), |ui| {
            crate::widgets::checkbox(
                ui,
                &mut p.open_loudness_log,
                tr("Open editor when new loudness log is generated"),
            );
        });
        if default_all(ui) {
            let defaults = Prefs::default();
            let p = &mut self.draft.prefs;
            p.record_waveforms = defaults.record_waveforms;
            p.normalize_peak_db = defaults.normalize_peak_db;
            p.record_bwf = defaults.record_bwf;
            p.ask_no_armed_track = defaults.ask_no_armed_track;
            p.audio_editor = defaults.audio_editor;
            p.metronome_custom = defaults.metronome_custom;
            p.metronome_normal = defaults.metronome_normal;
            p.metronome_accent = defaults.metronome_accent;
            p.open_loudness_log = defaults.open_loudness_log;
        }
    }

    /// VEGAS's Audio Device page.
    fn audio_device(&mut self, ui: &mut Ui, app: &BananaApp) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let devices = self.audio_devices.get_or_insert_with(AudioDevices::find);
        let p = &mut self.draft.prefs;
        let system = p.audio_device_type == AudioDeviceType::SystemDefault;
        let type_name = |t: AudioDeviceType| match t {
            AudioDeviceType::SystemDefault => tr("Windows Default Device"),
            AudioDeviceType::Windows => tr("Windows Audio Devices"),
        };
        row(ui, tr("Audio device type:"), WIDE_LIST, |ui| {
            egui::ComboBox::from_id_salt("pref-audio-type")
                .width(WIDE_LIST)
                .selected_text(type_name(p.audio_device_type))
                .show_ui(ui, |ui| {
                    for t in [AudioDeviceType::SystemDefault, AudioDeviceType::Windows] {
                        ui.selectable_value(&mut p.audio_device_type, t, type_name(t));
                    }
                    // ASIO drivers are to come.
                    for name in &devices.asio {
                        ui.add_enabled(false, egui::Button::selectable(false, name.as_str()))
                            .on_disabled_hover_text(tr("Coming soon to BananaV"));
                    }
                });
        });
        let default_out = devices
            .outputs
            .iter()
            .find(|(_, d)| *d)
            .map_or(String::new(), |(n, _)| n.clone());
        let front = if system {
            tr("Windows Default Device").to_string()
        } else {
            p.playback_device
                .clone()
                .unwrap_or_else(|| tr("Windows Default Device").to_string())
        };
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                row(
                    ui,
                    tr("Default stereo and front playback device:"),
                    WIDE_LIST,
                    |ui| {
                        ui.add_enabled_ui(!system, |ui| {
                            egui::ComboBox::from_id_salt("pref-audio-front")
                                .width(WIDE_LIST)
                                .truncate()
                                .selected_text(front.clone())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut p.playback_device,
                                        None,
                                        tr("Windows Default Device"),
                                    );
                                    for (name, _) in &devices.outputs {
                                        ui.selectable_value(
                                            &mut p.playback_device,
                                            Some(name.clone()),
                                            name.as_str(),
                                        );
                                    }
                                });
                        })
                        .response
                        .on_hover_text(trf!("The system's default now: {}", default_out));
                    },
                );
                // Rear and center channels to other devices are to come.
                row(ui, tr("Default rear playback device:"), WIDE_LIST, |ui| {
                    soon_list(ui, "rear", &front, WIDE_LIST);
                });
                row(
                    ui,
                    tr("Default center and LFE playback device:"),
                    WIDE_LIST,
                    |ui| {
                        soon_list(ui, "center", &front, WIDE_LIST);
                    },
                );
            });
        });
        ui.add_space(8.0);
        row(ui, tr("Playback buffering (seconds):"), 200.0, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                seconds_slider(ui, &mut p.playback_buffer, 0.02..=1.0);
            });
        });
        crate::widgets::checkbox(ui, &mut p.track_buffering, tr("Enable track buffering"));
        ui.add_enabled_ui(p.track_buffering, |ui| {
            row(ui, tr("Track buffering (seconds):"), 200.0, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    seconds_slider(ui, &mut p.track_buffer, 0.05..=1.0);
                });
            });
        });
        ui.add_space(8.0);
        // The recording inputs: the system's default (or each device), all
        // of it or one side.
        let input_label = |i: &bv_timeline::RecordInput| {
            let device = if i.device.is_empty() {
                tr("Windows Default Device").to_string()
            } else {
                i.device.clone()
            };
            match (i.channels, i.first) {
                (1, 0) => trf!("{} - Left", device),
                (1, _) => trf!("{} - Right", device),
                _ => device,
            }
        };
        let mut inputs: Vec<bv_timeline::RecordInput> = Vec::new();
        let mut add_device = |name: &str, channels: u16| {
            inputs.push(bv_timeline::RecordInput {
                device: name.to_string(),
                first: 0,
                channels: channels.min(2),
                bus: None,
            });
            if channels >= 2 {
                for first in [0, 1] {
                    inputs.push(bv_timeline::RecordInput {
                        device: name.to_string(),
                        first,
                        channels: 1,
                        bus: None,
                    });
                }
            }
        };
        add_device("", 2);
        if !system {
            for d in &devices.inputs {
                add_device(&d.name, d.channels);
            }
        }
        let mut current = p.record_device.clone();
        if system {
            current.device.clear();
        }
        row(ui, tr("Default audio recording device:"), WIDE_LIST, |ui| {
            egui::ComboBox::from_id_salt("pref-record-device")
                .width(WIDE_LIST)
                .truncate()
                .selected_text(input_label(&current))
                .show_ui(ui, |ui| {
                    for i in &inputs {
                        if ui.selectable_label(*i == current, input_label(i)).clicked() {
                            p.record_device = i.clone();
                        }
                    }
                });
        });
        crate::widgets::checkbox(
            ui,
            &mut p.record_latency_auto,
            tr("Automatically detect and offset for hardware recording latency"),
        );
        let rate = app.project().settings.sample_rate.0 as f32;
        ui.add_enabled_ui(!p.record_latency_auto, |ui| {
            row(ui, tr("User offset (samples) (ms):"), 240.0, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_sized(
                        vec2(130.0, 18.0),
                        egui::Slider::new(&mut p.record_latency_ms, 0.0..=500.0).show_value(false),
                    );
                    let samples = (p.record_latency_ms / 1000.0 * rate).round();
                    ui.label(trf!(
                        "{} ({} ms)",
                        crate::fmt::num(samples as f64, 0),
                        crate::fmt::num(p.record_latency_ms as f64, 0)
                    ));
                });
            });
        });
        ui.add_space(8.0);
        // Audio through the external preview device: with the external
        // hardware devices, to come.
        soon_check(
            ui,
            tr("Use audio-capable preview device during external video preview"),
            false,
        );
        let all = ui
            .with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                let all = ui
                    .horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let all = ui.button(tr("Default All")).clicked();
                            ui.add_enabled(false, egui::Button::new(tr("Advanced…")))
                                .on_disabled_hover_text(tr("Coming soon to BananaV"));
                            all
                        })
                        .inner
                    })
                    .inner;
                ui.separator();
                all
            })
            .inner;
        if all {
            let defaults = Prefs::default();
            let p = &mut self.draft.prefs;
            p.audio_device_type = defaults.audio_device_type;
            p.playback_device = defaults.playback_device;
            p.playback_buffer = defaults.playback_buffer;
            p.track_buffering = defaults.track_buffering;
            p.track_buffer = defaults.track_buffer;
            p.record_device = defaults.record_device;
            p.record_latency_auto = defaults.record_latency_auto;
            p.record_latency_ms = defaults.record_latency_ms;
        }
    }
}

/// The MIDI ports of this computer, found when the MIDI tab is first
/// shown.
#[derive(Debug, Default)]
struct MidiPorts {
    outputs: Vec<String>,
    inputs: Vec<String>,
}

impl MidiPorts {
    fn find() -> MidiPorts {
        MidiPorts {
            outputs: bv_platform::midi_outputs(),
            inputs: bv_platform::midi_inputs(),
        }
    }
}

/// The VST Effects page: the folder chosen in its list (the plug-ins
/// found are the interface's one search's, `vst::plugins`).
#[derive(Debug, Default)]
struct VstPage {
    folder: Option<usize>,
}

/// Height of a line of a list.
const LIST_ROW: f32 = 20.0;
/// Width of the Editing page's labels: its controls line up after them.
const EDIT_LABEL: f32 = 206.0;
/// Width of the Editing page's lists and number boxes.
const EDIT_LIST: f32 = 134.0;
const EDIT_NUMBER: f32 = 62.0;

/// A list with column headings, as VEGAS's list views: a box `height` tall
/// whose first line names the columns (`columns`' widths; the last one
/// takes the rest of the width). `rows` adds the lines, given each
/// column's width.
fn list_view(
    ui: &mut Ui,
    id: &str,
    columns: &[(&str, f32)],
    height: f32,
    rows: impl FnOnce(&mut Ui, &[f32]),
) {
    egui::Frame::new()
        .fill(theme::panel())
        .stroke(Stroke::new(1.0, theme::separator()))
        .show(ui, |ui| {
            let total = ui.available_width();
            ui.set_width(total);
            let mut widths: Vec<f32> = columns.iter().map(|c| c.1).collect();
            let fixed: f32 = widths.iter().rev().skip(1).sum();
            if let Some(last) = widths.last_mut() {
                *last = (total - fixed).max(*last);
            }
            let (head, _) = ui.allocate_exact_size(vec2(total, LIST_ROW), egui::Sense::hover());
            let painter = ui.painter();
            let font = egui::TextStyle::Body.resolve(ui.style());
            let line = Stroke::new(1.0, theme::separator());
            let mut x = head.left();
            for ((name, _), w) in columns.iter().zip(&widths) {
                crate::widgets::fit_text(
                    painter,
                    pos2(x + 6.0, head.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    font.clone(),
                    theme::text(),
                    w - 8.0,
                );
                x += w;
                if x < head.right() - 1.0 {
                    painter.vline(x, head.y_range(), line);
                }
            }
            painter.hline(head.x_range(), head.bottom(), line);
            egui::ScrollArea::vertical()
                .id_salt(("pref-list", id))
                .max_height(height - LIST_ROW)
                .min_scrolled_height(height - LIST_ROW)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    rows(ui, &widths);
                });
        });
}

/// A line of a list: its cells, side by side.
fn list_line(ui: &mut Ui, cells: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        cells(ui);
    });
}

/// A cell of a list's line, `width` wide.
fn cell<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(vec2(width, LIST_ROW), layout, |ui| {
        ui.set_width(width);
        ui.set_min_height(LIST_ROW);
        ui.add_space(4.0);
        add(ui)
    })
    .inner
}

/// A line of the Editing page: its label (or check box) in a column as
/// wide as VEGAS's, then the control and what follows it.
fn field<R>(ui: &mut Ui, label: impl FnOnce(&mut Ui), control: impl FnOnce(&mut Ui) -> R) -> R {
    field_in(ui, EDIT_LABEL, label, control)
}

/// [`field`] with a label column `width` wide.
fn field_in<R>(
    ui: &mut Ui,
    width: f32,
    label: impl FnOnce(&mut Ui),
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        ui.allocate_ui_with_layout(vec2(width, h), layout, |ui| {
            ui.set_width(width);
            label(ui);
        });
        control(ui)
    })
    .inner
}

/// A label column `min` wide, or as the widest of `labels` in the
/// language shown.
fn label_column(ui: &Ui, min: f32, labels: &[&str]) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    labels
        .iter()
        .map(|t| {
            ui.painter()
                .layout_no_wrap(t.to_string(), font.clone(), theme::text())
                .size()
                .x
                + 12.0
        })
        .fold(min, f32::max)
}

/// A number box of seconds (three decimals, as VEGAS's).
fn seconds_box(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) -> bool {
    crate::widgets::spin_box(ui, value, range, 1.0, 3, EDIT_NUMBER).changed()
}

/// Puts `name` in `list` or takes it out.
fn set_listed(list: &mut Vec<String>, name: &str, on: bool) {
    list.retain(|n| n != name);
    if on {
        list.push(name.to_string());
    }
}

/// MIDI Thru from `input` to `output`: an input goes to one output, and an
/// output takes one input.
fn set_thru(routes: &mut Vec<MidiRoute>, input: &str, output: &str) {
    routes.retain(|r| r.input != input && r.output != output);
    routes.push(MidiRoute {
        input: input.to_string(),
        output: output.to_string(),
    });
}

/// A small sheet of paper (the MIDI input list's line when there are no
/// inputs).
fn paint_sheet(painter: &egui::Painter, rect: Rect) {
    let r = Rect::from_center_size(rect.center(), vec2(10.0, 13.0));
    let fold = 3.5;
    let stroke = Stroke::new(1.0, theme::text_dim());
    painter.add(egui::Shape::closed_line(
        vec![
            r.left_top(),
            pos2(r.right() - fold, r.top()),
            pos2(r.right(), r.top() + fold),
            r.right_bottom(),
            r.left_bottom(),
        ],
        stroke,
    ));
    painter.line_segment(
        [
            pos2(r.right() - fold, r.top()),
            pos2(r.right() - fold, r.top() + fold),
        ],
        stroke,
    );
}

impl PreferencesDialog {
    /// VEGAS's MIDI page: the ports made available (to control surfaces
    /// and synchronization) and MIDI Thru, what comes in on an input
    /// played out on an output.
    fn midi(&mut self, ui: &mut Ui) {
        let ports = self.midi_ports.get_or_insert_with(MidiPorts::find);
        let p = &mut self.draft.prefs;
        ui.spacing_mut().item_spacing.y = 4.0;
        let not_active = tr("Not Active");
        let thru_list = |ui: &mut Ui,
                         id: (&str, &str),
                         width: f32,
                         current: Option<String>,
                         choices: &[String]| {
            let mut picked: Option<Option<String>> = None;
            egui::ComboBox::from_id_salt(id)
                .width(width)
                .truncate()
                .selected_text(current.clone().unwrap_or_else(|| not_active.to_string()))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current.is_none(), not_active).clicked() {
                        picked = Some(None);
                    }
                    for c in choices {
                        if ui
                            .selectable_label(current.as_ref() == Some(c), c.as_str())
                            .clicked()
                        {
                            picked = Some(Some(c.clone()));
                        }
                    }
                });
            picked
        };
        // The Device column as wide as its heading in the language.
        let device_width = {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let heading =
                ui.painter()
                    .layout_no_wrap(tr("Device").to_string(), font, theme::text());
            (heading.size().x + 16.0).max(46.0)
        };
        // A port's device (the instrument it plays): none in BananaV.
        let no_device = trc("midi device", "None");
        ui.label(tr("Make these devices available for MIDI output:"));
        let columns = [
            (tr("Port"), 196.0),
            (tr("Device"), device_width),
            (tr("MIDI Thru From"), 110.0),
        ];
        list_view(ui, "midi-out", &columns, 126.0, |ui, w| {
            for name in &ports.outputs {
                list_line(ui, |ui| {
                    let mut on = p.midi_outputs.contains(name);
                    cell(ui, w[0], |ui| {
                        if crate::widgets::checkbox(ui, &mut on, "").changed() {
                            set_listed(&mut p.midi_outputs, name, on);
                            if !on {
                                p.midi_thru.retain(|r| &r.output != name);
                            }
                        }
                        ui.add(egui::Label::new(name.as_str()).truncate());
                    });
                    cell(ui, w[1], |ui| ui.label(no_device));
                    cell(ui, w[2], |ui| {
                        let from = p
                            .midi_thru
                            .iter()
                            .find(|r| &r.output == name)
                            .map(|r| r.input.clone());
                        let inputs = p.midi_inputs.clone();
                        let picked = ui
                            .add_enabled_ui(on, |ui| {
                                thru_list(ui, ("midi-thru-from", name), w[2] - 14.0, from, &inputs)
                            })
                            .inner;
                        match picked {
                            Some(Some(input)) => set_thru(&mut p.midi_thru, &input, name),
                            Some(None) => p.midi_thru.retain(|r| &r.output != name),
                            None => {}
                        }
                    });
                });
            }
        });
        ui.add_space(10.0);
        ui.label(tr("Make these devices available for MIDI input:"));
        let columns = [
            (tr("Port"), 196.0),
            (tr("Device"), device_width),
            (tr("MIDI Thru To"), 110.0),
        ];
        list_view(ui, "midi-in", &columns, 132.0, |ui, w| {
            if ports.inputs.is_empty() {
                list_line(ui, |ui| {
                    cell(ui, w[0], |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(14.0, 16.0), egui::Sense::hover());
                        paint_sheet(ui.painter(), rect);
                        ui.add_space(4.0);
                        ui.label(tr("(None)"));
                    });
                    cell(ui, w[1], |_| {});
                    cell(ui, w[2], |ui| ui.label(not_active));
                });
            }
            for name in &ports.inputs {
                list_line(ui, |ui| {
                    let mut on = p.midi_inputs.contains(name);
                    cell(ui, w[0], |ui| {
                        if crate::widgets::checkbox(ui, &mut on, "").changed() {
                            set_listed(&mut p.midi_inputs, name, on);
                            if !on {
                                p.midi_thru.retain(|r| &r.input != name);
                            }
                        }
                        ui.add(egui::Label::new(name.as_str()).truncate());
                    });
                    cell(ui, w[1], |ui| ui.label(no_device));
                    cell(ui, w[2], |ui| {
                        let to = p
                            .midi_thru
                            .iter()
                            .find(|r| &r.input == name)
                            .map(|r| r.output.clone());
                        let outputs = p.midi_outputs.clone();
                        let picked = ui
                            .add_enabled_ui(on, |ui| {
                                thru_list(ui, ("midi-thru-to", name), w[2] - 14.0, to, &outputs)
                            })
                            .inner;
                        match picked {
                            Some(Some(output)) => set_thru(&mut p.midi_thru, name, &output),
                            Some(None) => p.midi_thru.retain(|r| &r.input != name),
                            None => {}
                        }
                    });
                });
            }
        });
        if default_all(ui) {
            let defaults = Prefs::default();
            let p = &mut self.draft.prefs;
            p.midi_outputs = defaults.midi_outputs;
            p.midi_inputs = defaults.midi_inputs;
            p.midi_thru = defaults.midi_thru;
        }
    }

    /// VEGAS's VST Effects page: the folders searched for VST plug-ins and
    /// the plug-ins found in them — each VST 3 module looked at in a
    /// BananaV of its own —, the VST 3 effects ticked offered as audio
    /// plug-ins, those that failed listed with why.
    fn vst_effects(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 4.0;
        let folders = self.draft.prefs.vst_folders.clone();
        let page = &mut self.vst;
        let found = vst::plugins(&folders, false, Some(ui.ctx()));
        if found.running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.label(tr("VST search folders:"));
        let columns = [("", 26.0), (tr("Location"), 200.0)];
        list_view(ui, "vst-folders", &columns, 118.0, |ui, w| {
            for (i, folder) in folders.iter().enumerate() {
                let width = w.iter().sum::<f32>();
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(width, LIST_ROW), egui::Sense::click());
                let painter = ui.painter();
                if page.folder == Some(i) {
                    painter.rect_filled(rect, 0.0, theme::hover());
                }
                let icon = Rect::from_center_size(
                    pos2(rect.left() + w[0] / 2.0, rect.center().y),
                    vec2(14.0, 14.0),
                );
                crate::icons::paint(
                    painter,
                    icon,
                    crate::icons::Icon::Folder,
                    theme::banana_dim(),
                );
                crate::widgets::fit_text(
                    painter,
                    pos2(rect.left() + w[0] + 6.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    folder.display(),
                    font.clone(),
                    theme::text(),
                    w[1] - 8.0,
                );
                if resp.clicked() {
                    page.folder = Some(i);
                }
            }
        });
        let chosen = page.folder.filter(|&i| i < folders.len());
        let mut new_folders = None;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pick = |from: Option<&PathBuf>| {
                    let mut d = rfd::FileDialog::new().set_title(tr("VST search folders:"));
                    if let Some(dir) = from {
                        d = d.set_directory(dir);
                    }
                    d.pick_folder()
                };
                if ui
                    .add_enabled(chosen.is_some(), egui::Button::new(tr("Remove path")))
                    .clicked()
                    && let Some(i) = chosen
                {
                    let mut f = folders.clone();
                    f.remove(i);
                    page.folder = None;
                    new_folders = Some(f);
                }
                if ui
                    .add_enabled(chosen.is_some(), egui::Button::new(tr("Edit path")))
                    .clicked()
                    && let Some(i) = chosen
                    && let Some(dir) = pick(folders.get(i))
                {
                    let mut f = folders.clone();
                    f[i] = dir;
                    new_folders = Some(f);
                }
                if ui.button(tr("Add path")).clicked()
                    && let Some(dir) = pick(None)
                    && !folders.contains(&dir)
                {
                    let mut f = folders.clone();
                    f.push(dir);
                    new_folders = Some(f);
                }
            });
        });
        if let Some(f) = new_folders {
            self.draft.prefs.vst_folders = f;
        }
        let left_out = self.draft.prefs.vst_left_out.clone();
        let off = self.draft.prefs.vst_effects_off.clone();
        ui.add_space(4.0);
        ui.label(tr("Select VST effects to be available as audio plug-ins:"));
        let columns = [
            ("", 40.0),
            (tr("Name"), 120.0),
            (tr("Vendor"), 90.0),
            (tr("I/O"), 40.0),
            (tr("Location"), 60.0),
        ];
        // The plug-in whose box was clicked.
        let mut toggled: Option<vst::VstPlugin> = None;
        list_view(ui, "vst-plugins", &columns, 196.0, |ui, w| {
            let width = w.iter().sum::<f32>();
            let message = match &found.plugins {
                Some(list) if list.is_empty() && !found.running => {
                    Some(tr("No VST effects in these folders.").to_string())
                }
                Some(list) if !list.is_empty() => None,
                _ => Some(tr("Searching the folders…").to_string()),
            };
            if let Some(text) = message {
                let (rect, _) = ui.allocate_exact_size(vec2(width, LIST_ROW), egui::Sense::hover());
                ui.painter().text(
                    pos2(rect.left() + 8.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    text,
                    font.clone(),
                    theme::text_dim(),
                );
                return;
            }
            let Some(list) = &found.plugins else {
                return;
            };
            for plugin in list {
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(width, LIST_ROW), egui::Sense::hover());
                let check = Rect::from_center_size(
                    pos2(rect.left() + 12.0, rect.center().y),
                    vec2(13.0, 13.0),
                );
                // VST 3 effects that work can be ticked; the others are
                // listed only.
                let usable = plugin.usable();
                let on = usable && !left_out.contains(&plugin.path) && !off.contains(&plugin.key());
                let click = ui.interact(
                    check.expand(3.0),
                    ui.id().with(("vst-on", &plugin.path, &plugin.class_id)),
                    if usable {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    },
                );
                if click.clicked() {
                    toggled = Some(plugin.clone());
                }
                crate::widgets::paint_check_box(ui, check, on, false, usable);
                let io = plugin
                    .io
                    .map_or_else(|| "–".to_string(), |(i, o)| format!("{i} / {o}"));
                let color = if plugin.failure.is_some() {
                    theme::text_disabled()
                } else {
                    theme::text()
                };
                let painter = ui.painter();
                let mut x = rect.left() + w[0];
                for (text, cw) in [
                    (
                        if plugin.bits32 {
                            trf!("{} (32-bit)", plugin.name)
                        } else {
                            plugin.name.clone()
                        },
                        w[1],
                    ),
                    (plugin.vendor.clone(), w[2]),
                    (io, w[3]),
                    (plugin.path.display().to_string(), w[4]),
                ] {
                    crate::widgets::fit_text(
                        painter,
                        pos2(x + 6.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        text,
                        font.clone(),
                        color,
                        cw - 8.0,
                    );
                    x += cw;
                }
                let tip = if let Some(why) = &plugin.failure {
                    Some(trf!("It failed when tried and is not offered: {}", why))
                } else if plugin.bits32 {
                    Some(
                        tr("A 32-bit plug-in: it runs in BananaV's 32-bit bridge, a process of its own.")
                            .to_string(),
                    )
                } else {
                    None
                };
                if let Some(tip) = tip {
                    resp.union(click).on_hover_text(tip);
                }
            }
        });
        if let Some(p) = toggled {
            let prefs = &mut self.draft.prefs;
            if p.class_id.is_empty() {
                let list = &mut prefs.vst_left_out;
                match list.iter().position(|x| *x == p.path) {
                    Some(i) => {
                        list.remove(i);
                    }
                    None => list.push(p.path.clone()),
                }
            } else {
                let key = p.key();
                let list = &mut prefs.vst_effects_off;
                match list.iter().position(|x| *x == key) {
                    Some(i) => {
                        list.remove(i);
                    }
                    None => list.push(key),
                }
                // A module left out whole comes back with this effect.
                prefs.vst_left_out.retain(|x| *x != p.path);
            }
        }
        // How the search goes: each VST 3 module tried in a process of its
        // own; what failed.
        let failed = found
            .plugins
            .as_ref()
            .map_or(0, |l| l.iter().filter(|p| p.failure.is_some()).count());
        let status = match found.progress {
            Some((done, total)) if found.running => Some(trf!(
                "Trying the VST 3 plug-ins, each in a process of its own: {} of {}…",
                done,
                total
            )),
            _ if failed > 0 => Some(trn!(
                failed,
                "{} plug-in failed when tried and is not offered.",
                "{} plug-ins failed when tried and are not offered.",
                failed
            )),
            _ => None,
        };
        if let Some(status) = status {
            ui.label(RichText::new(status).color(theme::text_dim()));
        }
        crate::tabs::hint(
            ui,
            tr("The VST effects ticked here (VST 3 and VST 2) are offered as audio plug-ins."),
        );
        let refresh = ui
            .with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                let refresh = ui
                    .add_enabled(!found.running, egui::Button::new(tr("Refresh")))
                    .on_hover_text(tr(
                        "Look at every plug-in again, each in a process of its own.",
                    ))
                    .clicked();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        crate::widgets::checkbox(
                            ui,
                            &mut self.draft.prefs.vst_generic_editor,
                            "",
                        )
                        .on_hover_text(tr(
                            "Show BananaV's controls for a plug-in's parameters instead of opening its own window.",
                        ));
                        ui.label(tr("Generic VST editor"));
                    });
                });
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        crate::widgets::checkbox(ui, &mut self.draft.prefs.vst_32bit, "")
                            .on_hover_text(tr(
                                "Run 32-bit VST plug-ins in BananaV's 32-bit bridge, a process of its own: a crash there costs that process only.",
                            ));
                        ui.label(tr("Enable support for 32-bit VST plug-ins"));
                    });
                });
                refresh
            })
            .inner;
        if refresh {
            vst::plugins(&folders, true, Some(ui.ctx()));
        }
    }

    /// VEGAS's Editing page.
    fn editing(&mut self, ui: &mut Ui, app: &BananaApp) {
        let rate = app.frame_rate();
        let fps = crate::fmt::num(rate.as_f64(), 3);
        let frames = |seconds: f64| {
            let n = (seconds * rate.as_f64()).round().max(0.0) as u64;
            trn!(n, "{} frame at {} fps", "{} frames at {} fps", n, fps)
        };
        ui.spacing_mut().item_spacing.y = 5.0;
        let p = &mut self.draft.prefs;
        for (value, label) in [
            (
                &mut p.loop_events,
                tr("Enable looping on events by default"),
            ),
            (
                &mut p.preserve_pitch,
                tr("Preserve pitch when stretching audio events"),
            ),
            (
                &mut p.group_edits,
                tr("Cut, copy, and delete grouped events"),
            ),
            (
                &mut p.no_quantize_audio,
                tr("Do not quantize to frames for audio-only edits"),
            ),
            (
                &mut p.collapse_loop_region,
                tr("Collapse loop region when no time selection is present"),
            ),
        ] {
            crate::widgets::checkbox(ui, value, label);
        }
        ui.add_space(6.0);
        let speed = |s: JklSpeed| match s {
            JklSpeed::Slow => tr("Slow"),
            JklSpeed::Medium => tr("Medium"),
            JklSpeed::Fast => tr("Fast"),
        };
        field(
            ui,
            |ui| {
                ui.label(tr("JKL / shuttle speed:"));
            },
            |ui| {
                egui::ComboBox::from_id_salt("pref-jkl")
                    .width(EDIT_LIST)
                    .selected_text(speed(p.jkl_speed))
                    .show_ui(ui, |ui| {
                        for s in JklSpeed::ALL {
                            ui.selectable_value(&mut p.jkl_speed, s, speed(s));
                        }
                    });
            },
        );
        field(
            ui,
            |ui| {
                ui.label(tr("Quick fade length for audio events (ms):"));
            },
            |ui| {
                let mut v = p.quick_fade_ms as f64;
                crate::widgets::spin_box(ui, &mut v, 0.0..=1000.0, 1.0, 0, EDIT_NUMBER);
                p.quick_fade_ms = v.round() as u32;
            },
        );
        field(
            ui,
            |ui| {
                ui.label(tr("New still image length (seconds):"));
            },
            |ui| {
                seconds_box(ui, &mut p.still_length, 0.001..=3600.0);
                ui.label(frames(p.still_length));
            },
        );
        // Audio CDs are laid out from Project Media with this gap between
        // their tracks: to come.
        field(
            ui,
            |ui| {
                ui.label(tr("Default time between CD tracks:"));
            },
            |ui| {
                let mut text = format!("{}:00", p.cd_track_gap.round() as u32);
                boxed(
                    ui,
                    false,
                    egui::TextEdit::singleline(&mut text).desired_width(EDIT_NUMBER - 14.0),
                )
                .on_disabled_hover_text(tr("Coming soon to BananaV"));
            },
        );
        field(
            ui,
            |ui| {
                ui.label(tr("Cursor preview duration (seconds):"));
            },
            |ui| {
                seconds_box(ui, &mut p.cursor_preview, 0.1..=60.0);
                ui.label(frames(p.cursor_preview));
            },
        );
        ui.add_space(6.0);
        crate::widgets::checkbox(
            ui,
            &mut p.overlap_added_media,
            tr("Automatically overlap multiple selected media when added"),
        );
        field(
            ui,
            |ui| {
                ui.label(tr("Cut-to-overlap conversion (seconds):"));
            },
            |ui| {
                seconds_box(ui, &mut p.cut_overlap, 0.0..=60.0);
                ui.label(frames(p.cut_overlap));
            },
        );
        let cut_align = |a: CutAlign| match a {
            CutAlign::BeforeCut => tr("Before Cut"),
            CutAlign::Centered => tr("Centered On Cut"),
            CutAlign::AfterCut => tr("After Cut"),
        };
        field(
            ui,
            |ui| {
                ui.add_space(20.0);
                ui.label(tr("Alignment:"));
            },
            |ui| {
                egui::ComboBox::from_id_salt("pref-cut-align")
                    .width(EDIT_LIST)
                    .selected_text(cut_align(p.cut_overlap_align))
                    .show_ui(ui, |ui| {
                        for a in CutAlign::ALL {
                            ui.selectable_value(&mut p.cut_overlap_align, a, cut_align(a));
                        }
                    });
            },
        );
        let fades = p.envelope_fades;
        field(
            ui,
            |ui| {
                crate::widgets::checkbox(
                    ui,
                    &mut p.envelope_fades,
                    tr("Time selection envelope fades (ms):"),
                );
            },
            |ui| {
                ui.add_enabled_ui(fades, |ui| {
                    let mut v = p.envelope_fade_ms as f64;
                    crate::widgets::spin_box(ui, &mut v, 0.0..=10_000.0, 1.0, 0, EDIT_NUMBER);
                    p.envelope_fade_ms = v.round() as u32;
                    ui.label(frames(p.envelope_fade_ms as f64 / 1000.0));
                });
            },
        );
        let fade_align = |a: FadeAlign| match a {
            FadeAlign::Outside => tr("Outside"),
            FadeAlign::Centered => tr("Centered"),
            FadeAlign::Inside => tr("Inside"),
        };
        field(
            ui,
            |ui| {
                ui.add_space(20.0);
                ui.add_enabled(fades, egui::Label::new(tr("Alignment:")));
            },
            |ui| {
                ui.add_enabled_ui(fades, |ui| {
                    egui::ComboBox::from_id_salt("pref-fade-align")
                        .width(EDIT_LIST)
                        .selected_text(fade_align(p.envelope_fade_align))
                        .show_ui(ui, |ui| {
                            for a in FadeAlign::ALL {
                                ui.selectable_value(&mut p.envelope_fade_align, a, fade_align(a));
                            }
                        });
                });
            },
        );
        ui.add_space(6.0);
        crate::widgets::checkbox(
            ui,
            &mut p.crop_video,
            tr("Automatically crop video files added to timeline"),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.crop_stills,
            tr("Automatically crop still images added to timeline"),
        );
        field(
            ui,
            |ui| {
                ui.label(tr("Smart split default length (frames):"));
            },
            |ui| {
                let mut v = p.smart_split_frames as f64;
                crate::widgets::spin_box(ui, &mut v, 1.0..=600.0, 1.0, 0, EDIT_NUMBER);
                p.smart_split_frames = v.round() as u32;
            },
        );
        for (value, label) in [
            (
                &mut p.ripple_colors,
                tr("Change cursor and selected event color when in Auto Ripple mode"),
            ),
            (
                &mut p.right_button_selection,
                tr("Use right mouse button to switch to the Selection Edit Tool"),
            ),
            (
                &mut p.click_moves_cursor,
                tr("Move timeline cursor when clicking on an event"),
            ),
        ] {
            crate::widgets::checkbox(ui, value, label);
        }
        if default_all(ui) {
            let d = Prefs::default();
            let p = &mut self.draft.prefs;
            p.loop_events = d.loop_events;
            p.preserve_pitch = d.preserve_pitch;
            p.group_edits = d.group_edits;
            p.no_quantize_audio = d.no_quantize_audio;
            p.collapse_loop_region = d.collapse_loop_region;
            p.jkl_speed = d.jkl_speed;
            p.quick_fade_ms = d.quick_fade_ms;
            p.still_length = d.still_length;
            p.cd_track_gap = d.cd_track_gap;
            p.cursor_preview = d.cursor_preview;
            p.overlap_added_media = d.overlap_added_media;
            p.cut_overlap = d.cut_overlap;
            p.cut_overlap_align = d.cut_overlap_align;
            p.envelope_fades = d.envelope_fades;
            p.envelope_fade_ms = d.envelope_fade_ms;
            p.envelope_fade_align = d.envelope_fade_align;
            p.crop_video = d.crop_video;
            p.crop_stills = d.crop_stills;
            p.smart_split_frames = d.smart_split_frames;
            p.ripple_colors = d.ripple_colors;
            p.right_button_selection = d.right_button_selection;
            p.click_moves_cursor = d.click_moves_cursor;
        }
    }

    /// VEGAS's CD Settings page, for writing audio CDs: BananaV does not
    /// write discs yet, so it shows VEGAS's usual settings.
    fn cd_settings(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        group(ui, tr("General"), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            soon_check(
                ui,
                tr("Use strict Red Book specification for DAO validation"),
                true,
            );
            soon_check(
                ui,
                tr("Include Wide SCSI devices when searching for drives"),
                false,
            );
            soon_check(
                ui,
                tr("Skip drive database; autodetect drive capabilities on startup"),
                false,
            );
        });
        ui.add_space(14.0);
        group(ui, tr("Advanced"), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            soon_check(ui, tr("Use SPTI direct"), false);
            soon_check(ui, tr("Use legacy disc drivers"), false);
        });
        // The settings stay VEGAS's usual ones until discs come.
        let _ = default_all(ui);
    }
}

/// The fade types of events' fades, in VEGAS's lists' order.
const FADE_CURVES: [FadeCurve; 5] = [
    FadeCurve::Linear,
    FadeCurve::Fast,
    FadeCurve::Slow,
    FadeCurve::Smooth,
    FadeCurve::Sharp,
];

pub(crate) fn fade_name(c: FadeCurve) -> &'static str {
    match c {
        FadeCurve::Linear => tr("Linear"),
        FadeCurve::Fast => tr("Fast"),
        FadeCurve::Slow => tr("Slow"),
        FadeCurve::Smooth => tr("Smooth"),
        FadeCurve::Sharp => tr("Sharp"),
    }
}

/// The control surfaces of the Available devices list: the ones BananaV
/// speaks to, and the ones to come.
fn surfaces() -> [(&'static str, Option<SurfaceKind>); 4] {
    [
        (tr("PreSonus FaderPort Driver"), None),
        (tr("Generic Control"), Some(SurfaceKind::Generic)),
        (tr("Mackie Control"), Some(SurfaceKind::MackieControl)),
        (tr("TranzPort Driver"), None),
    ]
}

fn surface_name(kind: SurfaceKind) -> &'static str {
    match kind {
        SurfaceKind::MackieControl => tr("Mackie Control"),
        SurfaceKind::Generic => tr("Generic Control"),
    }
}

fn mtc_rate_name(r: MtcRate) -> &'static str {
    match r {
        MtcRate::Film24 => tr("SMPTE Film Sync (24 fps)"),
        MtcRate::Ebu25 => tr("SMPTE EBU (25 fps)"),
        MtcRate::Drop2997 => tr("SMPTE Drop (29.97 fps)"),
        MtcRate::NonDrop2997 => tr("SMPTE Non-Drop (29.97 fps)"),
        MtcRate::NonDrop30 => tr("SMPTE Non-Drop 30 (30 fps)"),
    }
}

fn locate_name(l: LocateMessages) -> &'static str {
    match l {
        LocateMessages::Never => tr("Never"),
        LocateMessages::PlaybackStart => tr("When playback starts"),
        LocateMessages::PlaybackStartAndCursor => {
            tr("When playback starts and when the cursor moves")
        }
    }
}

/// A list of ports to choose one from, "(None)" first: `ports`, the ones
/// made available in the MIDI tab.
pub(crate) fn port_list(
    ui: &mut Ui,
    id: &str,
    value: &mut Option<String>,
    ports: &[String],
    width: f32,
) {
    let none = tr("(None)");
    egui::ComboBox::from_id_salt(("pref-port", id))
        .width(width)
        .truncate()
        .selected_text(value.clone().unwrap_or_else(|| none.to_string()))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, none);
            for p in ports {
                ui.selectable_value(value, Some(p.clone()), p.as_str());
            }
        });
}

/// The Display page's lists: which track color, envelope and snap type
/// the swatch beside each shows.
#[derive(Debug, Default)]
struct DisplayPage {
    track: usize,
    envelope: usize,
    snap: usize,
}

/// How far a group's contents are indented ([`group`]).
const GROUP_INDENT: f32 = 12.0;
/// The sample of each interface type.
const INTERFACE_SAMPLE: Vec2 = vec2(70.0, 20.0);

/// A line of the Display page: `label`, a list of `names` (`chosen`
/// shown) and, after it, `swatch` for the chosen one.
fn color_list(
    ui: &mut Ui,
    column: f32,
    label: &str,
    id: &str,
    names: &[String],
    chosen: &mut usize,
    swatch: impl FnOnce(&mut Ui, usize),
) {
    *chosen = (*chosen).min(names.len().saturating_sub(1));
    field_in(
        ui,
        column,
        |ui| {
            ui.label(label);
        },
        |ui| {
            let w = (ui.available_width() - SWATCH.x - ui.spacing().item_spacing.x).max(80.0);
            egui::ComboBox::from_id_salt(("pref-display", id))
                .width(w)
                .selected_text(names[*chosen].as_str())
                .show_ui(ui, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        ui.selectable_value(chosen, i, name.as_str());
                    }
                });
            swatch(ui, *chosen);
        },
    )
}

/// A color swatch that is greyed out (with the "Coming soon" tip) while
/// BananaV lacks what it colors.
fn soon_swatch(ui: &mut Ui, ready: bool, rgba: [u8; 4], name: &str) -> egui::Response {
    ui.add_enabled_ui(ready, |ui| swatch(ui, rgba, name))
        .inner
        .on_disabled_hover_text(tr("Coming soon to BananaV"))
}

impl PreferencesDialog {
    /// VEGAS's Deprecated Features page, where older features are switched
    /// on. BananaV has none yet: its list is empty.
    fn deprecated(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add(
            egui::Label::new(tr(
                "These features are no longer supported and may be removed from the application in a future version. To use the feature, select its checkbox below.",
            ))
            .wrap(),
        );
        ui.add_space(10.0);
        ui.label(tr("Deprecated Features:"));
        egui::Frame::new()
            .fill(theme::panel())
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(320.0);
            });
        // Nothing to set back.
        ui.add_enabled_ui(false, default_all);
    }

    /// VEGAS's Display page: the colors of tracks, envelopes and snapping,
    /// the icons' and active buttons' colors, the interface type, the
    /// docking area and the scale.
    fn display(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 5.0;
        // One column for the lists, the slider and the swatch, as VEGAS's.
        let grouped = [tr("Track:"), tr("Envelope Type:"), tr("Snap Type:")];
        let loose = [
            tr("Icon color strength:"),
            tr("Interface brightness:"),
            tr("Button Highlight Color:"),
        ];
        let column =
            (label_column(ui, 0.0, &grouped) + GROUP_INDENT).max(label_column(ui, 0.0, &loose));
        let page = &mut self.display;
        let p = &mut self.draft.prefs;
        let mut pick: Option<(ColorSetting, String)> = None;

        group(ui, tr("Track Colors"), |ui| {
            let names: Vec<String> = (0..8).map(|i| track_color_name(i).to_string()).collect();
            let label = tr("Track:");
            color_list(
                ui,
                column - GROUP_INDENT,
                label,
                "track",
                &names,
                &mut page.track,
                |ui, i| {
                    if swatch(ui, p.track_colors[i], &names[i]).clicked() {
                        pick = Some((ColorSetting::Track(i), names[i].clone()));
                    }
                },
            );
        });
        group(ui, tr("Envelope Colors"), |ui| {
            let all = EnvelopeColor::all();
            let names: Vec<String> = all.iter().map(|e| e.name()).collect();
            let label = tr("Envelope Type:");
            color_list(
                ui,
                column - GROUP_INDENT,
                label,
                "envelope",
                &names,
                &mut page.envelope,
                |ui, i| {
                    let e = all[i];
                    let rgba = p.envelope_colors.get(i).copied();
                    let rgba = rgba.unwrap_or_else(|| e.default_color());
                    if soon_swatch(ui, e.ready(), rgba, &names[i]).clicked() {
                        pick = Some((ColorSetting::Envelope(e), names[i].clone()));
                    }
                },
            );
        });
        group(ui, tr("Snap Colors"), |ui| {
            let names: Vec<String> = SnapKind::ALL.iter().map(|k| k.name().to_string()).collect();
            let label = tr("Snap Type:");
            color_list(
                ui,
                column - GROUP_INDENT,
                label,
                "snap",
                &names,
                &mut page.snap,
                |ui, i| {
                    let k = SnapKind::ALL[i];
                    let rgba = p.snap_colors.get(i).copied();
                    let rgba = rgba.unwrap_or_else(|| k.default_color());
                    if soon_swatch(ui, k.ready(), rgba, &names[i]).clicked() {
                        pick = Some((ColorSetting::Snap(k), names[i].clone()));
                    }
                },
            );
        });
        ui.add_space(8.0);
        field_in(
            ui,
            column,
            |ui| {
                ui.label(tr("Icon color strength:"));
            },
            |ui| {
                let w = (ui.available_width() - 48.0).max(80.0);
                ui.add_sized(
                    vec2(w, 18.0),
                    egui::Slider::new(&mut p.icon_strength, 0..=100).show_value(false),
                );
                ui.label(format!("{} %", p.icon_strength));
            },
        );
        // BananaV's own: the interface type's greys lighter or darker, shown
        // at once (kept with OK).
        field_in(
            ui,
            column,
            |ui| {
                ui.label(tr("Interface brightness:"));
            },
            |ui| {
                let w = (ui.available_width() - 48.0).max(80.0);
                let r = ui.add_sized(
                    vec2(w, 18.0),
                    egui::Slider::new(&mut p.interface_brightness, -40..=40).show_value(false),
                );
                if crate::widgets::bar_double_clicked(ui, &r, false) {
                    p.interface_brightness = 0;
                }
                ui.label(format!("{:+} %", p.interface_brightness));
            },
        );
        field_in(
            ui,
            column,
            |ui| {
                ui.label(tr("Button Highlight Color:"));
            },
            |ui| {
                let name = label_name(tr("Button Highlight Color:"));
                if swatch(ui, p.button_highlight, name).clicked() {
                    pick = Some((ColorSetting::ButtonHighlight, name.to_string()));
                }
            },
        );
        ui.add_space(4.0);
        group(ui, tr("Interface Type"), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // Each type beside a sample of its panels.
            for kind in Interface::ALL {
                field_in(
                    ui,
                    column - GROUP_INDENT + 30.0,
                    |ui| {
                        crate::widgets::radio_value(ui, &mut p.interface, kind, kind.name());
                    },
                    |ui| {
                        let (r, _) = ui.allocate_exact_size(INTERFACE_SAMPLE, egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, kind.palette().panel);
                        ui.painter().rect_stroke(
                            r,
                            0.0,
                            Stroke::new(1.0, theme::separator()),
                            egui::StrokeKind::Inside,
                        );
                    },
                );
            }
        });
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 3.0;
        crate::widgets::checkbox(
            ui,
            &mut p.auto_hide_dock,
            tr("Automatically hide docking area"),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.timeline_at_bottom,
            tr("Display timeline at bottom of main window"),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.tabs_at_top,
            tr("Position tabs at top of docked windows"),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.floating_dock,
            tr("Allow floating windows to dock (hold Ctrl to override)"),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.reverse_event_colors,
            tr("Reverse Event Background color"),
        )
        .on_hover_text(tr(
            "Events take their track's color and their waveforms the darker one.",
        ));
        crate::widgets::checkbox(
            ui,
            &mut p.high_dpi,
            tr("Use High DPI scaling (where available)"),
        );
        // BananaV's windows always take the scale of the display they are
        // on, which is what VEGAS's alternate setting is for (displays with
        // different scales).
        fixed_check(
            ui,
            tr("Use Alternate High DPI Settings"),
            true,
            tr("BananaV's windows always follow the scale of the display each one is on."),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.system_stats,
            tr("Show system statistics in the status bar"),
        );
        if default_all(ui) {
            p.reset_display();
        }
        if let Some((which, name)) = pick {
            let rgba = *self.draft.color_mut(which);
            self.picker = Some((which, ColorPicker::new(name, rgba)));
        }
    }

    /// VEGAS's External Control & Automation page: automation recording,
    /// the control surfaces and the fade types of new envelopes, fades and
    /// keyframes.
    fn external_control(&mut self, ui: &mut Ui, app: &BananaApp) {
        ui.spacing_mut().item_spacing.y = 5.0;
        let p = &mut self.draft.prefs;
        crate::widgets::checkbox(
            ui,
            &mut p.thin_automation,
            tr("Smooth and thin automation data after recording or drawing"),
        );
        crate::widgets::checkbox(
            ui,
            &mut p.automation_off_defaults,
            tr("Set controls to default values when automation is turned off"),
        );
        let count = |k: SurfaceKind| p.control_devices.iter().filter(|d| d.kind == k).count();
        // A Mackie Control and up to four Extenders, five generic ones.
        let mackies = count(SurfaceKind::MackieControl);
        // No more than Options › Internal's "Number of Control Plug-ins
        // allowed" in all.
        let room = p.control_devices.len() < p.max_control_devices as usize;
        let can_add = room
            && match self.surface_pick {
                SurfaceKind::MackieControl => mackies <= crate::surface::MAX_EXTENDERS,
                SurfaceKind::Generic => {
                    count(SurfaceKind::Generic) < crate::generic_surface::MAX_GENERIC
                }
            };
        ui.horizontal(|ui| {
            ui.label(tr("Available devices:"));
            egui::ComboBox::from_id_salt("pref-surfaces")
                .width(200.0)
                .selected_text(surface_name(self.surface_pick))
                .show_ui(ui, |ui| {
                    for (text, kind) in surfaces() {
                        match kind {
                            Some(k) => {
                                ui.selectable_value(&mut self.surface_pick, k, text);
                            }
                            None => {
                                ui.add_enabled(false, egui::Button::selectable(false, text))
                                    .on_disabled_hover_text(tr("Coming soon to BananaV"));
                            }
                        }
                    }
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(can_add, egui::Button::new(tr("Add")))
                    .clicked()
                {
                    // A second one is an Extender, over the next tracks.
                    let extender = self.surface_pick == SurfaceKind::MackieControl && mackies > 0;
                    p.control_devices.push(ControlDevice {
                        kind: self.surface_pick,
                        input: None,
                        output: None,
                        first_track: if extender {
                            (mackies * crate::surface::STRIPS) as u32
                        } else {
                            0
                        },
                        generic: Default::default(),
                        extender,
                        mackie_map: Vec::new(),
                    });
                }
            });
        });
        ui.label(tr("Active control devices:"));
        let columns = [
            (tr("Device"), 110.0),
            (tr("Template"), 80.0),
            (tr("Status"), 120.0),
        ];
        let external = app.settings.prefs.external_control;
        let mut open = None;
        let mut remove = None;
        let font = egui::TextStyle::Body.resolve(ui.style());
        list_view(ui, "pref-active-devices", &columns, 150.0, |ui, w| {
            for (i, d) in p.control_devices.iter().enumerate() {
                let width = w.iter().sum::<f32>();
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(width, LIST_ROW), egui::Sense::click());
                let generic = d.kind == SurfaceKind::Generic;
                let status = if generic && d.input.is_none() {
                    tr("Choose its MIDI input")
                } else if !generic && (d.input.is_none() || d.output.is_none()) {
                    tr("Choose its MIDI ports")
                } else if !external {
                    tr("External Control is off")
                } else {
                    tr("Ready")
                };
                let mut x = rect.left();
                let custom = if generic {
                    !d.generic.bindings.is_empty()
                } else {
                    !d.mackie_map.is_empty()
                };
                let template = if custom { tr("Custom") } else { tr("Default") };
                let name = if d.extender {
                    tr("Mackie Control Extender")
                } else {
                    surface_name(d.kind)
                };
                for (text, cw) in [(name, w[0]), (template, w[1]), (status, w[2])] {
                    crate::widgets::fit_text(
                        ui.painter(),
                        pos2(x + 6.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        text,
                        font.clone(),
                        theme::text(),
                        cw - 8.0,
                    );
                    x += cw;
                }
                if resp.double_clicked() {
                    open = Some(i);
                }
                resp.on_hover_text(tr("Double-click to set it up"))
                    .context_menu(|ui| {
                        if ui.button(tr("Properties…")).clicked() {
                            open = Some(i);
                            ui.close();
                        }
                        if ui.button(tr("Remove")).clicked() {
                            remove = Some(i);
                            ui.close();
                        }
                    });
            }
        });
        if let Some(i) = open {
            self.device_dialog = Some(i);
        }
        if let Some(i) = remove {
            self.draft.prefs.control_devices.remove(i);
            self.device_dialog = None;
        }
        ui.add_space(10.0);
        let p = &mut self.draft.prefs;
        let key_list = |ui: &mut Ui, id: &str, value: &mut KeyCurve| {
            egui::ComboBox::from_id_salt(("pref-curve", id))
                .width(EDIT_LIST)
                .selected_text(tr(value.name()))
                .show_ui(ui, |ui| {
                    for c in KeyCurve::ALL {
                        ui.selectable_value(value, c, tr(c.name()));
                    }
                });
        };
        let fade_list = |ui: &mut Ui, id: &str, value: &mut FadeCurve| {
            egui::ComboBox::from_id_salt(("pref-fade", id))
                .width(EDIT_LIST)
                .selected_text(fade_name(*value))
                .show_ui(ui, |ui| {
                    for c in FADE_CURVES {
                        ui.selectable_value(value, c, fade_name(c));
                    }
                });
        };
        let label = |text: &'static str| {
            move |ui: &mut Ui| {
                ui.label(text);
            }
        };
        let width = label_column(
            ui,
            EDIT_LABEL,
            &[
                tr("New audio envelopes:"),
                tr("New audio event gain:"),
                tr("New audio FX automation envelopes:"),
                tr("New video envelopes:"),
                tr("New video event opacity:"),
                tr("New video FX keyframes:"),
                tr("New video motion keyframes:"),
                tr("Track Motion default smoothness:"),
                tr("Pan/Crop default smoothness:"),
            ],
        );
        field_in(ui, width, label(tr("New audio envelopes:")), |ui| {
            key_list(ui, "audio-env", &mut p.new_audio_envelopes)
        });
        field_in(ui, width, label(tr("New audio event gain:")), |ui| {
            fade_list(ui, "audio-gain", &mut p.new_audio_event_gain)
        });
        field_in(
            ui,
            width,
            label(tr("New audio FX automation envelopes:")),
            |ui| key_list(ui, "audio-fx", &mut p.new_audio_fx_envelopes),
        );
        field_in(ui, width, label(tr("New video envelopes:")), |ui| {
            key_list(ui, "video-env", &mut p.new_video_envelopes)
        });
        field_in(ui, width, label(tr("New video event opacity:")), |ui| {
            fade_list(ui, "video-opacity", &mut p.new_video_event_opacity)
        });
        field_in(ui, width, label(tr("New video FX keyframes:")), |ui| {
            soon_list(
                ui,
                "video-fx",
                tr(p.new_video_fx_keyframes.name()),
                EDIT_LIST,
            )
        });
        field_in(ui, width, label(tr("New video motion keyframes:")), |ui| {
            key_list(ui, "motion", &mut p.new_motion_keyframes)
        });
        field_in(
            ui,
            width,
            label(tr("Track Motion default smoothness:")),
            |ui| {
                let mut v = p.track_motion_smoothness as f64;
                crate::widgets::spin_box(ui, &mut v, 0.0..=100.0, 1.0, 1, EDIT_NUMBER);
                p.track_motion_smoothness = v as f32;
            },
        );
        field_in(ui, width, label(tr("Pan/Crop default smoothness:")), |ui| {
            let mut v = p.pan_crop_smoothness as f64;
            crate::widgets::spin_box(ui, &mut v, 0.0..=100.0, 1.0, 1, EDIT_NUMBER);
            p.pan_crop_smoothness = v as f32;
        });
        if default_all(ui) {
            let d = Prefs::default();
            let p = &mut self.draft.prefs;
            p.thin_automation = d.thin_automation;
            p.automation_off_defaults = d.automation_off_defaults;
            p.new_audio_envelopes = d.new_audio_envelopes;
            p.new_audio_event_gain = d.new_audio_event_gain;
            p.new_audio_fx_envelopes = d.new_audio_fx_envelopes;
            p.new_video_envelopes = d.new_video_envelopes;
            p.new_video_event_opacity = d.new_video_event_opacity;
            p.new_video_fx_keyframes = d.new_video_fx_keyframes;
            p.new_motion_keyframes = d.new_motion_keyframes;
            p.track_motion_smoothness = d.track_motion_smoothness;
            p.pan_crop_smoothness = d.pan_crop_smoothness;
        }
    }

    /// The control surface's window (a double-click in Active control
    /// devices): the MIDI ports it talks through and the track its first
    /// strip controls.
    fn device_window(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        let Some(i) = self.device_dialog else {
            return;
        };
        let ports = self.midi_ports.get_or_insert_with(MidiPorts::find);
        let p = &mut self.draft.prefs;
        let Some(d) = p.control_devices.get_mut(i) else {
            self.device_dialog = None;
            return;
        };
        let inputs: Vec<String> = ports
            .inputs
            .iter()
            .filter(|n| p.midi_inputs.contains(n))
            .cloned()
            .collect();
        // A Generic Control has a dialog of its own.
        if d.kind == SurfaceKind::Generic {
            if !crate::generic_surface::window(ctx, app, d, &inputs, &mut self.generic_ui) {
                self.device_dialog = None;
            }
            return;
        }
        let outputs: Vec<String> = ports
            .outputs
            .iter()
            .filter(|n| p.midi_outputs.contains(n))
            .cloned()
            .collect();
        let pair = (inputs.as_slice(), outputs.as_slice());
        if !crate::mackie_config::window(ctx, app, d, pair, &mut self.mackie_ui) {
            self.device_dialog = None;
        }
    }

    /// External Control & Automation with Configure Mackie Control open on
    /// device `i` (screen tests).
    pub(crate) fn mackie_for_review(&mut self, i: usize, ui: crate::mackie_config::MackieUi) {
        self.tab = PrefTab::ExternalControl;
        self.device_dialog = Some(i);
        self.mackie_ui = ui;
    }

    /// External Control & Automation with Configure Generic Control open on
    /// device `i` (screen tests).
    pub(crate) fn generic_for_review(&mut self, i: usize, ui: crate::generic_surface::GenericUi) {
        self.tab = PrefTab::ExternalControl;
        self.device_dialog = Some(i);
        self.generic_ui = ui;
    }

    /// VEGAS's Sync page: the MIDI timecode and clock BananaV sends, and the
    /// MIDI timecode it follows (Options › Timecode turns each on).
    fn sync(&mut self, ui: &mut Ui) {
        let ports = self.midi_ports.get_or_insert_with(MidiPorts::find);
        let p = &mut self.draft.prefs;
        let outputs: Vec<String> = ports
            .outputs
            .iter()
            .filter(|n| p.midi_outputs.contains(n))
            .cloned()
            .collect();
        let inputs: Vec<String> = ports
            .inputs
            .iter()
            .filter(|n| p.midi_inputs.contains(n))
            .cloned()
            .collect();
        ui.spacing_mut().item_spacing.y = 6.0;
        let label_w = label_column(
            ui,
            110.0,
            &[tr("Output device:"), tr("Frame rate:"), tr("Input device:")],
        );
        let width = (ui.available_width() - label_w - 30.0).max(160.0);
        let line = |ui: &mut Ui, label: &str, enabled: bool, control: &mut dyn FnMut(&mut Ui)| {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                let h = ui.spacing().interact_size.y;
                let layout = egui::Layout::left_to_right(egui::Align::Center);
                ui.allocate_ui_with_layout(vec2(label_w, h), layout, |ui| {
                    ui.set_width(label_w);
                    ui.add_enabled(enabled, egui::Label::new(label));
                });
                ui.add_enabled_ui(enabled, |ui| control(ui));
            });
        };
        let rate_list = |ui: &mut Ui, id: &str, value: &mut MtcRate, show: bool| {
            egui::ComboBox::from_id_salt(("pref-mtc-rate", id))
                .width(width)
                .selected_text(if show { mtc_rate_name(*value) } else { "" })
                .show_ui(ui, |ui| {
                    for r in MtcRate::ALL {
                        ui.selectable_value(value, r, mtc_rate_name(r));
                    }
                });
        };
        ui.label(tr("Generate MIDI Timecode settings:"));
        line(ui, tr("Output device:"), true, &mut |ui| {
            port_list(ui, "mtc-out", &mut p.mtc_output, &outputs, width)
        });
        let on = p.mtc_output.is_some();
        line(ui, tr("Frame rate:"), on, &mut |ui| {
            rate_list(ui, "out", &mut p.mtc_output_rate, true)
        });
        ui.label(tr("Generate MIDI Clock settings:"));
        line(ui, tr("Output device:"), true, &mut |ui| {
            port_list(ui, "clock-out", &mut p.clock_output, &outputs, width)
        });
        ui.label(tr("Trigger from MIDI Timecode settings:"));
        line(ui, tr("Input device:"), true, &mut |ui| {
            port_list(ui, "mtc-in", &mut p.mtc_input, &inputs, width)
        });
        let on = p.mtc_input.is_some();
        line(ui, tr("Frame rate:"), on, &mut |ui| {
            rate_list(ui, "in", &mut p.mtc_input_rate, on)
        });
        let any = p.mtc_output.is_some() || p.clock_output.is_some() || p.mtc_input.is_some();
        let (advanced, all) = ui
            .with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                let pair = ui
                    .horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let all = ui.button(tr("Default All")).clicked();
                            let advanced = ui
                                .add_enabled(any, egui::Button::new(tr("Advanced…")))
                                .clicked();
                            (advanced, all)
                        })
                        .inner
                    })
                    .inner;
                ui.separator();
                pair
            })
            .inner;
        if advanced {
            self.advanced_sync = true;
        }
        if all {
            let d = Prefs::default();
            let p = &mut self.draft.prefs;
            p.mtc_output = d.mtc_output;
            p.mtc_output_rate = d.mtc_output_rate;
            p.clock_output = d.clock_output;
            p.mtc_input = d.mtc_input;
            p.mtc_input_rate = d.mtc_input_rate;
            p.mtc_full_frames = d.mtc_full_frames;
            p.clock_send_start = d.clock_send_start;
            p.clock_song_position = d.clock_song_position;
            p.mtc_freewheel = d.mtc_freewheel;
            p.mtc_slack_frames = d.mtc_slack_frames;
            p.mtc_freewheel_seconds = d.mtc_freewheel_seconds;
            p.mtc_sync_delay = d.mtc_sync_delay;
            p.mtc_offset_quarters = d.mtc_offset_quarters;
        }
    }

    /// Sync's Advanced… window: how timecode and clock go out and how
    /// timecode coming in is followed.
    fn advanced_sync_window(&mut self, ctx: &egui::Context) {
        if !self.advanced_sync {
            return;
        }
        let p = &mut self.draft.prefs;
        let mut open = true;
        let mut close = false;
        let locate_list = |ui: &mut Ui, id: &str, value: &mut LocateMessages| {
            egui::ComboBox::from_id_salt(("pref-locate", id))
                .width(260.0)
                .selected_text(locate_name(*value))
                .show_ui(ui, |ui| {
                    for l in LocateMessages::ALL {
                        ui.selectable_value(value, l, locate_name(l));
                    }
                });
        };
        crate::floating::Window::new(tr("Advanced Sync Preferences"))
            .id(egui::Id::new("preferencias-sincronia"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                group(ui, tr("MIDI Timecode Output"), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("Full-frame message generation:"));
                        locate_list(ui, "mtc", &mut p.mtc_full_frames);
                    });
                });
                group(ui, tr("MIDI Clock Output"), |ui| {
                    crate::widgets::checkbox(
                        ui,
                        &mut p.clock_send_start,
                        tr("Send Start instead of Continue when beginning playback"),
                    );
                    ui.horizontal(|ui| {
                        ui.label(tr("Song Position Pointer generation:"));
                        locate_list(ui, "spp", &mut p.clock_song_position);
                    });
                });
                group(ui, tr("MIDI Timecode Input"), |ui| {
                    crate::widgets::checkbox(
                        ui,
                        &mut p.mtc_freewheel,
                        tr("Free-wheel for time code loss"),
                    );
                    egui::Grid::new("pref-mtc-in")
                        .num_columns(2)
                        .spacing(vec2(10.0, 6.0))
                        .show(ui, |ui| {
                            let wheel = p.mtc_freewheel;
                            ui.add_enabled(
                                wheel,
                                egui::Label::new(tr("Free-wheel slack time (frames):")),
                            );
                            ui.add_enabled_ui(wheel, |ui| {
                                let mut v = p.mtc_slack_frames as f64;
                                crate::widgets::spin_box(ui, &mut v, 1.0..=300.0, 1.0, 0, 70.0);
                                p.mtc_slack_frames = v.round() as u32;
                            });
                            ui.end_row();
                            ui.add_enabled(
                                wheel,
                                egui::Label::new(tr("Free-wheel playback time (seconds):")),
                            );
                            ui.add_enabled_ui(wheel, |ui| {
                                let mut v = p.mtc_freewheel_seconds as f64;
                                crate::widgets::spin_box(ui, &mut v, 0.0..=60.0, 0.5, 1, 70.0);
                                p.mtc_freewheel_seconds = v as f32;
                            });
                            ui.end_row();
                            ui.label(tr("Synchronization delay time (seconds):"));
                            let mut v = p.mtc_sync_delay as f64;
                            crate::widgets::spin_box(ui, &mut v, 0.0..=10.0, 0.1, 1, 70.0);
                            p.mtc_sync_delay = v as f32;
                            ui.end_row();
                            ui.label(tr("Offset adjust (quarter frames):"));
                            let mut v = p.mtc_offset_quarters as f64;
                            crate::widgets::spin_box(ui, &mut v, -400.0..=400.0, 1.0, 0, 70.0);
                            p.mtc_offset_quarters = v.round() as i32;
                            ui.end_row();
                        });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Close")).clicked() {
                        close = true;
                    }
                });
            });
        if !open || close {
            self.advanced_sync = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(name: &str, primary: bool) -> bv_platform::Display {
        bv_platform::Display {
            name: name.into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            hz: 60,
            primary,
            scale: 1.0,
        }
    }

    #[test]
    fn displays_are_numbered_and_chosen_as_the_system_does() {
        // Listed by the system in another order than its numbers.
        let all = [
            display(r"\.\DISPLAY2", false),
            display(r"\.\DISPLAY1", true),
        ];
        assert_eq!(display_label(0, &all[0]), "2. 1920 x 1080 (60)");
        assert_eq!(display_number(1, &all[1]), 1);
        assert_eq!(display_number(4, &display("odd", false)), 5);
        // Not chosen: the first that is not the main display.
        assert_eq!(preview_display(&all, None), Some(0));
        assert_eq!(preview_display(&all, Some(r"\.\DISPLAY1")), Some(1));
        // A display gone: the first other one.
        assert_eq!(preview_display(&all, Some(r"\.\DISPLAY9")), Some(0));
        assert_eq!(preview_display(&[], None), None);
    }

    #[test]
    fn midi_thru_takes_one_input_to_one_output() {
        let mut routes = Vec::new();
        set_thru(&mut routes, "Keys", "Synth A");
        set_thru(&mut routes, "Pads", "Synth B");
        // Keys to B: Pads no longer goes there, nor Keys to A.
        set_thru(&mut routes, "Keys", "Synth B");
        assert_eq!(
            routes,
            [MidiRoute {
                input: "Keys".into(),
                output: "Synth B".into()
            }]
        );
        let mut list = vec!["A".to_string()];
        set_listed(&mut list, "B", true);
        set_listed(&mut list, "A", false);
        assert_eq!(list, ["B"]);
    }
}
