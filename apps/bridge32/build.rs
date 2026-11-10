//! A deeper stack for the bridge's main thread, which runs the plug-in:
//! 8 MiB (some plug-ins keep large buffers on the stack).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}
