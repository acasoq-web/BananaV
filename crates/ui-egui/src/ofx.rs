//! OFX plug-ins in the interface: finding them when BananaV starts (each
//! binary described in a process of its own, results cached), naming and
//! listing them for the choosers (effects, transitions, generators), and
//! the settings of a plug-in's effect, transition or generated media as
//! its plug-in describes them — a control per parameter, keyframes, the
//! plug-in's messages — and, while they are open, the plug-in's on-screen
//! controls over the Video Preview (its overlay).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bv_core::{FrameRate, Tick};
use bv_engine::plugins::{
    TRANSITION_PARAM, animation, filter_context, frames, generator_context, plugin_param,
    sync_values, transition_context,
};
use bv_ofx::{
    ChangeReason, Context, Instance, Mark, ParamInfo, ParamType, PenAction, PluginInfo, Setup,
    Severity, Value, View,
};
use bv_timeline::{
    ClipId, Effect, Generator, KeyCurve, MediaId, MediaSource, PluginFx, PluginKey, PluginParam,
    Project, Transform,
};

use crate::app::BananaApp;
use crate::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use crate::fx_window::FxTarget;
use crate::i18n::tr;
use crate::ofx_keys::{KeyRow, KeyView};
use crate::theme;

/// The name to show for an effect: a plug-in's own, or BananaV's (in the
/// interface's language).
pub fn fx_name(e: &Effect) -> String {
    match &e.plugin {
        Some(p) if !p.name.is_empty() && is_own_id(&p.id) => tr(&p.name).to_string(),
        Some(p) if !p.name.is_empty() => p.name.clone(),
        Some(p) => p.id.clone(),
        // A VST 3 plug-in's own name.
        None => match &e.vst {
            Some(v) if !v.name.is_empty() => v.name.clone(),
            _ => tr(e.kind.name()).to_string(),
        },
    }
}

/// Whether `id` is one of BananaV's own plug-ins.
fn is_own_id(id: &str) -> bool {
    id.starts_with(bv_effects_prefix())
        && bv_ofx::registry()
            .info(id)
            .is_some_and(|i| bv_engine::plugins::is_own(&i))
}

fn bv_effects_prefix() -> &'static str {
    bv_engine::plugins::OWN_PREFIX
}

/// The name to show for a plug-in: BananaV's own in the interface's
/// language.
pub fn plugin_name(info: &PluginInfo) -> String {
    say(bv_engine::plugins::is_own(info), info.name())
}

/// `info`'s plug-in at its defaults (no values of its own).
pub fn plugin_fx(info: &PluginInfo) -> PluginFx {
    PluginFx {
        id: info.identifier.clone(),
        version: info.version,
        name: info.name().to_string(),
        params: BTreeMap::new(),
    }
}

/// A new effect of `info`'s plug-in, at its defaults.
pub fn new_effect(info: &PluginInfo) -> Effect {
    Effect::plugin(plugin_fx(info))
}

/// Media `info`'s plug-in generates, at its defaults.
pub fn new_generator(info: &PluginInfo) -> Generator {
    Generator::Plugin(Box::new(plugin_fx(info)))
}

/// The known plug-ins that work in a context `fits` finds, by name
/// (shared, not copied: these lists are asked for every frame).
fn known_with(fits: fn(&PluginInfo) -> Option<Context>) -> Vec<Arc<PluginInfo>> {
    let mut list: Vec<Arc<PluginInfo>> = bv_ofx::registry()
        .known()
        .into_iter()
        .filter(|p| fits(p).is_some())
        .collect();
    list.sort_by_cached_key(|p| p.name().to_lowercase());
    list
}

/// The OFX plug-ins that work as video effects (filters): not BananaV's
/// compositors that are nothing else.
pub fn filters() -> Vec<Arc<PluginInfo>> {
    let mut list = known_with(filter_context);
    list.retain(|p| !bv_engine::plugins::compositor_only(&p.identifier));
    list
}

/// The compositor plug-ins (a track's Custom compositing mode): BananaV's.
pub fn compositors() -> Vec<Arc<PluginInfo>> {
    let mut list = known_with(bv_engine::plugins::compositor_context);
    list.retain(|p| bv_engine::plugins::is_compositor(&p.identifier));
    list
}

/// The OFX plug-ins that draw transitions.
pub fn transitions() -> Vec<Arc<PluginInfo>> {
    known_with(transition_context)
}

/// The OFX plug-ins that generate pictures (media generators).
pub fn generators() -> Vec<Arc<PluginInfo>> {
    known_with(generator_context)
}

/// The folder an OFX plug-in shows in: its own grouping's last part
/// ("VEGAS\Color" shows in "Color"), or "OFX".
pub fn group_of(info: &PluginInfo) -> String {
    let group = info
        .grouping
        .rsplit(['/', '\\'])
        .find(|s| !s.trim().is_empty())
        .map_or_else(|| "OFX".to_string(), |s| s.trim().to_string());
    say(bv_engine::plugins::is_own(info), &group)
}

/// Who made another maker's plug-in, for the Third Party categories'
/// Filter: the name of the bundle it comes in ("Sapphire" for
/// `Sapphire.ofx.bundle`) — each maker's plug-ins come in their own, and
/// OpenFX names no maker —, or its binary's when it is in none.
pub fn provider(info: &PluginInfo) -> String {
    const BUNDLE: &str = ".ofx.bundle";
    let bundle = info.file.ancestors().find_map(|a| {
        let name = a.file_name()?.to_str()?;
        let cut = name.len().checked_sub(BUNDLE.len())?;
        (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(BUNDLE))
            .then(|| name[..cut].to_string())
    });
    bundle.unwrap_or_else(|| {
        let stem = info
            .file
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        stem.strip_suffix(".ofx")
            .map_or(stem.clone(), str::to_string)
    })
}

/// What a scan is doing or found.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ScanState {
    #[default]
    Idle,
    /// Describing binary `done` of `total`.
    Scanning { done: usize, total: usize },
    /// Plug-ins found, and the binaries that failed (file, why).
    Done {
        plugins: usize,
        failed: Vec<(PathBuf, String)>,
    },
}

/// The interface's side of OFX: the scan, the instances that answer the
/// effects' settings, a color being picked, the overlay shown.
pub struct OfxUi {
    ctx: Option<egui::Context>,
    scan: Arc<Mutex<ScanState>>,
    editors: HashMap<EditorKey, Editor>,
    /// The overlay of the settings open, and whether they were shown in
    /// this frame.
    overlay: Option<(EditorKey, FxPlace)>,
    overlay_seen: bool,
    /// The overlay has the pen (the mouse button went down on it), and
    /// where the pen was last told about.
    pen: bool,
    pen_at: Option<Pos2>,
    /// The overlay took the last press on the preview: a double click
    /// there is its (Mesh Warp's point put back), not the preview's zoom.
    pen_took: bool,
    /// The standard overlay's position dragged (its parameter, and where
    /// the pen took it from the point, in screen points), and the one
    /// under the pen.
    position_drag: Option<(String, egui::Vec2)>,
    position_hover: Option<String>,
    /// What plug-ins' buttons may ask for (BananaV's picture extension),
    /// as the app gave it last.
    pictures: crate::ofx_pictures::Sources,
    /// What to tell the user (a picture a button asked for missing).
    notice: Option<String>,
    /// Motion a button asked to have analyzed, for the app to start.
    analysis: Option<AnalysisAsk>,
    /// A mask a button asked to have tracked.
    mask_track: Option<crate::mask_tracking::MaskTrackAsk>,
    /// An event length a button asked for, for the app to set.
    length: Option<LengthAsk>,
    /// The keyframe controllers' state, by effect.
    keys: HashMap<EditorKey, KeyView>,
    /// The keyframes copied in a keyframe controller (their parameters,
    /// their times from the first's).
    key_clip: Vec<(String, PluginKey)>,
    /// The curve editors' state, by effect and parameter.
    curve_views: HashMap<(EditorKey, String), crate::ofx_curves::CurvesView>,
    /// The groups opened or closed by the user, by effect and group.
    groups_open: HashMap<(EditorKey, String), bool>,
    /// The color pickers' state, by effect and parameter.
    color_views: HashMap<(EditorKey, String), crate::ofx_color::ColorView>,
    /// An effect of an FX chain was shown this frame.
    fx_seen: bool,
    /// Another window watches an effect's input meanwhile (a Color
    /// Grading picker waiting): the FX windows leave the watch alone.
    pub(crate) watch_held: bool,
    /// What an overlay drew with OpenGL itself, as a texture.
    overlay_picture: Option<egui::TextureHandle>,
    /// The overlay as drawn last, kept while what it shows stays the same.
    overlay_drawn: Option<OverlayDrawn>,
}

/// An overlay as drawn last: for which effect, view and settings, its
/// marks and when.
struct OverlayDrawn {
    key: EditorKey,
    view: View,
    values: BTreeMap<String, PluginParam>,
    marks: Vec<Mark>,
    at: std::time::Instant,
    /// The plug-in asked to be drawn again meanwhile.
    again: bool,
}

/// While playing, an overlay is drawn again at most this often for a new
/// time alone: one drawn with OpenGL is read back from the graphics card
/// each time, more than a frame of playback can spare.
const OVERLAY_WHILE_PLAYING: std::time::Duration = std::time::Duration::from_millis(100);

impl std::fmt::Debug for OfxUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OfxUi")
            .field("editors", &self.editors.len())
            .finish_non_exhaustive()
    }
}

impl Default for OfxUi {
    fn default() -> OfxUi {
        OfxUi {
            ctx: None,
            scan: Arc::new(Mutex::new(ScanState::Idle)),
            editors: HashMap::new(),
            overlay: None,
            overlay_seen: false,
            pen: false,
            pen_at: None,
            pen_took: false,
            position_drag: None,
            position_hover: None,
            pictures: Default::default(),
            notice: None,
            analysis: None,
            mask_track: None,
            length: None,
            keys: HashMap::new(),
            key_clip: Vec::new(),
            curve_views: HashMap::new(),
            groups_open: HashMap::new(),
            color_views: HashMap::new(),
            fx_seen: false,
            watch_held: false,
            overlay_picture: None,
            overlay_drawn: None,
        }
    }
}

/// Where the scan keeps what binaries said.
fn cache_path() -> Option<PathBuf> {
    Some(
        bv_platform::config_dir()?
            .join("BananaV")
            .join("ofx_cache.json"),
    )
}

impl OfxUi {
    pub fn new(ctx: egui::Context) -> OfxUi {
        OfxUi {
            ctx: Some(ctx),
            ..OfxUi::default()
        }
    }

    /// The timeline time the settings of `key` show when its keyframe
    /// controller keeps a cursor of its own (not in sync).
    pub fn cursor_of(&self, key: &EditorKey) -> Option<Tick> {
        self.keys.get(key).filter(|v| !v.sync).and_then(|v| v.local)
    }

    /// The height the keyframe controller of `fx` takes (none when it has
    /// no animated parameter).
    pub fn keyframes_height(&self, key: &EditorKey, fx: &PluginFx) -> f32 {
        if !fx.params.values().any(|p| !p.keys.is_empty()) {
            return 0.0;
        }
        let default = KeyView::default();
        crate::ofx_keys::height(self.keys.get(key).unwrap_or(&default))
    }

    /// Gives the keyframe controller of `key` a height (the bar above it
    /// dragged).
    pub fn set_keyframes_height(&mut self, key: &EditorKey, height: f32) {
        self.keys.entry(key.clone()).or_default().height = height.max(crate::ofx_keys::MIN_HEIGHT);
    }

    pub fn scan_state(&self) -> ScanState {
        self.scan.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Finds the plug-ins in the standard folders and `extra` ones, in the
    /// background: what the cache knows is there at once; binaries that
    /// changed are described, each in a BananaV of its own.
    pub fn scan(&self, extra: &[PathBuf]) {
        let Some(path) = cache_path() else {
            return;
        };
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut folders = bv_ofx::plugin::standard_folders();
        folders.extend(extra.iter().cloned());
        let state = Arc::clone(&self.scan);
        let ctx = self.ctx.clone();
        let _ = std::thread::Builder::new()
            .name("bv-ofx-scan".into())
            .spawn(move || {
                let mut cache = bv_ofx::scan::Cache::load(&path);
                let known = |c: &bv_ofx::scan::Cache| c.plugins().into_iter().cloned().collect();
                bv_ofx::registry().set_scanned(known(&cache));
                let scanner = bv_ofx::scan::Scanner {
                    exe,
                    args: Vec::new(),
                    library_folders: Vec::new(),
                    env: Vec::new(),
                    timeout: std::time::Duration::from_secs(120),
                };
                let total = folders
                    .iter()
                    .flat_map(|f| bv_ofx::plugin::find_bundles(f))
                    .map(|b| bv_ofx::plugin::bundle_binaries(&b).len())
                    .sum();
                let mut done = 0;
                let changed = cache.refresh(&folders, |file| {
                    if let Ok(mut s) = state.lock() {
                        *s = ScanState::Scanning { done, total };
                    }
                    done += 1;
                    scanner.describe(file)
                });
                if changed {
                    let _ = cache.save(&path);
                }
                let failed: Vec<(PathBuf, String)> = cache
                    .binaries
                    .iter()
                    .filter_map(|b| {
                        b.result
                            .as_ref()
                            .err()
                            .map(|e| (b.file.clone(), e.to_string()))
                    })
                    .collect();
                let plugins: Vec<PluginInfo> = known(&cache);
                let n = plugins.len();
                // Options › Preferences › File I/O › Log Plugin loading.
                if crate::logs::wants(crate::logs::LogKind::Plugins) {
                    let mut lines = vec![format!(
                        "OpenFX plug-ins: {n} in {} binaries, {} failed",
                        cache.binaries.len(),
                        failed.len()
                    )];
                    for b in &cache.binaries {
                        let file = b.file.display();
                        match &b.result {
                            Ok(found) => lines.push(format!(
                                "{file}: {}",
                                found
                                    .iter()
                                    .map(|p| p.identifier.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )),
                            Err(e) => lines.push(format!("{file}: failed: {e}")),
                        }
                        for (_, id, e) in &b.failures {
                            lines.push(format!("{file}: {id} failed: {e}"));
                        }
                    }
                    crate::logs::log(crate::logs::LogKind::Plugins, &lines);
                }
                bv_ofx::registry().set_scanned(plugins);
                if let Ok(mut s) = state.lock() {
                    *s = ScanState::Done { plugins: n, failed };
                }
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
    }

    /// Lets go of the instances of effects not shown since the last call
    /// (and of the overlay of settings closed).
    pub fn end_frame(&mut self) {
        self.editors.retain(|_, e| std::mem::take(&mut e.shown));
        let editors = &self.editors;
        self.keys.retain(|k, _| editors.contains_key(k));
        self.curve_views.retain(|(k, _), _| editors.contains_key(k));
        self.groups_open.retain(|(k, _), _| editors.contains_key(k));
        self.color_views.retain(|(k, _), _| editors.contains_key(k));
        // No FX window shows an effect: the preview keeps no input for it.
        if !std::mem::take(&mut self.fx_seen) && !self.watch_held {
            bv_engine::plugins::watch_input(None);
        }
        if !std::mem::take(&mut self.overlay_seen) {
            self.overlay = None;
            self.pen = false;
            self.pen_at = None;
            self.pen_took = false;
            self.position_drag = None;
            self.position_hover = None;
        }
    }

    /// Whether a plug-in's overlay took the last press on the preview (a
    /// double click there is the overlay's).
    pub fn overlay_took_press(&self) -> bool {
        self.overlay.is_some() && self.pen_took
    }

    /// What plug-ins' buttons may ask for now: the Video Preview's and the
    /// Trimmer's frames.
    pub(crate) fn set_pictures(&mut self, sources: crate::ofx_pictures::Sources) {
        self.pictures = sources;
    }

    /// What to tell the user, once.
    pub(crate) fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }
}

/// What an editor's instance belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditorOwner {
    /// An effect of an FX chain.
    Fx(FxTarget),
    /// The transition an event comes in with.
    Transition(ClipId),
    /// Media a plug-in generates.
    Generator(MediaId),
    /// A video track's compositor (its Custom Compositing Mode's, or with
    /// `true` its Parent Composite Mode's).
    Compositor(bv_timeline::TrackId, bool),
}

impl EditorOwner {
    /// The context the plug-in works in for it.
    fn context(self, info: &PluginInfo) -> Context {
        match self {
            EditorOwner::Fx(_) => filter_context(info).unwrap_or(Context::Filter),
            EditorOwner::Transition(_) => Context::Transition,
            EditorOwner::Generator(_) => Context::Generator,
            EditorOwner::Compositor(..) => Context::General,
        }
    }
}

/// Which effect, transition or generator an editor's instance is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EditorKey {
    pub owner: EditorOwner,
    pub index: usize,
    pub id: String,
}

impl EditorKey {
    /// The effect as the engine knows it (an FX chain's, a transition's).
    fn effect_key(&self) -> Option<bv_engine::plugins::EffectKey> {
        use bv_timeline::scene::FxOwner;
        let owner = match self.owner {
            EditorOwner::Fx(FxTarget::Clip(id)) => FxOwner::Event(id),
            EditorOwner::Fx(FxTarget::Media(id, false)) => FxOwner::Media(id),
            EditorOwner::Fx(FxTarget::Track(id)) => FxOwner::Track(id),
            EditorOwner::Fx(FxTarget::VideoOutput) => FxOwner::Output,
            EditorOwner::Transition(id) => FxOwner::Transition(id),
            EditorOwner::Compositor(id, parent) => FxOwner::Compositor(id, parent),
            _ => return None,
        };
        Some(bv_engine::plugins::EffectKey {
            owner,
            index: self.index,
            id: self.id.clone(),
        })
    }
}

/// A picture as an input clip (the preview's, for a plug-in reading it).
struct Picture(Arc<bv_core::ImageRgbaF32>, (f64, f64));

impl bv_ofx::ClipSource for Picture {
    fn picture(&self, _time: f64) -> Option<Arc<bv_core::ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        self.1
    }
}

