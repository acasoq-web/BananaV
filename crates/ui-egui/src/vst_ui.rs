//! VST effects in the interface — VST 3, and VST 2 through BananaV's own
//! compatibility (`bv-vst2`); 32-bit ones of either kind in BananaV's
//! 32-bit bridge: the
//! plug-ins found offered in the Plug-In Chooser (in Options › Preferences
//! › VST Effects' folders, those left out there not, nor those that failed
//! when tried), and an effect's page in the FX window — its parameters as
//! rows (BananaV's generic editor: sliders with the plug-in's own texts,
//! check boxes for its switches), its programs (the presets it holds) in
//! the window's preset list, its settings opened from and saved to the
//! standard preset files (`.vstpreset`; `.fxp` and `.fxb`, in the standard
//! folders: other programs find them), and its own editor — inside
//! the FX window, as VEGAS shows it (Windows: [`embedded`]; a 32-bit
//! plug-in's from its bridge's process), or in a window of its own
//! elsewhere —, unless Preferences' "Generic VST editor" asks for BananaV's
//! rows alone.
//!
//! The page's instance has the effect's state and values; what the user
//! changes — on the rows or in the plug-in's window — goes back to the
//! effect, its state read again when a gesture ends. The mixer runs
//! instances of its own (`bv_mixer`'s `vst.rs`), following the effect.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bv_timeline::{Effect, VstFormat, VstFx};
use bv_vst3::{Instance, ParamInfo, restart};

use crate::egui::{self, RichText, Ui};
use crate::fx_window::FxTarget;
use crate::i18n::tr;
use crate::theme;
use crate::vst::VstPlugin;

/// The pages shown, and the context the plug-ins' search repaints.
#[derive(Debug, Default)]
pub(crate) struct VstUi {
    ctx: Option<egui::Context>,
    pages: HashMap<(FxTarget, usize), Page>,
}

impl VstUi {
    pub(crate) fn new(ctx: egui::Context) -> VstUi {
        VstUi {
            ctx: Some(ctx),
            ..VstUi::default()
        }
    }

    /// Starts looking for the plug-ins in `folders` (in the background:
    /// the cache's known at once).
    pub(crate) fn search(&self, folders: &[PathBuf]) {
        crate::vst::plugins(folders, false, self.ctx.as_ref());
    }

    /// The VST effects found in `folders` (searched in the background the
    /// first time, and again when the folders change), less those
    /// `left_out` (by file) or turned `off` (by class), and those that
    /// failed when tried. While the search goes on, `ctx` is repainted.
    pub(crate) fn effects(
        &mut self,
        ctx: &egui::Context,
        folders: &[PathBuf],
        left_out: &[PathBuf],
        off: &[String],
    ) -> Vec<VstPlugin> {
        let found = crate::vst::plugins(folders, false, Some(ctx));
        if found.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        found
            .plugins
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.usable() && !left_out.contains(&p.path) && !off.contains(&p.key()))
            .collect()
    }

    /// The programs of the plug-in on the page of `target`'s effect
    /// `index` (shown before), and the one it has now.
    pub(crate) fn programs(&self, target: FxTarget, index: usize) -> (Vec<String>, Option<usize>) {
        let Some(page) = self.pages.get(&(target, index)) else {
            return (Vec::new(), None);
        };
        (page.programs.clone(), page.program())
    }

    /// Chooses program `program` of the plug-in on the page of `target`'s
    /// effect `index`: the effect takes the plug-in's state then (its
    /// values with it). Whether it could.
    pub(crate) fn choose_program(
        &mut self,
        target: FxTarget,
        index: usize,
        program: usize,
        v: &mut VstFx,
    ) -> bool {
        let Some(page) = self.pages.get_mut(&(target, index)) else {
            return false;
        };
        let Some(inst) = page.instance.as_mut() else {
            return false;
        };
        if !inst.set_program(program) {
            return false;
        }
        // The state holds the program's values: none of the effect's own
        // stays over them.
        v.params.clear();
        page.known.clear();
        page.asked.clear();
        page.read_state(v);
        true
    }

    /// Closes the pages not drawn since the last call (and their plug-ins'
    /// windows): once a frame, after the FX window. While a plug-in's
    /// window is open, what its editor put on the run loop runs (Linux's
    /// timers and file descriptors), frames following one another.
    pub(crate) fn frame_end(&mut self, ctx: &egui::Context) {
        self.pages.retain(|_, p| std::mem::take(&mut p.drawn));
        if self.window_open() {
            bv_vst3::run_loop();
            if cfg!(target_os = "linux") {
                ctx.request_repaint_after(std::time::Duration::from_millis(15));
            } else {
                // VST 2 editors' idle time, and a bridged editor's news.
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            }
        }
    }

    /// Whether a page's plug-in window is open (for tests).
    pub(crate) fn window_open(&self) -> bool {
        self.pages
            .values()
            .any(|p| p.window.is_some() || p.bridged_window || p.embedded.is_some())
    }

    /// Whether the plug-in on the page of `target`'s effect `index` keeps
    /// banks of its programs (a VST 2 plug-in's `.fxb`).
    pub(crate) fn keeps_banks(&self, target: FxTarget, index: usize) -> bool {
        matches!(
            self.pages
                .get(&(target, index))
                .and_then(|p| p.instance.as_ref()),
            Some(Plug::V2(_))
        )
    }

    /// The preset bar's VST files for the plug-in on the page of
    /// `target`'s effect `index`, as VEGAS's: opened into it — the effect
    /// `v` with its settings then given back — or saved from it. What to
    /// tell the user, if anything.
    pub(crate) fn file(
        &mut self,
        target: FxTarget,
        index: usize,
        v: &VstFx,
        what: VstFile,
    ) -> (Option<VstFx>, Option<String>) {
        let Some(page) = self.pages.get_mut(&(target, index)) else {
            return (None, None);
        };
        let bank = matches!(what, VstFile::OpenBank | VstFile::SaveBank);
        match what {
            VstFile::OpenPreset | VstFile::OpenBank => {
                let mut new = v.clone();
                match page.open_preset_file(&mut new, bank) {
                    Ok(Some(path)) => (Some(new), Some(trf!("Opened {}", path.display()))),
                    Ok(None) => (None, None),
                    Err(e) => (None, Some(trf!("Could not open the preset: {}", e))),
                }
            }
            VstFile::SavePreset | VstFile::SaveBank => match page.save_preset_file(v, bank) {
                Ok(Some(path)) => (None, Some(trf!("Saved to {}", path.display()))),
                Ok(None) => (None, None),
                Err(e) => (None, Some(trf!("Could not save the preset: {}", e))),
            },
        }
    }
}

