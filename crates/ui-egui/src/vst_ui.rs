//! VST 3 effects in the interface: the plug-ins found offered in the
//! Plug-In Chooser (in Options › Preferences › VST Effects' folders, those
//! left out there not), and an effect's page in the FX window — its
//! parameters as rows (BananaV's generic editor: sliders with the
//! plug-in's own texts, check boxes for its switches) and its own editor
//! in a window of its own (Windows), opened with the page unless
//! Preferences' "Generic VST editor" asks for BananaV's rows alone.
//!
//! The page's instance has the effect's state and values; what the user
//! changes — on the rows or in the plug-in's window — goes back to the
//! effect, its state read again when a gesture ends. The mixer runs
//! instances of its own (`bv_mixer`'s `vst.rs`), following the effect.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use bv_timeline::{Effect, VstFx};
use bv_vst3::{Edit, Instance, ParamInfo, restart};

use crate::egui::{self, RichText, Ui};
use crate::fx_window::FxTarget;
use crate::i18n::tr;
use crate::theme;
use crate::vst::VstPlugin;

/// The plug-ins found and the pages shown.
#[derive(Debug, Default)]
pub(crate) struct VstUi {
    search: Option<crate::vst::Search>,
    found: Option<(Vec<PathBuf>, Vec<VstPlugin>)>,
    pages: HashMap<(FxTarget, usize), Page>,
}

impl VstUi {
    /// The VST 3 effects found in `folders` (searched in the background
    /// the first time, and again when the folders change), less those
    /// `left_out`.
    pub(crate) fn effects(&mut self, folders: &[PathBuf], left_out: &[PathBuf]) -> Vec<VstPlugin> {
        if let Some(found) = self.search.as_ref().and_then(crate::vst::Search::result) {
            let searched = self.search.take().map(|s| s.folders).unwrap_or_default();
            self.found = Some((searched, found));
        }
        let stale = self.found.as_ref().is_none_or(|(f, _)| f != folders);
        if stale && self.search.as_ref().is_none_or(|s| s.folders != folders) {
            self.search = Some(crate::vst::Search::start(folders.to_vec()));
        }
        self.found
            .as_ref()
            .map(|(_, all)| {
                all.iter()
                    .filter(|p| p.vst3 && !left_out.contains(&p.path))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
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
            }
        }
    }

    /// Whether a page's plug-in window is open (for tests).
    pub(crate) fn window_open(&self) -> bool {
        self.pages.values().any(|p| p.window.is_some())
    }
}

/// A new effect of plug-in `p`.
pub(crate) fn new_effect(p: &VstPlugin) -> Effect {
    Effect::vst(VstFx {
        path: p.path.clone(),
        class_id: p.class_id.clone(),
        name: p.name.clone(),
        vendor: p.vendor.clone(),
        ..VstFx::default()
    })
}

/// A VST 3 effect's parameters as its plug-in describes them (FX
/// Automation lists them): read once from an instance of the plug-in
/// made for that, by module and class.
pub(crate) fn param_infos(v: &VstFx) -> Vec<ParamInfo> {
    use std::collections::HashMap;
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
    let read = bv_vst3::load(&v.path).ok().and_then(|m| {
        let class = m.effect(&v.class_id).cloned()?;
        Instance::new(&m, &class, true).ok().map(|i| i.params())
    });
    let params = read.unwrap_or_default();
    if let Ok(mut k) = KNOWN.lock() {
        k.get_or_insert_with(HashMap::new)
            .insert(key, params.clone());
    }
    params
}

/// What a page keeps: its plug-in's instance (or why there is none), the
/// parameters it shows, the state and values it has from the effect, and
/// the plug-in's window.
struct Page {
    identity: (PathBuf, String),
    instance: Option<Instance>,
    error: Option<String>,
    params: Vec<ParamInfo>,
    state: Vec<u8>,
    known: BTreeMap<u32, f64>,
    /// After the instance (closed first: the plug-in's view leaves before
    /// its window goes).
    window: Option<bv_platform::PluginWindow>,
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
        self.window = None;
    }
}

/// The longest block the page's instance is set up for (it only takes
/// parameter changes: no sound goes through it).
const BLOCK: usize = 1024;