struct Editor {
    instance: Arc<Instance>,
    setup: Setup,
    /// The values the instance has.
    values: BTreeMap<String, PluginParam>,
    shown: bool,
}

/// Where an effect works: the frame rate, its owner's time at the cursor
/// and length, its pictures' full size, and where their canonical
/// coordinates land in the frame.
#[derive(Clone, Copy, Debug)]
pub struct FxPlace {
    pub rate: FrameRate,
    pub time: Tick,
    pub duration: Tick,
    pub size: (u32, u32),
    pub map: Affine,
    /// The owner's times its keyframe controller shows.
    pub span: (Tick, Tick),
    /// The timeline's time of the owner's 0, when its time runs with the
    /// timeline's (to move the cursor to a keyframe).
    pub origin: Option<Tick>,
}

/// An affine map of points: canonical coordinates (x to the right, y up,
/// in a picture's full-size pixels) to the frame's (0–1 across, 0–1 down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine([[f64; 3]; 2]);

impl Affine {
    /// The affine map that `f` (affine) is.
    fn from_fn(f: impl Fn([f64; 2]) -> [f64; 2]) -> Affine {
        let (o, x, y) = (f([0.0, 0.0]), f([1.0, 0.0]), f([0.0, 1.0]));
        Affine([
            [x[0] - o[0], y[0] - o[0], o[0]],
            [x[1] - o[1], y[1] - o[1], o[1]],
        ])
    }

    pub fn apply(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b] = self.0;
        [a[0] * x + a[1] * y + a[2], b[0] * x + b[1] * y + b[2]]
    }

    /// A step `d`, without the move.
    fn linear(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b] = self.0;
        [a[0] * x + a[1] * y, b[0] * x + b[1] * y]
    }

    pub fn inverse(&self) -> Option<Affine> {
        let [[a, b, c], [d, e, f]] = self.0;
        let det = a * e - b * d;
        if det.abs() < 1e-15 {
            return None;
        }
        let (ia, ib, id, ie) = (e / det, -b / det, -d / det, a / det);
        Some(Affine([
            [ia, ib, -(ia * c + ib * f)],
            [id, ie, -(id * c + ie * f)],
        ]))
    }
}

/// The frame's own coordinates: a picture of the frame's size, as is.
pub fn frame_map(w: u32, h: u32) -> Affine {
    let (w, h) = (w.max(1) as f64, h.max(1) as f64);
    Affine::from_fn(|[x, y]| [x / w, (h - y) / h])
}

/// An event's picture, `src` pixels (`par` times as wide as the frame's),
/// placed in a frame of `frame` pixels by `tf` — as the compositors place
/// it (`bv_render::layer::place_par`).
pub fn event_map(src: (u32, u32), par: f32, tf: &Transform, frame: (u32, u32)) -> Affine {
    let (sw, sh) = (src.0.max(1) as f64, src.1.max(1) as f64);
    let (w, h) = (frame.0.max(1) as f64, frame.1.max(1) as f64);
    let fit = bv_timeline::scene::fit_rect_par(src.0.max(1), src.1.max(1), par, frame.0, frame.1);
    let scale = tf.scale.max(1e-4) as f64;
    let kx = fit.w as f64 * scale / sw;
    let ky = fit.h as f64 * scale * tf.stretch.max(1e-3) as f64 / sh;
    let fx = if tf.flip_h { -1.0 } else { 1.0 };
    let fy = if tf.flip_v { -1.0 } else { 1.0 };
    let (sin, cos) = (-(tf.rotation_deg as f64).to_radians()).sin_cos();
    let (cx, cy) = (w / 2.0 + tf.x as f64 * w, h / 2.0 - tf.y as f64 * h);
    Affine::from_fn(|[x, y]| {
        // Down from the top, from the picture's middle, framed.
        let rx = (x - sw / 2.0) * kx * fx;
        let ry = (sh - y - sh / 2.0) * ky * fy;
        let dx = rx * cos + ry * sin;
        let dy = -rx * sin + ry * cos;
        [(dx + cx) / w, (dy + cy) / h]
    })
}

/// Where event `clip`'s media lands in the frame at `t`.
fn clip_map(p: &Project, clip: &bv_timeline::Clip, t: Tick) -> Affine {
    let s = &p.settings;
    let frame = (s.width, s.height);
    let Some(v) = p.media(clip.media).and_then(|m| m.video) else {
        return frame_map(s.width, s.height);
    };
    let par = v.pixel_aspect.ratio() / s.pixel_aspect.ratio();
    let at = t.clamp(clip.start, clip.end());
    event_map((v.width, v.height), par, &clip.transform_at(at), frame)
}

/// A parameter's value now, from the effect or its plug-in's default.
pub(crate) fn value_now(info: &ParamInfo, fx: &PluginFx, place: &FxPlace) -> Value {
    match fx.params.get(&info.name) {
        Some(p) => animation(p, place.rate).at(frames(place.time, place.rate), info.ty),
        None => info.default.clone(),
    }
}

/// Sets a parameter's value now: a key at the cursor when it has keys.
pub(crate) fn set_now(info: &ParamInfo, fx: &mut PluginFx, place: &FxPlace, v: Value) {
    let Some(value) = bv_engine::plugins::plugin_value(&v) else {
        return;
    };
    let p = fx
        .params
        .entry(info.name.clone())
        .or_insert_with(|| PluginParam {
            value: value.clone(),
            keys: Vec::new(),
        });
    if p.keys.is_empty() {
        p.value = value;
        return;
    }
    // The cursor is on a frame of the timeline: so is the keyframe.
    let at = place.time;
    match p.keys.iter_mut().find(|k| k.at == at) {
        Some(k) => k.value = value,
        None => {
            // The curve of the keyframe before it, as VEGAS's new ones.
            let i = p.keys.partition_point(|k| k.at < at);
            let curve = i
                .checked_sub(1)
                .and_then(|b| p.keys.get(b))
                .map_or(KeyCurve::Linear, |k| k.curve);
            p.keys.insert(i, PluginKey::new(at, value, curve));
        }
    }
}

/// Whether a parameter animates as a whole with its effect — through the
/// FX window's Animate button, not a clock of its own — as VEGAS's Color
/// Curves does: curves, and a plug-in's own data (Mesh Warp's mesh).
fn animates_whole(info: &ParamInfo) -> bool {
    matches!(info.ty, ParamType::Parametric | ParamType::Custom) && info.animates && !info.secret
}

/// Whether the FX window shows VEGAS's Animate button under the settings
/// of `fx`: it has parameters that animate as a whole and no keyframes yet
/// (then its keyframe controller shows instead).
pub fn animate_button(ofx: &OfxUi, key: &EditorKey, fx: &PluginFx) -> bool {
    !fx.params.values().any(|p| !p.keys.is_empty())
        && ofx
            .editors
            .get(key)
            .is_some_and(|e| e.instance.param_infos().iter().any(animates_whole))
}

/// `fx` with its parameters that animate as a whole animated (the Animate
/// button): a keyframe at its owner's start for each.
pub fn animate_whole(ofx: &OfxUi, key: &EditorKey, fx: &PluginFx, place: &FxPlace) -> PluginFx {
    let mut new = fx.clone();
    let infos = ofx
        .editors
        .get(key)
        .map(|e| e.instance.param_infos())
        .unwrap_or_default();
    for info in infos.iter().filter(|i| animates_whole(i)) {
        if !new
            .params
            .get(&info.name)
            .is_some_and(|p| !p.keys.is_empty())
        {
            toggle_animation(info, &mut new, place);
        }
    }
    new
}

/// Turns a parameter's animation on (a keyframe at its owner's start, its
/// value now, as VEGAS does) or off (its value now, without keyframes).
fn toggle_animation(info: &ParamInfo, fx: &mut PluginFx, place: &FxPlace) {
    let now = value_now(info, fx, place);
    let Some(value) = bv_engine::plugins::plugin_value(&now) else {
        return;
    };
    let animated = fx
        .params
        .get(&info.name)
        .is_some_and(|p| !p.keys.is_empty());
    let keys = if animated {
        Vec::new()
    } else {
        vec![PluginKey {
            at: place.span.0,
            value: value.clone(),
            // VEGAS's keyframes start linear.
            curve: KeyCurve::Linear,
            handles: Vec::new(),
        }]
    };
    fx.params
        .insert(info.name.clone(), PluginParam { value, keys });
}

/// The editor's instance for `key`, made (or remade when the place
/// changed) and given the effect's values.
fn editor<'a>(
    ofx: &'a mut OfxUi,
    key: &EditorKey,
    fx: &PluginFx,
    place: &FxPlace,
) -> Result<&'a mut Editor, String> {
    let plugin = bv_ofx::registry()
        .plugin(&fx.id)
        .map_err(|e| e.to_string())?;
    let context = key.owner.context(&plugin.info);
    let setup = Setup {
        frame_rate: place.rate.as_f64(),
        duration: frames(place.duration, place.rate),
        size: (place.size.0 as f64, place.size.1 as f64),
        pixel_aspect: 1.0,
        interactive: true,
    };
    if ofx.editors.get(key).is_none_or(|e| e.setup != setup) {
        let made = match key.owner {
            EditorOwner::Fx(_) => Instance::new_in(&plugin, &bv_engine::plugins::FILTERS, &setup),
            _ => Instance::new(&plugin, context, &setup),
        };
        let instance = made.map_err(|e| e.to_string())?;
        ofx.editors.insert(
            key.clone(),
            Editor {
                instance: Arc::new(instance),
                setup,
                values: BTreeMap::new(),
                shown: true,
            },
        );
        if let Some(e) = ofx.editors.get_mut(key) {
            sync_values(&e.instance, &fx.params, place.rate);
            e.values = fx.params.clone();
            // Its values given, the plug-in hears its settings open (it may
            // set what shows enabled by them).
            e.instance.editing(true);
        }
    }
    let e = ofx.editors.get_mut(key).ok_or_else(String::new)?;
    if e.values != fx.params {
        // Changed elsewhere (undone, a preset, the keyframe controller):
        // the plug-in hears of it as of the user's edits, as the
        // specification asks.
        let names: Vec<String> = e
            .values
            .keys()
            .chain(fx.params.keys())
            .filter(|n| e.values.get(*n) != fx.params.get(*n))
            .cloned()
            .collect::<std::collections::BTreeSet<String>>()
            .into_iter()
            .collect();
        sync_values(&e.instance, &fx.params, place.rate);
        e.values = fx.params.clone();
        if !names.is_empty() {
            let _ = e
                .instance
                .changed(&names, ChangeReason::User, frames(place.time, place.rate));
        }
    }
    e.shown = true;
    Ok(e)
}