/// The preset bar's VST files, as VEGAS's: a program's preset and a bank
/// of programs, opened and saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VstFile {
    OpenPreset,
    SavePreset,
    OpenBank,
    SaveBank,
}

/// The plug-in's editor inside the FX window: the child window it lives
/// in, the size its plug-in asked for (one here), the size it takes, and
/// where it was put last (pixels in the FX window's client area).
struct Embedded {
    window: bv_platform::EmbeddedWindow,
    asked: Arc<Mutex<Option<(i32, i32)>>>,
    size: (i32, i32),
    placed: Option<[i32; 4]>,
    shown: bool,
}

/// A new effect of plug-in `p`.
pub(crate) fn new_effect(p: &VstPlugin) -> Effect {
    Effect::vst(VstFx {
        format: if p.vst3 {
            VstFormat::Vst3
        } else {
            VstFormat::Vst2
        },
        path: p.path.clone(),
        class_id: p.class_id.clone(),
        name: p.name.clone(),
        vendor: p.vendor.clone(),
        ..VstFx::default()
    })
}

/// A VST 2 plug-in's parameters as VST 3's are described (one list for
/// both kinds): by index, continuous, their defaults the values they have.
fn vst2_params(inst: &bv_vst2::Instance) -> Vec<ParamInfo> {
    inst.params()
        .into_iter()
        .map(|p| ParamInfo {
            id: p.index,
            title: p.name.clone(),
            short_title: p.name,
            units: p.label,
            steps: 0,
            default: inst.value(p.index).map_or(0.0, f64::from),
            unit: 0,
            flags: 0,
        })
        .collect()
}

/// A VST effect's parameters as its plug-in describes them (FX Automation
/// lists them): read once from an instance of the plug-in made for that,
/// by module and class.
pub(crate) fn param_infos(v: &VstFx) -> Vec<ParamInfo> {
    use std::sync::Mutex;
    /// Each module's and class's parameters.
    type Known = HashMap<(PathBuf, String), Vec<ParamInfo>>;
    static KNOWN: Mutex<Option<Known>> = Mutex::new(None);
    let key = (v.path.clone(), v.class_id.clone());
    if let Ok(k) = KNOWN.lock()
        && let Some(p) = k.as_ref().and_then(|m| m.get(&key))
    {
        return p.clone();
    }
    let read = match v.format {
        VstFormat::Vst3 if bv_vst3::is_32bit(&v.path) => {
            bv_vst3::Bridged::spawn(&v.path, &v.class_id, true)
                .ok()
                .map(|b| b.params())
        }
        VstFormat::Vst3 => bv_vst3::load(&v.path).ok().and_then(|m| {
            let class = m.effect(&v.class_id).cloned()?;
            Instance::new(&m, &class, true).ok().map(|i| i.params())
        }),
        VstFormat::Vst2 => bv_vst2::Instance::new(&v.path, bv_vst2::shell_id(&v.class_id))
            .ok()
            .map(|i| vst2_params(&i)),
    };
    let params = read.unwrap_or_default();
    if let Ok(mut k) = KNOWN.lock() {
        k.get_or_insert_with(HashMap::new)
            .insert(key, params.clone());
    }
    params
}

/// What a plug-in's editor reported, of either kind.
enum PlugEdit {
    /// A parameter's new value (normalized); `queue`: the processor is
    /// still to hear it (a VST 3 controller's).
    Perform(u32, f64, bool),
    /// Its parameters' names or values changed: read them again.
    Refresh,
    /// A gesture ended (its state is to be read).
    End,
}

/// A VST 3 editor's reports as the page takes them.
fn v3_edits(edits: Vec<bv_vst3::Edit>) -> Vec<PlugEdit> {
    edits
        .into_iter()
        .filter_map(|e| match e {
            bv_vst3::Edit::Perform(id, v) => Some(PlugEdit::Perform(id, v, true)),
            bv_vst3::Edit::Restart(flags) => Some(
                if flags & (restart::PARAM_VALUES_CHANGED | restart::PARAM_TITLES_CHANGED) != 0 {
                    PlugEdit::Refresh
                } else {
                    PlugEdit::End
                },
            ),
            bv_vst3::Edit::End(_) | bv_vst3::Edit::Dirty => Some(PlugEdit::End),
            bv_vst3::Edit::Begin(_) => None,
        })
        .collect()
}

/// A page's plug-in, of either kind; a 32-bit VST 3 module in the 32-bit
/// bridge (a 32-bit VST 2 plug-in's instance goes there by itself).
#[derive(Debug)]
enum Plug {
    V3(Box<Instance>),
    V3Bridged(Box<bv_vst3::Bridged>),
    V2(bv_vst2::Instance),
}

impl Plug {
    fn crashed(&self) -> bool {
        match self {
            Plug::V3(i) => i.crashed(),
            Plug::V3Bridged(i) => i.crashed(),
            Plug::V2(i) => i.crashed(),
        }
    }

    /// A 32-bit plug-in in the bridge: each call a request to its process,
    /// its editor in a window of the bridge's own.
    fn bridged(&self) -> bool {
        match self {
            Plug::V3(_) => false,
            Plug::V3Bridged(_) => true,
            Plug::V2(i) => i.bridged(),
        }
    }

    /// A VST 3 plug-in's class.
    fn class3(&self) -> Option<&bv_vst3::ClassInfo> {
        match self {
            Plug::V3(i) => Some(i.class()),
            Plug::V3Bridged(i) => Some(i.class()),
            Plug::V2(_) => None,
        }
    }

