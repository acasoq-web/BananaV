//! Proof of concept with the OFX plug-ins of an installed VEGAS — the
//! user's own copy, used here only as test material: nothing of it ships
//! with BananaV. Every bundle is described and each plug-in tried (created,
//! fed a test card, rendered at its defaults and with numbers changed) in a
//! process of its own, so one that crashes or refuses costs nothing. Run
//! with:
//!
//! `cargo test -p bv-ofx --test vegas_poc -- --ignored --nocapture`
//!
//! `BV_OFX_POC_DIR` names the plug-ins' folder (VEGAS 23's by default) and
//! `BV_OFX_POC_LIBS` the folder with the libraries they need (the folder
//! above theirs by default); `BV_OFX_POC_ONLY` (texts, commas between)
//! tries only the plug-ins whose name or identifier holds one of them,
//! the others only described. The report is `target/ofx-poc/report.md`,
//! the pictures PNG files beside it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bv_ofx::scan::{Scanner, read_raw};
use bv_ofx::{Error, plugin};

const DEFAULT_DIR: &str =
    r"F:\SteamLibrary\steamapps\common\VEGAS Pro 23 Steam Edition\App\OFX Video Plug-Ins";

fn png(raw: &Path) -> Option<PathBuf> {
    let img = read_raw(raw)?;
    let rgba: Vec<u8> = img
        .data
        .iter()
        .flat_map(|px| px.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    let out = raw.with_extension("png");
    std::fs::write(&out, bv_png::encode_rgba8(img.width, img.height, &rgba, 6)).ok()?;
    let _ = std::fs::remove_file(raw);
    Some(out)
}

fn result(r: &Result<(), Error>) -> String {
    match r {
        Ok(()) => "ok".into(),
        Err(e) => format!("**{e}**"),
    }
}

#[test]
#[ignore = "needs an installed VEGAS; writes a report"]
fn vegas_plugins_in_bananav() {
    let dir = std::env::var_os("BV_OFX_POC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DIR));
    if !dir.is_dir() {
        eprintln!("no plug-ins at {}", dir.display());
        return;
    }
    let libs = std::env::var_os("BV_OFX_POC_LIBS")
        .map(PathBuf::from)
        .or_else(|| dir.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let only: Vec<String> = std::env::var("BV_OFX_POC_ONLY")
        .map(|o| {
            o.split(',')
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ofx-poc");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let scanner = Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_bv-ofx-scan")),
        args: Vec::new(),
        library_folders: vec![libs.clone()],
        env: Vec::new(),
        timeout: Duration::from_secs(180),
    };

    let mut report = String::new();
    let _ = writeln!(report, "# OFX plug-ins of {} in BananaV\n", dir.display());
    let _ = writeln!(report, "Libraries from {}.\n", libs.display());
    let (mut binaries, mut described, mut tried, mut passed) = (0, 0, 0, 0);
    for bundle in plugin::find_bundles(&dir) {
        for binary in plugin::bundle_binaries(&bundle) {
            binaries += 1;
            let name = binary.file_name().unwrap().to_string_lossy().into_owned();
            let scan = scanner.describe(&binary);
            let _ = writeln!(report, "## {name}\n");
            if !scan.missing_suites.is_empty() {
                let list: Vec<_> = scan
                    .missing_suites
                    .iter()
                    .map(|(s, v)| format!("{s} v{v}"))
                    .collect();
                let _ = writeln!(
                    report,
                    "Suites asked for and missing: {}.\n",
                    list.join(", ")
                );
            }
            for (i, id, e) in &scan.failures {
                let _ = writeln!(report, "- plug-in {i} `{id}`: **{e}**");
            }
            let plugins = match &scan.result {
                Ok(p) => p.clone(),
                Err(e) => {
                    let _ = writeln!(report, "Describing: **{e}**\n");
                    for m in &scan.messages {
                        let _ = writeln!(report, "> {m}");
                    }
                    let _ = writeln!(report);
                    continue;
                }
            };
            for p in &plugins {
                described += 1;
                let _ = writeln!(
                    report,
                    "### {} — `{}` {}.{}\n",
                    p.name(),
                    p.identifier,
                    p.version[0],
                    p.version[1]
                );
                let contexts: Vec<_> = p.contexts.iter().map(|c| format!("{c:?}")).collect();
                let params = p.described.first().map_or(0, |d| d.params.len());
                let _ = writeln!(
                    report,
                    "Group: {}. Contexts: {}. Depths: {}. {} parameters.\n",
                    if p.grouping.is_empty() {
                        "—"
                    } else {
                        &p.grouping
                    },
                    contexts.join(", "),
                    p.depths.join(", "),
                    params
                );
                let named = |t: &String| {
                    p.identifier.to_lowercase().contains(t) || p.name().to_lowercase().contains(t)
                };
                if !only.is_empty() && !only.iter().any(named) {
                    continue;
                }
                tried += 1;
                let trial = scanner.try_plugin(&binary, p.index, &out);
                if trial.passed() {
                    passed += 1;
                }
                for s in &trial.steps {
                    let _ = writeln!(report, "- {}: {}", s.what, result(&s.result));
                }
                for raw in &trial.pictures {
                    if let Some(png) = png(raw) {
                        let file = png.file_name().unwrap().to_string_lossy().into_owned();
                        let _ = writeln!(report, "\n![{file}]({file})");
                    }
                }
                if !trial.missing_suites.is_empty() {
                    let list: Vec<_> = trial
                        .missing_suites
                        .iter()
                        .map(|(s, v)| format!("{s} v{v}"))
                        .collect();
                    let _ = writeln!(report, "\nMissing suites: {}.", list.join(", "));
                }
                for m in trial.messages.iter().take(8) {
                    let _ = writeln!(report, "> {m}");
                }
                let _ = writeln!(report);
            }
        }
    }
    let summary = format!(
        "{binaries} binaries, {described} plug-ins described, {passed} of {tried} rendered with every step ok."
    );
    report.insert_str(0, &format!("{summary}\n\n"));
    std::fs::write(out.join("report.md"), &report).unwrap();
    eprintln!("{summary}\nReport: {}", out.join("report.md").display());
    // The host survived every plug-in: whatever they did happened in
    // their own processes.
    assert!(binaries > 0, "no plug-in binaries in {}", dir.display());
}