/// Draws a plug-in effect's settings, as VEGAS draws an OFX effect's: a
/// bar with its name, About and help, then a row per parameter — its
/// name, its control, its number and the clock that animates it. Returns
/// the changed effect when the user, or the plug-in in turn, changed it.
pub fn params(
    ui: &mut Ui,
    ofx: &mut OfxUi,
    key: EditorKey,
    fx: &PluginFx,
    place: &FxPlace,
    columns: usize,
) -> Option<PluginFx> {
    let (infos, overlay, plugin) = match editor(ofx, &key, fx, place) {
        Ok(e) => (
            e.instance.param_infos(),
            e.instance.has_overlay(),
            Arc::clone(e.instance.plugin()),
        ),
        Err(why) => {
            ui.label(
                RichText::new(trf!("The plug-in {} could not be loaded: {}", fx.name, why))
                    .color(theme::error()),
            );
            return None;
        }
    };
    let info = &plugin.info;
    let own = bv_engine::plugins::is_own(info);
    // As wide as its rows (measured last frame): the window fits them.
    let width_id = egui::Id::new(("ofx-rows-width", &key.id, key.index));
    let width: f32 = ui.data(|d| d.get_temp(width_id)).unwrap_or(440.0);
    header(ui, &key, info, &infos, own, width);
    if let Some(m) = ofx
        .editors
        .get(&key)
        .and_then(|e| e.instance.persistent_message())
    {
        let color = match m.severity {
            Severity::Error | Severity::Fatal => theme::error(),
            _ => theme::text_dim(),
        };
        ui.label(RichText::new(say(own, &m.text)).color(color));
    }
    ui.add_space(4.0);
    // An FX chain's effect: the preview keeps the picture it gives it, for
    // the plug-in to read (an analysis's button).
    if matches!(key.owner, EditorOwner::Fx(_)) && !ofx.watch_held {
        ofx.fx_seen = true;
        bv_engine::plugins::watch_input(key.effect_key());
    }
    // Its on-screen controls show over the preview meanwhile — and what
    // the host draws for it there: the features a motion analysis
    // followed (`stabilization_points`), and VEGAS's standard handles of
    // its positions when it draws nothing itself (`position_handles`).
    let analyzes = infos.iter().any(|i| i.analysis == "Motion");
    let positions = infos.iter().any(|i| is_position(i) && !i.secret);
    if overlay || analyzes || positions {
        ofx.overlay = Some((key.clone(), *place));
        ofx.overlay_seen = true;
    }
    let mut new = fx.clone();
    let mut touched: Vec<String> = Vec::new();
    // In `columns` columns side by side (the FX window's Columns buttons),
    // in order, about as many rows in each.
    let rows_width = {
        let mut ctx = Rows {
            ofx: &mut *ofx,
            key: &key,
            infos: &infos,
            fx: &mut new,
            place,
            touched: &mut touched,
            own,
            depth: 0,
        };
        let units = ctx.shown("");
        let weights: Vec<usize> = units.iter().map(|i| ctx.weight(i, 0)).collect();
        let parts = split_columns(&weights, columns);
        let grids = ui.horizontal_top(|ui| {
            let mut first = 0;
            for (c, &n) in parts.iter().enumerate() {
                if c > 0 {
                    ui.add_space(18.0);
                }
                ui.vertical(|ui| {
                    egui::Grid::new(("ofx-params", &key.id, key.index, c))
                        .num_columns(4)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            for info in &units[first..first + n] {
                                ctx.unit(ui, info, 0);
                            }
                        });
                });
                first += n;
            }
        });
        grids.response.rect.width().ceil()
    };
    if (rows_width - width).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(width_id, rows_width));
        ui.ctx().request_repaint();
    }
    if touched.is_empty() {
        return (new != *fx).then_some(new);
    }
    rich_text(ui, &key, &infos, fx, &mut new, &touched);
    // The plug-in hears of the change, and may change others in turn —
    // reading, if it wants, the picture the preview gave it last.
    let e = ofx.editors.get_mut(&key)?;
    let input = key
        .effect_key()
        .and_then(|k| bv_engine::plugins::watched_input(&k))
        .map(|(_, picture, full)| (picture, full));
    if let Some((picture, full)) = &input {
        e.instance
            .connect("Source", Arc::new(Picture(Arc::clone(picture), *full)));
    }
    // A button asking for the picture's motion (BananaV's extension): the
    // app analyzes it and keeps it in the parameter the button names.
    for name in &touched {
        let Some(info) = infos.iter().find(|i| {
            &i.name == name
                && matches!(
                    i.analysis.as_str(),
                    "Motion" | "Rotation" | "Histogram" | "Histograms" | "ColorHistograms"
                )
        }) else {
            continue;
        };
        let Some(effect) = key.effect_key() else {
            continue;
        };
        // How hard to look: the method's first option fast, the others
        // accurate.
        let choice = |name: &str| {
            new.params.get(name).and_then(|p| match &p.value {
                bv_timeline::PluginValue::Num(v) => v.first().copied(),
                _ => None,
            })
        };
        let method = choice(&info.analysis_method);
        let kind = match info.analysis.as_str() {
            "Rotation" => bv_engine::AnalysisKind::Rotation {
                exclude: choice(&info.analysis_exclude).unwrap_or(0.0) as i32,
                layout: choice(&info.analysis_layout).unwrap_or(0.0) as i32,
            },
            "Histogram" => bv_engine::AnalysisKind::Histogram,
            "Histograms" => bv_engine::AnalysisKind::Histograms,
            "ColorHistograms" => bv_engine::AnalysisKind::ColorHistograms,
            _ => bv_engine::AnalysisKind::Motion,
        };
        // The work's parameters: the frames' width, the features, the
        // steps.
        let numbers =
            |names: &str| -> Vec<f64> { names.split_whitespace().filter_map(choice).collect() };
        let tuned = numbers(&info.analysis_tuning);
        let accurate = method.is_none_or(|m| m >= 0.5);
        let mut tuning = (tuned.len() == 3).then(|| bv_engine::track::Tuning {
            width: tuned[0].max(1.0) as u32,
            features: tuned[1].max(1.0) as usize,
            iterations: tuned[2].max(1.0) as usize,
            follow: bv_engine::track::Follow::LucasKanade,
            grid: None,
        });
        // How the features spread and are followed (Video Stabilization's
        // Expert view).
        let grid = numbers(&info.analysis_grid);
        let follow = choice(&info.analysis_follow);
        if grid.len() == 3 || follow.is_some() {
            let base = tuning.unwrap_or(
                if accurate {
                    bv_engine::track::Precision::Accurate
                } else {
                    bv_engine::track::Precision::Fast
                }
                .tuning(),
            );
            tuning = Some(bv_engine::track::Tuning {
                grid: (grid.len() == 3)
                    .then(|| [grid[0], grid[1], grid[2]].map(|v| v.max(1.0) as usize)),
                follow: bv_engine::track::Follow::of_index(follow.unwrap_or(0.0) as i32),
                ..base
            });
        }
        // The area: its middle (rows up), width and height, as parts of the
        // frame, kept inside it as VEGAS keeps it.
        let region = numbers(&info.analysis_region);
        let area = info
            .analysis_region
            .split_whitespace()
            .next()
            .and_then(|middle| new.params.get(middle))
            .and_then(|p| match &p.value {
                bv_timeline::PluginValue::Num(v) if v.len() >= 2 => Some([v[0], v[1]]),
                _ => None,
            })
            .zip((region.len() >= 3).then(|| [region[1], region[2]]))
            .map(|([x, y], [w, h])| {
                let (mut l, mut w) = (x - w / 2.0, w);
                if l < 0.0 {
                    w += l;
                    l = 0.0;
                }
                let (mut b, mut h) = (y - h / 2.0, h);
                if b < 0.0 {
                    h += b;
                    b = 0.0;
                }
                let (w, h) = (w.max(0.1), h.max(0.1));
                [l, 1.0 - (b + h), l + w, 1.0 - b]
            });
        let setup = bv_engine::MotionSetup {
            area,
            halvings: choice(&info.analysis_reduction).map(|v| v.max(0.0) as u32),
        };
        ofx.analysis = Some(AnalysisAsk {
            key: effect,
            param: info.analysis_param.clone(),
            accurate,
            kind,
            tuning,
            setup,
        });
    }
    // A mask's Start (BananaV's extension): the app tracks the mask.
    for name in &touched {
        let Some(info) = infos
            .iter()
            .find(|i| &i.name == name && !i.mask_tracking.is_empty())
        else {
            continue;
        };
        let Some(effect) = key.effect_key() else {
            continue;
        };
        ofx.mask_track = Some(crate::mask_tracking::MaskTrackAsk {
            key: effect,
            spec: info.mask_tracking.clone(),
            params: new.params.clone(),
        });
    }
    // A button asking for its event's length (BananaV's extension): the
    // app sets it from the rate the button names.
    for name in &touched {
        let Some(info) = infos
            .iter()
            .find(|i| &i.name == name && !i.event_length.is_empty())
        else {
            continue;
        };
        let Some(effect) = key.effect_key() else {
            continue;
        };
        let rate = new
            .params
            .get(&info.event_length)
            .and_then(|p| match &p.value {
                bv_timeline::PluginValue::Num(v) => v.first().copied(),
                _ => None,
            })
            .or_else(|| {
                infos
                    .iter()
                    .find(|i| i.name == info.event_length)
                    .and_then(|i| match &i.default {
                        bv_ofx::Value::Num(v) => v.first().copied(),
                        _ => None,
                    })
            })
            .unwrap_or(1.0);
        ofx.length = Some(LengthAsk { key: effect, rate });
    }
    // Buttons that ask for a picture (BananaV's extension) get it on their
    // clip first.
    for name in &touched {
        let Some(info) = infos
            .iter()
            .find(|i| &i.name == name && !i.picture.is_empty())
        else {
            continue;
        };
        let Some(kind) = crate::ofx_pictures::Kind::of(&info.picture) else {
            continue;
        };
        let given = input.as_ref().map(|(p, _)| Arc::clone(p));
        match ofx.pictures.picture(kind, given) {
            Ok(Some(img)) => {
                let full = (img.width as f64, img.height as f64);
                e.instance
                    .connect(&info.picture_clip, Arc::new(Picture(img, full)));
            }
            Ok(None) => {}
            Err(say) => ofx.notice = Some(say.to_string()),
        }
    }
    sync_values(&e.instance, &new.params, place.rate);
    let changed = e
        .instance
        .changed(&touched, ChangeReason::User, frames(place.time, place.rate));
    if !changed.is_empty() {
        let values = e.instance.values();
        for name in changed {
            if let Some((_, a)) = values.iter().find(|(n, _)| *n == name)
                && let Some(p) = plugin_param(a, place.rate)
            {
                new.params.insert(name, p);
            }
        }
    }
    e.values = new.params.clone();
    Some(new)
}

/// What effect `after` — `before` with an analysis's result arrived in
/// `names` (BananaV's `kBananaVParamPropAnalysisParam`) — sets in turn:
/// its editor's instance (made for the moment when its window is closed)
/// hears of the result as of the user's edit, and the values it changes
/// come back, for the same edit.
pub(crate) fn response(
    ofx: &mut OfxUi,
    key: EditorKey,
    (before, after): (&PluginFx, &PluginFx),
    names: &[String],
    place: &FxPlace,
) -> BTreeMap<String, bv_timeline::PluginParam> {
    let Ok(e) = editor(ofx, &key, before, place) else {
        return BTreeMap::new();
    };
    sync_values(&e.instance, &after.params, place.rate);
    let changed = e
        .instance
        .changed(names, ChangeReason::User, frames(place.time, place.rate));
    let values = e.instance.values();
    let mut out = BTreeMap::new();
    for name in changed {
        if let Some((_, a)) = values.iter().find(|(n, _)| *n == name)
            && let Some(p) = plugin_param(a, place.rate)
        {
            out.insert(name, p);
        }
    }
    // What the editor holds now: not news when the window draws it.
    e.values = after.params.clone();
    e.values.extend(out.clone());
    out
}

/// Motion a push button asked the host to analyze (BananaV's
/// `kBananaVParamPropAnalysis`): the effect, the parameter the result goes
/// to, and how hard to look.
#[derive(Clone, Debug)]
pub(crate) struct AnalysisAsk {
    pub key: bv_engine::plugins::EffectKey,
    pub param: String,
    pub accurate: bool,
    pub kind: bv_engine::AnalysisKind,
    /// The analysis's own work, when the effect sets it.
    pub tuning: Option<bv_engine::track::Tuning>,
    /// The area analyzed and the frames' size, when the effect sets them.
    pub setup: bv_engine::MotionSetup,
}

/// An event length a push button asked for (BananaV's
/// `kBananaVParamPropEventLength`): the effect, and the rate its event's
/// media is played at.
#[derive(Clone, Debug)]
pub(crate) struct LengthAsk {
    pub key: bv_engine::plugins::EffectKey,
    pub rate: f64,
}

impl OfxUi {
    /// The analysis a button asked for since last asked.
    pub(crate) fn take_analysis(&mut self) -> Option<AnalysisAsk> {
        self.analysis.take()
    }

    /// The mask a button asked to have tracked since last asked.
    pub(crate) fn take_mask_track(&mut self) -> Option<crate::mask_tracking::MaskTrackAsk> {
        self.mask_track.take()
    }

    /// The event length a button asked for since last asked.
    pub(crate) fn take_length(&mut self) -> Option<LengthAsk> {
        self.length.take()
    }
}

/// OpenFX's layout hint that puts the next parameter on a parameter's
/// line (`kOfxParamPropLayoutHintNoNewLine`).
const NO_NEW_LINE: i32 = 2;

/// Where a push button stands among buttons laid on one line.
enum ButtonRun<'a> {
    /// It starts its line: the line's buttons, itself first.
    Starts(Vec<&'a ParamInfo>),
    /// An earlier button's line has it.
    Inside,
}

/// Whether BananaV gives the picture push button `info` asks for (one
/// asking for none: yes).
fn picture_ready(info: &ParamInfo) -> bool {
    crate::ofx_pictures::Kind::of(&info.picture).is_none_or(|k| k.ready())
}

/// How many of the parameters (each `weights` rows high) go in each of
/// `columns` columns, in order: about as many rows in each, none empty
/// while there are parameters for it.
fn split_columns(weights: &[usize], columns: usize) -> Vec<usize> {
    let columns = columns.clamp(1, weights.len().max(1));
    let total: usize = weights.iter().sum();
    let mut out = Vec::with_capacity(columns);
    let (mut count, mut rows, mut done) = (0, 0, 0);
    for (i, &w) in weights.iter().enumerate() {
        let left = columns - out.len();
        // The parameters left must fill the columns left.
        let must = weights.len() - i < left;
        let target = (total - done).div_ceil(left);
        if count > 0 && left > 1 && (must || rows + w > target && rows * 2 >= w) {
            out.push(count);
            done += rows;
            (count, rows) = (0, 0);
        }
        count += 1;
        rows += w;
    }
    out.push(count);
    out
}

/// A text of a plug-in: BananaV's own are in the interface's language.
pub(crate) fn say(own: bool, text: &str) -> String {
    if own {
        tr(text).to_string()
    } else {
        text.to_string()
    }
}

/// The bar over a plug-in's settings: its name, About (who it is) and the
/// help (what each parameter does).
fn header(
    ui: &mut Ui,
    key: &EditorKey,
    info: &PluginInfo,
    infos: &[ParamInfo],
    own: bool,
    width: f32,
) {
    let name = say(own, info.name());
    egui::Frame::new()
        .fill(theme::bg())
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(width - 16.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&name)
                        .strong()
                        .size(15.0)
                        .color(theme::text_strong()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let help = ui.add(egui::Button::new("?").frame(false));
                    egui::Popup::menu(&help)
                        .id(egui::Id::new(("ofx-help", &key.id, key.index)))
                        .width(360.0)
                        .show(|ui| {
                            ui.label(RichText::new(&name).strong());
                            let shown = infos.iter().filter(|i| {
                                !i.secret && !i.hint.is_empty() && i.ty != ParamType::Page
                            });
                            let mut any = false;
                            for i in shown {
                                any = true;
                                ui.add_space(4.0);
                                ui.label(RichText::new(say(own, &i.label)).strong());
                                ui.label(say(own, &i.hint));
                            }
                            if !any {
                                ui.label(tr("The plug-in describes no parameter."));
                            }
                        });
                    let about = ui.add(egui::Button::new(tr("About")).frame(false));
                    egui::Popup::menu(&about)
                        .id(egui::Id::new(("ofx-about", &key.id, key.index)))
                        .width(360.0)
                        .show(|ui| {
                            ui.label(RichText::new(&name).strong());
                            let [major, minor] = info.version;
                            let who = if own {
                                trf!("BananaV, version {}.{}", major, minor)
                            } else {
                                trf!("OFX plug-in, version {}.{}", major, minor)
                            };
                            ui.label(who);
                            ui.label(RichText::new(&info.identifier).color(theme::text_dim()));
                            ui.label(trf!("Group: {}", group_of(info)));
                            if !info.description.is_empty() {
                                ui.add_space(4.0);
                                ui.label(say(own, &info.description));
                            }
                        });
                });
            });
        });
}

/// Whether a parameter is a position in the frame (its box picks it).
pub(crate) fn is_position(info: &ParamInfo) -> bool {
    info.ty == ParamType::Double2D
        && matches!(
            info.double_type.as_str(),
            "OfxParamDoubleTypeXY"
                | "OfxParamDoubleTypeXYAbsolute"
                | "OfxParamDoubleTypeNormalisedXY"
                | "OfxParamDoubleTypeNormalisedXYAbsolute"
        )
}

/// Whether a double click on a parameter's name puts it back at its
/// default: values set with a control, not texts typed (a title's words).
fn resets(info: &ParamInfo) -> bool {
    info.ty.has_value()
        && !matches!(
            info.ty,
            ParamType::String | ParamType::Custom | ParamType::Bytes | ParamType::Parametric
        )
}

/// What a position's numbers are counted against to show them 0–1: the
/// frame, or nothing when the plug-in already counts them so.
fn position_scale(info: &ParamInfo, place: &FxPlace) -> [f64; 2] {
    if info.double_type.contains("Normalised") {
        [1.0, 1.0]
    } else {
        [place.size.0.max(1) as f64, place.size.1.max(1) as f64]
    }
}

/// Two numbers as the interface writes them, each with its decimals:
/// "0.500, 0.500", or "0,500; 0,500" where the comma is the decimal
/// separator.
fn pair_text(v: [f64; 2], digits: [usize; 2]) -> String {
    numbers_each(&v, &digits)
}

/// Numbers as the interface writes them, each with its own decimals.
fn numbers_each(v: &[f64], digits: &[usize]) -> String {
    let sep = if crate::i18n::language().decimal_separator() == ',' {
        "; "
    } else {
        ", "
    };
    v.iter()
        .zip(digits)
        .map(|(x, &d)| crate::fmt::num(*x, d))
        .collect::<Vec<_>>()
        .join(sep)
}

/// How far a row's name steps in for each group around it.
const INDENT: f32 = 12.0;

