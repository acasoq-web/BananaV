//! What the tests that load BananaV's effects as a vendor ships them share.

use std::path::{Path, PathBuf};

/// Builds `bananav-ofx` and puts it in `folder` as
/// `BananaV.ofx.bundle/Contents/<arch>/BananaV.ofx`; the binary's path.
pub fn bundle_in(folder: &Path) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("target").join("ofx-bundle");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args(["build", "-q", "-p", "bananav-ofx", "--target-dir"])
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(status.success());
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
    let lib = target
        .join("debug")
        .join(format!("{DLL_PREFIX}bananav_ofx{DLL_SUFFIX}"));
    let contents = folder
        .join("BananaV.ofx.bundle")
        .join("Contents")
        .join(bv_ofx::plugin::architectures()[0]);
    std::fs::create_dir_all(&contents).unwrap();
    let binary = contents.join("BananaV.ofx");
    std::fs::copy(&lib, &binary).unwrap();
    binary
}
