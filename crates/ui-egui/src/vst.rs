//! VST plug-ins on this computer (Options › Preferences › VST Effects):
//! found in the search folders and read without running them — a VST 3
//! bundle's module information or its library's exports, a VST 2 library's
//! exports, and the version resource for who made it. BananaV runs the
//! VST 3 effects chosen here (`vst_ui.rs`); VST 2 ones are listed only.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// A VST plug-in found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VstPlugin {
    pub name: String,
    pub vendor: String,
    /// Input and output channels, when the plug-in tells without running.
    pub io: Option<(u32, u32)>,
    /// Its file (a VST 3 bundle's folder).
    pub path: PathBuf,
    pub vst3: bool,
    /// A VST 3 effect's class (32 hexadecimal digits) when its module
    /// information gives it; else empty (the module's first effect).
    pub class_id: String,
}

/// Folders deep within a search folder looked into, and entries read at
/// most (a folder of the whole disk does not hang the search).
const DEPTH: usize = 8;
const MAX_ENTRIES: usize = 50_000;

/// The plug-ins in `folders` and the folders within them, by name.
pub(crate) fn scan(folders: &[PathBuf]) -> Vec<VstPlugin> {
    let mut found = Vec::new();
    let mut budget = MAX_ENTRIES;
    for folder in folders {
        walk(folder, 0, &mut budget, &mut found);
    }
    found.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    found.dedup_by(|a, b| a.path == b.path && a.name == b.name);
    found
}

