//! The guard around calls into plug-ins (`src/guard.c`): Windows'
//! structured exception handling, built with the MSVC compiler.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(bv_ofx_guard)");
    println!("cargo:rerun-if-changed=src/guard.c");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        cc::Build::new()
            .file("src/guard.c")
            .warnings(true)
            .compile("bv_ofx_guard");
        println!("cargo:rustc-cfg=bv_ofx_guard");
    }
}
