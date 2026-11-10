//! VST plug-ins on this computer (Options › Preferences › VST Effects):
//! found in the search folders. Each VST 3 module and each VST 2 library
//! is looked at in a process of its own — a BananaV (`bv_vst3::scan`,
//! `bv_vst2::scan`), or BananaV's 32-bit bridge for a 32-bit library —:
//! loaded, each of its effects made and tried there, so a plug-in that
//! crashes or hangs costs that process only; several at once, what they
//! said kept in caches until their files change (`vst3_cache.json`,
//! `vst2_cache.json` beside the settings), what the caches know listed at
//! once. Effects that failed are listed with why, never offered. Where
//! BananaV cannot start a scan of its own (the tests' programs) the files
//! are read without running them: a bundle's module information, a
//! library's exports. BananaV runs the effects chosen here (`vst_ui.rs`).
//!
//! One search serves the whole interface (the Plug-In Chooser and the
//! Preferences page): it starts with BananaV, and again when the folders
//! change, 32-bit plug-ins are turned on or off, or Refresh is pressed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use bv_vst3::scan::{Cache, EffectInfo, ModuleScan, Scanner};

use crate::egui;

/// A VST plug-in found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VstPlugin {
    /// Its name as listed: an effect whose name an earlier one has gets
    /// its module's file name too ("Echo (Plugs)").
    pub name: String,
    pub vendor: String,
    /// Input and output channels, when known.
    pub io: Option<(u32, u32)>,
    /// Its file (a VST 3 bundle's folder).
    pub path: PathBuf,
    pub vst3: bool,
    /// A VST 3 effect's class (32 hexadecimal digits) when known; else
    /// empty (the module's first effect).
    pub class_id: String,
    /// Its categories ("Fx|Delay") and version, when known.
    pub category: String,
    pub version: String,
    /// Its programs (presets of its own) and latency (samples at 48 kHz),
    /// when tried.
    pub programs: u32,
    pub latency: u32,
    /// Why it cannot be used: it failed when tried (listed, never
    /// offered).
    pub failure: Option<String>,
    /// A 32-bit plug-in (run in BananaV's 32-bit bridge).
    pub bits32: bool,
}

impl VstPlugin {
    /// Offered as an audio effect: an effect (VST 3 or VST 2) that did not
    /// fail.
    pub fn usable(&self) -> bool {
        self.failure.is_none()
    }

    /// What the user's choices keep it by: its class (the same effect
    /// wherever it is installed), else its file.
    pub fn key(&self) -> String {
        if self.class_id.is_empty() {
            self.path.to_string_lossy().into_owned()
        } else {
            self.class_id.to_ascii_uppercase()
        }
    }
}

/// Folders deep within a search folder looked into, and entries read at
/// most (a folder of the whole disk does not hang the search).
const DEPTH: usize = 8;
const MAX_ENTRIES: usize = 50_000;
/// A module's scan given up after this long (some plug-ins check their
/// licence online as they start).
const TIMEOUT: Duration = Duration::from_secs(60);

/// What the folders hold: VST 2 libraries and VST 3 modules (to look at).
#[derive(Debug, Default)]
struct Files {
    vst2: Vec<PathBuf>,
    vst3: Vec<PathBuf>,
}

fn walk(dir: &Path, depth: usize, budget: &mut usize, found: &mut Files) {
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
            "vst3" => found.vst3.push(path),
            // VST 2 libraries: Windows' and Linux's; macOS's bundles.
            "dll" | "so" if !is_dir && vst2_entry(&path) => found.vst2.push(path),
            "vst"
                if is_dir
                    && cfg!(target_os = "macos")
                    && vst2_entry(&bv_vst2::bundle_executable(&path)) =>
            {
                found.vst2.push(path)
            }
            _ if is_dir && depth < DEPTH => walk(&path, depth + 1, budget, found),
            _ => {}
        }
    }
}

