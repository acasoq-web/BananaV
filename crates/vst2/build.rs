//! The guard around calls into plug-ins: on Windows `src/guard.c`'s
//! structured exception handling, built with the MSVC compiler; on macOS
//! `bv_apple::guard`'s signal handlers (nothing built here).

fn main() {
    println!("cargo::rustc-check-cfg=cfg(bv_vst2_guard)");
    println!("cargo:rerun-if-changed=src/guard.c");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        cc::Build::new()
            .file("src/guard.c")
            .warnings(true)
            .compile("bv_vst2_guard");
        println!("cargo:rustc-cfg=bv_vst2_guard");
    }
    if os == "macos" {
        println!("cargo:rustc-cfg=bv_vst2_guard");
    }
}
