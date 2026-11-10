//! The guard around calls into plug-ins on macOS (`src/guard.c`), built
//! with the Mac's compiler. Checked from another system (`tools/
//! mac_check.sh`), nothing is built or linked: the Rust around it is read
//! all the same.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(bv_apple_guard)");
    println!("cargo:rerun-if-changed=src/guard.c");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "macos" {
        return;
    }
    let on_a_mac = std::env::var("HOST").is_ok_and(|h| h.contains("apple"));
    if on_a_mac {
        cc::Build::new()
            .file("src/guard.c")
            .warnings(true)
            .compile("bv_apple_guard");
    }
    println!("cargo:rustc-cfg=bv_apple_guard");
}