/// The plug-ins' files in `folders` and the folders within them.
fn files(folders: &[PathBuf]) -> Files {
    let mut found = Files::default();
    let mut budget = MAX_ENTRIES;
    for folder in folders {
        walk(folder, 0, &mut budget, &mut found);
    }
    found.vst3.sort();
    found.vst3.dedup();
    found.vst2.sort();
    found.vst2.dedup();
    found
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

/// A 32-bit x86 library on 64-bit Windows: run in the 32-bit bridge.
fn bits32_on_64(path: &Path) -> bool {
    cfg!(windows)
        && bv_vst2::this_machine() != bv_vst2::Machine::X86
        && bv_vst2::machine(path) == Some(bv_vst2::Machine::X86)
}

/// Whether `path` is a VST 2 library BananaV can run: exporting a plug-in's
/// entry point, made for this processor or 32-bit (for the bridge).
fn vst2_entry(path: &Path) -> bool {
    let Some(lib) = bv_platform::read_library(path) else {
        return false;
    };
    let entry = lib
        .exports
        .iter()
        .any(|e| e == "VSTPluginMain" || e == "main" || e == "main_macho");
    entry && (lib.native || bits32_on_64(path))
}

/// A VST 2 library read without running it (the tests' programs).
fn vst2_read(path: &Path) -> VstPlugin {
    let bits32 = bits32_on_64(path);
    VstPlugin {
        name: stem(path),
        vendor: company(path),
        path: path.to_path_buf(),
        bits32,
        failure: (bits32 && !bv_bridge::enabled())
            .then(|| bv_vst2::scan::Failure::NeedsBridge.to_string()),
        ..VstPlugin::default()
    }
}

/// A VST 3 library (`file`) read without running it, listed as `shown`
/// (its bundle, or itself); a 32-bit one (`bits32`) runs in the bridge.
fn vst3_library(file: &Path, shown: &Path, bits32: bool) -> Option<VstPlugin> {
    let lib = bv_platform::read_library(file)?;
    let entry = lib.exports.iter().any(|e| e == "GetPluginFactory");
    ((lib.native || bits32) && entry).then(|| VstPlugin {
        name: stem(shown),
        vendor: company(file),
        path: shown.to_path_buf(),
        vst3: true,
        ..VstPlugin::default()
    })
}

/// A VST 3 module read without running it: a bundle's module information
/// when it has some (each effect class of the module), else its library;
/// a 32-bit module marked so (and failed while 32-bit plug-ins are off).
fn vst3_read(module: &Path) -> Vec<VstPlugin> {
    let bits32 = bv_vst3::is_32bit(module);
    let mark = |mut p: VstPlugin| {
        p.bits32 = bits32;
        if bits32 && !bv_bridge::enabled() {
            p.failure = Some(bv_vst3::scan::Failure::NeedsBridge.to_string());
        }
        p
    };
    if !module.is_dir() {
        return vst3_library(module, module, bits32)
            .into_iter()
            .map(mark)
            .collect();
    }
    let contents = module.join("Contents");
    for info in [
        contents.join("Resources").join("moduleinfo.json"),
        contents.join("moduleinfo.json"),
    ] {
        if let Ok(text) = std::fs::read_to_string(&info) {
            return module_info(&text, module).into_iter().map(mark).collect();
        }
    }
    let file = if bits32 {
        bv_vst3::library_32(module)
    } else {
        bv_vst3::library_of(module)
    };
    vst3_library(&file, module, bits32)
        .into_iter()
        .map(mark)
        .collect()
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
                category: subs.join("|"),
                version: c["Version"].as_str().unwrap_or_default().to_string(),
                ..VstPlugin::default()
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

/// A tried effect as listed.
fn tried(module: &ModuleScan, e: &EffectInfo) -> VstPlugin {
    VstPlugin {
        name: e.name.clone(),
        vendor: if e.vendor.is_empty() {
            module.vendor.clone()
        } else {
            e.vendor.clone()
        },
        io: (e.inputs > 0 || e.outputs > 0).then_some((e.inputs, e.outputs)),
        path: module.path.clone(),
        vst3: true,
        class_id: e.class_id.clone(),
        category: e.sub_categories.clone(),
        version: e.version.clone(),
        programs: e.programs,
        latency: e.latency,
        failure: e.error.clone(),
        bits32: module.bits32,
    }
}

/// A tried VST 2 plug-in as listed.
fn tried2(module: &bv_vst2::scan::ModuleScan, e: &bv_vst2::scan::EffectInfo) -> VstPlugin {
    VstPlugin {
        name: e.name.clone(),
        vendor: e.vendor.clone(),
        io: (e.inputs > 0 || e.outputs > 0).then_some((e.inputs, e.outputs)),
        path: module.path.clone(),
        vst3: false,
        class_id: bv_vst2::class_id(e.unique_id, e.id),
        category: String::new(),
        version: if e.version > 0 {
            e.version.to_string()
        } else {
            String::new()
        },
        programs: e.programs,
        latency: e.latency,
        failure: e.error.clone(),
        bits32: module.bits32(),
    }
}

/// What a VST 2 library said, as listed: its effects (instruments and
/// generators left out), or the library itself with why it failed.
fn listed2(module: &bv_vst2::scan::ModuleScan) -> Vec<VstPlugin> {
    match &module.result {
        Ok(effects) => effects
            .iter()
            .filter(|e| e.is_effect())
            .map(|e| tried2(module, e))
            .collect(),
        Err(why) => vec![VstPlugin {
            name: stem(&module.path),
            path: module.path.clone(),
            failure: Some(why.to_string()),
            bits32: module.bits32(),
            ..VstPlugin::default()
        }],
    }
}

/// What a module said, as listed: its effects, or the module itself with
/// why it failed. Instruments are left out.
fn listed(module: &ModuleScan) -> Vec<VstPlugin> {
    match &module.result {
        Ok(effects) => effects
            .iter()
            .filter(|e| !e.sub_categories.split('|').any(|s| s == "Instrument"))
            .map(|e| tried(module, e))
            .collect(),
        Err(why) => vec![VstPlugin {
            name: stem(&module.path),
            vendor: module.vendor.clone(),
            path: module.path.clone(),
            vst3: true,
            failure: Some(why.to_string()),
            bits32: module.bits32,
            ..VstPlugin::default()
        }],
    }
}

/// The list: by name, VST 3 effects with names of their own.
fn sorted(mut all: Vec<VstPlugin>) -> Vec<VstPlugin> {
    all.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.class_id.cmp(&b.class_id))
    });
    all.dedup_by(|a, b| a.path == b.path && a.name == b.name && a.class_id == b.class_id);
    // Names of their own among the 64-bit plug-ins, and among the 32-bit
    // ones (a plug-in installed both ways keeps its name: the 32-bit one
    // is marked so where it is listed).
    for bits32 in [false, true] {
        let at: Vec<usize> = (0..all.len())
            .filter(|&i| all[i].bits32 == bits32)
            .collect();
        let pairs: Vec<(&str, &Path)> = at
            .iter()
            .map(|&i| (all[i].name.as_str(), all[i].path.as_path()))
            .collect();
        let names = bv_vst3::scan::unique_names(&pairs);
        for (&i, name) in at.iter().zip(names) {
            all[i].name = name;
        }
    }
    all
}

