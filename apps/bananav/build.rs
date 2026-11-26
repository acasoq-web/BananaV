//! A deeper stack for BananaV's main thread, the interface's, which calls
//! the plug-ins that run in this process (their editors, their overlays,
//! what they do when a setting changes): 16 MiB, as the engine's threads
//! that call them have (`bv_ofx::PLUGIN_STACK`). Windows gives a program's
//! main thread 1 MiB, and plug-ins' code may recurse deeply or keep large
//! buffers on the stack. It is only reserved address space until used.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
}
