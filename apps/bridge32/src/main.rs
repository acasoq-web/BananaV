//! BananaV's 32-bit bridge: a 32-bit plug-in (VST 2 or VST 3) in a process
//! of its own, serving BananaV through its standard input and output, or
//! looked at for BananaV's list of plug-ins. Started by BananaV only.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.as_slice() {
        [flag, rest @ ..] if flag == bv_vst2::scan::CHILD_FLAG => bv_vst2::scan::child_main(rest),
        [flag, file, id] if flag == bv_vst2::bridge::SERVE_FLAG => {
            bv_vst2::bridge::serve(Path::new(file), id.parse().unwrap_or(0))
        }
        [flag, rest @ ..] if flag == bv_vst3::scan::CHILD_FLAG => bv_vst3::scan::child_main(rest),
        [flag, file, class, controller] if flag == bv_vst3::bridge::SERVE_FLAG => {
            bv_vst3::bridge::serve(Path::new(file), class, controller == "1")
        }
        _ => 1,
    };
    std::process::exit(code);
}