fn walk(dir: &Path, depth: usize, budget: &mut usize, found: &mut Vec<VstPlugin>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = entry.path();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        match ext.as_str() {
            "vst3" if is_dir => found.extend(vst3_bundle(&path)),
            "vst3" => found.extend(vst3_library(&path, &path)),
            // VST 2 libraries: Windows' and Linux's.
            "dll" | "so" if !is_dir => found.extend(vst2_library(&path)),
            _ if is_dir && depth < DEPTH => walk(&path, depth + 1, budget, found),
            _ => {}
        }
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn company(path: &Path) -> String {
    bv_platform::version_texts(path)
        .and_then(|t| t.company)
        .unwrap_or_default()
}

/// A VST 2 plug-in: a library for this processor exporting its entry
/// point.
fn vst2_library(path: &Path) -> Option<VstPlugin> {
    let lib = bv_platform::read_library(path)?;
    let entry = lib
        .exports
        .iter()
        .any(|e| e == "VSTPluginMain" || e == "main");
    (lib.native && entry).then(|| VstPlugin {
        name: stem(path),
        vendor: company(path),
        io: None,
        path: path.to_path_buf(),
        vst3: false,
        class_id: String::new(),
    })
}

/// A VST 3 library (`file`), listed as `shown` (its bundle, or itself).
fn vst3_library(file: &Path, shown: &Path) -> Option<VstPlugin> {
    let lib = bv_platform::read_library(file)?;
    let entry = lib.exports.iter().any(|e| e == "GetPluginFactory");
    (lib.native && entry).then(|| VstPlugin {
        name: stem(shown),
        vendor: company(file),
        io: None,
        path: shown.to_path_buf(),
        vst3: true,
        class_id: String::new(),
    })
}

/// The effects of a VST 3 bundle: from its module information when it has
/// some (each effect class of the module), else its library.
fn vst3_bundle(bundle: &Path) -> Vec<VstPlugin> {
    let contents = bundle.join("Contents");
    for info in [
        contents.join("Resources").join("moduleinfo.json"),
        contents.join("moduleinfo.json"),
    ] {
        if let Ok(text) = std::fs::read_to_string(&info) {
            return module_info(&text, bundle);
        }
    }
    // The library for this system and processor, as the VST 3 bundle
    // format lays it out: `x86_64-win/Plugs.vst3`, `aarch64-linux/Plugs.so`…
    let (arch, file) = match (cfg!(windows), cfg!(target_arch = "aarch64")) {
        (true, true) => ("arm64-win", bundle.file_name().map(PathBuf::from)),
        (true, false) => ("x86_64-win", bundle.file_name().map(PathBuf::from)),
        (false, arm) => (
            if arm { "aarch64-linux" } else { "x86_64-linux" },
            bundle
                .file_stem()
                .map(|s| PathBuf::from(s).with_extension("so")),
        ),
    };
    let file = contents.join(arch).join(file.unwrap_or_default());
    vst3_library(&file, bundle).into_iter().collect()
}

/// The effect classes of a VST 3 module information file (the JSON its
/// SDK writes into the bundle): instruments are left out.
fn module_info(text: &str, bundle: &Path) -> Vec<VstPlugin> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&plain_json(text)) else {
        return Vec::new();
    };
    let factory_vendor = json["Factory Info"]["Vendor"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let Some(classes) = json["Classes"].as_array() else {
        return Vec::new();
    };
    classes
        .iter()
        .filter(|c| c["Category"].as_str() == Some("Audio Module Class"))
        .filter_map(|c| {
            let subs: Vec<&str> = c["Sub Categories"]
                .as_array()
                .map(|a| a.iter().filter_map(|s| s.as_str()).collect())
                .unwrap_or_default();
            if subs.contains(&"Instrument") {
                return None;
            }
            let io = if subs.contains(&"Mono") {
                Some((1, 1))
            } else if subs.contains(&"Stereo") {
                Some((2, 2))
            } else {
                None
            };
            Some(VstPlugin {
                name: c["Name"].as_str()?.to_string(),
                vendor: c["Vendor"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                    .map_or_else(|| factory_vendor.clone(), str::to_string),
                io,
                path: bundle.to_path_buf(),
                vst3: true,
                class_id: c["CID"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// `text` without what JSON5 allows and JSON does not (module information
/// files may hold both): `//` comments and commas before a closing bracket.
fn plain_json(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut in_string) = (0, false);
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// A search going on in the background.
#[derive(Debug)]
pub(crate) struct Search {
    /// The folders searched.
    pub folders: Vec<PathBuf>,
    rx: mpsc::Receiver<Vec<VstPlugin>>,
}

impl Search {
    pub fn start(folders: Vec<PathBuf>) -> Search {
        let (tx, rx) = mpsc::channel();
        let searched = folders.clone();
        std::thread::spawn(move || {
            let found = scan(&searched);
            // Options › Preferences › File I/O › Log Plugin loading.
            if crate::logs::wants(crate::logs::LogKind::Plugins) {
                let mut lines = vec![format!("VST plug-ins: {} found", found.len())];
                lines.extend(
                    found
                        .iter()
                        .map(|p| format!("{} ({}): {}", p.name, p.vendor, p.path.display())),
                );
                crate::logs::log(crate::logs::LogKind::Plugins, &lines);
            }
            let _ = tx.send(found);
        });
        Search { folders, rx }
    }

    /// The plug-ins, once found.
    pub fn result(&self) -> Option<Vec<VstPlugin>> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_read_from_a_bundles_module_information() {
        let text = r#"{
            // Written by the SDK.
            "Name": "Plugs",
            "Factory Info": { "Vendor": "Makers", },
            "Classes": [
                { "Category": "Audio Module Class", "Name": "Echo",
                  "CID": "42414E414E4156544553544741494E31",
                  "Sub Categories": ["Fx", "Delay", "Stereo"], },
                { "Category": "Audio Module Class", "Name": "Synth",
                  "Vendor": "Others", "Sub Categories": ["Instrument"] },
                { "Category": "Component Controller Class", "Name": "Echo UI" },
                { "Category": "Audio Module Class", "Name": "Gate",
                  "Vendor": "Others", "Sub Categories": ["Fx"] }
            ]
        }"#;
        let found = module_info(text, Path::new("C:\\VST3\\Plugs.vst3"));
        let names: Vec<String> = found
            .iter()
            .map(|p| format!("{} by {}, {:?}", p.name, p.vendor, p.io))
            .collect();
        assert_eq!(
            names,
            ["Echo by Makers, Some((2, 2))", "Gate by Others, None"]
        );
        assert!(found.iter().all(|p| p.vst3));
        assert_eq!(found[0].class_id, "42414E414E4156544553544741494E31");
        assert_eq!(found[1].class_id, "");
    }

    #[test]
    fn a_folder_without_plugins_lists_none() {
        let dir = std::env::temp_dir().join("bananav-vst-empty");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("readme.txt"), "no plug-ins").unwrap();
        std::fs::write(dir.join("fake.dll"), "not a library").unwrap();
        assert!(scan(std::slice::from_ref(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