    fn params(&self) -> Vec<ParamInfo> {
        match self {
            Plug::V3(i) => i.params(),
            Plug::V3Bridged(i) => i.params(),
            Plug::V2(i) => vst2_params(i),
        }
    }

    fn programs(&self) -> Vec<String> {
        match self {
            Plug::V3(i) => i.programs(),
            Plug::V3Bridged(i) => i.programs(),
            Plug::V2(i) => i.programs(),
        }
    }

    fn value(&self, id: u32) -> Option<f64> {
        match self {
            Plug::V3(i) => i.value(id),
            Plug::V3Bridged(i) => i.value(id),
            Plug::V2(i) => i.value(id).map(f64::from),
        }
    }

    /// Value `value` of parameter `id` as the plug-in writes it (a VST 2
    /// plug-in writes its current value only).
    fn text(&self, id: u32, value: f64) -> Option<String> {
        match self {
            Plug::V3(i) => i.text(id, value),
            Plug::V3Bridged(i) => i.text(id, value),
            Plug::V2(i) => Some(i.display(id)),
        }
    }

    fn set(&mut self, id: u32, value: f64) {
        match self {
            Plug::V3(i) => i.set(id, value),
            Plug::V3Bridged(i) => i.set(id, value),
            Plug::V2(i) => i.set(id, value),
        }
    }

    fn queue(&mut self, id: u32, value: f64) {
        match self {
            Plug::V3(i) => i.queue(id, value),
            Plug::V3Bridged(i) => i.queue(id, value),
            Plug::V2(i) => i.queue(id, value),
        }
    }

    fn flush(&mut self) {
        match self {
            Plug::V3(i) => i.flush(),
            Plug::V3Bridged(i) => i.flush(),
            Plug::V2(i) => i.flush(),
        }
    }

    fn set_program(&mut self, index: usize) -> bool {
        match self {
            Plug::V3(i) => i.set_program(index),
            Plug::V3Bridged(i) => i.set_program(index),
            Plug::V2(i) => i.set_program(index),
        }
    }

    fn set_state(&mut self, state: &[u8], controller: &[u8]) -> bool {
        match self {
            Plug::V3(i) => i.set_state(state, controller),
            Plug::V3Bridged(i) => i.set_state(state, controller),
            Plug::V2(i) => i.set_state(state),
        }
    }

    fn state(&self) -> Option<Vec<u8>> {
        match self {
            Plug::V3(i) => i.state(),
            Plug::V3Bridged(i) => i.state(),
            Plug::V2(i) => i.state(),
        }
    }

    fn controller_state(&self) -> Option<Vec<u8>> {
        match self {
            Plug::V3(i) => i.controller_state(),
            Plug::V3Bridged(i) => i.controller_state(),
            Plug::V2(_) => None,
        }
    }

    fn close_editor(&mut self) {
        match self {
            Plug::V3(i) => i.close_editor(),
            Plug::V3Bridged(i) => i.close_editor(),
            Plug::V2(i) => i.close_editor(),
        }
    }

    fn take_edits(&self) -> Vec<PlugEdit> {
        match self {
            Plug::V3(i) => v3_edits(i.take_edits()),
            Plug::V3Bridged(i) => v3_edits(i.take_edits()),
            Plug::V2(i) => i
                .take_edits()
                .into_iter()
                .filter_map(|e| match e {
                    bv_vst2::Edit::Perform(id, v) => {
                        Some(PlugEdit::Perform(id, f64::from(v), false))
                    }
                    bv_vst2::Edit::Changed => Some(PlugEdit::Refresh),
                    bv_vst2::Edit::End(_) => Some(PlugEdit::End),
                    bv_vst2::Edit::Begin(_) => None,
                })
                .collect(),
        }
    }
}

/// How long a bridged plug-in's parameter values are kept before they are
/// asked again (its own changes — meters, its processing's — show then).
const ASKED_FOR: std::time::Duration = std::time::Duration::from_millis(250);

/// What a bridged plug-in answered for the page's rows: each answer a
/// request to its process, the rows drawn every frame. Its values are kept
/// a moment (and follow what the page sets and its editor reports); its
/// texts by value, until its parameters, state or program change.
#[derive(Debug, Default)]
struct Asked {
    values: HashMap<u32, f64>,
    since: Option<std::time::Instant>,
    texts: HashMap<(u32, u64), Option<String>>,
}

/// Texts kept at most (then forgotten all at once).
const ASKED_TEXTS: usize = 4096;

impl Asked {
    /// Everything forgotten (its parameters, state or program changed).
    fn clear(&mut self) {
        self.values.clear();
        self.since = None;
        self.texts.clear();
    }

    /// Parameter `id`'s value is `value` (set by the page, or reported).
    fn note(&mut self, id: u32, value: f64) {
        self.values.insert(id, value);
    }

    fn value(&mut self, inst: &Plug, id: u32) -> Option<f64> {
        if !inst.bridged() {
            return inst.value(id);
        }
        if self.since.is_none_or(|t| t.elapsed() > ASKED_FOR) {
            self.values.clear();
            self.since = Some(std::time::Instant::now());
        }
        if let Some(&v) = self.values.get(&id) {
            return Some(v);
        }
        let v = inst.value(id)?;
        self.values.insert(id, v);
        Some(v)
    }

    fn text(&mut self, inst: &Plug, id: u32, value: f64) -> Option<String> {
        if !inst.bridged() {
            return inst.text(id, value);
        }
        if self.texts.len() >= ASKED_TEXTS {
            self.texts.clear();
        }
        self.texts
            .entry((id, value.to_bits()))
            .or_insert_with(|| inst.text(id, value))
            .clone()
    }
}

