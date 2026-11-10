//! `cargo xtask <comando>` — automação de CI do BananaV.
//!
//! Comandos:
//!   fmt [--fix]  confere (ou aplica) a formatação
//!   lint         clippy em todo o workspace, warnings viram erro
//!   test         testes de todo o workspace
//!   layers       confere a regra de camadas entre crates
//!   wasm         compila para wasm32 as crates marcadas com `wasm = true`
//!   ci           tudo acima, na ordem (e, no Windows, a ponte de 32 bits)
//!   i18n         lists, per interface language, the texts its catalog
//!                lacks and the entries no text uses (also written to
//!                `target/i18n/<tag>.txt`)
//!   perf         the performance guards in an optimized build of their
//!                own (`target/perf`): the interface's frame budget while
//!                the preview plays, and with `BV_PLAY_FILE=<video>` how
//!                fast the preview plays that file

#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::process::{Command, ExitCode};

use serde_json::Value;

const HELP: &str = "uso: cargo xtask <comando>
  fmt [--fix]  confere (ou aplica) a formatação
  lint         clippy em todo o workspace, warnings viram erro
  test         testes de todo o workspace
  layers       confere a regra de camadas entre crates
  wasm         compila para wasm32 as crates marcadas com `wasm = true`
  ci           tudo acima, na ordem
  i18n         texts each interface language lacks (target/i18n/<tag>.txt)
  perf         the interface's frame budget while playing, in an optimized
               build (BV_PLAY_FILE=<video>: the preview playing that file)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let rest = &args[args.len().min(1)..];
    let result = match cmd {
        "fmt" => fmt(rest.iter().any(|a| a == "--fix")),
        "lint" => lint(),
        "test" => test(),
        "layers" => layers().map(|table| print_layers(&table)),
        "wasm" => wasm(),
        "ci" => ci(),
        "i18n" => i18n(),
        "perf" => perf(),
        _ => {
            println!("{HELP}");
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, String>;

fn cargo(args: &[&str]) -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .status()
        .map_err(|e| format!("não foi possível executar cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` falhou", args.join(" ")))
    }
}

/// `cargo` with `envs` set.
fn cargo_env(args: &[&str], envs: &[(&str, &str)]) -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .envs(envs.iter().copied())
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`cargo {}` failed", args.join(" ")))
    }
}

/// The performance guards, measured in an optimized build of their own in
/// `target/perf` (no link-time optimization, so that it builds in minutes;
/// with line tables, so that `tools/stacks.cs` names what a run of it
/// spends its time on): the interface's frame budget while the preview
/// plays (`frame_budget`), and with `BV_PLAY_FILE` set, the preview
/// playing that file in the engine (`play_file`, its other variables in
/// that test).
fn perf() -> Result<()> {
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("perf");
    let target = target.to_string_lossy().into_owned();
    let envs = [
        ("CARGO_PROFILE_RELEASE_LTO", "off"),
        ("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16"),
        ("CARGO_PROFILE_RELEASE_DEBUG", "line-tables-only"),
        ("CARGO_TARGET_DIR", target.as_str()),
    ];
    let guard = |package: &str, test: &str| {
        cargo_env(
            &[
                "test",
                "--release",
                "-p",
                package,
                "--test",
                test,
                "--",
                "--ignored",
                "--nocapture",
            ],
            &envs,
        )
    };
    guard("bv-ui-egui", "frame_budget")?;
    if std::env::var_os("BV_PLAY_FILE").is_some() {
        guard("bv-engine", "play_file")
    } else {
        println!("(BV_PLAY_FILE=<video> also times the preview playing that file)");
        Ok(())
    }
}

fn fmt(fix: bool) -> Result<()> {
    if fix {
        cargo(&["fmt", "--all"])
    } else {
        cargo(&["fmt", "--all", "--check"])
    }
}

fn lint() -> Result<()> {
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])
}

fn test() -> Result<()> {
    cargo(&["test", "--workspace", "--quiet"])
}

/// The catalogs' coverage report (the ignored `i18n::coverage::report`
/// test prints it).
fn i18n() -> Result<()> {
    cargo(&[
        "test",
        "-p",
        "bv-ui-egui",
        "--lib",
        "--quiet",
        "i18n::coverage::report",
        "--",
        "--ignored",
        "--exact",
        "--nocapture",
    ])
}

fn ci() -> Result<()> {
    fmt(false)?;
    let table = layers()?;
    print_layers(&table);
    lint()?;
    test()?;
    wasm()?;
    bridge32()
}

/// BananaV's 32-bit bridge (`apps/bridge32`), built for 32-bit Windows on a
/// Windows computer whose Rust has that target (`rustup target add
/// i686-pc-windows-msvc`); elsewhere a note says it was left out.
fn bridge32() -> Result<()> {
    const TARGET: &str = "i686-pc-windows-msvc";
    if !cfg!(windows) {
        return Ok(());
    }
    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let installed = std::path::Path::new(&sysroot)
        .join("lib")
        .join("rustlib")
        .join(TARGET)
        .is_dir();
    if !installed {
        eprintln!(
            "(the 32-bit bridge was not built: `rustup target add {TARGET}` adds its target)"
        );
        return Ok(());
    }
    cargo(&["build", "--target", TARGET, "-p", "bananav-bridge32"])
}