/// A section's or a box's triangle in `r`: down when open, right when not.
fn paint_triangle(ui: &Ui, r: Rect, open: bool) {
    let c = r.center();
    let pts = if open {
        vec![
            c + vec2(-5.0, -3.0),
            c + vec2(5.0, -3.0),
            c + vec2(0.0, 3.0),
        ]
    } else {
        vec![
            c + vec2(-3.0, -5.0),
            c + vec2(3.0, 0.0),
            c + vec2(-3.0, 5.0),
        ]
    };
    let color = if ui.is_enabled() {
        theme::text()
    } else {
        theme::text_dim()
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// The decimals number `i` of a parameter shows: its plug-in's, fewer
/// for a wide range — an angle's 0–360 shows one where a 0–1 shows three.
fn number_digits(info: &ParamInfo, i: usize) -> usize {
    let digits = info.digits.clamp(0, 6) as usize;
    let span = |a: &[f64], b: &[f64]| {
        let (lo, hi) = (*a.get(i)?, *b.get(i)?);
        (lo.is_finite() && hi.is_finite() && hi > lo).then_some(hi - lo)
    };
    let wide = span(&info.display_min, &info.display_max)
        .or_else(|| span(&info.min, &info.max))
        .map_or(0, |r| r.log10().floor().max(0.0) as usize);
    digits.saturating_sub(wide)
}

/// Two numbers typed (separated as `pair_text` writes them, or by spaces).
fn parse_pair(s: &str) -> Option<[f64; 2]> {
    let v = parse_numbers(s, 2)?;
    Some([v[0], v[1]])
}

/// Numbers as the interface writes them: "0.500, 0.250", or "0,500;
/// 0,250" where the comma is the decimal separator.
pub(crate) fn numbers_text(v: &[f64], digits: usize) -> String {
    let sep = if crate::i18n::language().decimal_separator() == ',' {
        "; "
    } else {
        ", "
    };
    v.iter()
        .map(|x| crate::fmt::num(*x, digits))
        .collect::<Vec<_>>()
        .join(sep)
}

/// `n` numbers typed (separated as [`numbers_text`] writes them, or by
/// spaces).
pub(crate) fn parse_numbers(s: &str, n: usize) -> Option<Vec<f64>> {
    let parts: Vec<&str> = if s.contains(';') {
        s.split(';').collect()
    } else if crate::i18n::language().decimal_separator() == ',' {
        s.split_whitespace().collect()
    } else {
        s.split([',', ' '])
            .filter(|p| !p.trim().is_empty())
            .collect()
    };
    if parts.len() != n {
        return None;
    }
    parts.iter().map(|p| crate::fmt::parse(p)).collect()
}

/// What the rows of a plug-in's settings work on.
struct Rows<'a> {
    ofx: &'a mut OfxUi,
    key: &'a EditorKey,
    infos: &'a [ParamInfo],
    fx: &'a mut PluginFx,
    place: &'a FxPlace,
    touched: &'a mut Vec<String>,
    own: bool,
    /// How deep in groups the rows being drawn are (their names step in).
    depth: usize,
}

impl<'a> Rows<'a> {
    /// The parameters shown under `parent`.
    fn shown(&self, parent: &str) -> Vec<&'a ParamInfo> {
        // A transition's progress is the host's to set.
        let transition = matches!(self.key.owner, EditorOwner::Transition(_));
        let infos: &'a [ParamInfo] = self.infos;
        infos
            .iter()
            .filter(|i| {
                i.parent == parent
                    && !i.secret
                    && !(transition && i.name == TRANSITION_PARAM)
                    && !matches!(i.ty, ParamType::Page | ParamType::Custom | ParamType::Bytes)
            })
            .collect()
    }

    /// The rows of the parameters whose parent is `parent` (groups show as
    /// headings).
    fn rows(&mut self, ui: &mut Ui, parent: &str, depth: usize) {
        for info in self.shown(parent) {
            self.unit(ui, info, depth);
        }
    }

    /// Whether group `info` is open: as the user left it, or as its
    /// plug-in describes it.
    fn group_open(&self, info: &ParamInfo) -> bool {
        self.ofx
            .groups_open
            .get(&(self.key.clone(), info.name.clone()))
            .copied()
            .unwrap_or(info.open)
    }

    /// A parameter's row, or a group's heading — VEGAS's section: its name
    /// and the triangle that opens or closes it — and, open, its
    /// parameters' rows, their names stepped in.
    fn unit(&mut self, ui: &mut Ui, info: &ParamInfo, depth: usize) {
        if info.ty == ParamType::PushButton {
            match self.button_run(info) {
                ButtonRun::Inside => return,
                ButtonRun::Starts(run) if run.len() > 1 => {
                    self.buttons_row(ui, &run);
                    return;
                }
                ButtonRun::Starts(_) => {}
            }
        }
        if info.ty != ParamType::Group {
            self.row(ui, info);
            return;
        }
        let open = self.group_open(info);
        let label = say(self.own, &info.label);
        // One widget: the triangle and the name, a click on either.
        let galley = egui::WidgetText::from(RichText::new(&label).strong()).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let indent = INDENT * depth as f32;
        let size = vec2(indent + 16.0 + galley.size().x, galley.size().y.max(16.0));
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let tri = Rect::from_center_size(
            pos2(rect.left() + indent + 6.0, rect.center().y),
            vec2(12.0, 12.0),
        );
        paint_triangle(ui, tri, open);
        ui.painter().galley(
            pos2(
                rect.left() + indent + 16.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            theme::text(),
        );
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, &label)
        });
        if resp.clicked() {
            self.ofx
                .groups_open
                .insert((self.key.clone(), info.name.clone()), !open);
        }
        ui.end_row();
        if open && depth < 8 {
            // A group's own interface first, under its heading (Credit
            // Roll's Position box).
            if let Some(size) = info.interact {
                let id = egui::Id::new(("ofx-param", &self.key.id, self.key.index, &info.name));
                self.interact_row(ui, info, size, id);
            }
            let outer = self.depth;
            self.depth = depth + 1;
            self.rows(ui, &info.name, depth + 1);
            self.depth = outer;
        }
    }

    /// Where push button `info` stands among buttons laid on one line
    /// (OpenFX's no-new-line layout hint: the next one on its line).
    fn button_run(&self, info: &ParamInfo) -> ButtonRun<'a> {
        let siblings = self.shown(&info.parent);
        let Some(i) = siblings.iter().position(|s| s.name == info.name) else {
            return ButtonRun::Starts(Vec::new());
        };
        let joins = |s: &ParamInfo| s.ty == ParamType::PushButton && s.layout_hint == NO_NEW_LINE;
        if i > 0 && joins(siblings[i - 1]) {
            return ButtonRun::Inside;
        }
        let mut run = vec![siblings[i]];
        let mut j = i;
        while joins(siblings[j])
            && j + 1 < siblings.len()
            && siblings[j + 1].ty == ParamType::PushButton
        {
            j += 1;
            run.push(siblings[j]);
        }
        ButtonRun::Starts(run)
    }

    /// Push buttons on one line: a row of their own, side by side where a
    /// control goes.
    fn buttons_row(&mut self, ui: &mut Ui, run: &[&ParamInfo]) {
        ui.label("");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for info in run {
                let ready = picture_ready(info);
                let label = say(self.own, &info.label);
                let r = ui.add_enabled(info.enabled && ready, egui::Button::new(label));
                let r = if !ready {
                    r.on_disabled_hover_text(tr("Coming soon to BananaV"))
                } else if info.hint.is_empty() {
                    r
                } else {
                    r.on_hover_text(say(self.own, &info.hint))
                };
                if r.clicked() {
                    self.touched.push(info.name.clone());
                }
            }
        });
        ui.label("");
        ui.label("");
        ui.end_row();
    }

    /// How many rows a parameter takes: a group, its heading's and its
    /// parameters'.
    fn weight(&self, info: &ParamInfo, depth: usize) -> usize {
        // A button on an earlier one's line takes none.
        if info.ty == ParamType::PushButton && matches!(self.button_run(info), ButtonRun::Inside) {
            return 0;
        }
        if info.ty == ParamType::Parametric {
            return 10;
        }
        // Its own interface under it: about a row per 30 pixels.
        let interface = info
            .interact
            .map_or(0, |[_, h]| (h / 30.0).round() as usize);
        if info.ty != ParamType::Group && info.interact.is_some() {
            return 1 + interface;
        }
        if info.ty == ParamType::Group && depth < 8 && self.group_open(info) {
            1 + interface
                + self
                    .shown(&info.name)
                    .iter()
                    .map(|i| self.weight(i, depth + 1))
                    .sum::<usize>()
        } else {
            1
        }
    }

    /// One parameter: its name, its control and number, its clock; a
    /// position's box under it when it is open.
    fn row(&mut self, ui: &mut Ui, info: &ParamInfo) {
        if info.ty == ParamType::Parametric {
            self.curves_row(ui, info);
            return;
        }
        let own = self.own;
        let label = say(own, &info.label);
        let id = egui::Id::new(("ofx-param", &self.key.id, self.key.index, &info.name));
        let position = is_position(info);
        // A position's box, or the parameter's own interface (open at
        // first, as VEGAS's color wheels are), shows under its row.
        let colour = matches!(info.ty, ParamType::Rgb | ParamType::Rgba);
        let boxed = position || colour || info.interact.is_some();
        let open_id = id.with("open");
        let mut open = boxed
            && ui.data(|d| {
                d.get_temp::<bool>(open_id)
                    .unwrap_or(info.interact.is_some() && !info.interact_closed)
            });
        // The name (a position's opens its box), stepped in inside groups
        // and greyed with its parameter. A double click on it puts the
        // parameter back at its default, whatever its control: a slider's
        // own double click does so, as VEGAS's, but a position's box is
        // closed at first and a color's swatch opens its picker.
        let indent = INDENT * self.depth as f32;
        let mut reset = false;
        let name = if info.ty == ParamType::PushButton {
            ui.label("")
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(indent);
                if boxed {
                    let (r, resp) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::click());
                    paint_triangle(ui, r, open);
                    if resp.clicked() {
                        open = !open;
                        ui.data_mut(|d| d.insert_temp(open_id, open));
                    }
                }
                let resp = ui
                    .add_enabled_ui(info.enabled, |ui| {
                        ui.add(
                            egui::Label::new(format!("{label}:"))
                                .selectable(false)
                                .sense(Sense::click()),
                        )
                    })
                    .inner;
                reset = resp.double_clicked() && resets(info);
            })
            .response
        };
        if !info.hint.is_empty() {
            name.on_hover_text(say(own, &info.hint));
        }
        // The control and its number.
        let now = value_now(info, self.fx, self.place);
        let picked = self
            .cells(ui, info, &now, id, &label)
            .or_else(|| (reset && now != info.default).then(|| info.default.clone()));
        if let Some(v) = picked {
            set_now(info, self.fx, self.place, v);
            self.touched.push(info.name.clone());
        }
        // The clock.
        let animated = self
            .fx
            .params
            .get(&info.name)
            .is_some_and(|p| !p.keys.is_empty());
        if info.animates && info.ty.has_value() {
            let tip = if animated {
                tr("Animated: changes set keyframes. Click to stop animating.")
            } else {
                tr("Animate")
            };
            let r = ui
                .add_enabled_ui(info.enabled, |ui| {
                    crate::ofx_controls::animate(ui, animated, tip).on_hover_text(tip)
                })
                .inner;
            if r.clicked() {
                toggle_animation(info, self.fx, self.place);
                self.touched.push(info.name.clone());
            }
        } else {
            ui.label("");
        }
        ui.end_row();
        if position && open {
            ui.label("");
            let scale = position_scale(info, self.place);
            let mut v = [now.num(0) / scale[0], now.num(1) / scale[1]];
            // A double click puts it back where the plug-in puts it.
            let reset = [
                info.default.num(0) / scale[0],
                info.default.num(1) / scale[1],
            ];
            let r = ui
                .add_enabled_ui(info.enabled, |ui| {
                    crate::ofx_controls::pad(ui, &mut v, reset, &label)
                })
                .inner;
            if r.changed() {
                let v = Value::Num(vec![v[0] * scale[0], v[1] * scale[1]]);
                set_now(info, self.fx, self.place, v);
                self.touched.push(info.name.clone());
            }
            ui.label("");
            ui.label("");
            ui.end_row();
        } else if open && let Some(size) = info.interact {
            self.interact_row(ui, info, size, id);
        } else if open && colour {
            self.colour_row(ui, info, &now, id);
        }
    }

    /// A color parameter's picker (VEGAS's, see [`crate::ofx_color`]),
    /// under its row, from the control's column over the number's.
    fn colour_row(&mut self, ui: &mut Ui, info: &ParamInfo, now: &Value, id: egui::Id) {
        use crate::ofx_controls::{FIELD_W, SLIDER_W};
        ui.label("");
        let width = SLIDER_W + 10.0 + FIELD_W;
        let (cell, _) =
            ui.allocate_exact_size(vec2(SLIDER_W, crate::ofx_color::HEIGHT), Sense::hover());
        ui.label("");
        ui.label("");
        ui.end_row();
        let rgb = [now.num(0), now.num(1), now.num(2)];
        let view = self
            .ofx
            .color_views
            .entry((self.key.clone(), info.name.clone()))
            .or_insert_with(|| crate::ofx_color::ColorView::starting_in(&info.color_model));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    cell.min,
                    vec2(width, crate::ofx_color::HEIGHT),
                ))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        if !info.enabled {
            child.disable();
        }
        let alpha = (info.ty == ParamType::Rgba).then(|| now.num(3));
        if let Some((c, a)) =
            crate::ofx_color::picker(&mut child, view, id.with("picker"), rgb, alpha, width)
        {
            let mut v = c.to_vec();
            if let Some(a) = a {
                v.push(a);
            }
            set_now(info, self.fx, self.place, Value::Num(v));
            self.touched.push(info.name.clone());
        }
    }

    /// Parameter `info`'s own interface (BananaV's extension), `size` big,
    /// under its row — from the control's column, over the number's — drawn
    /// as its plug-in draws it and given the pen. What the plug-in changes
    /// goes to the effect.
    fn interact_row(&mut self, ui: &mut Ui, info: &ParamInfo, size: [f64; 2], id: egui::Id) {
        use crate::ofx_controls::SLIDER_W;
        ui.label("");
        let (w, h) = (size[0] as f32, size[1] as f32);
        let (cell, _) = ui.allocate_exact_size(vec2(SLIDER_W.min(w), h), Sense::hover());
        let rect = Rect::from_min_size(cell.min, vec2(w, h));
        ui.label("");
        ui.label("");
        ui.end_row();
        let Some(instance) = self
            .ofx
            .editors
            .get(self.key)
            .map(|e| Arc::clone(&e.instance))
        else {
            return;
        };
        let bg = theme::panel();
        let view = View {
            time: frames(self.place.time, self.place.rate),
            render_scale: 1.0,
            pixel_scale: [1.0, 1.0],
            background: [bg.r(), bg.g(), bg.b()].map(|c| c as f64 / 255.0),
            viewport: [w as i32, h as i32],
            to_frame: None,
        };
        // Its pixels count up from its bottom left corner.
        let local = |q: Pos2| [(q.x - rect.left()) as f64, (rect.bottom() - q.y) as f64];
        let whole = |q: Pos2| {
            let [x, y] = local(q);
            [x as i32, y as i32]
        };
        let resp = ui.interact(rect, id.with("interact"), Sense::click_and_drag());
        let label = say(self.own, &info.label);
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, info.enabled, &label)
        });
        let (pressed, down, released, pos) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer.interact_pos().or(i.pointer.hover_pos()),
            )
        });
        let (pen_id, at_id) = (id.with("pen"), id.with("pen-at"));
        let mut holding = ui.data(|d| d.get_temp::<bool>(pen_id).unwrap_or(false));
        let mut answer = None;
        if info.enabled
            && let Some(q) = pos
        {
            let pen =
                |action| instance.param_pen(&info.name, action, &view, local(q), whole(q), 1.0);
            let last: Option<Pos2> = ui.data(|d| d.get_temp(at_id));
            if pressed && resp.hovered() {
                answer = Some(pen(PenAction::Down));
                holding = true;
            } else if holding && released {
                answer = Some(pen(PenAction::Up));
                holding = false;
            } else if (holding && down || resp.hovered()) && last != Some(q) {
                answer = Some(pen(PenAction::Motion));
            }
            ui.data_mut(|d| {
                d.insert_temp(pen_id, holding);
                d.insert_temp(at_id, q);
            });
        }
        if let Some(a) = answer
            && !a.changed.is_empty()
        {
            let values = instance.values();
            for name in &a.changed {
                if let Some((_, v)) = values.iter().find(|(n, _)| n == name)
                    && let Some(p) = plugin_param(v, self.place.rate)
                {
                    self.fx.params.insert(name.clone(), p);
                }
            }
            if let Some(e) = self.ofx.editors.get_mut(self.key) {
                e.values = self.fx.params.clone();
            }
            ui.ctx().request_repaint();
        }
        let marks = instance.param_draw(&info.name, &view);
        let mut painter = ui.painter_at(rect);
        if !info.enabled {
            painter.multiply_opacity(0.5);
        }
        paint_marks(&painter, &marks, |[x, y]| {
            pos2(rect.left() + x as f32, rect.bottom() - y as f32)
        });
        if bv_ofx::take_redraw() {
            ui.ctx().request_repaint();
        }
    }

    /// A parametric parameter's row: its curves' editor (in the name's and
    /// the control's cells). No clock: curves animate as a whole, through
    /// the window's Animate button (see [`animate_button`]).
    fn curves_row(&mut self, ui: &mut Ui, info: &ParamInfo) {
        let own = self.own;
        let label = say(own, &info.label);
        let labels: Vec<String> = (0..info.curves as usize)
            .map(|k| {
                info.dimension_labels
                    .get(k)
                    .map_or_else(|| (k + 1).to_string(), |l| say(own, l))
            })
            .collect();
        let id = egui::Id::new(("ofx-param", &self.key.id, self.key.index, &info.name));
        let mut curves = match value_now(info, self.fx, self.place) {
            Value::Curves(c) => c,
            _ => Vec::new(),
        };
        curves.resize(info.curves.max(1) as usize, Vec::new());
        let view = self
            .ofx
            .curve_views
            .entry((self.key.clone(), info.name.clone()))
            .or_default();
        if let Some(c) = crate::ofx_curves::editor(ui, view, id, info, &curves, &label, &labels) {
            set_now(info, self.fx, self.place, Value::Curves(c));
            self.touched.push(info.name.clone());
        }
        ui.label("");
        ui.label("");
        ui.end_row();
    }

    /// A parameter's two middle cells: its control and its number. The new
    /// value when the user changed it.
    fn cells(
        &mut self,
        ui: &mut Ui,
        info: &ParamInfo,
        v: &Value,
        id: egui::Id,
        label: &str,
    ) -> Option<Value> {
        use crate::ofx_controls::{CHOICE_W, FIELD_W, SLIDER_W, field, slider};
        let enabled = info.enabled;
        let range = |i: usize| -> Option<(f64, f64)> {
            let get = |a: &[f64], b: &[f64]| {
                let (lo, hi) = (*a.get(i)?, *b.get(i)?);
                (lo.is_finite() && hi.is_finite() && hi > lo && hi - lo <= 1e7).then_some((lo, hi))
            };
            get(&info.display_min, &info.display_max).or_else(|| get(&info.min, &info.max))
        };
        let limits = |i: usize, x: f64| {
            let lo = info.min.get(i).copied().unwrap_or(f64::MIN);
            let hi = info.max.get(i).copied().unwrap_or(f64::MAX);
            x.clamp(lo.min(hi), hi.max(lo))
        };
        let empty = |ui: &mut Ui| {
            ui.label("");
        };
        let mut new: Option<Value> = None;
        match info.ty {
            ParamType::Double | ParamType::Integer => {
                let mut picked: Option<f64> = None;
                let int = info.ty == ParamType::Integer;
                let digits = if int {
                    0
                } else {
                    info.digits.clamp(0, 6) as usize
                };
                let mut x = v.num(0);
                match range(0) {
                    Some(r) => {
                        let reset = info.default.num(0);
                        ui.add_enabled_ui(enabled, |ui| {
                            let log = info.logarithmic;
                            if slider(ui, &mut x, r, reset, int, label, log).changed() {
                                picked = Some(x);
                            }
                        });
                    }
                    None => empty(ui),
                }
                ui.add_enabled_ui(enabled, |ui| {
                    let text = crate::fmt::num(x, digits);
                    if let Some(t) = field(ui, id.with("number"), &text, FIELD_W, label)
                        && let Some(typed) = crate::fmt::parse(&t)
                    {
                        picked = Some(typed);
                    }
                });
                picked.map(|x| {
                    let x = limits(0, x);
                    Value::Num(vec![if int { x.round() } else { x }])
                })
            }
            ParamType::Boolean => {
                let mut on = v.num(0) != 0.0;
                ui.add_enabled_ui(enabled, |ui| {
                    if crate::widgets::checkbox(ui, &mut on, "").changed() {
                        new = Some(Value::Num(vec![on as i32 as f64]));
                    }
                });
                empty(ui);
                new
            }
            ParamType::Choice => {
                let mut i = v.num(0).round().max(0.0) as usize;
                let before = i;
                let text = info
                    .options
                    .get(i)
                    .map(|o| say(self.own, o))
                    .unwrap_or_default();
                ui.add_enabled_ui(enabled, |ui| {
                    egui::ComboBox::from_id_salt(id.with("choice"))
                        .width(CHOICE_W)
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            for (j, o) in info.options.iter().enumerate() {
                                ui.selectable_value(&mut i, j, say(self.own, o));
                            }
                        });
                });
                empty(ui);
                (i != before).then(|| Value::Num(vec![i as f64]))
            }
            ParamType::StrChoice => {
                let current = v.text().to_string();
                let mut picked = current.clone();
                let shown = info
                    .option_values
                    .iter()
                    .position(|o| *o == current)
                    .and_then(|j| info.options.get(j))
                    .map(|o| say(self.own, o))
                    .unwrap_or(current.clone());
                ui.add_enabled_ui(enabled, |ui| {
                    egui::ComboBox::from_id_salt(id.with("strchoice"))
                        .width(CHOICE_W)
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (j, o) in info.options.iter().enumerate() {
                                let value = info.option_values.get(j).unwrap_or(o);
                                ui.selectable_value(&mut picked, value.clone(), say(self.own, o));
                            }
                        });
                });
                empty(ui);
                (picked != current).then_some(Value::Text(picked))
            }
            ParamType::Rgb | ParamType::Rgba => {
                // VEGAS's: the swatch (a click opens or closes the picker)
                // and the numbers in the picker's color model.
                let rgb = [v.num(0), v.num(1), v.num(2)];
                let alpha = (info.ty == ParamType::Rgba).then(|| v.num(3));
                let open_id = id.with("open");
                ui.add_enabled_ui(enabled, |ui| {
                    let r =
                        crate::ofx_color::swatch(ui, rgb, alpha.unwrap_or(1.0), SLIDER_W, label);
                    if r.clicked() {
                        let open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
                        ui.data_mut(|d| d.insert_temp(open_id, !open));
                    }
                    // A double click puts the plug-in's color back, as a
                    // slider's puts its number (its two clicks leave the
                    // picker as it was).
                    if r.double_clicked() && *v != info.default {
                        new = Some(info.default.clone());
                    }
                });
                let view = self
                    .ofx
                    .color_views
                    .entry((self.key.clone(), info.name.clone()))
                    .or_insert_with(|| crate::ofx_color::ColorView::starting_in(&info.color_model));
                ui.add_enabled_ui(enabled, |ui| {
                    let text = crate::ofx_color::text(view, rgb, alpha);
                    if let Some(t) = field(ui, id.with("colour"), &text, FIELD_W, label)
                        && let Some((c, a)) = crate::ofx_color::parse(view, &t, alpha.is_some())
                    {
                        let mut n = c.to_vec();
                        if let Some(old) = alpha {
                            n.push(a.unwrap_or(old));
                        }
                        new = Some(Value::Num(n));
                    }
                });
                new
            }
            ParamType::Double3D if info.interact.is_some() => {
                empty(ui);
                let digits: Vec<usize> = (0..3).map(|i| number_digits(info, i)).collect();
                let shown: Vec<f64> = (0..3).map(|i| v.num(i)).collect();
                ui.add_enabled_ui(enabled, |ui| {
                    let text = numbers_each(&shown, &digits);
                    if let Some(t) = field(ui, id.with("triple"), &text, FIELD_W, label)
                        && let Some(n) = parse_numbers(&t, 3)
                    {
                        new = Some(Value::Num(
                            n.iter().enumerate().map(|(i, &x)| limits(i, x)).collect(),
                        ));
                    }
                });
                new
            }
            ParamType::Double2D if is_position(info) || info.interact.is_some() => {
                empty(ui);
                // A position counts against the frame; others as they are,
                // each number with its own decimals ("103.5, 0.500").
                let (scale, digits) = if is_position(info) {
                    let d = info.digits.clamp(0, 6) as usize;
                    (position_scale(info, self.place), [d, d])
                } else {
                    ([1.0, 1.0], [number_digits(info, 0), number_digits(info, 1)])
                };
                let shown = [v.num(0) / scale[0], v.num(1) / scale[1]];
                ui.add_enabled_ui(enabled, |ui| {
                    let text = pair_text(shown, digits);
                    if let Some(t) = field(ui, id.with("pair"), &text, FIELD_W, label)
                        && let Some(p) = parse_pair(&t)
                    {
                        new = Some(Value::Num(vec![
                            limits(0, p[0] * scale[0]),
                            limits(1, p[1] * scale[1]),
                        ]));
                    }
                });
                new
            }
            ParamType::Double2D
            | ParamType::Integer2D
            | ParamType::Double3D
            | ParamType::Integer3D => {
                let n = info.ty.dimension();
                let int = info.ty.is_int();
                let mut xs: Vec<f64> = (0..n).map(|i| v.num(i)).collect();
                let mut changed = false;
                ui.add_enabled_ui(enabled, |ui| {
                    ui.horizontal(|ui| {
                        for (i, x) in xs.iter_mut().enumerate() {
                            if let Some(l) = info.dimension_labels.get(i) {
                                ui.label(RichText::new(l).color(theme::text_dim()));
                            }
                            let speed = if int {
                                1.0
                            } else {
                                info.increment.abs().max(1e-3) * 0.01
                            };
                            let mut d = egui::DragValue::new(x).speed(speed);
                            if !int {
                                d = d.max_decimals(info.digits.clamp(0, 6) as usize);
                            }
                            changed |= ui.add(d).changed();
                        }
                    });
                });
                empty(ui);
                changed.then(|| {
                    Value::Num(
                        xs.iter()
                            .enumerate()
                            .map(|(i, &x)| {
                                let x = limits(i, x);
                                if int { x.round() } else { x }
                            })
                            .collect(),
                    )
                })
            }
            ParamType::String => {
                let mut s = v.text().to_string();
                let mode = info.string_mode.as_str();
                if mode == "OfxParamStringIsLabel" {
                    ui.label(s);
                    empty(ui);
                    return None;
                }
                if mode == "OfxParamStringIsMultiLine" || mode == "OfxParamStringIsRichTextFormat" {
                    let mut changed = false;
                    let rich = !info.rich_text.is_empty();
                    ui.add_enabled_ui(enabled, |ui| {
                        // As wide as a slider at least, however long its
                        // name (GL Transition's code).
                        let out = egui::TextEdit::multiline(&mut s)
                            .desired_rows(3)
                            .desired_width(SLIDER_W)
                            .min_size(vec2(SLIDER_W, 0.0))
                            .lock_focus(true)
                            .show(ui);
                        changed = out.response.changed();
                        // Rich text: the part selected, kept while the
                        // other settings are worked (see `rich_text`).
                        if rich && out.response.has_focus() {
                            let picked = out
                                .cursor_range
                                .map(|r| {
                                    let (a, b) = (r.primary.index.0, r.secondary.index.0);
                                    (a.min(b), a.max(b))
                                })
                                .filter(|(a, b)| a < b);
                            let key = id.with("selection");
                            ui.data_mut(|d| match picked {
                                Some(x) => {
                                    d.insert_temp(key, x);
                                }
                                None => d.remove::<(usize, usize)>(key),
                            });
                        }
                    });
                    empty(ui);
                    return changed.then_some(Value::Text(s));
                }
                // As VEGAS draws one: a file's or a folder's Browse… where a
                // slider goes, the text where a number goes (taken when it
                // is left).
                let file = mode == "OfxParamStringIsFilePath";
                let folder = mode == "OfxParamStringIsDirectoryPath";
                let mut picked = None;
                if file || folder {
                    ui.add_enabled_ui(enabled, |ui| {
                        let h = ui.spacing().interact_size.y;
                        let browse = egui::Button::new(tr("Browse…"));
                        if ui.add_sized(vec2(SLIDER_W, h), browse).clicked() {
                            let d = rfd::FileDialog::new();
                            let path = if folder {
                                d.pick_folder()
                            } else if info.file_to_write {
                                d.save_file()
                            } else {
                                d.pick_file()
                            };
                            picked = path.map(|p| p.display().to_string());
                        }
                    });
                } else {
                    empty(ui);
                }
                ui.add_enabled_ui(enabled, |ui| {
                    if let Some(t) = field(ui, id.with("text"), &s, FIELD_W, label) {
                        picked = Some(t);
                    }
                });
                picked.map(Value::Text)
            }
            ParamType::PushButton => {
                // A push: no value, the plug-in hears it. One asking for a
                // picture BananaV cannot give yet waits greyed.
                let ready = picture_ready(info);
                let mut pushed = false;
                ui.add_enabled_ui(enabled && ready, |ui| {
                    let r = ui.button(label);
                    let r = if ready {
                        r
                    } else {
                        r.on_disabled_hover_text(tr("Coming soon to BananaV"))
                    };
                    pushed = r.clicked();
                });
                empty(ui);
                pushed.then_some(Value::None)
            }
            _ => {
                empty(ui);
                empty(ui);
                None
            }
        }
    }
}

