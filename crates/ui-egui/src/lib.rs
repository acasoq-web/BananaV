//! Interface do BananaV em egui.
//!
//! A interface só conversa com o resto do sistema por dois caminhos: o modelo
//! ([`bv_timeline::Project`], editado via [`bv_timeline::History`]) e a
//! [`bv_engine::Engine`]. Trocar o egui por outra biblioteca de interface não
//! exige mexer em nada abaixo desta crate.

#[macro_use]
mod i18n;

mod app;
mod archive;
mod ask;
mod automation;
mod autosave;
mod beats_ui;
mod burn_cd;
mod burn_dvd;
mod burn_job;
mod camera_media;
mod captions_ui;
mod clip_windows;
mod color_picker;
mod command_ui;
mod commands;
mod compositors;
mod cursors;
mod customize_toolbar;
mod dialogs;
mod dock;
mod ducking_ui;
mod envelope_look;
mod event_properties;
mod external_monitor;
mod floating;
mod flow_ui;
mod fmt;
mod fx_automation_ui;
mod fx_packages;
mod fx_window;
mod generated_copies;
mod generated_media;
mod generic_surface;
mod grading_curves;
mod header_buttons;
mod history_label;
mod icons;
mod interchange_ui;
mod interface_fonts;
mod internal_prefs;
mod keyboard_dialog;
mod keymap;
mod layouts;
mod logs;
mod mackie_config;
mod marker_flag;
mod mask_tracking;
mod media_files;
mod media_general;
mod media_menu;
mod media_preview;
mod media_properties;
mod menu_dialogs;
mod menus;
mod midi;
mod modern_eq;
mod more_dialogs;
mod motion_tracking;
mod multicam;
mod nested_timelines;
mod nested_ui;
mod nonrealtime_ui;
mod ofx;
mod ofx_color;
mod ofx_controls;
mod ofx_curves;
mod ofx_keys;
mod ofx_pictures;
mod otio_ui;
mod pancrop;
mod pancrop_keys;
mod pancrop_mask;
mod preferences;
mod prerender_ui;
mod presets;
mod project_properties;
mod proxies;
mod realtime_ui;
mod recording;
mod render;
mod safe_start;
mod screen_capture;
mod sequence_dialog;
mod settings;
mod stabilize_ui;
mod subclip_ui;
mod subtitles;
mod surface;
mod sync;
mod tabs;
mod takes;
mod theme;
mod timefmt;
mod timeline;
mod title_bar;
mod toolbar_buttons;
mod track_defaults;
mod track_motion;
mod transform_ui;
mod transition_window;
mod transport_buttons;
mod updates;
mod veg_generators;
mod veg_rtf;
mod veg_ui;
mod view360;
mod vst;
mod vst_ui;
mod vu_meters;
mod welcome;
mod widgets;

pub use app::BananaApp;
pub use i18n::{Language, set_language, tr};

/// The computer's fonts for the letters egui's lack (Chinese, Japanese,
/// Korean, other scripts) added to `ctx` now, in `lang`'s order — the
/// program adds them as it starts; the captures call this.
pub fn install_system_fonts(ctx: &egui::Context, lang: Language) {
    interface_fonts::install_now(ctx, lang);
}

/// Aplica o tema do BananaV a um contexto egui (usado também nos testes).
pub fn apply_theme(ctx: &egui::Context) {
    theme::apply(ctx);
}

pub use eframe::egui;

/// Linux: BananaV's windows are X11 ones, through XWayland under Wayland. A
/// Wayland window cannot place its dialogs, nor see the mouse, the keypad
/// or the screen outside its windows; `BANANAV_WAYLAND=1` asks for
/// Wayland's own windows all the same.
#[cfg(target_os = "linux")]
fn event_loop_builder() -> Option<eframe::EventLoopBuilderHook> {
    let x11 =
        std::env::var_os("DISPLAY").is_some() && std::env::var_os("BANANAV_WAYLAND").is_none();
    x11.then(|| -> eframe::EventLoopBuilderHook {
        Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        })
    })
}

#[cfg(not(target_os = "linux"))]
fn event_loop_builder() -> Option<eframe::EventLoopBuilderHook> {
    None
}