impl Page {
    fn open(v: &VstFx, rate: f64) -> Page {
        let made = bv_vst3::load(&v.path)
            .map_err(|e| e.to_string())
            .and_then(|m| {
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
                Ok(inst)
            });
        let (instance, error, params) = match made {
            Ok(inst) => {
                let params = inst.params();
                (Some(inst), None, params)
            }
            Err(why) => (None, Some(why), Vec::new()),
        };
        Page {
            identity: (v.path.clone(), v.class_id.clone()),
            instance,
            error,
            params,
            state: v.state.clone(),
            known: v.params.clone(),
            window: None,
            opened: false,
            drawn: true,
        }
    }

    /// Shows the plug-in's own editor in a window of its own; whether it
    /// could.
    #[allow(unsafe_code)]
    fn open_window(&mut self, title: &str) -> bool {
        let Some(inst) = self.instance.as_mut() else {
            return false;
        };
        let Some(ed) = inst.editor() else {
            return false;
        };
        // The system's kind of window: Windows' HWND, X11's window id.
        let platform = if cfg!(windows) {
            bv_vst3::PLATFORM_HWND
        } else {
            bv_vst3::PLATFORM_X11
        };
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
        // SAFETY: the window lives until the editor is closed: the page
        // closes the editor before it lets the window go (`Page`'s drop,
        // `follow_window`).
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

    /// The plug-in's window: closed by its button, resized by the user.
    /// Whether it closed.
    fn follow_window(&mut self) -> bool {
        let Some(win) = &self.window else {
            return false;
        };
        let Some(inst) = self.instance.as_mut() else {
            return false;
        };
        if win.closed() {
            inst.close_editor();
            self.window = None;
            return true;
        }
        if let Some((w, h)) = win.take_resized()
            && let Some(ed) = inst.editor()
        {
            let taken = ed.resized(w, h);
            if taken != (w, h) {
                win.resizer().set_client_size(taken.0, taken.1);
            }
        }
        false
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
        }
        let mut any = false;
        for (&id, &value) in &v.params {
            if self.known.get(&id) != Some(&value) {
                inst.set(id, value);
                self.known.insert(id, value);
                any = true;
            }
        }
        if any {
            inst.flush();
        }
    }

    /// What the plug-in's controller reported (its window's edits) into
    /// the effect; whether a gesture ended (its state is to be read).
    fn take_edits(&mut self, v: &mut VstFx) -> bool {
        let Some(inst) = self.instance.as_mut() else {
            return false;
        };
        let mut ended = false;
        let mut any = false;
        for edit in inst.take_edits() {
            match edit {
                Edit::Perform(id, value) => {
                    v.params.insert(id, value);
                    self.known.insert(id, value);
                    inst.queue(id, value);
                    any = true;
                }
                Edit::Restart(flags) => {
                    if flags & (restart::PARAM_VALUES_CHANGED | restart::PARAM_TITLES_CHANGED) != 0
                    {
                        self.params = inst.params();
                        for p in &self.params {
                            if let Some(value) = inst.value(p.id) {
                                v.params.insert(p.id, value);
                                self.known.insert(p.id, value);
                            }
                        }
                    }
                    ended = true;
                }
                Edit::End(_) | Edit::Dirty => ended = true,
                Edit::Begin(_) => {}
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

/// A VST 3 effect's page in the FX window (`target`'s effect `index`):
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
    // The plug-in's own window, once, unless the generic editor is asked
    // for.
    if !page.opened && !generic && bv_platform::PLUGIN_WINDOWS && page.instance.is_some() {
        page.opened = true;
        page.open_window(&v.name);
    }

    ui.horizontal(|ui| {
        ui.label(RichText::new(&v.name).strong());
        if !v.vendor.is_empty() {
            ui.label(RichText::new(&v.vendor).color(theme::text_dim()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let can = bv_platform::PLUGIN_WINDOWS && page.instance.is_some();
            let open = page.window.is_some();
            let text = if open {
                tr("Close Plug-In Window")
            } else {
                tr("Show Plug-In Window")
            };
            let r = ui.add_enabled(can, egui::Button::new(text));
            let r = if bv_platform::PLUGIN_WINDOWS {
                r
            } else {
                r.on_disabled_hover_text(tr("Coming soon to BananaV"))
            };
            if r.clicked() {
                if open {
                    if let Some(i) = page.instance.as_mut() {
                        i.close_editor();
                    }
                    page.window = None;
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
                        .or_else(|| inst.value(p.id))
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
                    let text = inst.text(p.id, value).unwrap_or_default();
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
                        ended |= done;
                    }
                }
            });
    }
    if ended {
        page.read_state(v);
    }
}