/// Where the effects of `target` work, at the cursor.
pub fn place_of(target: FxTarget, p: &bv_timeline::Project, cursor: Tick) -> FxPlace {
    if let FxTarget::Transition(clip) = target
        && let Some(place) = transition_place(p, clip, cursor)
    {
        return place;
    }
    let s = &p.settings;
    let frame = (s.width, s.height);
    let media_size = |m: Option<&bv_timeline::MediaInfo>| {
        m.and_then(|m| m.video)
            .map_or(frame, |v| (v.width, v.height))
    };
    let whole = frame_map(s.width, s.height);
    let mut span = None;
    let mut origin = None;
    let (time, duration, size, map) = match target {
        // An event's effects work on its framed picture, the frame's
        // (VEGAS's default: Pan/Crop first in the event's chain).
        FxTarget::Clip(id) => match p.clip(id) {
            Some(c) => {
                let local = (cursor - c.start).clamp(Tick::ZERO, c.duration);
                origin = Some(c.start);
                (local, c.duration, frame, whole)
            }
            None => (Tick::ZERO, Tick::ZERO, frame, whole),
        },
        FxTarget::Media(id, _) => {
            let m = p.media(id);
            // Shown as the event of it at the cursor shows it, at the
            // media's time there.
            let event = p
                .clips()
                .find(|(_, c)| c.media == id && c.range().contains(cursor))
                .map(|(_, c)| c);
            let time = event
                .zip(m)
                .and_then(|(c, m)| bv_timeline::scene::media_time(c, m, cursor))
                .unwrap_or(Tick::ZERO);
            if let Some(c) = event {
                span = Some((c.source_in, c.source_in + c.to_source(c.duration)));
                origin = (c.rate == 1.0 && c.freeze.is_none()).then(|| c.start - c.source_in);
            }
            let map = event.map_or(whole, |c| clip_map(p, c, cursor));
            (
                time,
                m.map_or(Tick::ZERO, |m| m.duration),
                media_size(m),
                map,
            )
        }
        FxTarget::Track(_)
        | FxTarget::VideoOutput
        | FxTarget::Bus(_)
        | FxTarget::Chain(_)
        | FxTarget::InputBus(_)
        | FxTarget::Master
        | FxTarget::NewChain
        | FxTarget::Transition(_) => {
            origin = Some(Tick::ZERO);
            (cursor, p.duration(), frame, whole)
        }
    };
    FxPlace {
        rate: s.frame_rate,
        time,
        duration,
        size,
        map,
        span: span.unwrap_or((Tick::ZERO, duration)),
        origin,
    }
}