/// What a page keeps: its plug-in's instance (or why there is none), the
/// parameters it shows, the state and values it has from the effect, and
/// the plug-in's window.
struct Page {
    identity: (PathBuf, String),
    instance: Option<Plug>,
    error: Option<String>,
    /// What the last preset file opened or saved said.
    message: Option<String>,
    params: Vec<ParamInfo>,
    /// The plug-in's programs.
    programs: Vec<String>,
    state: Vec<u8>,
    known: BTreeMap<u32, f64>,
    /// A bridged plug-in's answers for the rows.
    asked: Asked,
    /// After the instance (closed first: the plug-in's view leaves before
    /// its window goes).
    window: Option<bv_platform::PluginWindow>,
    /// A 32-bit plug-in's editor open in its bridge's own window.
    bridged_window: bool,
    /// Its editor inside the FX window (before the instance goes: the
    /// editor is closed first).
    embedded: Option<Embedded>,
    /// The plug-in has an editor (asked once).
    has_editor: Option<bool>,
    /// Its editor would not go inside the FX window: the rows show.
    embed_failed: bool,
    /// The plug-in's window was opened once by the page (it is not opened
    /// again by itself).
    opened: bool,
    drawn: bool,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("identity", &self.identity)
            .field("error", &self.error)
            .finish()
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        if let Some(i) = &mut self.instance {
            i.close_editor();
        }
        self.embedded = None;
        self.window = None;
    }
}

/// The longest block the page's instance is set up for (it only takes
/// parameter changes: no sound goes through it).
const BLOCK: usize = 1024;

/// The standard folder of `vendor`'s `plugin`'s preset files: under
/// `Documents/VST3 Presets` (VST 2: `Documents/VST2 Presets`) on Windows,
/// `~/Library/Audio/Presets` on macOS, `~/.vst3/presets`
/// (`~/.vst/presets`) on Linux.
fn presets_folder(format: VstFormat, vendor: &str, plugin: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let base = bv_platform::home_dir()?
            .join("Library")
            .join("Audio")
            .join("Presets");
        return Some(bv_vst3::preset::folder(&base, vendor, plugin));
    }
    let base = match (cfg!(windows), format) {
        (true, VstFormat::Vst3) => {
            bv_platform::user_dir(bv_platform::UserDir::Documents)?.join("VST3 Presets")
        }
        (true, VstFormat::Vst2) => {
            bv_platform::user_dir(bv_platform::UserDir::Documents)?.join("VST2 Presets")
        }
        (false, VstFormat::Vst3) => bv_platform::home_dir()?.join(".vst3").join("presets"),
        (false, VstFormat::Vst2) => bv_platform::home_dir()?.join(".vst").join("presets"),
    };
    Some(bv_vst3::preset::folder(&base, vendor, plugin))
}

/// The page's plug-in made from effect `v`, set up, with the effect's
/// state and values.
fn make(v: &VstFx, rate: f64) -> Result<Plug, String> {
    // Read from another editor's project, its plug-in not found here.
    if v.path.as_os_str().is_empty() {
        return Err(tr("it is not installed here").to_string());
    }
    match v.format {
        VstFormat::Vst3 if bv_vst3::is_32bit(&v.path) => {
            let mut inst =
                bv_vst3::Bridged::spawn(&v.path, &v.class_id, true).map_err(|e| e.to_string())?;
            if !v.state.is_empty() {
                inst.set_state(&v.state, &v.controller_state);
            }
            inst.setup(rate, BLOCK, 2).map_err(|e| e.to_string())?;
            for (&id, &value) in &v.params {
                inst.set(id, value);
            }
            inst.flush();
            Ok(Plug::V3Bridged(Box::new(inst)))
        }
        VstFormat::Vst3 => {
            let m = bv_vst3::load(&v.path).map_err(|e| e.to_string())?;
            let class = m
                .effect(&v.class_id)
                .cloned()
                .ok_or_else(|| bv_vst3::Error::NoClass.to_string())?;
            let mut inst = Instance::new(&m, &class, true).map_err(|e| e.to_string())?;
            if !v.state.is_empty() {
                inst.set_state(&v.state, &v.controller_state);
            }
            inst.setup(rate, BLOCK, 2).map_err(|e| e.to_string())?;
            for (&id, &value) in &v.params {
                inst.set(id, value);
            }
            inst.flush();
            Ok(Plug::V3(Box::new(inst)))
        }
        VstFormat::Vst2 => {
            let mut inst = bv_vst2::Instance::new(&v.path, bv_vst2::shell_id(&v.class_id))
                .map_err(|e| e.to_string())?;
            if !v.state.is_empty() {
                inst.set_state(&v.state);
            }
            inst.setup(rate, BLOCK, 2).map_err(|e| e.to_string())?;
            for (&id, &value) in &v.params {
                inst.set(id, value);
            }
            Ok(Plug::V2(inst))
        }
    }
}

impl Page {
    fn open(v: &VstFx, rate: f64) -> Page {
        let (instance, error, params, programs) = match make(v, rate) {
            Ok(inst) => {
                let params = inst.params();
                let programs = inst.programs();
                (Some(inst), None, params, programs)
            }
            Err(why) => (None, Some(why), Vec::new(), Vec::new()),
        };
        Page {
            identity: (v.path.clone(), v.class_id.clone()),
            instance,
            error,
            message: None,
            params,
            programs,
            state: v.state.clone(),
            known: v.params.clone(),
            asked: Asked::default(),
            window: None,
            bridged_window: false,
            embedded: None,
            has_editor: None,
            embed_failed: false,
            opened: false,
            drawn: true,
        }
    }

    /// Whether the plug-in has an editor (asked once).
    fn editor_here(&mut self) -> bool {
        let instance = &mut self.instance;
        *self.has_editor.get_or_insert_with(|| match instance {
            Some(Plug::V3(i)) => i.has_editor(),
            Some(Plug::V3Bridged(i)) => i.has_editor(),
            Some(Plug::V2(i)) => i.has_editor(),
            None => false,
        })
    }