/// A search's state, shared by the whole interface: one for each list of
/// folders searched (the latest few kept).
#[derive(Debug, Default)]
struct Shared {
    plugins: Option<Vec<VstPlugin>>,
    /// Modules looked at, of how many (while looking).
    progress: Option<(usize, usize)>,
    running: bool,
    /// Which search of these folders is the current one (an older one's
    /// answer is left).
    generation: u64,
}

/// The searches, by folders, the latest last.
type Searches = Vec<(Vec<PathBuf>, Shared)>;

static SHARED: Mutex<Searches> = Mutex::new(Vec::new());

/// How many lists of folders' searches are kept.
const KEPT: usize = 4;

/// `f` on the search of `folders` (made when there is none).
fn with_shared<T>(folders: &[PathBuf], f: impl FnOnce(&mut Shared) -> T) -> T {
    let mut all = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    let i = match all.iter().position(|(f, _)| f == folders) {
        Some(i) => i,
        None => {
            if all.len() >= KEPT {
                all.remove(0);
            }
            all.push((folders.to_vec(), Shared::default()));
            all.len() - 1
        }
    };
    f(&mut all[i].1)
}

/// What the search knows.
#[derive(Clone, Debug, Default)]
pub(crate) struct Found {
    /// The plug-ins: what is known while the search goes on (the cache's),
    /// all once it is done; none before anything is known.
    pub plugins: Option<Vec<VstPlugin>>,
    /// Modules looked at, of how many, while looking.
    pub progress: Option<(usize, usize)>,
    pub running: bool,
}

