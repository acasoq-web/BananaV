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
mod customize_toolbar;
mod dialogs;
mod dock;
mod ducking_ui;
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
mod transition_window;
mod transport_buttons;
mod view360;
mod vst;
mod vst_ui;
mod vu_meters;
mod welcome;
mod widgets;

pub use app::BananaApp;
pub use i18n::{Language, set_language};

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
/// When BananaV was started to describe or try OFX plug-ins in a process
/// of its own (`--ofx-scan`, see `bv_ofx::scan`): does that, and gives the
/// exit code.
pub fn plugin_scan_child() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    (args.first().map(String::as_str) == Some(bv_ofx::scan::CHILD_FLAG))
        .then(|| bv_ofx::scan::child_main(&args[1..]))
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

/// How the interface is drawn. On Windows through DirectX 12 (OpenGL where
/// there is none), Windows' own way to the screen: with monitors on two
/// graphics cards — one in the processor, one apart —, a window drawn on
/// one card and shown on the other's monitor is copied by Windows itself,
/// so dragging it from one monitor to the other stays smooth, as VEGAS's
/// does. `WGPU_BACKEND` still picks another.
fn interface_graphics() -> eframe::egui_wgpu::WgpuSetup {
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
    eframe::egui_wgpu::WgpuSetup::CreateNew(setup)
}

pub fn run() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("BananaV")
        // The title bar is BananaV's, with the menus in it, as VEGAS 23's
        // (`title_bar.rs`).
        .with_decorations(false)
        .with_inner_size([1600.0, 960.0])
        .with_min_inner_size([1024.0, 640.0]);
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
            wgpu_setup: interface_graphics(),
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
            let mut app = BananaApp::new(cc.egui_ctx.clone());
            app.startup(std::env::args_os().skip(1));
            Ok(Box::new(app))
        }),
    );
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