/// Where the transition event `clip` comes in with works: its time at the
/// cursor (from the transition's start) and its length.
pub fn transition_place(p: &Project, clip: ClipId, cursor: Tick) -> Option<FxPlace> {
    let c = p.clip(clip)?;
    let track = p.clip_track(clip)?;
    let (fade_in, _) = track.effective_fades(c, p.auto_crossfade);
    let length = if fade_in > Tick::ZERO {
        fade_in
    } else {
        c.fade_in
    };
    let s = &p.settings;
    Some(FxPlace {
        rate: s.frame_rate,
        time: (cursor - c.start).clamp(Tick::ZERO, length),
        duration: length,
        size: (s.width, s.height),
        map: frame_map(s.width, s.height),
        span: (Tick::ZERO, length),
        origin: Some(c.start),
    })
}

/// Where the media event `clip` shows (generated by a plug-in) works: the
/// media's time at the cursor, its length and frame size.
pub fn generator_place(p: &Project, clip: ClipId, cursor: Tick) -> Option<FxPlace> {
    let c = p.clip(clip)?;
    let m = p.media(c.media)?;
    let at = cursor.clamp(c.start, c.end());
    let time = bv_timeline::scene::media_time(c, m, at).unwrap_or(Tick::ZERO);
    let size = m.video.map_or((p.settings.width, p.settings.height), |v| {
        (v.width, v.height)
    });
    Some(FxPlace {
        rate: p.settings.frame_rate,
        time,
        duration: m.duration,
        size,
        map: clip_map(p, c, cursor),
        // The media's times the event shows.
        span: (c.source_in, c.source_in + c.to_source(c.duration)),
        origin: (c.rate == 1.0 && c.freeze.is_none()).then(|| c.start - c.source_in),
    })
}

/// A change made in the keyframe controller. A keyframe is its
/// parameter's name and its time.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyEdit {
    /// A keyframe at a time for every animated parameter (its value there).
    Add(Tick),
    /// A keyframe at a time for one parameter (its value there).
    AddTo(String, Tick),
    /// A parameter's keyframe takes a value.
    Value(String, Tick, Vec<f64>),
    /// A parameter's keyframes are these now (moved, copied or changed by
    /// a drag).
    Keys(String, Vec<PluginKey>),
    /// Keyframes take an interpolation curve.
    Curve(Vec<(String, Tick)>, KeyCurve),
    /// Keyframes go.
    Remove(Vec<(String, Tick)>),
    /// Keyframes come in (pasted), replacing any of their parameter at
    /// their times.
    Insert(Vec<(String, PluginKey)>),
}

/// A keyframe at `at` for `p`, with its value there (numbers; a text has
/// no in-between) and the curve of the keyframe before it, as VEGAS's —
/// unless it has one there.
fn add_key(p: &mut PluginParam, at: Tick, rate: FrameRate) {
    if p.keys.iter().any(|k| k.at == at) {
        return;
    }
    let ty = match &p.value {
        bv_timeline::PluginValue::Num(v) => match v.len() {
            2 => ParamType::Double2D,
            3 => ParamType::Double3D,
            4 => ParamType::Rgba,
            _ => ParamType::Double,
        },
        bv_timeline::PluginValue::Curves(_) => ParamType::Parametric,
        _ => return,
    };
    let now = animation(p, rate).at(frames(at, rate), ty);
    let Some(value) = bv_engine::plugins::plugin_value(&now) else {
        return;
    };
    let i = p.keys.partition_point(|k| k.at < at);
    let curve = i
        .checked_sub(1)
        .and_then(|b| p.keys.get(b))
        .map_or(KeyCurve::Linear, |k| k.curve);
    p.keys.insert(i, PluginKey::new(at, value, curve));
}

/// The keyframes `edit` adds to custom parameters (Mesh Warp's mesh),
/// whose values between keyframes only their plug-in knows: its
/// instance's, with the curve of the keyframe before.
fn custom_keys(
    instance: &Instance,
    infos: &[ParamInfo],
    fx: &PluginFx,
    edit: &KeyEdit,
    rate: FrameRate,
) -> Vec<(String, PluginKey)> {
    let (only, at) = match edit {
        KeyEdit::Add(at) => (None, *at),
        KeyEdit::AddTo(name, at) => (Some(name.as_str()), *at),
        _ => return Vec::new(),
    };
    infos
        .iter()
        .filter(|i| i.ty == ParamType::Custom && only.is_none_or(|n| n == i.name))
        .filter_map(|i| {
            let p = fx
                .params
                .get(&i.name)
                .filter(|p| !p.keys.is_empty() && !p.keys.iter().any(|k| k.at == at))?;
            let now = instance.params().get(&i.name)?.value_at(frames(at, rate));
            let value = bv_engine::plugins::plugin_value(&now)?;
            let curve = p
                .keys
                .iter()
                .rev()
                .find(|k| k.at < at)
                .map_or(KeyCurve::Linear, |k| k.curve);
            Some((i.name.clone(), PluginKey::new(at, value, curve)))
        })
        .collect()
}

/// `fx` with `edit` made (`rate`: its owner's frame rate).
fn edit_keys(fx: &PluginFx, edit: &KeyEdit, rate: FrameRate) -> PluginFx {
    let mut new = fx.clone();
    match edit {
        KeyEdit::Add(at) => {
            for p in new.params.values_mut().filter(|p| !p.keys.is_empty()) {
                add_key(p, *at, rate);
            }
        }
        KeyEdit::AddTo(name, at) => {
            if let Some(p) = new.params.get_mut(name) {
                add_key(p, *at, rate);
            }
        }
        KeyEdit::Value(name, at, v) => {
            if let Some(p) = new.params.get_mut(name) {
                for k in p.keys.iter_mut().filter(|k| k.at == *at) {
                    k.value = bv_timeline::PluginValue::Num(v.clone());
                }
            }
        }
        KeyEdit::Keys(name, keys) => {
            if let Some(p) = new.params.get_mut(name) {
                p.keys = keys.clone();
                p.keys.sort_by_key(|k| k.at);
                p.keys.dedup_by_key(|k| k.at);
            }
        }
        KeyEdit::Curve(keys, curve) => {
            for (name, at) in keys {
                if let Some(p) = new.params.get_mut(name)
                    && let Some(i) = p.keys.iter().position(|k| k.at == *at)
                {
                    // A Bézier keyframe's handles keep the curve as it was.
                    bv_engine::plugins::set_curve(p, i, *curve, rate);
                }
            }
        }
        KeyEdit::Remove(keys) => {
            for (name, at) in keys {
                let Some(p) = new.params.get_mut(name) else {
                    continue;
                };
                if let Some(i) = p.keys.iter().position(|k| k.at == *at) {
                    let gone = p.keys.remove(i);
                    // Without keyframes, the value stays the last one's.
                    if p.keys.is_empty() {
                        p.value = gone.value;
                    }
                }
            }
        }
        KeyEdit::Insert(keys) => {
            for (name, key) in keys {
                let p = new
                    .params
                    .entry(name.clone())
                    .or_insert_with(|| PluginParam {
                        value: key.value.clone(),
                        keys: Vec::new(),
                    });
                p.keys.retain(|k| k.at != key.at);
                let i = p.keys.partition_point(|k| k.at < key.at);
                p.keys.insert(i, key.clone());
            }
        }
    }
    new
}

/// The keyframe controller of the plug-in effect `key`, under its
/// settings (see [`crate::ofx_keys`]), `height` high, when it has animated
/// parameters. Returns the changed effect and where to move the
/// timeline's cursor.
pub fn keyframes(
    ui: &mut Ui,
    ofx: &mut OfxUi,
    key: &EditorKey,
    fx: &PluginFx,
    place: &FxPlace,
    height: f32,
) -> (Option<PluginFx>, Option<Tick>) {
    let infos: Vec<ParamInfo> = ofx
        .editors
        .get(key)
        .map(|e| e.instance.param_infos())
        .unwrap_or_default();
    let own = bv_ofx::registry()
        .info(&fx.id)
        .is_some_and(|i| bv_engine::plugins::is_own(&i));
    let rows: Vec<KeyRow<'_>> = infos
        .iter()
        // What the plug-in hides has no row either.
        .filter(|i| !i.secret)
        .filter_map(|i| {
            let p = fx.params.get(&i.name).filter(|p| !p.keys.is_empty())?;
            // A position's numbers count against the frame, as its box
            // shows them.
            let (ranges, scale) = if is_position(i) {
                let s = position_scale(i, place);
                (vec![(0.0, s[0]), (0.0, s[1])], s.to_vec())
            } else {
                (curve_ranges(i, p), vec![1.0; i.ty.dimension().max(1)])
            };
            // Its numbers' names: the plug-in's (a wheel's Angle and
            // Magnitude), the letters it was given as they are written.
            let parts = i
                .dimension_labels
                .iter()
                .map(|l| {
                    if l.chars().count() == 1 {
                        l.to_uppercase()
                    } else {
                        say(own, l)
                    }
                })
                .collect();
            Some(KeyRow {
                name: &i.name,
                label: say(own, &i.label),
                param: p,
                ty: i.ty,
                ranges,
                scale,
                parts,
            })
        })
        .collect();
    if rows.is_empty() {
        return (None, None);
    }
    let title = if own {
        tr(&fx.name).to_string()
    } else {
        fx.name.clone()
    };
    let view = ofx.keys.entry(key.clone()).or_default();
    let id = egui::Id::new(("ofx-keys", &key.id, key.index));
    let answer = crate::ofx_keys::controller(
        ui,
        view,
        id,
        &title,
        &rows,
        place,
        &mut ofx.key_clip,
        height,
    );
    let mut new = fx.clone();
    for e in &answer.edits {
        // Pasted keyframes go only where this effect has their parameter.
        let e = match e {
            KeyEdit::Insert(keys) => KeyEdit::Insert(
                keys.iter()
                    .filter(|(n, _)| infos.iter().any(|i| i.name == *n && i.ty.has_value()))
                    .cloned()
                    .collect(),
            ),
            e => e.clone(),
        };
        let custom = ofx
            .editors
            .get(key)
            .map(|ed| custom_keys(&ed.instance, &infos, &new, &e, place.rate))
            .unwrap_or_default();
        new = edit_keys(&new, &e, place.rate);
        if !custom.is_empty() {
            new = edit_keys(&new, &KeyEdit::Insert(custom), place.rate);
        }
    }
    let changed = (new != *fx).then_some(new);
    // Where the cursor goes: the timeline's, or the controller's own while
    // it is not in sync.
    let seek = answer.seek.and_then(|t| Some(place.origin? + t));
    if !view.sync {
        if view.local.is_none() {
            view.local = place.origin.map(|o| o + place.time);
        }
        if let Some(t) = seek {
            view.local = Some(t);
        }
        return (changed, None);
    }
    view.local = None;
    (changed, seek)
}

/// The values each number of a parameter spans in the Curves view: its
/// range within its limits, or its keyframes' (with some room).
fn curve_ranges(info: &ParamInfo, p: &PluginParam) -> Vec<(f64, f64)> {
    let span = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() && b > a && b - a <= 1e7 => {
            Some((a, b))
        }
        _ => None,
    };
    (0..info.ty.dimension().max(1))
        .map(|d| {
            let at = |v: &[f64]| v.get(d).or(v.first()).copied();
            let display = span(at(&info.display_min), at(&info.display_max));
            let limits = span(at(&info.min), at(&info.max));
            match (display, limits) {
                (Some((a, b)), Some((c, e))) if b.min(e) > a.max(c) => (a.max(c), b.min(e)),
                (Some(r), _) | (None, Some(r)) => r,
                (None, None) => keys_range(p, d),
            }
        })
        .collect()
}

/// The values number `d` of a parameter takes (its keyframes' and its
/// own), with some room.
fn keys_range(p: &PluginParam, d: usize) -> (f64, f64) {
    let values: Vec<f64> = std::iter::once(&p.value)
        .chain(p.keys.iter().map(|k| &k.value))
        .filter_map(|v| match v {
            bv_timeline::PluginValue::Num(n) => n.get(d).copied(),
            _ => None,
        })
        .collect();
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    let pad = ((hi - lo) * 0.15).max(1e-3 * (1.0 + hi.abs()));
    (lo - pad, hi + pad)
}

/// The plug-in effect `key` is for, as the project has it now.
fn current_fx(p: &Project, key: &EditorKey) -> Option<PluginFx> {
    match key.owner {
        EditorOwner::Fx(target) => target.chain(p)?.get(key.index)?.plugin.as_deref().cloned(),
        EditorOwner::Transition(clip) => p.clip(clip)?.transition_fx.as_deref().cloned(),
        EditorOwner::Generator(media) => match &p.media(media)?.source {
            MediaSource::Generated(Generator::Plugin(fx)) => Some((**fx).clone()),
            _ => None,
        },
        EditorOwner::Compositor(track, parent) => {
            let t = p.track(track)?;
            let fx = if parent {
                &t.parent_compositor
            } else {
                &t.compositor
            };
            fx.as_deref().cloned()
        }
    }
}

/// Gives the project `fx` where `key` says (an edit of its settings).
fn set_fx(app: &mut BananaApp, key: &EditorKey, fx: PluginFx) {
    let id = egui::Id::new(("ofx-overlay", key.clone()));
    let index = key.index;
    match key.owner {
        EditorOwner::Fx(target) => {
            let Some(mut chain) = target.chain(app.project()) else {
                return;
            };
            let Some(e) = chain.get_mut(index) else {
                return;
            };
            e.plugin = Some(Box::new(fx));
            app.set_fx_params(
                id,
                chain,
                move |p| target.chain(p),
                move |p, c| target.set_chain(p, c),
            );
        }
        EditorOwner::Transition(clip) => app.modify_merged(id, |p| {
            if let Some(c) = p.clip_mut(clip) {
                c.transition_fx = Some(Box::new(fx));
            }
        }),
        EditorOwner::Generator(media) => app.modify_merged(id, |p| {
            if let Some(m) = p.media.iter_mut().find(|m| m.id == media) {
                m.source = MediaSource::Generated(Generator::Plugin(Box::new(fx)));
            }
        }),
        EditorOwner::Compositor(track, parent) => app.modify_merged(id, |p| {
            if let Some(t) = p.track_by_id_mut(track) {
                let slot = if parent {
                    &mut t.parent_compositor
                } else {
                    &mut t.compositor
                };
                *slot = Some(Box::new(fx));
            }
        }),
    }
}

/// An overlay color as the screen's.
fn color32(c: [f32; 4]) -> Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(b(c[0]), b(c[1]), b(c[2]), b(c[3]))
}

/// Where a text's point is, as the draw suite's alignment flags say.
fn text_align(flags: i32) -> egui::Align2 {
    use egui::Align;
    let x = match flags & 3 {
        3 => Align::Center,
        2 => Align::Max,
        _ => Align::Min,
    };
    let y = match flags & (4 | 8 | 16) {
        f if f & 4 != 0 && f & 16 != 0 => Align::Center,
        4 => Align::Min,
        _ => Align::Max,
    };
    egui::Align2([x, y])
}

