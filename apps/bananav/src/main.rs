//! BananaV — video editor.

// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A BananaV of its own describing OFX plug-ins (a crash there costs
    // nothing).
    if let Some(code) = bv_ui_egui::plugin_scan_child() {
        std::process::exit(code);
    }
    if let Err(e) = bv_ui_egui::run() {
        eprintln!("BananaV: {e}");
        std::process::exit(1);
    }
}