    /// Opens the plug-in's editor inside window `parent` (the FX window, a
    /// system's handle): a child window made for it there. Whether it
    /// could.
    #[allow(unsafe_code)]
    fn open_embedded(&mut self, parent: isize, title: &str) -> bool {
        let Some(plug) = self.instance.as_mut() else {
            return false;
        };
        let Some(window) = bv_platform::EmbeddedWindow::open(parent, 0, 0, 400, 300) else {
            return false;
        };
        let asked: Arc<Mutex<Option<(i32, i32)>>> = Arc::new(Mutex::new(None));
        // What the plug-in asks for: its window resized at once (it wants
        // it so when the call returns), the page's room next frame.
        let ask = {
            let asked = Arc::clone(&asked);
            let sizer = window.sizer();
            move |w: i32, h: i32| {
                sizer.set_size(w, h);
                *asked.lock().unwrap_or_else(|e| e.into_inner()) = Some((w, h));
            }
        };
        let ok = match plug {
            Plug::V3(inst) => {
                let Some(ed) = inst.editor() else {
                    return false;
                };
                if !ed.supports(bv_vst3::PLATFORM_NATIVE) {
                    inst.close_editor();
                    return false;
                }
                let size = ed.size().unwrap_or((400, 300));
                *asked.lock().unwrap_or_else(|e| e.into_inner()) = Some(size);
                // SAFETY: the window lives until the editor is closed: the
                // page closes the editor before it lets the window go
                // (`Page`'s drop, `close_embedded`).
                let ok =
                    unsafe { ed.attach(window.handle(), bv_vst3::PLATFORM_NATIVE, Box::new(ask)) };
                if !ok {
                    inst.close_editor();
                }
                ok
            }
            Plug::V3Bridged(inst) => {
                inst.has_editor() && inst.open_editor(title, Some(window.handle() as isize))
            }
            Plug::V2(inst) => {
                // SAFETY: as above.
                inst.has_editor()
                    && unsafe { inst.open_editor(window.handle(), Box::new(ask), title) }
            }
        };
        if !ok {
            return false;
        }
        let size = self.editor_size(&asked).unwrap_or((400, 300));
        window.sizer().set_size(size.0, size.1);
        self.embedded = Some(Embedded {
            window,
            asked,
            size,
            placed: None,
            shown: true,
        });
        true
    }

    /// The size the plug-in's editor takes: what it asked for here, else
    /// what it tells (a bridged one's, as its bridge told last).
    fn editor_size(&mut self, asked: &Mutex<Option<(i32, i32)>>) -> Option<(i32, i32)> {
        let asked = *asked.lock().unwrap_or_else(|e| e.into_inner());
        let told = match self.instance.as_mut()? {
            Plug::V3(i) => i.editor().and_then(|e| e.size()),
            Plug::V3Bridged(i) => i.editor_size(),
            Plug::V2(i) => i.editor_size(),
        };
        let size = match self.instance.as_ref()? {
            // A bridged plug-in's asks come with its bridge's answers.
            Plug::V3Bridged(_) => told.or(asked),
            Plug::V2(i) if i.bridged() => told.or(asked),
            _ => asked.or(told),
        };
        size.filter(|&(w, h)| w > 0 && h > 0)
    }

    /// A frame of the editor inside the FX window: a VST 2 editor's idle
    /// time, a bridged one's news. Whether it is still open.
    fn follow_embedded(&mut self) -> bool {
        match self.instance.as_mut() {
            Some(Plug::V2(i)) => i.follow_editor(),
            Some(Plug::V3Bridged(i)) => i.poll_editor(),
            Some(Plug::V3(_)) => true,
            None => false,
        }
    }

    /// Closes the editor inside the FX window (the editor first, then its
    /// window).
    fn close_embedded(&mut self) {
        if let Some(i) = self.instance.as_mut() {
            i.close_editor();
        }
        self.embedded = None;
    }

    /// Takes the editor's room in `ui` and puts the editor there (hidden
    /// while an egui menu is open over the window: a system's window would
    /// cover it). The room, in points.
    fn place_embedded(&mut self, ui: &mut Ui) -> egui::Vec2 {
        let asked = match &self.embedded {
            Some(emb) => Arc::clone(&emb.asked),
            None => return egui::Vec2::ZERO,
        };
        let size = self.editor_size(&asked);
        let Some(emb) = self.embedded.as_mut() else {
            return egui::Vec2::ZERO;
        };
        if let Some(size) = size {
            emb.size = size;
        }
        let ppp = ui.ctx().pixels_per_point();
        let (w, h) = emb.size;
        // The editor's measure: pixels, or points on macOS (where only the
        // interface's zoom lies between them and egui's points).
        let unit = if bv_platform::EDITOR_POINTS {
            ui.ctx().zoom_factor()
        } else {
            ppp
        };
        let room = egui::vec2(w as f32 / unit, h as f32 / unit);
        let (rect, _) = ui.allocate_exact_size(room, egui::Sense::hover());
        let place = [
            (rect.min.x * ppp).round() as i32,
            (rect.min.y * ppp).round() as i32,
            w,
            h,
        ];
        if emb.placed != Some(place) {
            emb.window.place(place[0], place[1], place[2], place[3]);
            emb.placed = Some(place);
        }
        let show = !egui::Popup::is_any_open(ui.ctx()) && !crate::floating::modal(ui.ctx());
        if show != emb.shown {
            emb.window.set_visible(show);
            emb.shown = show;
        }
        room
    }

    /// The program the plug-in has now: a VST 3 plug-in's program
    /// parameter's, a VST 2 plug-in's own.
    fn program(&self) -> Option<usize> {
        let inst = self.instance.as_ref()?;
        if let Plug::V2(i) = inst {
            return i.program();
        }
        let p = self.params.iter().find(|p| p.program_change())?;
        let count = usize::try_from(p.steps).ok().filter(|&s| s > 0)? + 1;
        let v = inst.value(p.id)?;
        Some(((v * (count - 1) as f64).round().max(0.0) as usize).min(count - 1))
    }