/// A search's key: its folders, and a mark when 32-bit plug-ins are on.
fn key(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut k = folders.to_vec();
    if bv_bridge::enabled() {
        k.push(PathBuf::from("\u{0}32-bit"));
    }
    k
}

/// The plug-ins in `folders`: what was found, a search started in the
/// background when nothing was for them yet (or `again`: every module
/// looked at anew, the cache set aside). `ctx` is repainted as it goes.
pub(crate) fn plugins(folders: &[PathBuf], again: bool, ctx: Option<&egui::Context>) -> Found {
    let k = key(folders);
    with_shared(&k, |s| {
        let never = s.plugins.is_none() && !s.running;
        if never || (again && !s.running) {
            s.generation += 1;
            s.plugins = None;
            s.progress = None;
            s.running = true;
            let (generation, folders, ctx) = (s.generation, folders.to_vec(), ctx.cloned());
            let k = k.clone();
            let _ = std::thread::Builder::new()
                .name("bv-vst-search".into())
                .spawn(move || search(generation, &k, &folders, again, ctx.as_ref()));
        }
        Found {
            plugins: s.plugins.clone(),
            progress: s.progress,
            running: s.running,
        }
    })
}

/// The best of `known` for an effect `wanted` of kind `format`: its
/// class's number the same, then the most of its checks (its name's, its
/// file's) agreeing.
fn best<'a>(
    known: &'a [VstPlugin],
    wanted: &bv_timeline::VstWanted,
    format: bv_timeline::VstFormat,
) -> Option<&'a VstPlugin> {
    known
        .iter()
        .filter(|p| p.vst3 == format.is_vst3())
        .filter_map(|p| {
            let file = p
                .path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let score = if p.vst3 {
                wanted.score(&p.class_id, &p.name, &file)?
            } else {
                // A VST 2 plug-in: the number it is known by.
                let id = u32::from_str_radix(p.class_id.get(..8)?, 16).ok()?;
                wanted.score_number(id, &p.name, &file)?
            };
            Some((score, p))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, p)| p)
}

/// The VST 3 effects of `project` read from another editor's project
/// (their plug-in wanted, `VstFx::wanted`), found among the plug-ins known
/// in `folders`: each found gets its plug-in's file and class. How many
/// were not found (their sound goes through untouched).
pub(crate) fn find_wanted(project: &mut bv_timeline::Project, folders: &[PathBuf]) -> usize {
    let wanted = |effects: &[bv_timeline::Effect]| {
        effects
            .iter()
            .any(|e| e.vst.as_ref().is_some_and(|v| v.wanted.is_some()))
    };
    let any = project.media.iter().any(|m| wanted(&m.effects))
        || project
            .tracks
            .iter()
            .any(|t| wanted(&t.effects) || t.clips.iter().any(|c| wanted(&c.effects)))
        || project.busses.iter().any(|b| wanted(&b.channel.effects))
        || project.fx_chains.iter().any(|c| wanted(&c.channel.effects))
        || wanted(&project.master_fx);
    if !any {
        return 0;
    }
    let known: Vec<VstPlugin> = plugins(folders, false, None)
        .plugins
        .unwrap_or_default()
        .into_iter()
        .filter(VstPlugin::usable)
        .collect();
    let mut missing = 0;
    let mut visit = |effects: &mut Vec<bv_timeline::Effect>| {
        for e in effects {
            let Some(v) = e.vst.as_deref_mut() else {
                continue;
            };
            let Some(wanted) = v.wanted else {
                continue;
            };
            match best(&known, &wanted, v.format) {
                Some(p) => {
                    v.path = p.path.clone();
                    v.class_id = p.class_id.clone();
                    if v.name.is_empty() {
                        v.name = p.name.clone();
                    }
                    v.vendor = p.vendor.clone();
                    v.wanted = None;
                }
                None => missing += 1,
            }
        }
    };
    for m in &mut project.media {
        visit(&mut m.effects);
    }
    for t in &mut project.tracks {
        visit(&mut t.effects);
        for c in &mut t.clips {
            visit(&mut c.effects);
        }
    }
    for b in &mut project.busses {
        visit(&mut b.channel.effects);
    }
    for c in &mut project.fx_chains {
        visit(&mut c.channel.effects);
    }
    visit(&mut project.master_fx);
    missing
}