/// Opens the main window and runs until it is closed.
/// When BananaV was started to describe or try plug-ins in a process of
/// its own — OFX ones (`--ofx-scan`, see `bv_ofx::scan`), a VST 3 module
/// (`--vst3-scan`, see `bv_vst3::scan`) or a VST 2 library
/// (`--vst2-scan`, see `bv_vst2::scan`): does that, and gives the exit
/// code.
pub fn plugin_scan_child() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags = [
        bv_ofx::scan::CHILD_FLAG,
        bv_vst3::scan::CHILD_FLAG,
        bv_vst2::scan::CHILD_FLAG,
    ];
    if args.first().is_some_and(|a| flags.contains(&a.as_str())) {
        bv_platform::background_process();
    }
    match args.first().map(String::as_str) {
        Some(bv_ofx::scan::CHILD_FLAG) => Some(bv_ofx::scan::child_main(&args[1..])),
        Some(bv_vst3::scan::CHILD_FLAG) => Some(bv_vst3::scan::child_main(&args[1..])),
        Some(bv_vst2::scan::CHILD_FLAG) => Some(bv_vst2::scan::child_main(&args[1..])),
        _ => None,
    }
}

/// The program's icon, for the main window's title bar and taskbar button:
/// the official one (`packaging/icons/bananav.png`, a 256 px render of
/// `bananav-icon.svg`).
fn window_icon() -> Option<egui::IconData> {
    let png = bv_png::decode(include_bytes!("../../../packaging/icons/bananav.png")).ok()?;
    Some(egui::IconData {
        rgba: png.to_rgba8(),
        width: png.width,
        height: png.height,
    })
}

/// How the interface is drawn: through `graphics` (see
/// [`safe_start::Graphics`]: on Windows DirectX 12, OpenGL where there is
/// none, unless a start failed on it). `WGPU_BACKEND` still picks another.
fn interface_graphics(graphics: safe_start::Graphics) -> eframe::egui_wgpu::WgpuSetup {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if std::env::var_os("WGPU_BACKEND").is_none()
        && let Some(backends) = graphics.window_backends()
    {
        setup.instance_descriptor.backends = backends;
    }
    eframe::egui_wgpu::WgpuSetup::CreateNew(setup)
}

pub fn run() -> eframe::Result {
    // A start that ended early (a driver failing on DirectX 12) makes this
    // one draw through another interface; crashes write Crash.log.
    let graphics = safe_start::begin();
    // macOS: Quit and the files Finder opens get BananaV's answers.
    bv_platform::before_run();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("BananaV")
        .with_inner_size([1600.0, 960.0])
        .with_min_inner_size([1024.0, 640.0]);
    viewport = if cfg!(target_os = "macos") {
        // BananaV's bar under a clear title bar: the system's window
        // buttons over its left end, as Mac programs have them.
        viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
    } else {
        // The title bar is BananaV's, with the menus in it, as VEGAS 23's
        // (`title_bar.rs`).
        viewport.with_decorations(false)
    };
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        // One frame waiting to be shown at most, as egui advises for
        // interfaces (eframe's default lets two wait): what is dragged
        // follows the pointer a frame sooner.
        wgpu_options: eframe::WgpuConfiguration {
            surface: eframe::SurfaceConfig::LOW_LATENCY,
            wgpu_setup: interface_graphics(graphics),
            ..Default::default()
        },
        event_loop_builder: event_loop_builder(),
        ..Default::default()
    };
    let ran = eframe::run_native(
        "BananaV",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            // Double-clicks as slow as the system takes them (Windows' half
            // second; egui's own is 0.3 s, which missed many).
            let delay = bv_platform::double_click_time().clamp(0.2, 0.9);
            cc.egui_ctx
                .options_mut(|o| o.input_options.max_double_click_delay = delay);
            // What the system asks (Quit, a file to open) wakes the
            // interface up.
            let ctx = cc.egui_ctx.clone();
            bv_platform::set_wake(move || ctx.request_repaint());
            bv_platform::ui_thread_started();
            let mut app = BananaApp::new(cc.egui_ctx.clone());
            // The computer's fonts behind egui's: Chinese, Japanese, Korean
            // and other scripts show their letters, not boxes.
            interface_fonts::install(&cc.egui_ctx, crate::i18n::language());
            app.startup(std::env::args_os().skip(1));
            Ok(Box::new(app))
        }),
    );
    match &ran {
        // Closed normally: the start counts as one that went well.
        Ok(()) => safe_start::settle(),
        // The window could not open on this interface: the next one is
        // tried at once.
        Err(e) => {
            safe_start::window_failed(e);
        }
    }
    // The window and the engine are gone: the plug-ins loaded hear they
    // are done with (some close programs they started), a few seconds at
    // most.
    bv_ofx::unload_all(std::time::Duration::from_secs(3));
    ran
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_window_icon_is_the_logo() {
        let icon = super::window_icon().expect("the icon decodes");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // The rounded tile leaves its corners clear and is solid in the middle.
        assert_eq!(icon.rgba[3], 0);
        assert_eq!(icon.rgba[(128 * 256 + 128) * 4 + 3], 255);
    }
}