    /// Saves the plug-in's settings as a preset file (the standard one of
    /// its kind) — all its programs as a bank (`.fxb`) when `bank` —, in
    /// the standard folder unless the user picks another. The file saved,
    /// if any.
    fn save_preset_file(&mut self, v: &VstFx, bank: bool) -> Result<Option<PathBuf>, String> {
        let Some(inst) = self.instance.as_mut() else {
            return Ok(None);
        };
        if bank {
            let Plug::V2(i) = inst else {
                return Err(tr("A VST 3 plug-in keeps no banks.").to_string());
            };
            let bytes = i
                .bank_file()
                .ok_or_else(|| tr("The plug-in gave no settings.").to_string())?;
            let extension = bv_vst2::preset::BANK_EXTENSION;
            let vendor = i.describe().vendor;
            let mut dialog = rfd::FileDialog::new()
                .set_title(tr("Save VST Bank As"))
                .add_filter(tr("VST 2 bank"), &[extension])
                .set_file_name(format!(
                    "{}.{extension}",
                    bv_vst3::preset::file_name(&v.name)
                ));
            if let Some(dir) = presets_folder(v.format, &vendor, &v.name) {
                let _ = std::fs::create_dir_all(&dir);
                dialog = dialog.set_directory(dir);
            }
            let Some(path) = dialog.save_file() else {
                return Ok(None);
            };
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            return Ok(Some(path));
        }
        let inst = &*inst;
        let (bytes, extension, filter, vendor) = match (inst, inst.class3()) {
            (Plug::V3(_) | Plug::V3Bridged(_), Some(class)) => {
                let component = inst
                    .state()
                    .ok_or_else(|| tr("The plug-in gave no settings.").to_string())?;
                let controller = inst.controller_state().unwrap_or_default();
                let info = bv_vst3::preset::Info {
                    plugin: class.name.clone(),
                    category: class.sub_categories.clone(),
                    name: v.name.clone(),
                };
                (
                    bv_vst3::preset::write(&class.id(), &component, &controller, &info),
                    bv_vst3::preset::EXTENSION,
                    tr("VST 3 preset"),
                    class.vendor.clone(),
                )
            }
            (Plug::V2(i), _) => (
                i.preset_file()
                    .ok_or_else(|| tr("The plug-in gave no settings.").to_string())?,
                bv_vst2::preset::PROGRAM_EXTENSION,
                tr("VST 2 preset"),
                i.describe().vendor,
            ),
            _ => return Ok(None),
        };
        let folder = presets_folder(v.format, &vendor, &v.name);
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr("Save VST Preset As"))
            .add_filter(filter, &[extension])
            .set_file_name(format!(
                "{}.{extension}",
                bv_vst3::preset::file_name(&v.name)
            ));
        if let Some(dir) = &folder {
            let _ = std::fs::create_dir_all(dir);
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else {
            return Ok(None);
        };
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(Some(path))
    }