/// Draws an overlay's marks, `to_screen` placing canonical points.
fn paint_marks(painter: &egui::Painter, marks: &[Mark], to_screen: impl Fn([f64; 2]) -> Pos2) {
    let line = |points: Vec<Pos2>, pen: &bv_ofx::Pen| {
        let stroke = Stroke::new(pen.width.max(0.5), color32(pen.colour));
        if pen.stipple == 0 {
            painter.add(egui::Shape::line(points, stroke));
        } else {
            let (dash, gap) = match pen.stipple {
                1 => (1.5, 2.5),
                3 => (3.0, 5.0),
                _ => (5.0, 3.0),
            };
            painter.extend(egui::Shape::dashed_line(&points, stroke, dash, gap));
        }
    };
    let filled = |points: Vec<Pos2>, colour: [f32; 4]| {
        // A fan of triangles from the first point.
        let mut mesh = egui::Mesh::default();
        for p in &points {
            mesh.colored_vertex(*p, color32(colour));
        }
        for i in 1..points.len().saturating_sub(1) as u32 {
            mesh.add_triangle(0, i, i + 1);
        }
        painter.add(egui::Shape::mesh(mesh));
    };
    let corners = |a: [f64; 2], b: [f64; 2]| [a, [b[0], a[1]], b, [a[0], b[1]]];
    for m in marks {
        match m {
            Mark::Lines { points, pen } => {
                for [a, b] in points.as_chunks::<2>().0 {
                    line(vec![to_screen(*a), to_screen(*b)], pen);
                }
            }
            Mark::Strip {
                points,
                closed,
                pen,
            } => {
                let mut pts: Vec<Pos2> = points.iter().map(|p| to_screen(*p)).collect();
                if *closed && let Some(&first) = pts.first() {
                    pts.push(first);
                }
                line(pts, pen);
            }
            Mark::Rect { a, b, colour } => {
                filled(corners(*a, *b).map(&to_screen).to_vec(), *colour);
            }
            Mark::Polygon { points, colour } => {
                filled(points.iter().map(|p| to_screen(*p)).collect(), *colour);
            }
            Mark::Ellipse { a, b, pen } => {
                let (c, r) = (
                    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
                    [(b[0] - a[0]).abs() / 2.0, (b[1] - a[1]).abs() / 2.0],
                );
                let pts = (0..=48)
                    .map(|i| {
                        let t = std::f64::consts::TAU * i as f64 / 48.0;
                        to_screen([c[0] + r[0] * t.cos(), c[1] + r[1] * t.sin()])
                    })
                    .collect();
                line(pts, pen);
            }
            Mark::Text {
                text,
                at,
                align,
                colour,
            } => {
                painter.text(
                    to_screen(*at),
                    text_align(*align),
                    text,
                    egui::FontId::proportional(12.0),
                    color32(*colour),
                );
            }
            // A picture of the whole view: `overlay_ui` lays it.
            Mark::Picture { .. } => {}
        }
    }
}

/// [`params`]' rich text (BananaV's `kBananaVParamPropRichText`): a text
/// parameter's runs follow its text as it is typed, and a parameter a part
/// may set, changed while part of the text is selected, sets that part (a
/// run) instead of the whole.
fn rich_text(
    ui: &Ui,
    key: &EditorKey,
    infos: &[ParamInfo],
    old: &PluginFx,
    new: &mut PluginFx,
    touched: &[String],
) {
    let text_of = |fx: &PluginFx, name: &str| {
        fx.params.get(name).and_then(|p| match &p.value {
            bv_timeline::PluginValue::Text(t) => Some(t.clone()),
            _ => None,
        })
    };
    for info in infos.iter().filter(|i| !i.rich_text.is_empty()) {
        let words: Vec<&str> = info.rich_text.split_whitespace().collect();
        let Some((&runs_name, styled)) = words.split_first() else {
            continue;
        };
        let had = text_of(new, runs_name).unwrap_or_default();
        let mut runs = had.clone();
        if touched.contains(&info.name) {
            let before =
                text_of(old, &info.name).unwrap_or_else(|| info.default.text().to_string());
            let after = text_of(new, &info.name).unwrap_or_else(|| before.clone());
            runs = shift_runs(&runs, &before, &after);
        }
        let picked_id =
            egui::Id::new(("ofx-param", &key.id, key.index, &info.name)).with("selection");
        if let Some((a, b)) = ui.data(|d| d.get_temp::<(usize, usize)>(picked_id)) {
            for name in styled.iter().filter(|n| touched.iter().any(|t| t == *n)) {
                let Some(p) = new.params.get(*name).filter(|p| p.keys.is_empty()) else {
                    continue;
                };
                let value = match &p.value {
                    bv_timeline::PluginValue::Num(v) => v.first().map(|x| {
                        if x.fract() == 0.0 {
                            format!("{}", *x as i64)
                        } else {
                            format!("{x}")
                        }
                    }),
                    bv_timeline::PluginValue::Text(t) => {
                        Some(t.replace([';', '=', '\n', '\r'], " "))
                    }
                    _ => None,
                };
                let Some(value) = value else {
                    continue;
                };
                runs.push_str(&format!("{a} {b} {name}={value}\n"));
                // The whole keeps its own.
                match old.params.get(*name) {
                    Some(o) => {
                        new.params.insert((*name).to_string(), o.clone());
                    }
                    None => {
                        new.params.remove(*name);
                    }
                }
            }
        }
        if runs != had {
            new.params.insert(
                runs_name.to_string(),
                PluginParam {
                    value: bv_timeline::PluginValue::Text(runs),
                    keys: Vec::new(),
                },
            );
        }
    }
}

/// Rich text's runs (lines `start end ...`) as the text goes from `old` to
/// `new`: those after what changed move with it, those across it stretch
/// or shrink, those inside what was deleted go.
pub(crate) fn shift_runs(runs: &str, old: &str, new: &str) -> String {
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let p = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let room = a.len().min(b.len()) - p;
    let s = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(room)
        .take_while(|(x, y)| x == y)
        .count();
    let (old_end, new_end) = (a.len() - s, b.len() - s);
    let place = |x: usize, end: bool| {
        if x <= p {
            x
        } else if x >= old_end {
            x - old_end + new_end
        } else if end {
            new_end
        } else {
            p
        }
    };
    let mut out = String::new();
    for line in runs.lines() {
        let mut parts = line.splitn(3, ' ');
        let start = parts.next().and_then(|v| v.parse::<usize>().ok());
        let end = parts.next().and_then(|v| v.parse::<usize>().ok());
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        let (start, end) = (place(start, false), place(end, true));
        if end > start {
            out.push_str(&format!("{start} {end} {}\n", parts.next().unwrap_or("")));
        }
    }
    out
}

/// Video Stabilization's Show Points (its Expert view): the features its
/// analysis followed at this frame, each with its track over the next
/// frames, those followed through fewer than Minimum Track Length frames
/// left out.
fn stabilization_points(
    ui: &Ui,
    app: &BananaApp,
    key: &EditorKey,
    place: &FxPlace,
    frame: Rect,
    area: Rect,
) {
    let Some(effect) = key.effect_key() else {
        return;
    };
    let Some(points) = app.stabilization_points.get(&effect) else {
        return;
    };
    let fx = match effect.owner {
        bv_timeline::scene::FxOwner::Event(c) => app
            .project()
            .clip(c)
            .and_then(|c| c.effects.get(effect.index))
            .and_then(|e| e.plugin.as_deref()),
        _ => None,
    };
    let Some(fx) = fx.filter(|f| f.id == effect.id) else {
        return;
    };
    let num = |name: &str| {
        fx.params.get(name).and_then(|p| match &p.value {
            bv_timeline::PluginValue::Num(v) => v.first().copied(),
            _ => None,
        })
    };
    if num("views") != Some(2.0) || num("showFeatures").unwrap_or(0.0) < 0.5 {
        return;
    }
    let least = num("showFeaturesMinLen").unwrap_or(1.0).max(1.0) as usize;
    let k = frames(place.time, place.rate).round().max(0.0) as usize;
    let (w, h) = (points.size.0.max(1) as f64, points.size.1.max(1) as f64);
    let (cw, ch) = (f64::from(place.size.0), f64::from(place.size.1));
    let to_screen = |p: [f32; 2]| {
        // The analysis's pixels (y down) to canonical ones (y up).
        let c = [f64::from(p[0]) / w * cw, (1.0 - f64::from(p[1]) / h) * ch];
        let [x, y] = place.map.apply(c);
        pos2(
            frame.left() + (x * frame.width() as f64) as f32,
            frame.top() + (y * frame.height() as f64) as f32,
        )
    };
    // VEGAS's: a green ring at each point followed long enough and a line
    // to where it went in the next frame.
    let painter = ui.painter_at(area);
    let color = egui::Color32::from_rgb(0x0A, 0xFF, 0x0A);
    let stroke = egui::Stroke::new(1.0, color);
    let reach = least.clamp(1, 1000) + 1;
    for path in points.tracks_at(k, reach) {
        // A track's length: the frames it was followed through.
        if path.len() - 1 < least {
            continue;
        }
        let (from, to) = (to_screen(path[0]), to_screen(path[1]));
        painter.circle_stroke(from, 2.0, stroke);
        painter.line_segment([from, to], stroke);
    }
}

/// The overlay of the plug-in settings open, over the Video Preview's
/// frame `frame` (on screen): drawn, and given the left mouse button when
/// it wants it, over all of the preview's picture area `area` — its
/// controls go on past the frame's edges, where they can still be seen and
/// grabbed. `nav` is the preview's own response to the mouse.
/// How far a standard handle reaches, in screen points, and its ring's
/// radius and cross's arms (the plug-ins' handles' look, `Draw::handle`).
const POSITION_REACH: f32 = 13.0;
const POSITION_RING: f32 = 8.0;
const POSITION_ARM: f32 = 13.0;

