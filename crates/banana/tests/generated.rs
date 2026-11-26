//! The kernels' Rust twins are up to date with the kernels.

#[path = "../gen/gen.rs"]
mod banana_gen;

#[test]
fn the_kernels_on_the_processor_are_up_to_date() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let want = banana_gen::generate(&dir.join("kernels")).expect("the kernels");
    let have = banana_gen::read(&dir.join("src/generated.rs")).expect("generated.rs");
    assert!(
        want == have,
        "src/generated.rs is out of date: run `cargo xtask banana`"
    );
}