    /// Opens a preset file — a bank (`.fxb`) when `bank` — into the
    /// plug-in and the effect: its states, its values with them. The file
    /// opened, if any.
    fn open_preset_file(&mut self, v: &mut VstFx, bank: bool) -> Result<Option<PathBuf>, String> {
        let Some(inst) = self.instance.as_mut() else {
            return Ok(None);
        };
        let (title, filter, extensions, vendor): (&str, &str, &[&str], String) = match &*inst {
            Plug::V3(_) | Plug::V3Bridged(_) if bank => {
                return Err(tr("A VST 3 plug-in keeps no banks.").to_string());
            }
            Plug::V3(_) | Plug::V3Bridged(_) => (
                tr("Open VST Preset"),
                tr("VST 3 preset"),
                &[bv_vst3::preset::EXTENSION],
                inst.class3().map(|c| c.vendor.clone()).unwrap_or_default(),
            ),
            Plug::V2(i) if bank => (
                tr("Open VST Bank"),
                tr("VST 2 bank"),
                &[bv_vst2::preset::BANK_EXTENSION],
                i.describe().vendor,
            ),
            Plug::V2(i) => (
                tr("Open VST Preset"),
                tr("VST 2 preset"),
                &[bv_vst2::preset::PROGRAM_EXTENSION],
                i.describe().vendor,
            ),
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .add_filter(filter, extensions);
        if let Some(dir) = presets_folder(v.format, &vendor, &v.name).filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.pick_file() else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let taken = match inst {
            Plug::V3(_) | Plug::V3Bridged(_) => {
                let preset = bv_vst3::preset::read(&bytes)
                    .ok_or_else(|| tr("The file is no VST 3 preset.").to_string())?;
                let id = inst.class3().map(|c| c.id()).unwrap_or_default();
                if !preset.is_for(&id) {
                    return Err(tr("The preset is for another plug-in.").to_string());
                }
                inst.set_state(&preset.component, &preset.controller)
            }
            Plug::V2(i) => {
                let preset = bv_vst2::preset::read(&bytes)
                    .ok_or_else(|| tr("The file is no VST 2 preset.").to_string())?;
                if !preset.is_for(i.describe().unique_id) {
                    return Err(tr("The preset is for another plug-in.").to_string());
                }
                i.set_state(&bytes)
            }
        };
        if !taken {
            return Err(tr("The plug-in refused the preset.").to_string());
        }
        // The state holds the preset's values: none of the effect's own
        // stays over them.
        v.params.clear();
        self.known.clear();
        self.asked.clear();
        self.params = inst.params();
        self.programs = inst.programs();
        self.read_state(v);
        Ok(Some(path))
    }

    /// Shows the plug-in's own editor in a window of its own; whether it
    /// could.
    #[allow(unsafe_code)]
    fn open_window(&mut self, title: &str) -> bool {
        let Some(plug) = self.instance.as_mut() else {
            return false;
        };
        match plug {
            Plug::V3(inst) => {
                let Some(ed) = inst.editor() else {
                    return false;
                };
                // The system's kind of window: Windows' HWND, macOS's NSView,
                // X11's window id.
                let platform = bv_vst3::PLATFORM_NATIVE;
                if !ed.supports(platform) {
                    inst.close_editor();
                    return false;
                }
                let (w, h) = ed.size().unwrap_or((400, 300));
                let Some(win) = bv_platform::PluginWindow::open(title, w, h, ed.resizable()) else {
                    inst.close_editor();
                    return false;
                };
                let resizer = win.resizer();
                // SAFETY: the window lives until the editor is closed: the
                // page closes the editor before it lets the window go
                // (`Page`'s drop, `follow_window`).
                let ok = unsafe {
                    ed.attach(
                        win.handle(),
                        platform,
                        Box::new(move |w, h| resizer.set_client_size(w, h)),
                    )
                };
                if !ok {
                    inst.close_editor();
                    return false;
                }
                win.show();
                self.window = Some(win);
                true
            }
            // A 32-bit plug-in's editor in its bridge's own window.
            Plug::V3Bridged(inst) => {
                let ok = inst.has_editor() && inst.open_editor(title, None);
                self.bridged_window = ok;
                ok
            }
            Plug::V2(inst) if inst.bridged() => {
                // SAFETY: no window of BananaV's is given.
                let ok =
                    unsafe { inst.open_editor(std::ptr::null_mut(), Box::new(|_, _| {}), title) };
                self.bridged_window = ok;
                ok
            }
            Plug::V2(inst) => {
                if !inst.has_editor() {
                    return false;
                }
                let (w, h) = inst.editor_size().unwrap_or((400, 300));
                let Some(win) = bv_platform::PluginWindow::open(title, w, h, false) else {
                    return false;
                };
                let resizer = win.resizer();
                // SAFETY: as above.
                let ok = unsafe {
                    inst.open_editor(
                        win.handle(),
                        Box::new(move |w, h| resizer.set_client_size(w, h)),
                        title,
                    )
                };
                if !ok {
                    return false;
                }
                // Some plug-ins only know their size once open.
                if let Some((w, h)) = inst.editor_size() {
                    win.resizer().set_client_size(w, h);
                }
                win.show();
                self.window = Some(win);
                true
            }
        }
    }

    /// The plug-in's window: closed by its button, resized by the user;
    /// a VST 2 editor's idle time. Whether it closed.
    fn follow_window(&mut self) -> bool {
        let Some(plug) = self.instance.as_mut() else {
            return false;
        };
        if self.bridged_window {
            let open = match plug {
                Plug::V3Bridged(inst) => inst.poll_editor(),
                Plug::V2(inst) => inst.follow_editor(),
                Plug::V3(_) => true,
            };
            if !open {
                self.bridged_window = false;
                return true;
            }
            return false;
        }
        let Some(win) = &self.window else {
            return false;
        };
        if win.closed() {
            plug.close_editor();
            self.window = None;
            return true;
        }
        match plug {
            Plug::V3(inst) => {
                if let Some((w, h)) = win.take_resized()
                    && let Some(ed) = inst.editor()
                {
                    let taken = ed.resized(w, h);
                    if taken != (w, h) {
                        win.resizer().set_client_size(taken.0, taken.1);
                    }
                }
            }
            Plug::V2(inst) => {
                inst.follow_editor();
            }
            Plug::V3Bridged(_) => {}
        }
        false
    }

    /// Closes the plug-in's window (its editor first).
    fn close_window(&mut self) {
        if let Some(i) = self.instance.as_mut() {
            i.close_editor();
        }
        self.window = None;
        self.bridged_window = false;
    }

    /// The effect's state and values (undo, a preset) into the instance.
    fn follow_effect(&mut self, v: &VstFx) {
        let Some(inst) = self.instance.as_mut() else {
            return;
        };
        if v.state != self.state {
            if !v.state.is_empty() {
                inst.set_state(&v.state, &v.controller_state);
            }
            self.state = v.state.clone();
            self.known.clear();
            self.asked.clear();
        }
        let mut any = false;
        for (&id, &value) in &v.params {
            if self.known.get(&id) != Some(&value) {
                inst.set(id, value);
                self.known.insert(id, value);
                self.asked.note(id, value);
                any = true;
            }
        }
        if any {
            inst.flush();
        }
    }

    /// What the plug-in's editor reported into the effect; whether a
    /// gesture ended (its state is to be read).
    fn take_edits(&mut self, v: &mut VstFx) -> bool {
        let Some(inst) = self.instance.as_mut() else {
            return false;
        };
        let mut ended = false;
        let mut any = false;
        for edit in inst.take_edits() {
            match edit {
                PlugEdit::Perform(id, value, queue) => {
                    v.params.insert(id, value);
                    self.known.insert(id, value);
                    self.asked.note(id, value);
                    if queue {
                        inst.queue(id, value);
                        any = true;
                    }
                }
                PlugEdit::Refresh => {
                    self.asked.clear();
                    self.params = inst.params();
                    self.programs = inst.programs();
                    for p in &self.params {
                        if let Some(value) = inst.value(p.id) {
                            v.params.insert(p.id, value);
                            self.known.insert(p.id, value);
                        }
                    }
                    ended = true;
                }
                PlugEdit::End => ended = true,
            }
        }
        if any {
            inst.flush();
        }
        ended
    }

    /// The instance's states into the effect.
    fn read_state(&mut self, v: &mut VstFx) {
        let Some(inst) = self.instance.as_ref() else {
            return;
        };
        if let Some(s) = inst.state() {
            v.state = s.clone();
            self.state = s;
        }
        if let Some(c) = inst.controller_state() {
            v.controller_state = c;
        }
    }
}

/// A VST effect's page as VEGAS shows a plug-in with an editor of its
/// own: the editor itself, inside the FX window titled `parent` (Windows
/// and macOS), its room taken in `ui` — a 32-bit plug-in's too, from its
/// bridge's process. `None` when it cannot show so (no editor, the plug-in not
/// loaded or crashed): the rows show instead ([`page`]). Else the room it
/// takes, in points. Changes go into `e`.
pub(crate) fn embedded(
    ui: &mut Ui,
    vst: &mut VstUi,
    rate: f64,
    target: FxTarget,
    index: usize,
    e: &mut Effect,
    parent: &str,
) -> Option<egui::Vec2> {
    let v = e.vst.as_deref_mut()?;
    let key = (target, index);
    let identity = (v.path.clone(), v.class_id.clone());
    if vst.pages.get(&key).is_none_or(|p| p.identity != identity) {
        vst.pages.insert(key, Page::open(v, rate));
    }
    let page = vst.pages.get_mut(&key)?;
    let crashed = page.instance.as_ref().is_none_or(Plug::crashed);
    if crashed || page.embed_failed || !page.editor_here() {
        if page.embedded.is_some() {
            page.close_embedded();
        }
        return None;
    }
    page.drawn = true;
    // A window of its own open before (the rows' button): closed.
    if page.window.is_some() || page.bridged_window {
        page.close_window();
    }
    page.follow_effect(v);
    let mut ended = page.take_edits(v);
    if page.embedded.is_none() {
        match bv_platform::window_handle(parent) {
            Some(handle) => {
                if !page.open_embedded(handle, &v.name) {
                    page.embed_failed = true;
                    return None;
                }
            }
            // The FX window is made as this frame ends: room for now.
            None => {
                ui.ctx().request_repaint();
                let (_, _) = ui.allocate_exact_size(egui::vec2(400.0, 300.0), egui::Sense::hover());
                return Some(egui::vec2(400.0, 300.0));
            }
        }
    }
    if !page.follow_embedded() {
        page.close_embedded();
        ended = true;
    }
    let room = page.place_embedded(ui);
    if ended {
        page.read_state(v);
    }
    Some(room)
}

/// A VST effect's page in the FX window (`target`'s effect `index`):
/// changes go into `e`.
pub(crate) fn page(
    ui: &mut Ui,
    vst: &mut VstUi,
    generic: bool,
    rate: f64,
    target: FxTarget,
    index: usize,
    e: &mut Effect,
) {
    let Some(v) = e.vst.as_deref_mut() else {
        return;
    };
    let key = (target, index);
    let identity = (v.path.clone(), v.class_id.clone());
    if vst.pages.get(&key).is_none_or(|p| p.identity != identity) {
        vst.pages.insert(key, Page::open(v, rate));
    }
    let Some(page) = vst.pages.get_mut(&key) else {
        return;
    };
    page.drawn = true;
    page.follow_effect(v);
    let mut ended = page.take_edits(v);
    ended |= page.follow_window();
    // A 32-bit plug-in's editor lives in its bridge, outside BananaV's
    // windows: on any system that has the bridge.
    let windows = bv_platform::PLUGIN_WINDOWS || page.instance.as_ref().is_some_and(Plug::bridged);
    // The plug-in's own window, once, unless the generic editor is asked
    // for.
    if !page.opened && !generic && windows && page.instance.is_some() {
        page.opened = true;
        page.open_window(&v.name);
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new(&v.name).strong());
        if !v.vendor.is_empty() {
            ui.label(RichText::new(&v.vendor).color(theme::text_dim()));
        }
        if page.instance.as_ref().is_some_and(Plug::bridged) {
            ui.label(RichText::new(tr("32-bit")).color(theme::text_dim()))
                .on_hover_text(tr(
                    "A 32-bit plug-in: it runs in BananaV's 32-bit bridge, a process of its own.",
                ));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can = windows && page.instance.is_some();
            let open = page.window.is_some() || page.bridged_window;
            let text = if open {
                tr("Close Plug-In Window")
            } else {
                tr("Show Plug-In Window")
            };
            let r = ui.add_enabled(can, egui::Button::new(text));
            let r = if windows {
                r
            } else {
                r.on_disabled_hover_text(tr("Coming soon to BananaV"))
            };
            if r.clicked() {
                if open {
                    page.close_window();
                    ended = true;
                } else if !page.open_window(&v.name) {
                    page.error = Some(tr("The plug-in has no window of its own.").to_string());
                }
            }
        });
    });
    if let Some(why) = &page.error {
        ui.label(
            RichText::new(trf!("The plug-in could not be loaded: {}", why)).color(theme::error()),
        );
    }
    if let Some(note) = &page.message {
        ui.label(RichText::new(note).color(theme::text_dim()));
    }
    if page.instance.as_ref().is_some_and(Plug::crashed) {
        ui.label(
            RichText::new(tr(
                "The plug-in stopped working and was turned off until BananaV restarts: the sound goes through it untouched.",
            ))
            .color(theme::error()),
        );
    }
    ui.add_space(4.0);
    if let Some(inst) = page.instance.as_mut() {
        let shown: Vec<&ParamInfo> = page.params.iter().filter(|p| p.shown()).collect();
        if shown.is_empty() {
            ui.label(
                RichText::new(tr("This plug-in shows no parameters.")).color(theme::text_dim()),
            );
        }
        egui::Grid::new(("vst-params", target, index))
            .num_columns(3)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for p in shown {
                    let value = v
                        .params
                        .get(&p.id)
                        .copied()
                        .or_else(|| page.asked.value(inst, p.id))
                        .unwrap_or(p.default);
                    ui.label(&p.title);
                    let mut changed = None;
                    ui.add_enabled_ui(!p.read_only(), |ui| {
                        if p.steps == 1 {
                            let mut on = value >= 0.5;
                            if crate::widgets::checkbox(ui, &mut on, "").changed() {
                                changed = Some((f64::from(u8::from(on)), true));
                            }
                        } else {
                            let mut x = value;
                            let mut slider = egui::Slider::new(&mut x, 0.0..=1.0).show_value(false);
                            if p.steps > 1 {
                                slider = slider.step_by(1.0 / f64::from(p.steps));
                            }
                            let r = ui.add_sized([220.0, 18.0], slider);
                            if r.double_clicked()
                                || crate::widgets::bar_double_clicked(ui, &r, false)
                            {
                                x = p.default;
                            }
                            if x != value {
                                changed = Some((x, !r.dragged()));
                            }
                            if r.drag_stopped() {
                                ended = true;
                            }
                        }
                    });
                    let text = page.asked.text(inst, p.id, value).unwrap_or_default();
                    let units = if p.units.is_empty() {
                        text
                    } else {
                        format!("{text} {}", p.units)
                    };
                    ui.label(RichText::new(units).color(theme::text_dim()));
                    ui.end_row();
                    if let Some((x, done)) = changed {
                        inst.set(p.id, x);
                        inst.flush();
                        v.params.insert(p.id, x);
                        page.known.insert(p.id, x);
                        page.asked.note(p.id, x);
                        ended |= done;
                    }
                }
            });
    }
    if ended {
        page.read_state(v);
    }
}