#[derive(Debug)]
struct Crate {
    name: String,
    /// `None` para ferramentas de desenvolvimento (fora do grafo do produto).
    layer: Option<u64>,
    wasm: bool,
    deps: Vec<(String, DepKind)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DepKind {
    Normal,
    Dev,
    Build,
}

fn metadata() -> Result<Vec<Crate>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let packages = v["packages"].as_array().ok_or("metadata sem pacotes")?;
    let mut crates = Vec::new();
    for p in packages {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        let meta = &p["metadata"]["bananav"];
        let layer = match &meta["layer"] {
            Value::Number(n) => Some(n.as_u64().ok_or(format!("{name}: camada inválida"))?),
            Value::String(s) if s == "tool" => None,
            _ => {
                return Err(format!(
                    "{name}: falta `[package.metadata.bananav] layer = N` no Cargo.toml"
                ));
            }
        };
        let deps = p["dependencies"]
            .as_array()
            .map(|d| {
                d.iter()
                    .map(|d| {
                        let kind = match d["kind"].as_str() {
                            Some("dev") => DepKind::Dev,
                            Some("build") => DepKind::Build,
                            _ => DepKind::Normal,
                        };
                        (d["name"].as_str().unwrap_or_default().to_string(), kind)
                    })
                    .collect()
            })
            .unwrap_or_default();
        crates.push(Crate {
            name,
            layer,
            wasm: meta["wasm"].as_bool().unwrap_or(false),
            deps,
        });
    }
    Ok(crates)
}

/// Regra: uma crate só depende (normal ou build) de crates de camada *menor*.
/// Testes (dev-dependencies) podem usar a mesma camada. Nenhuma crate do
/// produto depende de ferramentas.
fn check_layers(crates: &[Crate]) -> Vec<String> {
    let by_name: BTreeMap<&str, &Crate> = crates.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut errors = Vec::new();
    for c in crates {
        for (dep, kind) in &c.deps {
            let Some(d) = by_name.get(dep.as_str()) else {
                continue; // dependência externa
            };
            match (c.layer, d.layer) {
                (None, _) => {}
                (Some(_), None) => {
                    errors.push(format!("{} depende da ferramenta {}", c.name, d.name))
                }
                (Some(own), Some(theirs)) => {
                    let ok = match kind {
                        DepKind::Dev => theirs <= own,
                        DepKind::Normal | DepKind::Build => theirs < own,
                    };
                    if !ok {
                        errors.push(format!(
                            "{} (camada {own}) não pode depender de {} (camada {theirs}){}",
                            c.name,
                            d.name,
                            if *kind == DepKind::Dev {
                                " nem em testes"
                            } else {
                                ""
                            }
                        ));
                    }
                }
            }
        }
    }
    errors
}

fn layers() -> Result<Vec<Crate>> {
    let crates = metadata()?;
    let errors = check_layers(&crates);
    if errors.is_empty() {
        Ok(crates)
    } else {
        Err(format!("violação de camadas:\n  {}", errors.join("\n  ")))
    }
}

fn print_layers(crates: &[Crate]) {
    let mut by_layer: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
    for c in crates {
        if let Some(l) = c.layer {
            by_layer.entry(l).or_default().push(&c.name);
        }
    }
    println!("camadas OK");
    for (l, names) in by_layer {
        println!("  {l}: {}", names.join(", "));
    }
}

fn wasm() -> Result<()> {
    let crates = metadata()?;
    let mut args = vec![
        "build".to_string(),
        "--target".into(),
        "wasm32-unknown-unknown".into(),
    ];
    for c in crates.iter().filter(|c| c.wasm) {
        args.push("-p".into());
        args.push(c.name.clone());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    cargo(&args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, layer: Option<u64>, deps: &[(&str, DepKind)]) -> Crate {
        Crate {
            name: name.into(),
            layer,
            wasm: false,
            deps: deps.iter().map(|(n, k)| (n.to_string(), *k)).collect(),
        }
    }

    #[test]
    fn regras_de_camada() {
        let crates = vec![
            krate("core", Some(0), &[("serde", DepKind::Normal)]),
            krate("codec", Some(1), &[("core", DepKind::Normal)]),
            krate("codec2", Some(1), &[("codec", DepKind::Dev)]),
            krate("ui", Some(5), &[("codec", DepKind::Normal)]),
            krate(
                "ruim",
                Some(1),
                &[("ui", DepKind::Normal), ("codec", DepKind::Normal)],
            ),
            krate("tool", None, &[("ui", DepKind::Normal)]),
            krate("usa_tool", Some(3), &[("tool", DepKind::Dev)]),
        ];
        let errors = check_layers(&crates);
        assert_eq!(errors.len(), 3, "{errors:#?}");
        assert!(errors[0].contains("ruim") && errors[0].contains("ui"));
        assert!(errors[1].contains("ruim") && errors[1].contains("codec"));
        assert!(errors[2].contains("ferramenta"));
    }
}