/// VEGAS's standard overlay: where a plug-in draws nothing of its own on
/// the preview, each of its positions shown (two-dimensional doubles of
/// the frame) is a handle on it — a ring with a cross — dragged to move
/// it, keyed at the cursor when it is animated, one undo step a drag.
fn position_handles(
    ui: &Ui,
    app: &mut BananaApp,
    key: &EditorKey,
    place: &FxPlace,
    nav: &egui::Response,
    frame: Rect,
    area: Rect,
) {
    let Some(instance) = app.ofx.editors.get(key).map(|e| Arc::clone(&e.instance)) else {
        return;
    };
    if instance.has_overlay() || frame.width() < 1.0 || frame.height() < 1.0 {
        return;
    }
    let positions: Vec<ParamInfo> = instance
        .param_infos()
        .into_iter()
        .filter(|p| is_position(p) && !p.secret && p.enabled)
        .collect();
    let Some(inv) = place.map.inverse() else {
        return;
    };
    let Some(mut fx) = current_fx(app.project(), key) else {
        return;
    };
    if positions.is_empty() {
        app.ofx.position_drag = None;
        return;
    }
    let scale = |p: &ParamInfo| {
        if p.double_type.contains("Normalised") {
            [place.size.0.max(1) as f64, place.size.1.max(1) as f64]
        } else {
            [1.0, 1.0]
        }
    };
    let to_screen = |c: [f64; 2]| {
        let [x, y] = place.map.apply(c);
        pos2(
            frame.left() + (x * frame.width() as f64) as f32,
            frame.top() + (y * frame.height() as f64) as f32,
        )
    };
    let to_canonical = |q: Pos2| {
        inv.apply([
            ((q.x - frame.left()) / frame.width()) as f64,
            ((q.y - frame.top()) / frame.height()) as f64,
        ])
    };
    let points: Vec<(&ParamInfo, Pos2)> = positions
        .iter()
        .filter_map(|p| match value_now(p, &fx, place) {
            Value::Num(v) if v.len() >= 2 => {
                let k = scale(p);
                Some((p, to_screen([v[0] * k[0], v[1] * k[1]])))
            }
            _ => None,
        })
        .collect();
    let (pressed, down, pos) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
        )
    });
    let nearest = |q: Pos2| {
        points
            .iter()
            .map(|(p, at)| (p.name.clone(), *at, at.distance(q)))
            .filter(|c| c.2 <= POSITION_REACH)
            .min_by(|a, b| a.2.total_cmp(&b.2))
    };
    let over = |q: Pos2| area.contains(q) && (nav.hovered() || nav.is_pointer_button_down_on());
    if let Some(q) = pos {
        if pressed && over(q) {
            if let Some((name, at, _)) = nearest(q) {
                app.ofx.position_drag = Some((name, q - at));
                app.ofx.pen_took = true;
            }
        } else if !down {
            app.ofx.position_drag = None;
        }
        app.ofx.position_hover = if over(q) {
            nearest(q).map(|c| c.0)
        } else {
            None
        };
        if down && let Some((name, grip)) = app.ofx.position_drag.clone() {
            if let Some(p) = positions.iter().find(|p| p.name == name) {
                let c = to_canonical(q - grip);
                let k = scale(p);
                let v = Value::Num(vec![c[0] / k[0], c[1] / k[1]]);
                if value_now(p, &fx, place) != v {
                    set_now(p, &mut fx, place, v);
                    if let Some(e) = app.ofx.editors.get_mut(key) {
                        e.values = fx.params.clone();
                    }
                    set_fx(app, key, fx);
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            ui.ctx().request_repaint();
        }
    }
    let painter = ui.painter().with_clip_rect(area);
    for (p, at) in &points {
        let hot = app.ofx.position_hover.as_deref() == Some(p.name.as_str())
            || app
                .ofx
                .position_drag
                .as_ref()
                .is_some_and(|(n, _)| *n == p.name);
        let light = if hot {
            egui::Color32::from_rgb(255, 230, 115)
        } else {
            egui::Color32::WHITE
        };
        for (colour, width) in [(egui::Color32::from_black_alpha(204), 3.0), (light, 1.4)] {
            let stroke = Stroke::new(width, colour);
            painter.circle_stroke(*at, POSITION_RING, stroke);
            painter.line_segment(
                [
                    *at - egui::vec2(POSITION_ARM, 0.0),
                    *at + egui::vec2(POSITION_ARM, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    *at - egui::vec2(0.0, POSITION_ARM),
                    *at + egui::vec2(0.0, POSITION_ARM),
                ],
                stroke,
            );
        }
    }
}

pub fn overlay_ui(ui: &Ui, app: &mut BananaApp, nav: &egui::Response, frame: Rect, area: Rect) {
    let Some((key, place)) = app.ofx.overlay.clone() else {
        return;
    };
    stabilization_points(ui, app, &key, &place, frame, area);
    position_handles(ui, app, &key, &place, nav, frame, area);
    let Some(instance) = app
        .ofx
        .editors
        .get(&key)
        .map(|e| Arc::clone(&e.instance))
        .filter(|i| i.has_overlay())
    else {
        return;
    };
    let (Some(inv), true) = (
        place.map.inverse(),
        frame.width() > 1.0 && frame.height() > 1.0,
    ) else {
        return;
    };
    let len = |v: [f64; 2]| v[0].hypot(v[1]);
    let pixel_scale = [
        len(inv.linear([1.0 / frame.width() as f64, 0.0])),
        len(inv.linear([0.0, 1.0 / frame.height() as f64])),
    ];
    // The view is the whole picture area (smaller than the frame while the
    // preview is zoomed in): an OpenGL overlay draws there too, its
    // canonical coordinates taken through the frame's place in it.
    if area.width() < 1.0 || area.height() < 1.0 {
        return;
    }
    let in_area =
        |along: f32, offset: f32, size: f32| ((along / size) as f64, (offset / size) as f64);
    let (sx, tx) = in_area(frame.width(), frame.left() - area.left(), area.width());
    let (sy, ty) = in_area(frame.height(), frame.top() - area.top(), area.height());
    let [[a, b, c], [d, e, f]] = place.map.0;
    let view = View {
        time: frames(place.time, place.rate),
        render_scale: (1.0 / pixel_scale[0].max(1e-9)).min(1.0),
        pixel_scale,
        background: [0.0; 3],
        viewport: [area.width() as i32, area.height() as i32],
        to_frame: Some([[sx * a, sx * b, sx * c + tx], [sy * d, sy * e, sy * f + ty]]),
    };
    let to_screen = |p: [f64; 2]| {
        let [x, y] = place.map.apply(p);
        pos2(
            frame.left() + (x * frame.width() as f64) as f32,
            frame.top() + (y * frame.height() as f64) as f32,
        )
    };
    let to_canonical = |q: Pos2| {
        inv.apply([
            ((q.x - frame.left()) / frame.width()) as f64,
            ((q.y - frame.top()) / frame.height()) as f64,
        ])
    };
    // The viewport counts up from its bottom.
    let viewport = |q: Pos2| [(q.x - area.left()) as i32, (area.bottom() - q.y) as i32];
    let (pressed, down, released, pos, keys) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
            i.modifiers,
        )
    });
    // The keys held, which overlays hear with the pen (Mesh Warp's Shift).
    bv_ofx::interact::set_modifiers(
        (u32::from(keys.shift) * bv_ofx::interact::SHIFT)
            | (u32::from(keys.ctrl) * bv_ofx::interact::CONTROL)
            | (u32::from(keys.alt) * bv_ofx::interact::ALT),
    );
    let mut answer = None;
    // A click may have the plug-in read its picture (an eyedropper): the
    // one the preview gave it last.
    if pressed
        && let Some(k) = key.effect_key()
        && let Some((_, picture, full)) = bv_engine::plugins::watched_input(&k)
    {
        instance.connect("Source", Arc::new(Picture(picture, full)));
    }
    let pen =
        |action, q: Pos2| instance.overlay_pen(action, &view, to_canonical(q), viewport(q), 1.0);
    if pressed {
        app.ofx.pen_took = false;
    }
    if let Some(q) = pos {
        let over = area.contains(q) && (nav.hovered() || nav.is_pointer_button_down_on());
        if pressed && over {
            let a = pen(PenAction::Down, q);
            app.ofx.pen = a.taken;
            app.ofx.pen_took = a.taken;
            answer = Some(a);
        } else if app.ofx.pen && released {
            answer = Some(pen(PenAction::Up, q));
            app.ofx.pen = false;
        } else if (app.ofx.pen && down || over) && app.ofx.pen_at != Some(q) {
            answer = Some(pen(PenAction::Motion, q));
        }
        app.ofx.pen_at = Some(q);
    }
    let pen_acted = answer.is_some();
    if let Some(a) = answer {
        if !a.changed.is_empty() {
            // What the plug-in changed goes to the project.
            if let Some(mut fx) = current_fx(app.project(), &key) {
                let values = instance.values();
                for name in &a.changed {
                    if let Some((_, v)) = values.iter().find(|(n, _)| n == name)
                        && let Some(p) = plugin_param(v, place.rate)
                    {
                        fx.params.insert(name.clone(), p);
                    }
                }
                if let Some(e) = app.ofx.editors.get_mut(&key) {
                    e.values = fx.params.clone();
                }
                set_fx(app, &key, fx);
            }
        }
        ui.ctx().request_repaint();
    }
    if app.ofx.pen {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    // Drawn again only when what it shows changes — the view, the
    // settings, the pen, the plug-in asking — and while playing, for a new
    // time alone, at most every `OVERLAY_WHILE_PLAYING`.
    let values = app
        .ofx
        .editors
        .get(&key)
        .map(|e| e.values.clone())
        .unwrap_or_default();
    let asked = bv_ofx::take_redraw();
    let playing = app.is_playing();
    let only_time = |a: &View, b: &View| View { time: b.time, ..*a } == *b;
    let fresh = !pen_acted
        && !asked
        && app.ofx.overlay_drawn.as_ref().is_some_and(|d| {
            !d.again
                && d.key == key
                && d.values == values
                && (d.view == view
                    || (playing
                        && only_time(&d.view, &view)
                        && d.at.elapsed() < OVERLAY_WHILE_PLAYING))
        });
    let drawn_now = !fresh;
    if drawn_now {
        let marks = instance.overlay_draw(&view);
        app.ofx.overlay_drawn = Some(OverlayDrawn {
            key: key.clone(),
            view,
            values,
            marks,
            at: std::time::Instant::now(),
            // Asked to draw again while drawing: next frame.
            again: bv_ofx::take_redraw(),
        });
        if app.ofx.overlay_drawn.as_ref().is_some_and(|d| d.again) {
            ui.ctx().request_repaint();
        }
    }
    let Some(drawn) = app.ofx.overlay_drawn.as_ref() else {
        return;
    };
    let painter = ui.painter().with_clip_rect(area);
    // What an overlay drew with OpenGL itself: a picture of the view, sent
    // to the graphics card when drawn anew.
    for m in &drawn.marks {
        if let Mark::Picture {
            width,
            height,
            rgba,
        } = m
        {
            let options = egui::TextureOptions::LINEAR;
            let upload = |slot: &mut Option<egui::TextureHandle>| {
                let image = egui::ColorImage::from_rgba_premultiplied(
                    [*width as usize, *height as usize],
                    rgba,
                );
                match slot {
                    Some(t) => {
                        t.set(image, options);
                        t.id()
                    }
                    slot => slot
                        .insert(ui.ctx().load_texture("ofx-overlay", image, options))
                        .id(),
                }
            };
            let texture = match &app.ofx.overlay_picture {
                Some(t) if !drawn_now => t.id(),
                _ => upload(&mut app.ofx.overlay_picture),
            };
            let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(texture, area, whole, egui::Color32::WHITE);
        }
    }
    paint_marks(&painter, &drawn.marks, to_screen);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::PluginValue;

    /// Rich text's runs follow the text: past an insertion they move,
    /// across a deletion they shrink, inside it they go.
    #[test]
    fn rich_text_runs_follow_the_text() {
        let runs = "0 5 size=96\n6 11 bold=1\n";
        assert_eq!(
            shift_runs(runs, "Hello world", "Hello, world"),
            "0 5 size=96\n7 12 bold=1\n"
        );
        assert_eq!(shift_runs(runs, "Hello world", "world"), "0 5 bold=1\n");
        // Typed inside a run: it grows.
        assert_eq!(
            shift_runs("0 5 size=96\n", "Hello", "Helllo"),
            "0 6 size=96\n"
        );
    }

    #[test]
    fn a_plug_in_s_maker_is_its_bundle_s_name() {
        let made = |file: &str| {
            provider(&PluginInfo {
                identifier: "com.example.Blur".into(),
                version: [1, 0],
                file: PathBuf::from(file),
                index: 0,
                label: "Blur".into(),
                grouping: "Example".into(),
                description: String::new(),
                contexts: Vec::new(),
                depths: Vec::new(),
                multi_resolution: false,
                tiles: false,
                temporal: false,
                overlay: false,
                thread_safety: String::new(),
                single_instance: false,
                files: false,
                described: Vec::new(),
            })
        };
        assert_eq!(
            made(
                "C:/Program Files/Common Files/OFX/Plugins/Sapphire.ofx.bundle/Contents/Win64/Sapphire.ofx"
            ),
            "Sapphire"
        );
        assert_eq!(
            made("/usr/OFX/Plugins/Mocha Pro.OFX.Bundle/Contents/Linux-x86-64/mocha.ofx"),
            "Mocha Pro"
        );
        // Not in a bundle: the binary's name.
        assert_eq!(made("/plugins/Glow.ofx"), "Glow");
        assert_eq!(made("bv-ofx-testplug"), "bv-ofx-testplug");
    }

    fn num(x: f64) -> PluginValue {
        PluginValue::Num(vec![x])
    }

    fn key(s: i64, x: f64) -> PluginKey {
        PluginKey {
            at: Tick::from_seconds(s),
            value: num(x),
            curve: KeyCurve::Linear,
            handles: Vec::new(),
        }
    }

    fn effect() -> PluginFx {
        let p = |keys: Vec<PluginKey>| PluginParam {
            value: num(0.0),
            keys,
        };
        PluginFx {
            id: "org.example.Glow".into(),
            version: [1, 0],
            name: "Glow".into(),
            params: [
                ("a".to_string(), p(vec![key(0, 0.0), key(2, 1.0)])),
                ("b".to_string(), p(vec![key(2, 5.0)])),
            ]
            .into(),
        }
    }

    #[test]
    fn keyframes_change_curve_go_and_come_in_by_parameter() {
        let fx = effect();
        let rate = FrameRate::FPS_30;
        let s = Tick::from_seconds;
        let both = vec![("a".to_string(), s(2)), ("b".to_string(), s(2))];
        let curved = edit_keys(&fx, &KeyEdit::Curve(both.clone(), KeyCurve::Hold), rate);
        assert_eq!(curved.params["a"].keys[1].curve, KeyCurve::Hold);
        assert_eq!(curved.params["b"].keys[0].curve, KeyCurve::Hold);
        assert_eq!(curved.params["a"].keys[0].curve, KeyCurve::Linear);
        // Deleting a parameter's last keyframe leaves its value there.
        let gone = edit_keys(&fx, &KeyEdit::Remove(vec![("b".into(), s(2))]), rate);
        assert!(gone.params["b"].keys.is_empty());
        assert_eq!(gone.params["b"].value, num(5.0));
        assert_eq!(gone.params["a"].keys.len(), 2);
        // A parameter's keyframes as a drag leaves them.
        let moved = edit_keys(
            &fx,
            &KeyEdit::Keys("a".into(), vec![key(3, 1.0), key(0, 0.0)]),
            rate,
        );
        assert_eq!(moved.params["a"].keys, vec![key(0, 0.0), key(3, 1.0)]);
        // Pasted: in at their times, over any there; a parameter that was
        // not set yet gets them too.
        let pasted = edit_keys(
            &fx,
            &KeyEdit::Insert(vec![
                ("a".into(), key(2, 9.0)),
                ("a".into(), key(4, 8.0)),
                ("c".into(), key(1, 7.0)),
            ]),
            rate,
        );
        assert_eq!(
            pasted.params["a"].keys,
            vec![key(0, 0.0), key(2, 9.0), key(4, 8.0)]
        );
        assert_eq!(pasted.params["c"].keys, vec![key(1, 7.0)]);
    }

    #[test]
    fn keyframes_made_at_the_cursor_take_the_values_there() {
        let fx = effect();
        let rate = FrameRate::FPS_30;
        let s = Tick::from_seconds;
        // At 1 s: a halfway between its keyframes (with the curve of the
        // one before), b its only keyframe's.
        let added = edit_keys(&fx, &KeyEdit::Add(s(1)), rate);
        assert_eq!(added.params["a"].keys.len(), 3);
        assert_eq!(added.params["a"].keys[1].value, num(0.5));
        assert_eq!(added.params["a"].keys[1].curve, KeyCurve::Linear);
        assert_eq!(added.params["b"].keys[0].at, s(1));
        assert_eq!(added.params["b"].keys[0].value, num(5.0));
        // Where one is already, nothing more.
        let again = edit_keys(&added, &KeyEdit::Add(s(1)), rate);
        assert_eq!(again, added);
        // Curves too: their shape there.
        let mut curved = fx.clone();
        let curve = |y: f64| {
            PluginValue::Curves(vec![vec![
                bv_core::curve::CurvePoint::new(0.0, 0.0),
                bv_core::curve::CurvePoint::new(1.0, y),
            ]])
        };
        curved.params.insert(
            "c".into(),
            PluginParam {
                value: curve(1.0),
                keys: vec![
                    PluginKey {
                        at: s(0),
                        value: curve(1.0),
                        curve: KeyCurve::Linear,
                        handles: Vec::new(),
                    },
                    PluginKey {
                        at: s(2),
                        value: curve(0.0),
                        curve: KeyCurve::Linear,
                        handles: Vec::new(),
                    },
                ],
            },
        );
        let added = edit_keys(&curved, &KeyEdit::AddTo("c".into(), s(1)), rate);
        assert_eq!(added.params["c"].keys[1].value, curve(0.5));
        // One parameter's (a double-click on its row).
        let one = edit_keys(&fx, &KeyEdit::AddTo("b".into(), s(1)), rate);
        assert_eq!(one.params["b"].keys.len(), 2);
        assert_eq!(one.params["a"], fx.params["a"]);
        // A keyframe's value set ("Set to…").
        let set = edit_keys(&fx, &KeyEdit::Value("a".into(), s(2), vec![0.25]), rate);
        assert_eq!(set.params["a"].keys[1].value, num(0.25));
        assert_eq!(set.params["b"], fx.params["b"]);
    }

    #[test]
    fn settings_split_into_columns_of_about_as_many_rows() {
        assert_eq!(split_columns(&[1, 1, 1, 1, 1], 1), vec![5]);
        assert_eq!(split_columns(&[1, 1, 1, 1, 1], 2), vec![3, 2]);
        assert_eq!(split_columns(&[1, 1, 1, 1, 1], 3), vec![2, 2, 1]);
        // A group of four rows stays whole.
        assert_eq!(split_columns(&[1, 4, 1, 1], 2), vec![2, 2]);
        // Fewer parameters than columns: one in each.
        assert_eq!(split_columns(&[1, 1], 3), vec![1, 1]);
        assert_eq!(split_columns(&[], 3), vec![0]);
    }

    #[test]
    fn positions_are_typed_as_two_numbers() {
        assert_eq!(parse_pair("0.5, 0.25"), Some([0.5, 0.25]));
        assert_eq!(parse_pair("0.5; 0.25"), Some([0.5, 0.25]));
        assert_eq!(parse_pair(" 1  2 "), Some([1.0, 2.0]));
        assert_eq!(parse_pair("0.5"), None);
        assert_eq!(pair_text([0.5, 0.565], [3, 3]), "0.500, 0.565");
        // A wheel's angle and magnitude, each with its own decimals.
        assert_eq!(pair_text([103.5, 0.5], [1, 3]), "103.5, 0.500");
    }

    #[test]
    fn canonical_points_land_where_the_compositor_draws_them() {
        // A 1920×1080 frame shown whole: the middle is the middle, y up.
        let near =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        let m = frame_map(1920, 1080);
        assert!(near(m.apply([960.0, 540.0]), [0.5, 0.5]));
        assert!(near(m.apply([0.0, 0.0]), [0.0, 1.0]));
        // A 640×480 picture in it (pillarboxed), moved right a tenth.
        let tf = Transform {
            x: 0.1,
            ..Transform::default()
        };
        let m = event_map((640, 480), 1.0, &tf, (1920, 1080));
        let mid = m.apply([320.0, 240.0]);
        // (The transform holds f32s: 0.1 is a little more.)
        assert!(
            (mid[0] - 0.6).abs() < 1e-6 && (mid[1] - 0.5).abs() < 1e-6,
            "{mid:?}"
        );
        // Its left edge: 1440 px wide, from 240 + 192.
        let left = m.apply([0.0, 240.0]);
        assert!(
            (left[0] - (240.0 + 192.0) / 1920.0).abs() < 1e-6,
            "{left:?}"
        );
        // And back.
        let back = m.inverse().expect("invertible").apply(mid);
        assert!((back[0] - 320.0).abs() < 1e-6 && (back[1] - 240.0).abs() < 1e-6);
    }

    #[test]
    fn the_map_follows_the_compositor_through_pan_crop() {
        // A spot in a 64×36 picture, placed turned, moved, smaller and
        // mirrored: the compositor draws it where the map says.
        let (sw, sh) = (64u32, 36u32);
        let mut src = bv_core::ImageRgbaF32::transparent(sw, sh);
        for y in 9..12 {
            for x in 39..42 {
                *src.pixel_mut(x, y) = [1.0; 4];
            }
        }
        let tf = Transform {
            x: 0.1,
            y: -0.05,
            scale: 0.8,
            rotation_deg: 30.0,
            flip_h: true,
            ..Transform::default()
        };
        let (w, h) = (192u32, 108u32);
        let out = bv_render::place_par(&src, w, h, &tf, bv_timeline::RenderQuality::Best, 1.0);
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for y in 0..h {
            for x in 0..w {
                let a = out.pixel(x, y)[3] as f64;
                sx += a * (x as f64 + 0.5);
                sy += a * (y as f64 + 0.5);
                n += a;
            }
        }
        assert!(n > 0.5, "the spot shows");
        let (cx, cy) = (sx / n, sy / n);
        // The spot's middle, (40.5, 10.5) down from the top, in canonical
        // coordinates.
        let m = event_map((sw, sh), 1.0, &tf, (w, h));
        let [nx, ny] = m.apply([40.5, sh as f64 - 10.5]);
        let (px, py) = (nx * w as f64, ny * h as f64);
        assert!(
            (px - cx).abs() < 0.75 && (py - cy).abs() < 0.75,
            "map ({px:.2}, {py:.2}), drawn ({cx:.2}, {cy:.2})"
        );
    }
}
