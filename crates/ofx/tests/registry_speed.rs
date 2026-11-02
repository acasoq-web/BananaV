//! How long the registry takes to answer with the plug-ins this computer
//! has (the cache BananaV keeps): lookups run every frame, for every
//! effect drawn. Run with:
//!
//! `cargo test --release -p bv-ofx --test registry_speed -- --ignored --nocapture`

use std::time::Instant;

#[test]
#[ignore = "reads this computer's plug-in cache; prints timings"]
fn lookups_with_this_computers_plug_ins() {
    let Some(path) = std::env::var_os("APPDATA")
        .map(|a| {
            std::path::PathBuf::from(a)
                .join("BananaV")
                .join("ofx_cache.json")
        })
        .filter(|p| p.exists())
    else {
        eprintln!("no plug-in cache here");
        return;
    };
    let start = Instant::now();
    let cache = bv_ofx::scan::Cache::load(&path);
    let infos: Vec<bv_ofx::PluginInfo> = cache.plugins().into_iter().cloned().collect();
    eprintln!(
        "cache: {} plug-ins read in {:.0} ms",
        infos.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    let id = infos
        .last()
        .map(|p| p.identifier.clone())
        .unwrap_or_default();
    bv_ofx::registry().set_scanned(infos);
    let start = Instant::now();
    for _ in 0..20 {
        assert!(bv_ofx::registry().info(&id).is_some());
    }
    eprintln!(
        "info(): {:.3} ms a lookup",
        start.elapsed().as_secs_f64() * 1000.0 / 20.0
    );
    let start = Instant::now();
    for _ in 0..20 {
        assert!(!bv_ofx::registry().known().is_empty());
    }
    eprintln!(
        "known(): {:.3} ms a call",
        start.elapsed().as_secs_f64() * 1000.0 / 20.0
    );
}