/// Where a scan keeps what modules said: `name` beside the settings.
fn cache_path(name: &str) -> Option<PathBuf> {
    Some(bv_platform::config_dir()?.join("BananaV").join(name))
}

/// Search `generation` of `key` (`folders`): sets what it found while it is
/// the current one.
fn search(
    generation: u64,
    key: &[PathBuf],
    folders: &[PathBuf],
    again: bool,
    ctx: Option<&egui::Context>,
) {
    let current = |f: &mut dyn FnMut(&mut Shared)| {
        with_shared(key, |s| {
            if s.generation == generation {
                f(s);
            }
        });
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    };
    let found = files(folders);
    let scanners = (
        Scanner::bananav(TIMEOUT),
        bv_vst2::scan::Scanner::bananav(TIMEOUT),
    );
    let paths = (cache_path("vst3_cache.json"), cache_path("vst2_cache.json"));
    let (all, scanned) = match (scanners, paths) {
        ((Some(s3), Some(s2)), (Some(p3), Some(p2))) => {
            let mut c3 = if again {
                Cache::default()
            } else {
                Cache::load(&p3)
            };
            let mut c2 = if again {
                bv_vst2::scan::Cache::default()
            } else {
                bv_vst2::scan::Cache::load(&p2)
            };
            // What the caches know, at once.
            let known: Vec<VstPlugin> = found
                .vst3
                .iter()
                .filter_map(|m| c3.known(m))
                .flat_map(listed)
                .chain(
                    found
                        .vst2
                        .iter()
                        .filter_map(|m| c2.known(m))
                        .flat_map(listed2),
                )
                .collect();
            let known = sorted(known);
            current(&mut |s| s.plugins = Some(known.clone()));
            let workers =
                std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4));
            let todo3 = found.vst3.iter().filter(|m| c3.known(m).is_none()).count();
            let todo2 = found.vst2.iter().filter(|m| c2.known(m).is_none()).count();
            let total = todo3 + todo2;
            let tell = |done: usize| {
                if total > 0 {
                    current(&mut |s| s.progress = Some((done.min(total), total)));
                }
            };
            let changed3 = c3.refresh(&found.vst3, workers, |m| s3.describe(m), |d, _| tell(d));
            let changed2 = c2.refresh(
                &found.vst2,
                workers,
                |m| s2.describe(m),
                |d, _| tell(todo3 + d),
            );
            if with_shared(key, |s| s.generation == generation) {
                if changed3 {
                    let _ = c3.save(&p3);
                }
                if changed2 {
                    let _ = c2.save(&p2);
                }
            }
            let mut all: Vec<VstPlugin> = c3
                .modules
                .iter()
                .filter(|m| found.vst3.contains(&m.path))
                .flat_map(listed)
                .collect();
            all.extend(
                c2.modules
                    .iter()
                    .filter(|m| found.vst2.contains(&m.path))
                    .flat_map(listed2),
            );
            (all, true)
        }
        // No scan of BananaV's own (a test's program): read, not run.
        _ => {
            let mut all: Vec<VstPlugin> = found.vst3.iter().flat_map(|m| vst3_read(m)).collect();
            all.extend(found.vst2.iter().map(|p| vst2_read(p)));
            (all, false)
        }
    };
    let all = sorted(all);
    // Options › Preferences › File I/O › Log Plugin loading.
    if crate::logs::wants(crate::logs::LogKind::Plugins) {
        let failed = all.iter().filter(|p| p.failure.is_some()).count();
        let mut lines = vec![format!(
            "VST plug-ins: {} found, {failed} failed{}",
            all.len(),
            if scanned {
                ", each looked at in a process of its own"
            } else {
                ""
            }
        )];
        lines.extend(all.iter().map(|p| {
            let why = p
                .failure
                .as_ref()
                .map(|f| format!(": failed: {f}"))
                .unwrap_or_default();
            format!("{} ({}): {}{why}", p.name, p.vendor, p.path.display())
        }));
        crate::logs::log(crate::logs::LogKind::Plugins, &lines);
    }
    current(&mut |s| {
        s.plugins = Some(all.clone());
        s.progress = None;
        s.running = false;
    });
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
        assert!(found.iter().all(|p| p.vst3 && p.usable()));
        assert_eq!(found[0].class_id, "42414E414E4156544553544741494E31");
        assert_eq!(found[0].category, "Fx|Delay|Stereo");
        assert_eq!(found[1].class_id, "");
    }

    /// An effect of another editor's project finds its plug-in by its
    /// class's number, the checks of its name and file deciding between
    /// plug-ins of the same number.
    #[test]
    fn wanted_effects_find_their_plug_in() {
        let plugin = |name: &str, path: &str, id: &str| VstPlugin {
            name: name.into(),
            path: PathBuf::from(path),
            vst3: true,
            class_id: id.into(),
            ..VstPlugin::default()
        };
        let eq = "ABCDEF019182FAEB4D4147584D584645";
        let known = [
            plugin(
                "Gain",
                "C:/VST3/Gain.vst3",
                "42414E414E4156544553544741494E31",
            ),
            plugin("Copy", "C:/VST3/Copy.vst3", eq),
            plugin("Modern Equalizer", "C:/VST3/APU_modernEQ_x64.vst3", eq),
        ];
        let wanted = bv_timeline::VstWanted {
            number: 0xF6A7_FB77,
            name: 0x857C,
            file: 0x7DD2,
        };
        let found = best(&known, &wanted, bv_timeline::VstFormat::Vst3).expect("found");
        assert_eq!(found.name, "Modern Equalizer");
        let other = bv_timeline::VstWanted {
            number: 1,
            ..wanted
        };
        assert!(best(&known, &other, bv_timeline::VstFormat::Vst3).is_none());
    }

    #[test]
    fn a_folder_without_plugins_lists_none() {
        let dir = std::env::temp_dir().join("bananav-vst-empty");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("readme.txt"), "no plug-ins").unwrap();
        std::fs::write(dir.join("fake.dll"), "not a library").unwrap();
        let found = files(std::slice::from_ref(&dir));
        assert!(found.vst2.is_empty() && found.vst3.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Modules looked at: their effects listed with what they are, the
    /// failures with why (never offered), repeated names told apart.
    #[test]
    fn modules_looked_at_are_listed_by_their_effects() {
        use bv_vst3::scan::Failure;
        let effect = |name: &str, id: &str, error: Option<&str>| EffectInfo {
            class_id: id.into(),
            name: name.into(),
            sub_categories: "Fx|Delay".into(),
            inputs: 2,
            outputs: 2,
            programs: 3,
            error: error.map(str::to_string),
            ..EffectInfo::default()
        };
        let module = |path: &str, result| ModuleScan {
            path: PathBuf::from(path),
            modified: 0,
            size: 0,
            vendor: "Makers".into(),
            bits32: false,
            result,
        };
        let a = module(
            "C:/VST3/Plugs.vst3",
            Ok(vec![
                effect("Echo", "A1", None),
                effect("Gate", "A2", Some("it crashed in process")),
            ]),
        );
        let b = module("C:/VST3/More.vst3", Ok(vec![effect("Echo", "B1", None)]));
        let c = module(
            "C:/VST3/Broken.vst3",
            Err(Failure::Refused("the library would not load".into())),
        );
        let all = sorted([&a, &b, &c].into_iter().flat_map(listed).collect());
        let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Broken", "Echo", "Echo (Plugs)", "Gate"]);
        assert!(!all[0].usable() && all[0].failure.is_some());
        assert_eq!(all[1].path, Path::new("C:/VST3/More.vst3"));
        assert_eq!((all[1].io, all[1].programs), (Some((2, 2)), 3));
        assert!(all[2].usable());
        assert!(!all[3].usable());
        assert_eq!(all[2].key(), "A1");
    }
}
