//! How long the graphics card takes to compile every kernel, each one's
//! time with `BV_GPU_TRACE=1`:
//! `cargo test -p bv-gpu --test warmup -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::time::Instant;

use bv_gpu::Gpu;

#[test]
#[ignore = "measures the card"]
fn every_kernel_compiles_in_time() {
    let start = Instant::now();
    let Ok(g) = Gpu::open(None) else {
        println!("no graphics card: skipped");
        return;
    };
    println!("opened {:?} in {:?}", g.adapter().name, start.elapsed());
    let start = Instant::now();
    g.warm_up().expect("every kernel compiles");
    println!("every kernel in {:?}", start.elapsed());
}
