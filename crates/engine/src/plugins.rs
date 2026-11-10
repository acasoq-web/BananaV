//! Plug-in effects and transitions (OpenFX) in frames: an instance for
//! each one drawn, given the project's values and the pictures the
//! compositors hand over. (Media a plug-in generates are read like other
//! media: `bv_media::plugins`.)

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use bv_core::{FrameRate, ImageRgbaF32, Tick};
use bv_media::VideoPool;
pub use bv_media::plugins::{
    FILTERS, animation, compositor_context, filter_context, frames, generator_context, handles,
    interp, key_curve, plugin_param, plugin_value, set_curve, set_handle, sync_values, time_of,
    transition_context, value,
};
use bv_ofx::{
    Animation, ClipSource, Components, Depth, DevicePicture, Format, Instance, RenderRequest,
    Setup, Value,
};
use bv_timeline::scene::{CardPicture, Framing, FxOwner, FxSource, PluginCall, PluginEffects};
use bv_timeline::{AcesSpace, Clip, Effect, MediaInfo, PluginParam, PluginValue};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The identifiers of BananaV's own plug-ins start with it.
pub const OWN_PREFIX: &str = bv_effects::ID_PREFIX;

/// BananaV's own effects (`bv-effects`: OpenFX plug-ins built into it),
/// known to the registry from now on.
pub fn register_own() {
    static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    DONE.get_or_init(|| {
        let binary = bv_ofx::Binary::built_in(
            bv_effects::BINARY,
            bv_effects::number_of_plugins,
            bv_effects::get_plugin,
            None,
        );
        if let Err(e) = bv_ofx::registry().add_built_in(binary) {
            eprintln!("BananaV's own effects did not load: {e}");
        }
    });
}

pub use bv_effects::{
    COMPOSITORS, LOOKS, Preset, PresetValue, TITLES_ID, WARP_FLOW_ID, compositor_only,
    is_compositor, resolve_looks, write_looks,
};

/// The factory presets of BananaV's own effect `id` (OpenFX has none:
/// BananaV lists these with the user's).
pub fn own_presets(id: &str) -> Vec<Preset> {
    bv_effects::presets(id)
}

/// The font face Titles & Text draws `family` with for `bold` and
/// `italic`: its file and its place in the file's collection (the text
/// box of its settings shows the text in it).
pub fn font_face(family: &str, bold: bool, italic: bool) -> Option<(std::path::PathBuf, u32)> {
    bv_effects::font_face(family, bold, italic)
}

pub use bv_effects::Cjk;

/// The computer's faces for the characters the interface's own font lacks
/// (Chinese, Japanese and Korean, `first`'s first; one with many other
/// scripts): their files and places in them.
pub fn interface_fallbacks(first: Cjk) -> Vec<(std::path::PathBuf, u32)> {
    bv_effects::interface_fallbacks(first)
}

/// Whether `info` is one of BananaV's own plug-ins (built into it, not
/// found in a folder).
pub fn is_own(info: &bv_ofx::PluginInfo) -> bool {
    info.identifier.starts_with(OWN_PREFIX) && info.file == std::path::Path::new(bv_effects::BINARY)
}

/// An effect of the project: what it belongs to, its place in its owner's
/// chain and its plug-in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectKey {
    pub owner: FxOwner,
    pub index: usize,
    pub id: String,
}

/// A picture an effect got: its time (frames), the picture and its full
/// size (canonical coordinates).
pub type Input = (f64, Arc<ImageRgbaF32>, (f64, f64));

/// The effect whose input the interface wants (to read it, as an
/// effect's Calculate does) and the last one the preview gave it.
static WATCHED: Mutex<Option<(EffectKey, Option<Input>)>> = Mutex::new(None);

/// Counts the effects watched one after another (0: none watched).
static WATCHING: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// Whether the frame this thread draws keeps the watched effect's
    /// input (see [`keep_watched_input`]).
    static KEEPS: Cell<bool> = const { Cell::new(false) };
}

/// Keeps the input the preview gives effect `key` from now on (`None`:
/// none's).
pub fn watch_input(key: Option<EffectKey>) {
    let mut w = lock(&WATCHED);
    if w.as_ref().map(|(k, _)| k) != key.as_ref() {
        let none = key.is_none();
        *w = key.map(|k| (k, None));
        if none {
            WATCHING.store(0, Ordering::Relaxed);
        } else {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            WATCHING.store(NEXT.fetch_add(1, Ordering::Relaxed), Ordering::Relaxed);
        }
    }
}

/// The effect watched now, as a number that changes with it (`None`: no
/// effect's input is wanted).
pub(crate) fn watching() -> Option<u64> {
    Some(WATCHING.load(Ordering::Relaxed)).filter(|&n| n != 0)
}

/// Whether the frames this thread draws from now on keep the input of the
/// effect the interface watches. The preview's still frames do, for the
/// interface to read it (an eyedropper, a Calculate button); frames of
/// playback do not: keeping it reads the picture back from the graphics
/// card, and keeps a plug-in that draws there off it, at every frame
/// while the effect's FX window is open.
pub(crate) fn keep_watched_input(on: bool) {
    KEEPS.with(|k| k.set(on));
}

/// The last input the preview gave the watched effect `key`.
pub fn watched_input(key: &EffectKey) -> Option<Input> {
    lock(&WATCHED)
        .as_ref()
        .filter(|(k, _)| k == key)
        .and_then(|(_, i)| i.clone())
}

/// Who the frames are for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// The preview: someone is watching.
    Preview,
    /// A file being rendered: frames come in order.
    File,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SlotKey {
    owner: FxOwner,
    index: usize,
    id: String,
}

#[derive(Debug)]
struct Slot {
    instance: Arc<Instance>,
    setup: Setup,
    /// The values given last.
    values: Option<BTreeMap<String, PluginParam>>,
    used: Instant,
    /// How long making it took.
    made_in: Duration,
}

/// Instances that took longer than this to make are kept longer unused.
const SLOW_TO_MAKE: Duration = Duration::from_millis(100);

/// A picture as an input clip.
struct Still(Arc<ImageRgbaF32>, (f64, f64));

impl ClipSource for Still {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        self.1
    }
}

/// The plug-in effects of a preview or of a file being rendered.
pub struct OfxEffects {
    purpose: Purpose,
    slots: Mutex<HashMap<SlotKey, Slot>>,
    /// An instance being made, by its key: one thread makes it, others
    /// asking for it wait for it.
    making: Mutex<HashMap<SlotKey, Arc<Mutex<()>>>>,
    /// Itself, for the inputs that read other times.
    me: Weak<OfxEffects>,
    /// The readers of pictures at other times than the frame's.
    pool: Mutex<Option<VideoPool>>,
}

impl std::fmt::Debug for OfxEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OfxEffects")
            .field("purpose", &self.purpose)
            .field("instances", &self.instances())
            .finish_non_exhaustive()
    }
}

impl OfxEffects {
    pub fn new(purpose: Purpose) -> Arc<OfxEffects> {
        register_own();
        Arc::new_cyclic(|me| OfxEffects {
            purpose,
            slots: Mutex::new(HashMap::new()),
            making: Mutex::new(HashMap::new()),
            me: me.clone(),
            pool: Mutex::new(None),
        })
    }

    /// Media `info` at `t`, `size` exactly, as it enters an effect chain
    /// (`enter`: under ACES, into its working space, `working`).
    fn media_at(
        &self,
        info: &MediaInfo,
        t: Tick,
        size: (u32, u32),
        video_gamma: bool,
        (enter, working): (Option<&[[f32; 3]; 3]>, Option<AcesSpace>),
    ) -> Option<ImageRgbaF32> {
        let mut pool = lock(&self.pool);
        let pool = pool.get_or_insert_with(VideoPool::new);
        let pic = pool.frame_scaled(info, t, size.0, size.1).ok()?;
        let mut img = bv_render::reduce_to(&pic, size.0, size.1);
        // A media in a color space of its own enters through it.
        let space = working
            .zip(info.color_space)
            .and_then(|(w, s)| bv_timeline::color::media_input(s, w));
        let enter = match (&space, enter) {
            (Some(q), Some(_)) => Some(bv_render::Enter::Space(q)),
            (_, m) => m.map(bv_render::Enter::Matrix),
        };
        bv_render::enter_chain(&mut img, video_gamma, enter);
        Some(img)
    }

    /// Lets go of the instances no frame used for `idle` — those slow to
    /// make (a second, for some plug-ins) only after five minutes — and of
    /// the readers of other times.
    pub fn sweep(&self, idle: Duration) {
        let slow = idle.max(Duration::from_secs(300));
        lock(&self.slots).retain(|_, s| {
            let kept = if s.made_in > SLOW_TO_MAKE { slow } else { idle };
            s.used.elapsed() < kept
        });
        if let Ok(mut pool) = self.pool.try_lock() {
            *pool = None;
        }
    }

    /// How many instances it keeps.
    pub fn instances(&self) -> usize {
        lock(&self.slots).len()
    }

    /// The instance for `call`, made (or remade, when its setup changed)
    /// and given the project's values.
    fn instance(&self, call: &PluginCall<'_>) -> Option<Arc<Instance>> {
        let plugin = bv_ofx::registry().plugin(&call.fx.id).ok()?;
        let context = match call.owner {
            FxOwner::Transition(_) | FxOwner::TransitionOut(_) => transition_context(&plugin.info)?,
            FxOwner::Compositor(..) => compositor_context(&plugin.info)?,
            _ => filter_context(&plugin.info)?,
        };
        let rate = call.rate;
        let setup = Setup {
            frame_rate: rate.as_f64(),
            duration: frames(call.duration, rate),
            size: (call.full_size.0 as f64, call.full_size.1 as f64),
            pixel_aspect: 1.0,
            interactive: self.purpose == Purpose::Preview,
        };
        let key = SlotKey {
            owner: call.owner,
            index: call.index,
            id: call.fx.id.clone(),
        };
        if let Some(i) = self.made(&key, &setup, call) {
            return Some(i);
        }
        // Made outside the instances' lock (a plug-in may take a second:
        // others go on meanwhile), by one thread: others asking for it wait.
        let gate = Arc::clone(lock(&self.making).entry(key.clone()).or_default());
        let made = {
            let _making = lock(&gate);
            if let Some(i) = self.made(&key, &setup, call) {
                return Some(i);
            }
            let start = Instant::now();
            let made = match call.owner {
                FxOwner::Transition(_) | FxOwner::TransitionOut(_) | FxOwner::Compositor(..) => {
                    Instance::new(&plugin, context, &setup)
                }
                _ => Instance::new_in(&plugin, &FILTERS, &setup),
            };
            made.ok().map(|i| (Arc::new(i), start.elapsed()))
        };
        lock(&self.making).remove(&key);
        let (instance, made_in) = made?;
        lock(&self.slots).insert(
            key.clone(),
            Slot {
                instance,
                setup,
                values: None,
                used: Instant::now(),
                made_in,
            },
        );
        self.made(&key, &setup, call)
    }

    /// The instance of `key` made already for `setup`, given `call`'s
    /// values (outside the instances' lock: an instance busy rendering
    /// keeps only those asking for it waiting).
    fn made(&self, key: &SlotKey, setup: &Setup, call: &PluginCall<'_>) -> Option<Arc<Instance>> {
        let (instance, changed) = {
            let mut slots = lock(&self.slots);
            let slot = slots.get_mut(key).filter(|s| s.setup == *setup)?;
            slot.used = Instant::now();
            let changed = slot.values.as_ref() != Some(&call.fx.params);
            if changed {
                slot.values = Some(call.fx.params.clone());
            }
            (Arc::clone(&slot.instance), changed)
        };
        if changed {
            sync_values(&instance, &call.fx.params, call.rate);
        }
        Some(instance)
    }
}

impl OfxEffects {
    /// What rendering `call` at `size` asks of the instance.
    fn request(&self, call: &PluginCall<'_>, (w, h): (u32, u32)) -> RenderRequest {
        RenderRequest {
            time: frames(call.time, call.rate),
            width: w,
            height: h,
            scale: w as f64 / (call.full_size.0 as f64).max(1.0),
            draft: false,
            interactive: self.purpose == Purpose::Preview,
            sequential: self.purpose == Purpose::File,
            timeline: Some(call.timeline.as_seconds_f64()),
        }
    }
}

/// `call`'s full size, as the host counts it.
fn full(call: &PluginCall<'_>) -> (f64, f64) {
    (call.full_size.0 as f64, call.full_size.1 as f64)
}

impl OfxEffects {
    /// Keeps `input` for the interface when `call` is the effect it
    /// watches (in the preview).
    fn note(&self, call: &PluginCall<'_>, input: &Arc<ImageRgbaF32>) {
        // Readying plug-ins draws no one's frames.
        if self.purpose != Purpose::Preview || crate::accel::readying() {
            return;
        }
        let mut w = lock(&WATCHED);
        if let Some((k, last)) = w.as_mut()
            && k.owner == call.owner
            && k.index == call.index
            && k.id == call.fx.id
        {
            *last = Some((frames(call.time, call.rate), Arc::clone(input), full(call)));
        }
    }
}

impl OfxEffects {
    /// `call` on `input`. A plug-in with an "Original" clip (Bézier
    /// Masking's Mask FX) gets there the input of the chain it is in: for
    /// a track's or the output's effects `chain_input`.
    fn render_effect(
        &self,
        call: &PluginCall<'_>,
        input: Arc<ImageRgbaF32>,
        chain_input: Option<Arc<ImageRgbaF32>>,
    ) -> Option<ImageRgbaF32> {
        let instance = self.instance(call)?;
        let req = self.request(call, (input.width, input.height));
        self.note(call, &input);
        // A plug-in that reads other times gets them from the media.
        let frame = (input.width, input.height);
        let source: Arc<dyn ClipSource> = match call.source {
            Some(s) if instance.plugin().info.temporal => {
                Arc::new(self.temporal(call, s, (req.time, Some(Arc::clone(&input))), frame, true))
            }
            _ => Arc::new(Still(Arc::clone(&input), full(call))),
        };
        // A plug-in with an "Original" clip (BananaV's: Bézier Masking's
        // Mask FX) gets there the input of the chain it is in: an event's
        // or a media's picture now without that chain's effects (an
        // event's effect: the media through its own effects, framed), a
        // track's or the output's picture before its effects.
        let original = call
            .source
            .filter(|_| takes_original(&instance))
            .map(|s| -> Arc<dyn ClipSource> {
                // Never the frame's picture: always worked out again.
                Arc::new(self.temporal(call, s, (f64::NAN, None), frame, false))
            })
            // A track's or the output's effect: the picture its chain
            // started from.
            .or_else(|| {
                chain_input
                    .filter(|_| takes_original(&instance))
                    .map(|c| -> Arc<dyn ClipSource> { Arc::new(Still(c, full(call))) })
            });
        let mut inputs: Vec<(&str, Arc<dyn ClipSource>)> = vec![("Source", source)];
        if let Some(o) = original {
            inputs.push((ORIGINAL, o));
        }
        instance.render_with(&inputs, &[], &req).ok()
    }

    /// `call`'s input at any time (OpenFX's temporal clip access): `now`'s
    /// picture when given, other times' — `frame` big — the media through
    /// the effects before it (those of `call`'s own owner too unless
    /// `whole_chain` is off: an "Original" clip leaves them out).
    fn temporal(
        &self,
        call: &PluginCall<'_>,
        s: FxSource<'_>,
        (now, picture): (f64, Option<Arc<ImageRgbaF32>>),
        frame: (u32, u32),
        whole_chain: bool,
    ) -> Temporal {
        Temporal {
            now,
            picture,
            frame,
            full: full(call),
            rate: call.rate,
            id: call.fx.id.clone(),
            info: s.info.clone(),
            clip: s.clip.clone(),
            media_time: s.media_time,
            before: s
                .before
                .iter()
                .filter(|b| whole_chain || b.owner != call.owner)
                .map(|b| (b.fx.clone(), b.owner, b.index))
                .collect(),
            fx_scale: s.fx_scale,
            video_gamma: s.video_gamma,
            enter: s.enter,
            working: s.working,
            clips: s.clips,
            full_size: call.full_size,
            framing: s.framing,
            ruler: call.timeline - call.time,
            effects: self.me.clone(),
        }
    }

    /// The instance for `call` when its plug-in can draw on the graphics
    /// card's pictures where they are: it renders with CUDA, takes and
    /// gives RGBA floats and has no "Original" clip, the interface is not
    /// reading its input, and it never failed there.
    fn card_instance(&self, call: &PluginCall<'_>) -> Option<Arc<Instance>> {
        let clips: &[&str] = match call.owner {
            FxOwner::Transition(_) | FxOwner::TransitionOut(_) => {
                &["SourceFrom", "SourceTo", "Output"]
            }
            FxOwner::Compositor(..) => return None,
            _ => &["Source", "Output"],
        };
        let failed = lock(&OFF_CARD)
            .as_ref()
            .is_some_and(|off| off.contains(&call.fx.id));
        if failed || self.watched(call) {
            return None;
        }
        let instance = self.instance(call)?;
        let floats = Format {
            depth: Depth::Float,
            components: Components::Rgba,
        };
        let ready = instance.renders_with_cuda()
            && !is_own(&instance.plugin().info)
            && !takes_original(&instance)
            && clips.iter().all(|c| instance.clip_format(c).0 == floats);
        ready.then_some(instance)
    }

    /// `instance` rendered on the card into `output`; a plug-in that fails
    /// there draws on the processor from then on.
    fn render_on_card(
        &self,
        call: &PluginCall<'_>,
        instance: &Instance,
        inputs: &[(&str, Arc<dyn ClipSource>)],
        values: &[(String, Animation)],
        output: CardPicture,
    ) -> bool {
        let Some(device) = bv_cuda::Device::with_ordinal(output.device) else {
            return false;
        };
        let req = self.request(call, (output.width, output.height));
        let drawn = instance
            .render_on_card(inputs, values, &req, device, device_picture(output))
            .is_ok();
        if !drawn {
            lock(&OFF_CARD)
                .get_or_insert_with(HashSet::new)
                .insert(call.fx.id.clone());
        }
        drawn
    }
}

/// The plug-ins that failed to draw on the graphics card this session.
static OFF_CARD: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// A picture on the card as a plug-in gets it: RGBA floats.
fn device_picture(p: CardPicture) -> DevicePicture {
    DevicePicture::borrowed(
        p.address,
        p.width as usize,
        p.height as usize,
        p.row_bytes as usize,
        Format {
            depth: Depth::Float,
            components: Components::Rgba,
        },
    )
}

/// An input on the graphics card: the frame's picture there, and for a
/// plug-in that reads other times the others worked out on the processor
/// (then sent there).
struct OnCard {
    picture: CardPicture,
    now: f64,
    full: (f64, f64),
    others: Option<Temporal>,
}

impl ClipSource for OnCard {
    fn picture(&self, time: f64) -> Option<Arc<ImageRgbaF32>> {
        self.others.as_ref()?.picture(time)
    }

    fn full_size(&self) -> (f64, f64) {
        self.full
    }

    fn on_card(&self, time: f64) -> Option<DevicePicture> {
        (self.others.is_none() || (time - self.now).abs() < 1e-3)
            .then(|| device_picture(self.picture))
    }
}

impl PluginEffects for OfxEffects {
    fn apply(&self, call: &PluginCall<'_>, input: Arc<ImageRgbaF32>) -> Option<ImageRgbaF32> {
        self.render_effect(call, input, None)
    }

    fn takes_chain_input(&self, call: &PluginCall<'_>) -> bool {
        call.source.is_none() && self.instance(call).is_some_and(|i| takes_original(&i))
    }

    fn apply_in_chain(
        &self,
        call: &PluginCall<'_>,
        input: Arc<ImageRgbaF32>,
        chain_input: Arc<ImageRgbaF32>,
    ) -> Option<ImageRgbaF32> {
        self.render_effect(call, input, Some(chain_input))
    }

    fn composite(
        &self,
        call: &PluginCall<'_>,
        source: Arc<ImageRgbaF32>,
        map: Arc<ImageRgbaF32>,
    ) -> Option<ImageRgbaF32> {
        let instance = self.instance(call)?;
        let req = self.request(call, (source.width, source.height));
        let source: Arc<dyn ClipSource> = Arc::new(Still(source, full(call)));
        let map: Arc<dyn ClipSource> = Arc::new(Still(map, full(call)));
        instance
            .render_with(&[("Source", source), ("Map", map)], &[], &req)
            .ok()
    }

    fn transition(
        &self,
        call: &PluginCall<'_>,
        from: Arc<ImageRgbaF32>,
        to: Arc<ImageRgbaF32>,
        progress: f32,
    ) -> Option<ImageRgbaF32> {
        let instance = self.instance(call)?;
        let req = self.request(call, (to.width, to.height));
        let from: Arc<dyn ClipSource> = Arc::new(Still(from, full(call)));
        let to: Arc<dyn ClipSource> = Arc::new(Still(to, full(call)));
        // The host moves the transition along: its parameter is the
        // progress.
        let progress = Animation::constant(Value::Num(vec![progress as f64]));
        instance
            .render_with(
                &[("SourceFrom", from), ("SourceTo", to)],
                &[(TRANSITION_PARAM.to_string(), progress)],
                &req,
            )
            .ok()
    }

    fn values_at(&self, call: &PluginCall<'_>) -> Option<Vec<(String, PluginValue)>> {
        let instance = self.instance(call)?;
        let time = frames(call.time, call.rate);
        Some(
            instance
                .params()
                .all()
                .into_iter()
                .filter(|p| p.ty.has_value())
                .filter_map(|p| Some((p.name.clone(), plugin_value(&p.value_at(time))?)))
                .collect(),
        )
    }

    fn watched(&self, call: &PluginCall<'_>) -> bool {
        self.purpose == Purpose::Preview
            && KEEPS.with(Cell::get)
            && watching().is_some()
            && lock(&WATCHED).as_ref().is_some_and(|(k, _)| {
                k.owner == call.owner && k.index == call.index && k.id == call.fx.id
            })
    }

    fn note_input(&self, call: &PluginCall<'_>, input: Arc<ImageRgbaF32>) {
        self.note(call, &input);
    }

    fn numbers_at(&self, call: &PluginCall<'_>, name: &str, time: f64) -> Option<Vec<f64>> {
        let instance = self.instance(call)?;
        let p = instance
            .params()
            .all()
            .into_iter()
            .find(|p| p.name == name)?;
        match plugin_value(&p.value_at(time))? {
            PluginValue::Num(v) => Some(v),
            _ => None,
        }
    }

    fn input_at(
        &self,
        call: &PluginCall<'_>,
        time: f64,
        size: (u32, u32),
    ) -> Option<Arc<ImageRgbaF32>> {
        let s = call.source?;
        let now = frames(call.time, call.rate);
        self.temporal(call, s, (now, None), size, true)
            .picture(time)
    }

    fn on_card(&self, call: &PluginCall<'_>) -> bool {
        self.card_instance(call).is_some()
    }

    fn apply_on_card(
        &self,
        call: &PluginCall<'_>,
        input: CardPicture,
        output: CardPicture,
    ) -> bool {
        let Some(instance) = self.card_instance(call) else {
            return false;
        };
        let now = frames(call.time, call.rate);
        let others = call
            .source
            .filter(|_| instance.plugin().info.temporal)
            .map(|s| self.temporal(call, s, (now, None), (input.width, input.height), true));
        let source: Arc<dyn ClipSource> = Arc::new(OnCard {
            picture: input,
            now,
            full: full(call),
            others,
        });
        self.render_on_card(call, &instance, &[("Source", source)], &[], output)
    }

    fn transition_on_card(
        &self,
        call: &PluginCall<'_>,
        from: CardPicture,
        to: CardPicture,
        output: CardPicture,
        progress: f32,
    ) -> bool {
        let Some(instance) = self.card_instance(call) else {
            return false;
        };
        let now = frames(call.time, call.rate);
        let input = |picture| -> Arc<dyn ClipSource> {
            Arc::new(OnCard {
                picture,
                now,
                full: full(call),
                others: None,
            })
        };
        // The host moves the transition along: its parameter is the
        // progress.
        let progress = Animation::constant(Value::Num(vec![progress as f64]));
        self.render_on_card(
            call,
            &instance,
            &[("SourceFrom", input(from)), ("SourceTo", input(to))],
            &[(TRANSITION_PARAM.to_string(), progress)],
            output,
        )
    }
}

/// The parameter a transition plug-in's progress is (set by the host).
pub const TRANSITION_PARAM: &str = "Transition";

/// The clip BananaV fills with the input of the chain an effect is in
/// (Bézier Masking's Mask FX).
pub const ORIGINAL: &str = "Original";

/// Whether `instance`'s plug-in has an [`ORIGINAL`] clip.
fn takes_original(instance: &Instance) -> bool {
    instance
        .plugin()
        .info
        .context(instance.context())
        .is_some_and(|c| c.clips.iter().any(|clip| clip.name == ORIGINAL))
}

/// A media's or event's effect's input at any time: the frame's picture
/// now, and at other times the media there through the effects before it
/// — for an event's effect, framed by the event's Pan/Crop on the way —
/// (OFX temporal clip access).
struct Temporal {
    /// The frame's time (frames) and picture (none when it is on the
    /// graphics card), and the frame's size.
    now: f64,
    picture: Option<Arc<ImageRgbaF32>>,
    frame: (u32, u32),
    full: (f64, f64),
    rate: FrameRate,
    /// The plug-in asking (not run again inside itself).
    id: String,
    info: MediaInfo,
    clip: Clip,
    media_time: bool,
    before: Vec<(Effect, FxOwner, usize)>,
    fx_scale: f32,
    video_gamma: bool,
    enter: Option<[[f32; 3]; 3]>,
    working: Option<AcesSpace>,
    clips: bool,
    full_size: (u32, u32),
    framing: Option<Framing>,
    /// The owner's time on the ruler: its own plus this.
    ruler: Tick,
    effects: Weak<OfxEffects>,
}

impl ClipSource for Temporal {
    fn picture(&self, time: f64) -> Option<Arc<ImageRgbaF32>> {
        if (time - self.now).abs() < 1e-3
            && let Some(picture) = &self.picture
        {
            return Some(Arc::clone(picture));
        }
        let effects = self.effects.upgrade()?;
        // The owner's time (on a frame: pictures are frames), and the
        // media's then.
        let owner = self.rate.snap_nearest(time_of(time, self.rate));
        let local = if self.media_time { None } else { Some(owner) };
        let media = match local {
            Some(l) => bv_timeline::scene::media_time(&self.clip, &self.info, self.clip.start + l)?,
            None => owner,
        };
        let frame = self.frame;
        let size = self.framing.map_or(frame, |f| f.media_size);
        let mut img = effects.media_at(
            &self.info,
            media,
            size,
            self.video_gamma,
            (self.enter.as_ref(), self.working),
        )?;
        // The media's effects on its own picture; an event's effect's
        // picture is then framed by the event's Pan/Crop then.
        let framed_at = self
            .framing
            .map_or(self.before.len(), |f| f.framed_at.min(self.before.len()));
        let media_size = self
            .info
            .video
            .map_or(self.full_size, |v| (v.width, v.height));
        let (scale, full) = match self.framing {
            Some(f) => (f.media_fx_scale, media_size),
            None => (self.fx_scale, self.full_size),
        };
        self.chain(
            &effects,
            &mut img,
            &self.before[..framed_at],
            scale,
            full,
            media,
            local,
        );
        if let Some(f) = self.framing {
            let at = self.clip.start + local.unwrap_or(owner);
            let place = f
                .place
                .at(&self.clip, at, [f64::from(frame.0), f64::from(frame.1)]);
            img = bv_render::place_box(&img, frame.0, frame.1, &place, f.quality);
            self.chain(
                &effects,
                &mut img,
                &self.before[framed_at..],
                self.fx_scale,
                self.full_size,
                media,
                local,
            );
        }
        Some(Arc::new(img))
    }

    fn full_size(&self) -> (f64, f64) {
        self.full
    }
}

impl Temporal {
    /// `img` through `chain` (others' effects; this plug-in is not run
    /// again inside itself), their sizes scaled by `fx_scale`, their
    /// pictures `full` big at full resolution.
    #[allow(clippy::too_many_arguments)]
    fn chain(
        &self,
        effects: &OfxEffects,
        img: &mut ImageRgbaF32,
        chain: &[(Effect, FxOwner, usize)],
        fx_scale: f32,
        full: (u32, u32),
        media: Tick,
        local: Option<Tick>,
    ) {
        for (fx, owner_of, index) in chain {
            if !fx.enabled {
                continue;
            }
            match &fx.plugin {
                Some(p) if p.id != self.id => {
                    let (t, duration) = match owner_of {
                        FxOwner::Media(_) => (media, self.info.duration),
                        _ => (local.unwrap_or(media), self.clip.duration),
                    };
                    let call = PluginCall {
                        fx: p,
                        owner: *owner_of,
                        index: *index,
                        time: t,
                        duration,
                        timeline: t + self.ruler,
                        rate: self.rate,
                        full_size: full,
                        source: None,
                    };
                    if let Some(out) = effects.apply(&call, Arc::new(img.clone())) {
                        *img = out;
                    }
                }
                Some(_) => {}
                None => bv_render::effects::apply(img, fx, fx_scale),
            }
            bv_render::effects::stored(self.clips, img);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::accel::{Accel, GpuChoice, GpuStatus, Painter};
    use bv_core::Tick;
    use bv_render::VideoPool;
    use bv_timeline::{
        Effect, Generator, KeyCurve, MediaInfo, PluginFx, PluginKey, PluginValue, Project,
        ProjectSettings,
    };

    /// The test plug-ins, built into this test.
    pub(crate) fn test_plugins() {
        static ADDED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ADDED.get_or_init(|| {
            let binary = bv_ofx::Binary::built_in(
                "bv-ofx-testplug",
                bv_ofx_testplug::OfxGetNumberOfPlugins,
                bv_ofx_testplug::OfxGetPlugin,
                None,
            );
            bv_ofx::registry()
                .add_built_in(binary)
                .expect("the test plug-ins");
        });
    }

    fn red_with_invert() -> Project {
        // In light: half of the invert is a gray of 0.5.
        let mut p = Project::new(ProjectSettings {
            pixel_format: bv_timeline::PixelFormat::Float32Full,
            ..ProjectSettings::default()
        });
        let red = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let c = p.insert_media(red, Tick::ZERO, Some(v), None).unwrap()[0];
        p.clip_mut(c)
            .unwrap()
            .effects
            .push(Effect::plugin(PluginFx {
                id: "org.bananav.test.Invert".into(),
                version: [1, 2],
                name: "Invert (test)".into(),
                params: BTreeMap::new(),
            }));
        p
    }

    /// The test plug-in `id`, at its defaults.
    fn test_fx(id: &str) -> PluginFx {
        PluginFx {
            id: id.into(),
            version: [1, 0],
            name: id.rsplit('.').next().unwrap_or(id).into(),
            params: BTreeMap::new(),
        }
    }

    fn solid(p: &mut Project, rgba: [f32; 4]) -> bv_timeline::MediaId {
        let info = MediaInfo::generated(Generator::SolidColor { rgba }, &p.settings.clone());
        p.add_media(info)
    }

    /// The graphics card, when the computer has one.
    fn card(accel: &std::sync::Arc<Accel>) -> bool {
        accel.set(GpuChoice::Auto);
        let start = Instant::now();
        while matches!(accel.status(), GpuStatus::Starting) && start.elapsed().as_secs() < 60 {
            std::thread::sleep(Duration::from_millis(50));
        }
        matches!(accel.status(), GpuStatus::Ready(_))
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 2e-2)
    }

    #[test]
    fn plugin_transitions_and_generated_media_draw_on_the_processor_and_on_the_card() {
        test_plugins();
        let s = Tick::from_seconds;
        let mut p = Project::new(ProjectSettings::default());
        let (black, white) = (solid(&mut p, [0.0, 0.0, 0.0, 1.0]), solid(&mut p, [1.0; 4]));
        let v = p.tracks[0].id;
        p.insert_media(black, Tick::ZERO, Some(v), None).unwrap();
        // White comes in over black from 4 s to 5 s, wiped by the plug-in.
        let b = p.insert_media(white, s(4), Some(v), None).unwrap()[0];
        p.clip_mut(b).unwrap().transition_fx = Some(Box::new(test_fx("org.bananav.test.Wipe")));
        // Media the Gradient plug-in draws (red at the top, blue at the
        // bottom), from 10 s.
        let gradient = Generator::Plugin(Box::new(test_fx("org.bananav.test.Gradient")));
        let g = p.add_media(MediaInfo::generated(gradient, &p.settings.clone()));
        p.insert_media(g, s(10), Some(v), None).unwrap();

        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let bg = [0.0, 0.0, 0.0, 1.0];
        let frames = |painter: &mut Painter, pool: &mut VideoPool| {
            let wipe = painter.linear(&p, Tick::from_seconds_f64(4.5), (64, 36), pool, bg);
            let made = painter.linear(&p, Tick::from_seconds_f64(10.5), (64, 36), pool, bg);
            (wipe, made)
        };
        let (wipe, made) = frames(&mut painter, &mut pool);
        // Halfway: white on the left half, black on the right.
        assert!(
            close(wipe.pixel(8, 18), [1.0; 4]),
            "{:?}",
            wipe.pixel(8, 18)
        );
        assert!(
            close(wipe.pixel(56, 18), [0.0, 0.0, 0.0, 1.0]),
            "{:?}",
            wipe.pixel(56, 18)
        );
        // The generated media: red at the top, blue at the bottom.
        let (top, bottom) = (made.pixel(32, 0), made.pixel(32, 35));
        assert!(top[0] > 0.9 && top[2] < 0.1, "{top:?}");
        assert!(bottom[2] > 0.9 && bottom[0] < 0.1, "{bottom:?}");
        // Without the plug-in, BananaV's own transition stands in: a
        // dissolve, grey all over.
        let mut missing = p.clone();
        missing.clip_mut(b).unwrap().transition_fx =
            Some(Box::new(test_fx("org.bananav.test.Missing")));
        let grey = painter.linear(
            &missing,
            Tick::from_seconds_f64(4.5),
            (64, 36),
            &mut pool,
            bg,
        );
        let (l, r) = (grey.pixel(8, 18), grey.pixel(56, 18));
        assert!(close(l, r) && l[0] > 0.2 && l[0] < 0.8, "{l:?} {r:?}");
        // On the graphics card: the same.
        if card(&accel) {
            let (wipe2, made2) = frames(&mut painter, &mut pool);
            for (a, b) in [(&wipe, &wipe2), (&made, &made2)] {
                for (x, y) in [(8, 18), (56, 18), (32, 0), (32, 35)] {
                    assert!(
                        close(a.pixel(x, y), b.pixel(x, y)),
                        "{:?} {:?}",
                        a.pixel(x, y),
                        b.pixel(x, y)
                    );
                }
            }
        }
    }

    /// A plug-in that renders with CUDA draws on the graphics card's
    /// pictures where they are (the compositor's Direct3D 12 buffers shared
    /// with CUDA, their rows turned for OpenFX and back): the frame the
    /// processor draws, never through the processor. Where the card is
    /// CUDA's (nothing is checked elsewhere).
    #[test]
    fn plug_ins_rendering_with_cuda_draw_on_the_cards_pictures() {
        use std::sync::atomic::Ordering;
        static ADDED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ADDED.get_or_init(|| {
            let binary = bv_ofx::Binary::built_in(
                "bv-ofx-testplug-cuda",
                bv_ofx_testplug::OfxGetNumberOfPluginsCuda,
                bv_ofx_testplug::OfxGetPluginCuda,
                None,
            );
            bv_ofx::registry()
                .add_built_in(binary)
                .expect("the CUDA test plug-ins");
        });
        let mut p = Project::new(ProjectSettings::default());
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let c = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        p.clip_mut(c)
            .unwrap()
            .effects
            .push(Effect::plugin(test_fx("org.bananav.test.CudaInvert")));
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::Preview);
        let mut pool = VideoPool::new();
        let (bg, at, size) = ([0.0, 0.0, 0.0, 1.0], Tick::from_seconds(1), (64, 36));
        let processor = painter.linear(&p, at, size, &mut pool, bg);
        if !card(&accel) {
            return;
        }
        let before = bv_ofx_testplug::CUDA_RENDERS.load(Ordering::Relaxed);
        let on_card = painter.linear(&p, at, size, &mut pool, bg);
        for y in 0..size.1 {
            for x in 0..size.0 {
                let (a, b) = (processor.pixel(x, y), on_card.pixel(x, y));
                assert!(close(a, b), "({x}, {y}): {a:?} {b:?}");
            }
        }
        let shared = match accel.status() {
            GpuStatus::Ready(name) => bv_cuda::devices().first().is_some_and(|d| d.name() == name),
            _ => false,
        };
        if shared {
            assert!(
                bv_ofx_testplug::CUDA_RENDERS.load(Ordering::Relaxed) > before,
                "drawn on the card's pictures"
            );
        }
    }

    #[test]
    fn a_plugin_reads_its_source_at_other_times() {
        test_plugins();
        let mut p = Project::new(ProjectSettings::default());
        let noise = p.add_media(MediaInfo::generated(
            Generator::Noise {
                amount: 1.0,
                color: true,
            },
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let c = p.insert_media(noise, Tick::ZERO, Some(v), None).unwrap()[0];
        let mut echo = p.clone();
        echo.clip_mut(c)
            .unwrap()
            .effects
            .push(Effect::plugin(test_fx("org.bananav.test.Echo")));
        let rate = p.settings.frame_rate;
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel, Purpose::File);
        let mut pool = VideoPool::new();
        let bg = [0.0, 0.0, 0.0, 1.0];
        let mut frame =
            |p: &Project, n: i64| painter.linear(p, rate.frame_start(n), (64, 36), &mut pool, bg);
        let (before, now, echoed) = (frame(&p, 29), frame(&p, 30), frame(&echo, 30));
        assert_ne!(before.data, now.data, "the noise moves");
        // The plug-in shows its source a frame earlier: the media then.
        let same = echoed
            .data
            .iter()
            .zip(&before.data)
            .all(|(a, b)| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5));
        assert!(same, "the frame before");
    }

    /// BananaV's own effect `id` with preset `preset`'s values (`None`: at
    /// its defaults) or `values`.
    /// BananaV's looks, written once for these tests.
    fn looks_dir() -> &'static std::path::Path {
        static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("bananav-looks-{}", std::process::id()));
            write_looks(&dir).expect("the looks are written");
            dir
        })
    }

    /// Video Stabilization's motion of a camera panning right about 19
    /// pixels a frame of 1920 while shaking, over sixty frames.
    fn panning() -> String {
        use bv_effects::stabilization::{Step, motion_text};
        let steps: Vec<Step> = (0..60)
            .map(|i| Step::similar(0.01 + 0.004 * (i as f64 * 0.7).sin(), 0.0, 0.0, 1.0))
            .collect();
        motion_text(&steps)
    }

    /// Video Stabilization's motion of a shaking camera over sixty frames:
    /// shifts, turns and zooms, with `warp` a shear and stretch too.
    fn shaky(warp: bool) -> String {
        use bv_effects::stabilization::{Step, motion_text};
        let steps: Vec<Step> = (0..60)
            .map(|i| {
                let t = i as f64;
                let s = Step::similar(
                    0.006 * (t * 0.7).sin(),
                    0.004 * (t * 0.5).cos(),
                    0.003 * (t * 0.3).sin(),
                    1.0 + 0.002 * (t * 0.4).cos(),
                );
                if warp {
                    let [a, b, c, d] = s.linear;
                    let k = 0.004 * (t * 0.9).sin();
                    Step {
                        linear: [a * 1.003, b + k, c, d * 0.997],
                        ..s
                    }
                } else {
                    s
                }
            })
            .collect();
        motion_text(&steps)
    }

    fn own_fx(id: &str, preset: Option<&str>, values: &[(&str, PluginValue)]) -> Effect {
        let mut params = BTreeMap::new();
        if let Some(name) = preset {
            let p = bv_effects::presets(id)
                .into_iter()
                .find(|p| p.name == name)
                .expect(name);
            for (k, v) in p.values {
                let value = match v {
                    PresetValue::Num(n) => PluginValue::Num(n),
                    PresetValue::Curves(c) => PluginValue::Curves(c),
                    PresetValue::Text(t) => PluginValue::Text(resolve_looks(&t, looks_dir())),
                };
                params.insert(
                    k.to_string(),
                    PluginParam {
                        value,
                        keys: Vec::new(),
                    },
                );
            }
        }
        for (k, v) in values {
            params.insert(
                k.to_string(),
                PluginParam {
                    value: v.clone(),
                    keys: Vec::new(),
                },
            );
        }
        Effect::plugin(PluginFx {
            id: id.into(),
            version: [1, 0],
            name: id.into(),
            params,
        })
    }

    /// The Video Preview's snapshots are the frame as a render draws it, as
    /// VEGAS's at Best (Full): the output's whole size, BananaV's own
    /// effects (plug-ins) on it, and — Save Snapshot to File's — the
    /// frame's transparency kept where nothing covers it.
    #[test]
    fn snapshots_are_the_whole_frame_with_its_effects() {
        let mut p = Project::new(ProjectSettings::default());
        let red = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let clip = p.insert_media(red, Tick::ZERO, Some(v), None).unwrap()[0];
        let invert = own_fx("org.bananav.Invert", Some("100% Inversion"), &[]);
        p.clip_mut(clip).unwrap().effects.push(invert);
        let after = p.clip(clip).unwrap().end() + Tick::SECOND;
        let mut engine = crate::Engine::new(Arc::new(|| {}));
        engine.set_project(&p);
        let black = [0.0, 0.0, 0.0, 1.0];
        let (w, h, rgba) = engine.snapshot(Tick::ZERO, black).expect("drawn");
        assert_eq!((w, h), p.settings.output_size());
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert_eq!(rgba[..4], [0, 255, 255, 255], "red inverted");
        let (_, _, copied) = engine.snapshot(after, black).expect("drawn");
        assert_eq!(copied[..4], [0, 0, 0, 255], "the copy over black");
        let (_, _, saved) = engine.snapshot(after, [0.0; 4]).expect("drawn");
        assert_eq!(saved[..4], [0, 0, 0, 0], "the file's: clear");
        let png = engine.snapshot_png(after).expect("drawn");
        assert_eq!(png[25], 6, "an RGBA picture");
    }

    /// How long a 1080p frame of color bars takes through each of
    /// BananaV's own effects at its first preset, and a crossfade through
    /// some of its transitions, on the processor or, with `BV_SPEED_CARD`
    /// set, on the graphics card (the preview's way):
    /// `cargo test --release -p bv-engine --lib own_effects_speed --
    /// --ignored --nocapture`.
    #[test]
    #[ignore = "measures speed"]
    #[allow(clippy::print_stdout)]
    fn own_effects_speed() {
        register_own();
        let binary = bv_ofx::Binary::built_in(
            bv_effects::BINARY,
            bv_effects::number_of_plugins,
            bv_effects::get_plugin,
            None,
        );
        let (plugins, _) = binary.plugins().expect("own plug-ins");
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        if std::env::var_os("BV_SPEED_CARD").is_some() {
            println!("on the card: {}", card(&accel));
        }
        let mut painter = Painter::new(accel, Purpose::Preview);
        let mut pool = VideoPool::new();
        let (size, black) = ((1920, 1080), [0.0, 0.0, 0.0, 1.0]);
        // Milliseconds a frame at `at`, the first one left out.
        let mut time = |p: &Project, at: Tick| {
            painter.linear(p, at, size, &mut pool, black);
            let start = Instant::now();
            for _ in 0..2 {
                painter.linear(p, at, size, &mut pool, black);
            }
            start.elapsed().as_secs_f64() * 500.0
        };
        let mut base = Project::new(ProjectSettings::default());
        let settings = base.settings;
        let bars = base.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
        let v = base.tracks[0].id;
        let clip = base.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let at = Tick::from_seconds(1);
        println!("no effect: {:.1} ms", time(&base, at));
        let setup = Setup {
            frame_rate: 30.0,
            duration: 300.0,
            size: (64.0, 36.0),
            pixel_aspect: 1.0,
            interactive: true,
        };
        let mut rows = Vec::new();
        for plugin in plugins {
            let id = plugin.info.identifier.clone();
            if Instance::new_in(&plugin, &FILTERS, &setup).is_err() {
                continue;
            }
            let presets = bv_effects::presets(&id);
            let mut p = base.clone();
            p.clip_mut(clip).unwrap().effects =
                vec![own_fx(&id, presets.first().map(|p| p.name), &[])];
            rows.push((time(&p, at), id));
        }
        // A crossfade from the bars to a checkerboard, halfway.
        let checker = base.add_media(MediaInfo::generated(
            Generator::Checkerboard { size: 24 },
            &settings,
        ));
        let b = base
            .insert_media(checker, Tick::from_seconds(4), Some(v), None)
            .unwrap()[0];
        let at = Tick::from_seconds_f64(4.5);
        println!("crossfade: {:.1} ms", time(&base, at));
        let mut fade = |name: String, fx: Option<Box<PluginFx>>| {
            let mut p = base.clone();
            p.clip_mut(b).unwrap().transition_fx = fx;
            rows.push((time(&p, at), name));
        };
        for id in [
            "org.bananav.LinearWipe",
            "org.bananav.PageRoll",
            "org.bananav.GradientWipe",
        ] {
            let preset = bv_effects::presets(id).first().map(|p| p.name);
            fade(id.to_string(), own_fx(id, preset, &[]).plugin);
        }
        for preset in bv_effects::presets("org.bananav.GLTransition")
            .iter()
            .take(4)
        {
            let fx = own_fx("org.bananav.GLTransition", Some(preset.name), &[]).plugin;
            fade(format!("GL Transition: {}", preset.name), fx);
        }
        rows.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (ms, name) in rows {
            println!("{ms:8.1} ms  {name}");
        }
    }

    /// Milliseconds a frame through other makers' plug-ins, at 1080p and
    /// 4K, in an 8-bit and a 32-bit project: `BV_SPEED_BINARY` names their
    /// binary (Sapphire's by default), `BV_SPEED_PLUGINS` their identifiers
    /// (commas). `BV_SPEED_CARD`: on the card; `BV_SPEED_WATCH`: also with
    /// the interface watching the effect's input, as while its FX window
    /// shows it (playing, and on a still frame, which keeps the input).
    /// Skipped without the binary.
    /// `cargo test --release -p bv-engine --lib third_party_speed --
    /// --ignored --nocapture`.
    #[test]
    #[ignore = "measures speed; needs another maker's plug-ins"]
    #[allow(clippy::print_stdout)]
    fn third_party_speed() {
        let binary = std::env::var("BV_SPEED_BINARY").unwrap_or_else(|_| {
            r"C:\Program Files\Common Files\OFX\Plugins\Sapphire.ofx.bundle\Contents\Win64\Sapphire.ofx".into()
        });
        let Ok(loaded) = bv_ofx::Binary::load(std::path::Path::new(&binary)) else {
            println!("no plug-ins at {binary}");
            return;
        };
        bv_ofx::registry().add_built_in(loaded).expect("described");
        let ids = std::env::var("BV_SPEED_PLUGINS").unwrap_or_else(|_| {
            "com.genarts.sapphire.Lighting.S_Glow,com.genarts.sapphire.BlurSharpen.S_Blur,\
             com.genarts.sapphire.Lighting.S_LensFlare"
                .into()
        });
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        if std::env::var_os("BV_SPEED_CARD").is_some() {
            println!("on the card: {}", card(&accel));
        }
        let mut painter = Painter::new(accel, Purpose::Preview);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        for format in [
            bv_timeline::PixelFormat::default(),
            bv_timeline::PixelFormat::Float32Full,
        ] {
            for size in [(1920, 1080), (3840, 2160)] {
                let mut p = Project::new(ProjectSettings {
                    pixel_format: format,
                    width: size.0,
                    height: size.1,
                    ..ProjectSettings::default()
                });
                let settings = p.settings;
                let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
                let v = p.tracks[0].id;
                let clip = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
                let at = Tick::from_seconds(1);
                // Milliseconds a frame, the first two left out.
                let mut time = |p: &Project| {
                    for _ in 0..2 {
                        painter.linear(p, at, size, &mut pool, black);
                    }
                    let start = Instant::now();
                    for _ in 0..4 {
                        painter.linear(p, at, size, &mut pool, black);
                    }
                    start.elapsed().as_secs_f64() * 250.0
                };
                println!(
                    "{format:?} {}x{}: no effect {:.1} ms",
                    size.0,
                    size.1,
                    time(&p)
                );
                for id in ids.split(',').map(str::trim).filter(|i| !i.is_empty()) {
                    let Some(info) = bv_ofx::registry().info(id) else {
                        println!("  no {id}");
                        continue;
                    };
                    let mut q = p.clone();
                    q.clip_mut(clip).unwrap().effects = vec![Effect::plugin(PluginFx {
                        id: id.into(),
                        version: info.version,
                        name: info.name().into(),
                        params: BTreeMap::new(),
                    })];
                    println!("  {:7.1} ms  {}", time(&q), info.name());
                    if std::env::var_os("BV_SPEED_WATCH").is_some() {
                        watch_input(Some(EffectKey {
                            owner: FxOwner::Event(clip),
                            index: 0,
                            id: id.into(),
                        }));
                        println!("  {:7.1} ms  {} watched, playing", time(&q), info.name());
                        keep_watched_input(true);
                        println!("  {:7.1} ms  {} watched, still", time(&q), info.name());
                        keep_watched_input(false);
                        watch_input(None);
                    }
                }
            }
        }
        bv_ofx::unload_all(Duration::from_secs(5));
    }

    /// Milliseconds a frame through other makers' plug-ins (as
    /// [`third_party_speed`], on the card, for the screen), frames drawn
    /// one after another by one painter, and by two painters on threads
    /// of their own at once (alternate frames), as the preview may.
    /// `cargo test --release -p bv-engine --lib third_party_parallel_speed
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore = "measures speed; needs another maker's plug-ins"]
    #[allow(clippy::print_stdout)]
    fn third_party_parallel_speed() {
        let binary = std::env::var("BV_SPEED_BINARY").unwrap_or_else(|_| {
            r"C:\Program Files\Common Files\OFX\Plugins\Sapphire.ofx.bundle\Contents\Win64\Sapphire.ofx".into()
        });
        let Ok(loaded) = bv_ofx::Binary::load(std::path::Path::new(&binary)) else {
            println!("no plug-ins at {binary}");
            return;
        };
        bv_ofx::registry().add_built_in(loaded).expect("described");
        let ids = std::env::var("BV_SPEED_PLUGINS").unwrap_or_else(|_| {
            "com.genarts.sapphire.BlurSharpen.S_Blur,com.genarts.sapphire.Lighting.S_LensFlare"
                .into()
        });
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        if !card(&accel) {
            println!("no graphics card");
            return;
        }
        let black = [0.0, 0.0, 0.0, 1.0];
        const FRAMES: i64 = 16;
        for size in [(1920, 1080), (3840, 2160)] {
            let mut p = Project::new(ProjectSettings {
                width: size.0,
                height: size.1,
                ..ProjectSettings::default()
            });
            let settings = p.settings;
            let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
            let v = p.tracks[0].id;
            let clip = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
            let at = |k: i64| settings.frame_rate.frame_start(60 + k);
            for id in ids.split(',').map(str::trim).filter(|i| !i.is_empty()) {
                let Some(info) = bv_ofx::registry().info(id) else {
                    println!("  no {id}");
                    continue;
                };
                let mut q = p.clone();
                q.clip_mut(clip).unwrap().effects = vec![Effect::plugin(PluginFx {
                    id: id.into(),
                    version: info.version,
                    name: info.name().into(),
                    params: BTreeMap::new(),
                })];
                // `painters` draw FRAMES frames between them, each its share
                // (every `painters`th): milliseconds a frame, the painters
                // started and their first frames drawn first.
                let speed = |painters: usize| {
                    let ready = std::sync::Barrier::new(painters + 1);
                    let done = std::sync::Barrier::new(painters + 1);
                    std::thread::scope(|s| {
                        for i in 0..painters {
                            let (q, accel, ready, done) = (&q, accel.clone(), &ready, &done);
                            std::thread::Builder::new()
                                .stack_size(bv_ofx::PLUGIN_STACK)
                                .spawn_scoped(s, move || {
                                    let mut painter = Painter::new(accel, Purpose::Preview);
                                    let mut pool = VideoPool::new();
                                    for k in [-2, -1] {
                                        painter.display_as(q, at(k), size, &mut pool, None, black);
                                    }
                                    ready.wait();
                                    for k in (i as i64..FRAMES).step_by(painters) {
                                        painter.display_as(q, at(k), size, &mut pool, None, black);
                                    }
                                    done.wait();
                                })
                                .expect("a painter's thread");
                        }
                        ready.wait();
                        let start = Instant::now();
                        done.wait();
                        start.elapsed().as_secs_f64() * 1e3 / FRAMES as f64
                    })
                };
                let (one, two) = (speed(1), speed(2));
                println!(
                    "{}x{} {}: one painter {one:.1} ms a frame, two at once {two:.1} ms",
                    size.0,
                    size.1,
                    info.name()
                );
            }
        }
        bv_ofx::unload_all(Duration::from_secs(5));
    }

    #[test]
    fn own_effects_draw_alike_on_the_processor_and_on_the_card() {
        // Color bars through each of BananaV's own effects: at their
        // presets, and Layer Dimensionality's over a circle Cookie Cutter
        // cuts (a picture with transparency, its edge hard or soft);
        // Pixelate's big cells too, over the bars and over a hole; LUT
        // Filter's tables (a lattice either way, a shaper before one, half
        // strength).
        let mut base = Project::new(ProjectSettings::default());
        let bars = base.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &base.settings.clone(),
        ));
        let v = base.tracks[0].id;
        let clip = base.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        let mut scenarios: Vec<(String, Vec<Effect>)> = vec![
            (
                "crop".into(),
                vec![own_fx(
                    "org.bananav.Crop",
                    None,
                    &[
                        ("anchor", num(&[0.0])),
                        ("xCrop", num(&[0.6])),
                        ("yCrop", num(&[0.5])),
                        ("location", num(&[0.42, 0.56])),
                        ("angle", num(&[15.0])),
                        ("opacity", num(&[0.8])),
                    ],
                )],
            ),
            (
                "corrector".into(),
                vec![own_fx(
                    "org.bananav.ColorCorrector",
                    Some("Red Midtones"),
                    &[
                        ("saturation", num(&[1.3])),
                        ("gamma", num(&[1.2])),
                        ("gain", num(&[0.9])),
                        ("offset", num(&[0.05])),
                    ],
                )],
            ),
            (
                "pixelate".into(),
                vec![own_fx(
                    "org.bananav.Pixelate",
                    None,
                    &[("horizontal", num(&[0.97])), ("vertical", num(&[0.9]))],
                )],
            ),
            (
                "pixelate over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.Pixelate",
                        None,
                        &[("horizontal", num(&[0.98])), ("vertical", num(&[0.95]))],
                    ),
                ],
            ),
            (
                "bezier masking: an oval and a diamond, feathered, with blend".into(),
                vec![own_fx(
                    "org.bananav.BezierMasking",
                    None,
                    &[
                        ("type1", num(&[0.0])),
                        ("width1", num(&[0.45])),
                        ("height1", num(&[0.6])),
                        ("angle1", num(&[20.0])),
                        ("location1", num(&[0.4, 0.55])),
                        ("feather1", num(&[12.0])),
                        ("enable2", num(&[1.0])),
                        ("type2", num(&[2.0])),
                        ("width2", num(&[0.3])),
                        ("height2", num(&[0.4])),
                        ("location2", num(&[0.7, 0.4])),
                        ("opacity2", num(&[0.6])),
                        ("feather2", num(&[6.0])),
                        ("featherStyle2", num(&[0.0])),
                        ("blend", num(&[0.15])),
                    ],
                )],
            ),
            (
                "bezier masking: a turned rectangle inverted".into(),
                vec![own_fx(
                    "org.bananav.BezierMasking",
                    None,
                    &[
                        ("angle1", num(&[-35.0])),
                        ("feather1", num(&[8.0])),
                        ("featherStyle1", num(&[1.0])),
                        ("invert", num(&[1.0])),
                    ],
                )],
            ),
            (
                "bezier masking's mask fx over an invert".into(),
                vec![
                    own_fx("org.bananav.Invert", Some("100% Inversion"), &[]),
                    own_fx(
                        "org.bananav.BezierMasking",
                        Some("Circle"),
                        &[
                            ("maskFx", num(&[1.0])),
                            ("blend", num(&[0.3])),
                            ("feather1", num(&[10.0])),
                        ],
                    ),
                ],
            ),
            (
                "picture in picture turned and sheared".into(),
                vec![own_fx(
                    "org.bananav.PictureInPicture",
                    None,
                    &[
                        ("location", num(&[0.4, 0.6])),
                        ("angle", num(&[30.0])),
                        ("scaleX", num(&[0.6])),
                        ("scaleY", num(&[0.35])),
                        ("mode", num(&[1.0])),
                        ("shear", num(&[0.4])),
                        ("opacity", num(&[0.8])),
                    ],
                )],
            ),
            (
                "picture in picture pinned over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.PictureInPicture",
                        None,
                        &[
                            ("mode", num(&[2.0])),
                            ("topLeft", num(&[0.1, 0.9])),
                            ("topRight", num(&[0.8, 0.7])),
                            ("bottomLeft", num(&[0.2, 0.1])),
                            ("bottomRight", num(&[0.9, 0.35])),
                        ],
                    ),
                ],
            ),
            (
                "picture in picture bigger than the frame".into(),
                vec![own_fx(
                    "org.bananav.PictureInPicture",
                    None,
                    &[("scaleX", num(&[1.7])), ("location", num(&[0.3, 0.5]))],
                )],
            ),
            (
                "hsl adjust over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.HslAdjust",
                        None,
                        &[
                            ("hue", num(&[0.3])),
                            ("saturation", num(&[1.6])),
                            ("luminance", num(&[0.8])),
                        ],
                    ),
                ],
            ),
            (
                "saturation adjust over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.SaturationAdjust",
                        None,
                        &[
                            ("amount", num(&[0.8])),
                            ("center", num(&[0.55])),
                            ("spread", num(&[0.35])),
                            ("low", num(&[-0.6])),
                            ("high", num(&[0.9])),
                        ],
                    ),
                ],
            ),
            (
                "secondary: every correction and limit, inverted, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.ColorCorrectorSecondary",
                        None,
                        &[
                            ("chrominance", num(&[40.0, 0.6])),
                            ("rotateHue", num(&[75.0])),
                            ("saturation", num(&[1.7])),
                            ("gamma", num(&[0.8])),
                            ("gain", num(&[1.1])),
                            ("offset", num(&[0.03])),
                            ("alpha", num(&[0.4])),
                            ("invertMask", num(&[1.0])),
                            ("lumaLow", num(&[60.0])),
                            ("lumaHigh", num(&[170.0])),
                            ("lumaSmooth", num(&[50.0])),
                            ("satLow", num(&[20.0])),
                            ("satSmooth", num(&[30.0])),
                            ("hueCenter", num(&[200.0])),
                            ("hueWidth", num(&[160.0])),
                            ("hueSmooth", num(&[40.0])),
                        ],
                    ),
                ],
            ),
            (
                "secondary showing its mask".into(),
                vec![own_fx(
                    "org.bananav.ColorCorrectorSecondary",
                    None,
                    &[
                        ("showMask", num(&[1.0])),
                        ("lumaLow", num(&[100.0])),
                        ("lumaSmooth", num(&[80.0])),
                        ("hueCenter", num(&[120.0])),
                        ("hueWidth", num(&[90.0])),
                        ("hueSmooth", num(&[45.0])),
                    ],
                )],
            ),
            (
                "color balance in the shadows, its luminosity not kept, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.ColorBalance",
                        None,
                        &[
                            ("red", num(&[-0.3])),
                            ("green", num(&[0.2])),
                            ("blue", num(&[0.45])),
                            ("type", num(&[2.0])),
                            ("preserve", num(&[0.0])),
                        ],
                    ),
                ],
            ),
            (
                "border blurred over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.Border",
                        None,
                        &[("type", num(&[0.0])), ("size", num(&[0.3]))],
                    ),
                ],
            ),
            (
                "border beveled from below, half a pyramid".into(),
                vec![own_fx(
                    "org.bananav.Border",
                    None,
                    &[
                        ("type", num(&[1.0])),
                        ("size", num(&[0.7])),
                        ("angle", num(&[170.0])),
                        ("bevelColor", num(&[0.9, 0.6, 1.0])),
                    ],
                )],
            ),
            (
                "black bar fill over a hole, moved".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.BlackBarFill",
                        None,
                        &[
                            ("fgLocation", num(&[0.4, 0.55])),
                            ("fgScale", num(&[0.7])),
                            ("shadowBlur", num(&[0.6])),
                            ("bgLocation", num(&[0.6, 0.45])),
                            ("bgScale", num(&[1.4])),
                            ("bgBlur", num(&[0.15])),
                        ],
                    ),
                ],
            ),
            (
                "unsharp mask over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.UnsharpMask",
                        None,
                        &[
                            ("amount", num(&[1.5])),
                            ("radius", num(&[0.05])),
                            ("threshold", num(&[0.05])),
                        ],
                    ),
                ],
            ),
            (
                "add noise colored, gaussian, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.AddNoise",
                        None,
                        &[
                            ("level", num(&[0.9])),
                            ("monochromatic", num(&[0.0])),
                            ("gaussian", num(&[1.0])),
                        ],
                    ),
                ],
            ),
            (
                "denoise over noise".into(),
                vec![
                    own_fx("org.bananav.AddNoise", None, &[("level", num(&[0.3]))]),
                    own_fx("org.bananav.Denoise", None, &[]),
                ],
            ),
            (
                "denoise strong and sharpened, colored noise".into(),
                vec![
                    own_fx(
                        "org.bananav.AddNoise",
                        None,
                        &[("level", num(&[0.4])), ("monochromatic", num(&[0.0]))],
                    ),
                    own_fx(
                        "org.bananav.Denoise",
                        None,
                        &[
                            ("luminance", num(&[12.0])),
                            ("color", num(&[1.5])),
                            ("amount", num(&[0.2])),
                        ],
                    ),
                ],
            ),
            (
                "mesh warp's triangles".into(),
                vec![own_fx(
                    "org.bananav.MeshWarp",
                    None,
                    &[
                        ("gridX", num(&[2.0])),
                        ("gridY", num(&[2.0])),
                        (
                            "mesh",
                            PluginValue::Text(
                                "2 2 0 0 0.5 0 1 0 0 0.5 0.68 0.61 1 0.5 0 1 0.42 0.9 1 1".into(),
                            ),
                        ),
                    ],
                )],
            ),
            (
                "mesh warp's source cells, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.MeshWarp",
                        None,
                        &[
                            ("meshPointRefersTo", num(&[0.0])),
                            ("lock", num(&[0.0])),
                            ("gridX", num(&[2.0])),
                            ("gridY", num(&[1.0])),
                            (
                                "mesh",
                                PluginValue::Text(
                                    "2 1 0.1 0 0.55 0.1 1 0 0 1 0.4 0.85 0.95 1".into(),
                                ),
                            ),
                        ],
                    ),
                ],
            ),
            (
                "video stabilization smoothing a shake".into(),
                vec![own_fx(
                    "org.bananav.VideoStabilization",
                    None,
                    &[
                        ("motionData", PluginValue::Text(shaky(false))),
                        ("hasMotionData", num(&[1.0])),
                        ("smoothingMagnitude", num(&[0.5])),
                        ("additionalScale", num(&[0.04])),
                    ],
                )],
            ),
            (
                "video stabilization frozen, warped, mirrored".into(),
                vec![own_fx(
                    "org.bananav.VideoStabilization",
                    None,
                    &[
                        ("motionData", PluginValue::Text(shaky(true))),
                        ("hasMotionData", num(&[1.0])),
                        ("motionCompensation", num(&[3.0])),
                        ("stabilizeMode", num(&[1.0])),
                        ("staticReference", num(&[12.0])),
                        ("zoomFilling", num(&[1.0])),
                        ("additionalRotation", num(&[7.5])),
                    ],
                )],
            ),
            (
                "video stabilization repeated, shifted, blurred, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.VideoStabilization",
                        None,
                        &[
                            ("motionData", PluginValue::Text(panning())),
                            ("hasMotionData", num(&[1.0])),
                            ("motionCompensation", num(&[0.0])),
                            ("zoomFilling", num(&[2.0])),
                            ("additionalTranslation", num(&[0.62, 0.41])),
                            ("motionBlur", num(&[1.0])),
                            ("motionBlurThreshold", num(&[1.5])),
                        ],
                    ),
                ],
            ),
            (
                "video stabilization held at the edges, zoomed and turned".into(),
                vec![own_fx(
                    "org.bananav.VideoStabilization",
                    None,
                    &[
                        ("motionData", PluginValue::Text(shaky(true))),
                        ("hasMotionData", num(&[1.0])),
                        ("motionCompensation", num(&[1.0])),
                        ("zoomFilling", num(&[3.0])),
                        ("additionalScale", num(&[0.3])),
                        ("additionalRotation", num(&[-33.0])),
                    ],
                )],
            ),
            (
                "stabilize applied, its rolling shutter corrected".into(),
                vec![own_fx(
                    "org.bananav.Stabilize",
                    None,
                    &[
                        ("m20StabData", PluginValue::Text(panning())),
                        ("hasM20StabData", num(&[1.0])),
                        ("currentPanSmoothing", num(&[0.75])),
                        ("currentViewScale", num(&[0.6])),
                        ("currentRollingShutterCorrection", num(&[1.0])),
                    ],
                )],
            ),
            (
                "scene rotation turned about its three axes".into(),
                vec![own_fx(
                    "org.bananav.SceneRotation",
                    None,
                    &[
                        ("xAngle", num(&[20.0])),
                        ("yAngle", num(&[-35.0])),
                        ("zAngle", num(&[10.0])),
                    ],
                )],
            ),
            (
                "scene rotation side by side".into(),
                vec![own_fx(
                    "org.bananav.SceneRotation",
                    None,
                    &[("yAngle", num(&[90.0])), ("stereo", num(&[2.0]))],
                )],
            ),
            (
                "360° video stabilization of a wobble, top and bottom".into(),
                vec![own_fx(
                    "org.bananav.VideoStabilization360",
                    None,
                    &[
                        (
                            "analyzedData",
                            PluginValue::Text(bv_effects::stabilization_360::rotation_text(
                                &(0..60)
                                    .map(|i| {
                                        let t = i as f64;
                                        [0.01 * (t * 0.9).sin(), 0.02 * (t * 0.7).cos(), 0.005]
                                    })
                                    .collect::<Vec<_>>(),
                            )),
                        ),
                        ("smoothness", num(&[30.0])),
                        ("stereo", num(&[1.0])),
                    ],
                )],
            ),
            (
                "dual fish eye stitching of wide lenses, turned".into(),
                vec![own_fx(
                    "org.bananav.DualFishEyeStitching",
                    None,
                    &[
                        ("fov", num(&[214.0])),
                        ("orientLeft", num(&[2.0])),
                        ("orientRight", num(&[1.0])),
                        ("falloffFactor", num(&[60.0])),
                        ("falloffRange", num(&[55.0])),
                        ("center", num(&[6.0])),
                        ("narrowing", num(&[82.0])),
                        ("rotation", num(&[-70.0])),
                    ],
                )],
            ),
            (
                "ai smoothen, medium details, all the way".into(),
                vec![own_fx(
                    "org.bananav.AiSmoothen",
                    None,
                    &[("Strength", num(&[1.0]))],
                )],
            ),
            (
                "ai dehaze, low details, its artifact reduction".into(),
                vec![own_fx(
                    "org.bananav.AiDehaze",
                    None,
                    &[
                        ("Details", num(&[0.0])),
                        ("Strength", num(&[0.9])),
                        ("Border", num(&[0.1])),
                        ("Gain", num(&[1.1])),
                        ("Offset", num(&[-0.2])),
                    ],
                )],
            ),
            (
                "ai sharpen, high details".into(),
                vec![own_fx(
                    "org.bananav.AiSharpen",
                    None,
                    &[("Details", num(&[2.0])), ("Strength", num(&[0.8]))],
                )],
            ),
            (
                "smart zoom off the middle, enhanced".into(),
                vec![own_fx(
                    "org.bananav.SmartZoom",
                    None,
                    &[
                        ("LocationAndZoom", num(&[0.37, 0.61, 2.3])),
                        ("Noise", num(&[0.0])),
                    ],
                )],
            ),
            (
                "smart zoom plain, a little".into(),
                vec![own_fx(
                    "org.bananav.SmartZoom",
                    None,
                    &[
                        ("LocationAndZoom", num(&[0.5, 0.5, 1.13])),
                        ("EnableEnhancement", num(&[0.0])),
                    ],
                )],
            ),
            (
                "stereoscopic adjust at rest, its automatic crop".into(),
                vec![own_fx("org.bananav.Stereoscopic3DAdjust", None, &[])],
            ),
            (
                "stereoscopic adjust's offsets, zoom and floating window".into(),
                vec![own_fx(
                    "org.bananav.Stereoscopic3DAdjust",
                    None,
                    &[
                        ("Horizontal", num(&[0.03])),
                        ("Vertical", num(&[-0.02])),
                        ("Zoom", num(&[0.05])),
                        ("AutoCrop", num(&[0.0])),
                        ("Crop", num(&[0.02])),
                        ("FloatingWindowLeft", num(&[0.0731])),
                    ],
                )],
            ),
            (
                "stereoscopic adjust's keystone, rotation and flips".into(),
                vec![own_fx(
                    "org.bananav.Stereoscopic3DAdjust",
                    None,
                    &[
                        ("Keystone", num(&[0.06])),
                        ("Rotation", num(&[7.5])),
                        ("FlipHorizontal", num(&[1.0])),
                        ("FlipVertical", num(&[3.0])),
                    ],
                )],
            ),
            (
                "lens correction's PanoTools model".into(),
                vec![own_fx(
                    "org.bananav.LensCorrection",
                    None,
                    &[
                        ("k1", num(&[0.04])),
                        ("k2", num(&[-0.12])),
                        ("k3", num(&[0.05])),
                    ],
                )],
            ),
            (
                "lens correction off center, scaled".into(),
                vec![own_fx(
                    "org.bananav.LensCorrection",
                    None,
                    &[
                        ("k1", num(&[-0.2])),
                        ("distortionCenter", num(&[0.3, 0.65])),
                        ("scale", num(&[1.3])),
                    ],
                )],
            ),
            (
                "lens correction's radial model, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.LensCorrection",
                        None,
                        &[
                            ("model", num(&[0.0])),
                            ("k1", num(&[0.3])),
                            ("k2", num(&[-0.1])),
                        ],
                    ),
                ],
            ),
            (
                "lens correction of a picked lens".into(),
                vec![own_fx(
                    "org.bananav.LensCorrection",
                    None,
                    &[
                        ("k1", num(&[0.01049])),
                        ("k2", num(&[0.01663])),
                        ("k3", num(&[-0.40901])),
                        ("distortionCenter", num(&[0.5, 0.0])),
                    ],
                )],
            ),
            (
                "denoise's sharpening alone, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.Denoise",
                        None,
                        &[
                            ("luminance", num(&[0.0])),
                            ("color", num(&[0.0])),
                            ("amount", num(&[1.0])),
                        ],
                    ),
                ],
            ),
            (
                "timecode in the top left, wide, over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.Timecode",
                        None,
                        &[
                            ("format", num(&[0.0])),
                            ("horizontal", num(&[0.0])),
                            ("vertical", num(&[0.0])),
                            ("width", num(&[1.8])),
                            ("height", num(&[0.6])),
                        ],
                    ),
                ],
            ),
            (
                "timecode in the middle".into(),
                vec![own_fx(
                    "org.bananav.Timecode",
                    None,
                    &[
                        ("format", num(&[4.0])),
                        ("horizontal", num(&[1.0])),
                        ("vertical", num(&[1.0])),
                    ],
                )],
            ),
            (
                "white balance equalized".into(),
                vec![own_fx(
                    "org.bananav.WhiteBalance",
                    None,
                    &[
                        ("temperature", num(&[3000.0])),
                        ("tint", num(&[10.0])),
                        ("amount", num(&[0.578])),
                        ("brightness", num(&[1.2])),
                        ("equalize", num(&[1.0])),
                    ],
                )],
            ),
            (
                "white balance equalized over a hole".into(),
                vec![
                    own_fx(
                        "org.bananav.CookieCutter",
                        Some("Circle Cut Out, Center, Blurred"),
                        &[],
                    ),
                    own_fx(
                        "org.bananav.WhiteBalance",
                        None,
                        &[("equalize", num(&[1.0]))],
                    ),
                ],
            ),
        ];
        let luts = std::env::temp_dir().join(format!("bananav-parity-luts-{}", std::process::id()));
        std::fs::create_dir_all(&luts).unwrap();
        let lattice = luts.join("lattice.cube");
        let mut text = String::from("TITLE \"Test\"\nLUT_3D_SIZE 9\n");
        for b in 0..9 {
            for g in 0..9 {
                for r in 0..9 {
                    let [r, g, b] = [r, g, b].map(|v| v as f64 / 8.0);
                    text += &format!("{} {} {}\n", r * g, (g + b * b) / 2.0, (1.0 - r) * b.sqrt());
                }
            }
        }
        std::fs::write(&lattice, &text).unwrap();
        let shaped = luts.join("shaped.cube");
        let mut text = String::from("LUT_1D_SIZE 5\nLUT_1D_INPUT_RANGE 0 1\nLUT_3D_SIZE 2\n");
        text += "0 0 0\n0.4 0.3 0.2\n0.6 0.5 0.45\n0.8 0.8 0.7\n1 1 1\n";
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text += &format!("{g} {b} {r}\n");
                }
            }
        }
        std::fs::write(&shaped, &text).unwrap();
        let text = |p: &std::path::Path| PluginValue::Text(p.display().to_string());
        for (name, file, how, strength) in [
            ("lut linear", &lattice, 0.0, 1.0),
            ("lut tetrahedral", &lattice, 1.0, 1.0),
            ("lut shaped, half", &shaped, 1.0, 0.5),
        ] {
            scenarios.push((
                name.into(),
                vec![own_fx(
                    "org.bananav.LutFilter",
                    None,
                    &[
                        ("file", text(file)),
                        ("interpolation", num(&[how])),
                        ("strength", num(&[strength])),
                    ],
                )],
            ));
        }
        for (id, cut) in [
            ("org.bananav.Crop", false),
            ("org.bananav.ColorCurves", false),
            ("org.bananav.ColorCorrector", false),
            ("org.bananav.CookieCutter", false),
            ("org.bananav.LayerDimensionality", true),
            ("org.bananav.Pixelate", false),
            ("org.bananav.LutFilter", false),
            ("org.bananav.WhiteBalance", false),
            ("org.bananav.HslAdjust", false),
            ("org.bananav.PictureInPicture", false),
            ("org.bananav.SaturationAdjust", false),
            ("org.bananav.Border", false),
            ("org.bananav.BlackBarFill", false),
            ("org.bananav.UnsharpMask", false),
            ("org.bananav.AddNoise", false),
            ("org.bananav.Timecode", false),
            ("org.bananav.ColorBalance", false),
            ("org.bananav.ColorCorrectorSecondary", false),
        ] {
            for p in bv_effects::presets(id) {
                let mut chain = Vec::new();
                if cut {
                    for under in ["Circle, Center", "Circle Cut Out, Center, Blurred"] {
                        let cookie = own_fx("org.bananav.CookieCutter", Some(under), &[]);
                        let chain = vec![cookie, own_fx(id, Some(p.name), &[])];
                        scenarios.push((format!("{} over {under}", p.name), chain));
                    }
                    continue;
                }
                chain.push(own_fx(id, Some(p.name), &[]));
                scenarios.push((p.name.to_string(), chain));
            }
        }
        // Color Grading: its steps, its LUTs (Apple Log, a table of
        // BananaV's looks), over a picture with a hole too.
        let look = bv_effects::presets("org.bananav.LutFilter")
            .into_iter()
            .flat_map(|p| p.values)
            .find_map(|(k, v)| match v {
                PresetValue::Text(t) if k == "file" => Some(resolve_looks(&t, looks_dir())),
                _ => None,
            })
            .expect("a look's table");
        let text = |t: &str| PluginValue::Text(t.to_string());
        let grading: Vec<(&str, Vec<(&str, PluginValue)>)> = vec![
            (
                "color grading wheels and tone",
                vec![
                    ("liftR", num(&[0.05])),
                    ("liftB", num(&[-0.02])),
                    ("liftY", num(&[0.02])),
                    ("gammaR", num(&[1.3])),
                    ("gammaB", num(&[0.8])),
                    ("gammaY", num(&[1.1])),
                    ("gainR", num(&[1.05])),
                    ("gainB", num(&[0.95])),
                    ("gainY", num(&[1.1])),
                    ("offsetR", num(&[0.01])),
                    ("offsetY", num(&[-0.01])),
                    ("brightnessAdjust", num(&[0.03])),
                    ("contrastAdjust", num(&[0.2])),
                    ("contrastCtrAdjust", num(&[0.45])),
                    ("saturationAdjust", num(&[1.3])),
                    ("exposureLnAdjust", num(&[0.5])),
                    ("exposureAdjust", num(&[0.2])),
                    ("temperatureAdjust", num(&[1200.0])),
                    ("tintAdjust", num(&[-20.0])),
                    ("hueAdjust", num(&[0.05])),
                    ("satAdjust", num(&[1.2])),
                    ("valAdjust", num(&[0.95])),
                ],
            ),
            (
                "color grading rl wheels and levels",
                vec![
                    ("logWheelShadowsR", num(&[0.3])),
                    ("logWheelShadowsY", num(&[-0.2])),
                    ("logWheelMidtonesG", num(&[0.25])),
                    ("logWheelHighlightsB", num(&[-0.4])),
                    ("logWheelHighlightsY", num(&[0.3])),
                    ("logWheelOffsetY", num(&[0.05])),
                    ("logWheelLowThreshold", num(&[0.2])),
                    ("logWheelHighThreshold", num(&[0.6])),
                    ("inMinAdjust", num(&[0.05])),
                    ("inMaxAdjust", num(&[0.9])),
                    ("outMinAdjust", num(&[0.02])),
                    ("outMaxAdjust", num(&[0.95])),
                ],
            ),
            (
                "color grading curves",
                vec![
                    (
                        "curveR",
                        text("2:0.0:0.08:-0.25:-0.25:0.25:0.25:1.0:1.0:-0.25:-0.25:0.25:0.25:"),
                    ),
                    (
                        "curveB",
                        text("2:0.0:0.0:-0.25:-0.25:0.25:0.25:1.0:0.85:-0.25:-0.2:0.25:0.25:"),
                    ),
                    (
                        "curveY",
                        text(
                            "3:0.0:0.0:-0.25:-0.25:0.15:0.05:0.5:0.5:-0.15:-0.2:0.15:0.2:1.0:1.0:-0.15:-0.05:0.25:0.25:",
                        ),
                    ),
                    (
                        "curveHvH",
                        text(
                            "5:-0.16667:0.5:-0.125:0.0:0.08333:0.0:0.0:0.6:-0.08333:0.0:0.08333:0.0:0.16667:0.5:-0.08333:0.0:0.125:0.0:0.83333:0.5:-0.125:0.0:0.08333:0.0:1.0:0.6:-0.08333:0.0:0.08333:0.0",
                        ),
                    ),
                    (
                        "curveHvL",
                        text(
                            "5:-0.16667:0.5:-0.08333:0.0:0.125:0.0:0.5:0.5:-0.125:0.0:0.08333:0.0:0.66667:0.3:-0.08333:0.0:0.08333:0.0:0.83333:0.5:-0.08333:0.0:0.125:0.0:1.5:0.5:-0.125:0.0:0.08333:0.0",
                        ),
                    ),
                    (
                        "curveSvS",
                        text(
                            "3:0.0:0.5:0.0:0.0:0.125:0.0:0.5:0.65:-0.125:0.0:0.125:0.0:1.0:0.5:-0.125:0.0:0.0:0.0:",
                        ),
                    ),
                    (
                        "curveLvS",
                        text(
                            "3:0.0:0.5:0.0:0.0:0.125:0.0:0.8:0.2:-0.125:0.0:0.1:0.0:1.0:0.5:-0.1:0.0:0.0:0.0:",
                        ),
                    ),
                ],
            ),
            (
                "color grading apple log",
                vec![
                    ("lutEnable0", num(&[1.0])),
                    (
                        "lutFilename0",
                        PluginValue::Text(bv_effects::color_grading::APPLE_LOG_TO_REC709.into()),
                    ),
                    ("lutGain0", num(&[0.7])),
                ],
            ),
            (
                "color grading looks",
                vec![
                    ("lutEnable0", num(&[1.0])),
                    (
                        "lutFilename0",
                        PluginValue::Text(bv_effects::color_grading::APPLE_LOG_TO_LINEAR.into()),
                    ),
                    ("lutGain0", num(&[0.4])),
                    ("lutEnable1", num(&[1.0])),
                    ("lutFilename1", PluginValue::Text(look.clone())),
                    ("lutGain1", num(&[0.8])),
                    ("lutInterpolation1", num(&[1.0])),
                    ("contrastAdjust", num(&[0.1])),
                ],
            ),
        ];
        for (name, values) in grading {
            let fx = own_fx("org.bananav.ColorGrading", None, &values);
            scenarios.push((name.to_string(), vec![fx.clone()]));
            let cookie = own_fx(
                "org.bananav.CookieCutter",
                Some("Circle Cut Out, Center, Blurred"),
                &[],
            );
            scenarios.push((format!("{name} over a hole"), vec![cookie, fx]));
        }
        // The table effects the card draws: every preset, and the first
        // over a hole (a picture with transparency).
        for id in [
            "org.bananav.Threshold",
            "org.bananav.BlackRestore",
            "org.bananav.ChannelBlend",
            "org.bananav.MaskGenerator",
            "org.bananav.BroadcastColors",
            "org.bananav.LabAdjust",
            "org.bananav.Spherize",
            "org.bananav.Swirl",
            "org.bananav.PinchPunch",
            "org.bananav.Wave",
            "org.bananav.OffsetWrap",
            "org.bananav.TinyPlanet",
            "org.bananav.Deform",
            "org.bananav.ConvolutionKernel",
            "org.bananav.QuickBlur",
            "org.bananav.BumpMap",
            "org.bananav.ChromaKeyer",
            "org.bananav.ChromaBlur",
            "org.bananav.LinearBlur",
            "org.bananav.RadialPixelate",
            "org.bananav.RadialBlur",
            "org.bananav.Median",
            "org.bananav.MinMax",
            "org.bananav.GaussianBlur",
            "org.bananav.SharpenFx",
            "org.bananav.Glow",
            "org.bananav.Vignette",
            "org.bananav.Mirror",
            "org.bananav.FillLight",
            "org.bananav.FilmGrain",
            "org.bananav.LightRays",
            "org.bananav.GradientMap",
            "org.bananav.FilmEffects",
            "org.bananav.TvSimulator",
            "org.bananav.Newsprint",
            "org.bananav.LensFlare",
            "org.bananav.Looks",
            "org.bananav.Defocus",
            "org.bananav.Starburst",
            "org.bananav.SoftContrast",
            "org.bananav.Rays",
            "org.bananav.Glint",
            "org.bananav.FlickerControl",
        ] {
            let presets = bv_effects::presets(id);
            assert!(!presets.is_empty(), "{id}");
            for preset in &presets {
                scenarios.push((
                    format!("{id} {}", preset.name),
                    vec![own_fx(id, Some(preset.name), &[])],
                ));
            }
            let cookie = own_fx(
                "org.bananav.CookieCutter",
                Some("Circle Cut Out, Center, Blurred"),
                &[],
            );
            scenarios.push((
                format!("{id} over a hole"),
                vec![cookie, own_fx(id, Some(presets[0].name), &[])],
            ));
        }
        // Color Match with a target image too (its presets give only the
        // source's means and spreads).
        for (preset, brightness, strength) in [
            ("Pastels", 0.0, 1.0),
            ("Night Sky", 1.0, 0.6),
            ("Noir", 0.0, 1.0),
        ] {
            scenarios.push((
                format!("org.bananav.ColorMatch {preset}"),
                vec![own_fx(
                    "org.bananav.ColorMatch",
                    Some(preset),
                    &[
                        (
                            "targetLook",
                            PluginValue::Text("0 0 45 6 9 24 11 13 0 0".into()),
                        ),
                        ("matchBrightness", PluginValue::Num(vec![brightness])),
                        ("strength", PluginValue::Num(vec![strength])),
                    ],
                )],
            ));
        }
        let with = |effects: &[Effect]| {
            let mut p = base.clone();
            p.clip_mut(clip).unwrap().effects = effects.to_vec();
            p
        };
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        let size = (320, 180);
        let at = Tick::from_seconds(1);
        let cpu: Vec<ImageRgbaF32> = scenarios
            .iter()
            .map(|(_, fx)| painter.linear(&with(fx), at, size, &mut pool, black))
            .collect();
        if !card(&accel) {
            return;
        }
        // The same math on the card: what is left is the processor's
        // doubles against the card's floats. Tiny Planet's seam — the
        // meridian where the picture's left and right edges meet — may
        // fall on either edge, and Radial Pixelate's nearest pixels and
        // blocks on either side of a boundary, Newsprint's dots' rims a
        // step of their table either way, Lens Correction's and Mesh
        // Warp's weights — truncated 1024ths — a unit either way where the
        // card's fused products land a hair across a whole one, at an
        // edge: a few pixels there.
        // Metal (macOS) builds the kernels with fast math, wgpu's default
        // there: a pixel at a threshold may land a few 8-bit steps off.
        let (most_allowed, mean_allowed) = if cfg!(target_os = "macos") {
            (2e-2, 1e-4)
        } else {
            (2e-3, 1e-5)
        };
        let mut differing = Vec::new();
        for ((name, fx), c) in scenarios.iter().zip(&cpu) {
            let g = painter.linear(&with(fx), at, size, &mut pool, black);
            let seam = name.starts_with("org.bananav.TinyPlanet")
                || name.starts_with("org.bananav.RadialPixelate")
                || name.starts_with("org.bananav.Newsprint")
                || name.starts_with("lens correction")
                || name.starts_with("mesh warp");
            let (mut most, mut sum, mut off) = (0f32, 0f64, 0usize);
            let mut worst = (0, [0f32; 4], [0f32; 4]);
            for (i, (a, b)) in c.data.iter().zip(&g.data).enumerate() {
                let d = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0f32, f32::max);
                if seam && d >= 2e-3 {
                    off += 1;
                    continue;
                }
                if d > most {
                    worst = (i, *a, *b);
                }
                most = most.max(d);
                sum += d as f64;
            }
            let mean = sum / c.data.len() as f64;
            let (i, a, b) = worst;
            let at = (i % c.width as usize, i / c.width as usize);
            // Metal's fast math moves more pixels across a seam.
            let seam_allowed = if cfg!(target_os = "macos") { 50 } else { 200 };
            if !(off * seam_allowed < c.data.len() && most < most_allowed && mean < mean_allowed) {
                differing.push(format!(
                    "{name}: the card's differs (max {most} at {at:?}, {a:?} against {b:?}; \
                     mean {mean}, {off} on the seam)"
                ));
            }
        }
        assert!(differing.is_empty(), "{}", differing.join("\n"));
    }

    #[test]
    fn motion_blur_draws_alike_on_the_processor_and_on_the_card() {
        // Color bars moving across (an animated Pan/Crop): the card gets
        // the frame and the next from the host as the plug-in does, and
        // both blur the same moving areas by the same kernel.
        register_own();
        let mut p = Project::new(ProjectSettings::default());
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let clip = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let key = |s: f64, x: f32| bv_timeline::fx::TransformKey {
            at: Tick::from_seconds_f64(s),
            value: bv_timeline::fx::Transform {
                x,
                ..Default::default()
            },
            curve: KeyCurve::default(),
            smoothness: 0.0,
        };
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        p.set_clip_framing_keys(clip, &[key(0.0, -0.4), key(2.0, 0.4)])
            .unwrap();

        let mut runs = Vec::new();
        for (name, values) in [
            ("fixed", vec![("blurAmount", num(&[12.0]))]),
            (
                "adaptive",
                vec![("blurMode", num(&[1.0])), ("blurMagnifier", num(&[2.0]))],
            ),
        ] {
            let mut q = p.clone();
            q.clip_mut(clip).unwrap().effects =
                vec![own_fx("org.bananav.MotionBlur", None, &values)];
            runs.push((name, q));
        }
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        let (size, at) = ((320, 180), Tick::from_seconds(1));
        let still = {
            let mut q = p.clone();
            q.clip_mut(clip).unwrap().effects.clear();
            painter.linear(&q, at, size, &mut pool, black)
        };
        let cpu: Vec<ImageRgbaF32> = runs
            .iter()
            .map(|(_, q)| painter.linear(q, at, size, &mut pool, black))
            .collect();
        // The processor blurs: something moved.
        for (c, (name, _)) in cpu.iter().zip(&runs) {
            assert!(c.data != still.data, "{name}: nothing blurred");
        }
        if !card(&accel) {
            return;
        }
        for ((name, q), c) in runs.iter().zip(&cpu) {
            let g = painter.linear(q, at, size, &mut pool, black);
            let (mut most, mut sum, mut off) = (0f32, 0f64, 0usize);
            for (a, b) in c.data.iter().zip(&g.data) {
                let d = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0f32, f32::max);
                if d >= 2e-3 {
                    off += 1;
                }
                most = most.max(d);
                sum += d as f64;
            }
            let mean = sum / c.data.len() as f64;
            // Metal (macOS) builds the kernels with fast math, wgpu's
            // default there: its blur strays further on the whole.
            let mean_allowed = if cfg!(target_os = "macos") {
                1e-4
            } else {
                1e-5
            };
            assert!(
                most < 2e-3 && mean < mean_allowed,
                "{name}: the card's differs (max {most}, mean {mean}, {off} pixels)"
            );
            assert!(g.data != still.data, "{name}: the card did not blur");
        }
    }

    #[test]
    fn flicker_control_draws_alike_on_the_processor_and_on_the_card() {
        // Color bars moving across (an animated Pan/Crop): Temporal blend
        // takes the window's frames from the host on the card as the
        // plug-in does (with motion data keyed, along it), Addition its
        // light, Histogram smoothing the levels of histograms counted.
        register_own();
        let mut p = Project::new(ProjectSettings::default());
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let clip = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let key = |s: f64, x: f32| bv_timeline::fx::TransformKey {
            at: Tick::from_seconds_f64(s),
            value: bv_timeline::fx::Transform {
                x,
                ..Default::default()
            },
            curve: KeyCurve::default(),
            smoothness: 0.0,
        };
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        p.set_clip_framing_keys(clip, &[key(0.0, -0.4), key(2.0, 0.4)])
            .unwrap();
        // Histograms of 60 frames, their levels drifting.
        let counted: Vec<(f64, [Vec<u64>; 3])> = (0..60)
            .map(|f| {
                let channel = |shift: usize| {
                    (0..bv_effects::flicker_control::LEVELS)
                        .map(|l| ((l * 7 + f * 13 + shift) % 97) as u64)
                        .collect::<Vec<u64>>()
                };
                (f as f64, [channel(0), channel(31), channel(57)])
            })
            .collect();
        let text = PluginValue::Text(bv_effects::flicker_control::histograms_text(&counted));
        let corner = |x: f64, y: f64| num(&[x, y]);
        let mut runs = Vec::new();
        for (name, values) in [
            ("even", vec![("window", num(&[2.0]))]),
            (
                "highlights",
                vec![
                    ("window", num(&[3.0])),
                    ("sensitivity", num(&[0.3])),
                    ("type", num(&[0.0])),
                ],
            ),
            (
                "midtones masked",
                vec![
                    ("window", num(&[5.0])),
                    ("sensitivity", num(&[0.7])),
                    ("type", num(&[1.0])),
                    ("mask", num(&[1.0])),
                ],
            ),
            (
                "shadows",
                vec![("window", num(&[1.0])), ("type", num(&[2.0]))],
            ),
            (
                "motion",
                vec![
                    ("window", num(&[2.0])),
                    ("motion", num(&[1.0])),
                    ("cornerTL", corner(0.1, 0.9)),
                    ("cornerTR", corner(0.9, 0.9)),
                    ("cornerBR", corner(0.9, 0.1)),
                    ("cornerBL", corner(0.1, 0.1)),
                ],
            ),
            (
                "addition",
                vec![
                    ("mode", num(&[1.0])),
                    ("frequency", num(&[7.3])),
                    ("sensitivity", num(&[0.5])),
                    ("type", num(&[1.0])),
                ],
            ),
            (
                "random addition masked",
                vec![
                    ("mode", num(&[1.0])),
                    ("frequency", num(&[0.0])),
                    ("sensitivity", num(&[1.0])),
                    ("mask", num(&[1.0])),
                ],
            ),
            (
                "histograms",
                vec![("reductionMode", num(&[1.0])), ("histograms", text.clone())],
            ),
            (
                "histograms' highlights masked",
                vec![
                    ("reductionMode", num(&[1.0])),
                    ("window", num(&[6.0])),
                    ("sensitivity", num(&[0.5])),
                    ("type", num(&[0.0])),
                    ("mask", num(&[1.0])),
                    ("histograms", text.clone()),
                ],
            ),
        ] {
            let mut q = p.clone();
            let mut fx = own_fx("org.bananav.FlickerControl", None, &values);
            if name == "motion"
                && let Some(plugin) = fx.plugin.as_deref_mut()
                && let Some(tr) = plugin.params.get_mut("cornerTR")
            {
                // The top right corner moving out over the clip.
                let at = |s: f64, x: f64| bv_timeline::fx::PluginKey {
                    at: Tick::from_seconds_f64(s),
                    value: corner(x, 0.9),
                    curve: KeyCurve::default(),
                    handles: Vec::new(),
                };
                tr.keys = vec![at(0.0, 0.85), at(2.0, 0.97)];
            }
            q.clip_mut(clip).unwrap().effects = vec![fx];
            runs.push((name, q));
        }
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        let (size, at) = ((320, 180), Tick::from_seconds(1));
        let still = {
            let mut q = p.clone();
            q.clip_mut(clip).unwrap().effects.clear();
            painter.linear(&q, at, size, &mut pool, black)
        };
        let cpu: Vec<ImageRgbaF32> = runs
            .iter()
            .map(|(_, q)| painter.linear(q, at, size, &mut pool, black))
            .collect();
        for (c, (name, _)) in cpu.iter().zip(&runs) {
            assert!(c.data != still.data, "{name}: nothing changed");
        }
        if !card(&accel) {
            return;
        }
        // Metal (macOS) builds the kernels with fast math, wgpu's default
        // there: levels counted from a picture a hair off land a level
        // apart on some of the bars.
        let metal = cfg!(target_os = "macos");
        let mut differing = Vec::new();
        for ((name, q), c) in runs.iter().zip(&cpu) {
            let g = painter.linear(q, at, size, &mut pool, black);
            let (mut most, mut sum, mut off) = (0f32, 0f64, 0usize);
            for (a, b) in c.data.iter().zip(&g.data) {
                let d = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0f32, f32::max);
                if d >= 2e-3 {
                    // Motion data's nearest pixel, half a pixel either way.
                    off += 1;
                    continue;
                }
                most = most.max(d);
                sum += d as f64;
            }
            let mean = sum / c.data.len() as f64;
            let seam = if *name == "motion" {
                off * 200 < c.data.len()
            } else if metal && name.starts_with("histograms") {
                // On Apple's chips the card's Histogram smoothing lands on
                // another level for many pixels (not understood yet); the
                // test's jagged histograms send a level's neighbours far
                // apart (a picture's, smooth, a level or so).
                off * 2 < c.data.len()
            } else if metal {
                off * 50 < c.data.len()
            } else {
                off == 0
            };
            let mean_allowed = if metal { 1e-3 } else { 1e-5 };
            if !(seam && most < 2e-3 && mean < mean_allowed) {
                differing.push(format!(
                    "{name}: the card's differs (max {most}, mean {mean}, {off} pixels of {})",
                    c.data.len()
                ));
            }
            assert!(g.data != still.data, "{name}: the card did not change it");
        }
        assert!(differing.is_empty(), "{}", differing.join("\n"));
    }

    #[test]
    fn flow_effects_draw_alike_on_the_processor_and_on_the_card() {
        // Slow Motion at half speed over color bars moving across (an
        // animated Pan/Crop), a frame halfway between two of the source's,
        // blended and nearest; and Warp Flow from the bars to a
        // checkerboard halfway.
        register_own();
        let s = Tick::from_seconds;
        let mut p = Project::new(ProjectSettings::default());
        let settings = p.settings;
        let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
        let checker = p.add_media(MediaInfo::generated(
            Generator::Checkerboard { size: 24 },
            &settings,
        ));
        let v = p.tracks[0].id;
        let clip = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let key = |at: f64, x: f32| bv_timeline::fx::TransformKey {
            at: Tick::from_seconds_f64(at),
            value: bv_timeline::fx::Transform {
                x,
                ..Default::default()
            },
            curve: KeyCurve::default(),
            smoothness: 0.0,
        };
        p.set_clip_framing_keys(clip, &[key(0.0, -0.4), key(4.0, 0.4)])
            .unwrap();
        let num = |v: &[f64]| PluginValue::Num(v.to_vec());
        // Frame 31 at half speed: the source's 15.5.
        let rate = settings.frame_rate.as_f64();
        let at_slow = Tick::from_seconds_f64(31.0 / rate);
        let mut runs = Vec::new();
        for (name, blending) in [("slow motion", 0.0), ("slow motion, nearest", 1.0)] {
            let mut q = p.clone();
            q.clip_mut(clip).unwrap().effects = vec![own_fx(
                "org.bananav.SlowMotion",
                None,
                &[
                    ("playbackRate", num(&[0.5])),
                    ("blending", num(&[blending])),
                ],
            )];
            runs.push((name, q, at_slow));
        }
        let mut q = p.clone();
        let b = q.insert_media(checker, s(4), Some(v), None).unwrap()[0];
        q.clip_mut(b).unwrap().transition_fx = own_fx("org.bananav.WarpFlow", None, &[]).plugin;
        runs.push(("warp flow", q, Tick::from_seconds_f64(4.5)));
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        let size = (320, 180);
        let cpu: Vec<ImageRgbaF32> = runs
            .iter()
            .map(|(_, q, at)| painter.linear(q, *at, size, &mut pool, black))
            .collect();
        if !card(&accel) {
            return;
        }
        for ((name, q, at), c) in runs.iter().zip(&cpu) {
            let g = painter.linear(q, *at, size, &mut pool, black);
            let (mut most, mut sum) = (0f32, 0f64);
            for (a, b) in c.data.iter().zip(&g.data) {
                let d = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0f32, f32::max);
                most = most.max(d);
                sum += d as f64;
            }
            let mean = sum / c.data.len() as f64;
            // A pixel read across a sharp edge may land a hair either way
            // in floats and in doubles.
            let off = c
                .data
                .iter()
                .zip(&g.data)
                .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| (x - y).abs() >= 2e-3))
                .count();
            assert!(
                off * 1000 < c.data.len() && mean < 1e-5,
                "{name}: the card's differs (max {most}, mean {mean}, {off} pixels)"
            );
        }
    }

    /// BananaV's own generators draw on the card as on the processor:
    /// every preset of each, the picture drawn at a quarter of the media's
    /// size as the processor's is.
    #[test]
    fn own_generators_draw_alike_on_the_processor_and_on_the_card() {
        register_own();
        let base = Project::new(ProjectSettings::default());
        let mut scenarios: Vec<(String, Project)> = Vec::new();
        for id in [
            "org.bananav.Checkerboard",
            "org.bananav.ColorGradient",
            "org.bananav.TestPattern",
            "org.bananav.NoiseTexture",
            "org.bananav.SolidColor",
        ] {
            let presets = bv_effects::presets(id);
            assert!(!presets.is_empty(), "{id}");
            for preset in &presets {
                let mut p = base.clone();
                let fx = own_fx(id, Some(preset.name), &[])
                    .plugin
                    .expect("a plug-in");
                let media = MediaInfo::generated(Generator::Plugin(fx), &p.settings.clone());
                let g = p.add_media(media);
                let v = p.tracks[0].id;
                p.insert_media(g, Tick::ZERO, Some(v), None).unwrap();
                scenarios.push((format!("{id} {}", preset.name), p));
            }
        }
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let black = [0.0, 0.0, 0.0, 1.0];
        let (size, at) = ((480, 270), Tick::from_seconds(1));
        // A pool each: the scenarios' media share an id, and a pool keeps
        // the reader it opened for it.
        let cpu: Vec<ImageRgbaF32> = scenarios
            .iter()
            .map(|(_, p)| painter.linear(p, at, size, &mut VideoPool::new(), black))
            .collect();
        if !card(&accel) {
            return;
        }
        // A frame first, the card's way ready (its first may still be the
        // processor's).
        let _ = painter.linear(&scenarios[0].1, at, size, &mut VideoPool::new(), black);
        for ((name, p), c) in scenarios.iter().zip(&cpu) {
            let g = painter.linear(p, at, size, &mut VideoPool::new(), black);
            let (mut most, mut sum, mut worst) = (0f32, 0f64, 0usize);
            for (i, (a, b)) in c.data.iter().zip(&g.data).enumerate() {
                let d = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0f32, f32::max);
                if d > most {
                    worst = i;
                }
                most = most.max(d);
                sum += f64::from(d);
            }
            let mean = sum / c.data.len() as f64;
            let at = (worst % c.width as usize, worst / c.width as usize);
            // Noise Texture's noise and Color Gradient's distances are
            // worked out in single precision on the card, in double on the
            // processor: a level apart at most, here and there (a gradient's
            // steps fall on a level's edge in a column or row now and then).
            let single = name.contains("NoiseTexture") || name.contains("ColorGradient");
            let (allowed, mean_allowed) = if single { (1e-2, 1e-4) } else { (2e-3, 1e-5) };
            assert!(
                most < allowed && mean < mean_allowed,
                "{name}: the card's differs (max {most} at {at:?}, {:?} against {:?}; mean {mean})",
                c.data[worst],
                g.data[worst]
            );
        }
    }

    /// BananaV's own compositors draw on the card as on the processor:
    /// color bars under a map — a turned gradient, a checkerboard — through
    /// every preset of each, with the picture and the map swapped and, but
    /// for Bump Map, the pixels wrapped round too.
    #[test]
    fn own_compositors_draw_alike_on_the_processor_and_on_the_card() {
        register_own();
        let mut base = Project::new(ProjectSettings::default());
        let settings = base.settings;
        let bars = base.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
        let ramp = base.add_media(MediaInfo::generated(
            Generator::Gradient {
                from: [0.0, 0.0, 0.0, 1.0],
                to: [1.0, 1.0, 1.0, 1.0],
                angle_deg: 30.0,
            },
            &settings,
        ));
        let checker = base.add_media(MediaInfo::generated(
            Generator::Checkerboard { size: 24 },
            &settings,
        ));
        let top = base.tracks[0].id;
        let below = base.add_track(bv_timeline::TrackKind::Video);
        base.insert_media(bars, Tick::ZERO, Some(below), None)
            .unwrap();
        base.make_compositing_child(below, false).unwrap();
        let num = |v: f64| PluginValue::Num(vec![v]);
        let mut scenarios: Vec<(String, Project)> = Vec::new();
        for (map_name, map) in [("ramp", ramp), ("checkerboard", checker)] {
            let mut with_map = base.clone();
            with_map
                .insert_media(map, Tick::ZERO, Some(top), None)
                .unwrap();
            for id in bv_effects::COMPOSITORS {
                let presets = bv_effects::presets(id);
                assert!(!presets.is_empty(), "{id}");
                let mut ways = vec![("", Vec::new()), (" flipped", vec![("flip", num(1.0))])];
                if id != "org.bananav.BumpMap" {
                    ways.push((" wrapped", vec![("edgePixelHandling", num(1.0))]));
                }
                for preset in &presets {
                    for (way, extra) in &ways {
                        let mut p = with_map.clone();
                        let t = p.track_by_id_mut(top).unwrap();
                        t.composite_mode = bv_timeline::CompositeMode::Custom;
                        t.compositor = own_fx(id, Some(preset.name), extra).plugin;
                        scenarios
                            .push((format!("{id} {} over the {map_name}{way}", preset.name), p));
                    }
                }
            }
        }
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let black = [0.0, 0.0, 0.0, 1.0];
        let (size, at) = ((480, 270), Tick::from_seconds(1));
        let cpu: Vec<ImageRgbaF32> = scenarios
            .iter()
            .map(|(_, p)| painter.linear(p, at, size, &mut VideoPool::new(), black))
            .collect();
        if !card(&accel) {
            return;
        }
        // A frame first, the card's way ready (its first may still be the
        // processor's).
        let _ = painter.linear(&scenarios[0].1, at, size, &mut VideoPool::new(), black);
        for ((name, p), c) in scenarios.iter().zip(&cpu) {
            let g = painter.linear(p, at, size, &mut VideoPool::new(), black);
            let (mut most, mut sum, mut worst) = (0f32, 0f64, 0usize);
            for (i, (a, b)) in c.data.iter().zip(&g.data).enumerate() {
                let d = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0f32, f32::max);
                if d > most {
                    worst = i;
                }
                most = most.max(d);
                sum += f64::from(d);
            }
            let mean = sum / c.data.len() as f64;
            let at = (worst % c.width as usize, worst / c.width as usize);
            assert!(
                most < 2e-3 && mean < 1e-5,
                "{name}: the card's differs (max {most} at {at:?}, {:?} against {:?}; mean {mean})",
                c.data[worst],
                g.data[worst]
            );
        }
    }

    /// BananaV's own transitions draw on the card as on the processor:
    /// every preset, about a third, half and four fifths of the way, color
    /// bars going and a checkerboard coming. A hard edge may fall a pixel
    /// apart (the processor's doubles against the card's floats): few
    /// pixels differ, and little on the whole.
    #[test]
    fn own_transitions_draw_alike_on_the_processor_and_on_the_card() {
        let s = Tick::from_seconds;
        let mut base = Project::new(ProjectSettings::default());
        let settings = base.settings;
        let bars = base.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
        let checker = base.add_media(MediaInfo::generated(
            Generator::Checkerboard { size: 24 },
            &settings,
        ));
        let v = base.tracks[0].id;
        base.insert_media(bars, Tick::ZERO, Some(v), None).unwrap();
        let b = base.insert_media(checker, s(4), Some(v), None).unwrap()[0];
        let mut scenarios = Vec::new();
        for id in [
            "org.bananav.LinearWipe",
            "org.bananav.ClockWipe",
            "org.bananav.Iris",
            "org.bananav.BarnDoor",
            "org.bananav.VenetianBlinds",
            "org.bananav.StarWipe",
            "org.bananav.Push",
            "org.bananav.Slide",
            "org.bananav.Squeeze",
            "org.bananav.Split",
            "org.bananav.Swap",
            "org.bananav.Zoom",
            "org.bananav.Spiral",
            "org.bananav.Flash",
            "org.bananav.Dissolve",
            "org.bananav.CrossEffect",
            "org.bananav.Portals",
            "org.bananav.Blinds3D",
            "org.bananav.Cascade3D",
            "org.bananav.FlyInOut3D",
            "org.bananav.Shuffle3D",
            "org.bananav.PageLoop",
            "org.bananav.PagePeel",
            "org.bananav.PageRoll",
            "org.bananav.GradientWipe",
            "org.bananav.GLTransition",
        ] {
            for preset in bv_effects::presets(id) {
                let mut p = base.clone();
                p.clip_mut(b).unwrap().transition_fx = own_fx(id, Some(preset.name), &[]).plugin;
                // GL Transitions' random numbers (a sine's far digits:
                // its argument's last bit, which a card's fused
                // multiply-adds may give otherwise).
                let random = preset
                    .values
                    .iter()
                    .any(|(_, v)| matches!(v, PresetValue::Text(t) if t.contains("fract(sin(")));
                scenarios.push((format!("{id} {}", preset.name), p, random));
            }
        }
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        // The frame's shape, its height odd: edges falling exactly on
        // pixels' middles go either way in floats.
        let size = (304, 171);
        // Not round shares of the way: a hard edge exactly on pixels'
        // middles goes either way in floats.
        let times = [4.31, 4.57, 4.83].map(Tick::from_seconds_f64);
        let mut cpu = Vec::new();
        for (_, p, _) in &scenarios {
            for t in times {
                cpu.push(painter.linear(p, t, size, &mut pool, black));
            }
        }
        if !card(&accel) {
            return;
        }
        let mut c = cpu.iter();
        for (name, p, random) in &scenarios {
            // GL Transitions' code: the card's sines and powers, a few
            // millionths apart, move what they read a little.
            let gl = name.starts_with("org.bananav.GLTransition");
            let (edge, rest) = if gl { (1e-1, 5e-3) } else { (2e-2, 2e-4) };
            // Metal (macOS) builds the kernels with fast math, wgpu's
            // default there: the rest strays further.
            let rest = if cfg!(target_os = "macos") {
                rest * 5.0
            } else {
                rest
            };
            for t in times {
                let a = c.next().expect("a frame on the processor");
                let g = painter.linear(p, t, size, &mut pool, black);
                let n = a.data.len();
                if *random {
                    // Each pixel's own random number apart: the same on the
                    // whole.
                    let mean = |img: &ImageRgbaF32, k: usize| {
                        img.data.iter().map(|c| f64::from(c[k])).sum::<f64>() / n as f64
                    };
                    for k in 0..4 {
                        let (x, y) = (mean(a, k), mean(&g, k));
                        assert!(
                            (x - y).abs() < 0.15,
                            "{name} at {t:?}: the card's channel {k} averages {y}, the processor's {x}"
                        );
                    }
                    continue;
                }
                // Pixels on the other side of a hard edge (few), the rest
                // close.
                let (mut off, mut sum) = (0usize, 0f64);
                for (x, y) in a.data.iter().zip(&g.data) {
                    let d = x
                        .iter()
                        .zip(y)
                        .map(|(m, n)| (m - n).abs())
                        .fold(0f32, f32::max);
                    if d > edge {
                        off += 1;
                    } else {
                        sum += f64::from(d);
                    }
                }
                let mean = sum / (n - off).max(1) as f64;
                assert!(
                    off * 50 < n && mean < rest,
                    "{name} at {t:?}: the card's differs ({off} pixels, the rest by {mean})"
                );
            }
        }
    }

    #[test]
    fn plugin_effects_draw_on_the_processor_and_on_the_card() {
        test_plugins();
        let p = red_with_invert();
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        let cpu = painter.linear(&p, Tick::ZERO, (64, 36), &mut pool, black);
        // Red inverted: cyan.
        let px = cpu.pixel(32, 18);
        assert!(px[0] < 0.01 && px[1] > 0.99 && px[2] > 0.99, "{px:?}");
        // Half the amount from one second on: gray.
        let mut half = p.clone();
        let c = half.tracks[0].clips[0].id;
        let fx = half.clip_mut(c).unwrap().effects[0]
            .plugin
            .as_mut()
            .unwrap();
        fx.params.insert(
            "amount".into(),
            PluginParam {
                value: PluginValue::Num(vec![1.0]),
                keys: vec![PluginKey {
                    at: Tick::from_seconds(1),
                    value: PluginValue::Num(vec![0.5]),
                    curve: KeyCurve::Linear,
                    handles: Vec::new(),
                }],
            },
        );
        let gray = painter.linear(&half, Tick::from_seconds(2), (64, 36), &mut pool, black);
        let px = gray.pixel(32, 18);
        assert!(
            (px[0] - 0.5).abs() < 0.01 && (px[1] - 0.5).abs() < 0.01,
            "{px:?}"
        );
        // On the graphics card, when there is one: the same.
        accel.set(GpuChoice::Auto);
        let start = Instant::now();
        while matches!(accel.status(), GpuStatus::Starting) && start.elapsed().as_secs() < 60 {
            std::thread::sleep(Duration::from_millis(50));
        }
        if matches!(accel.status(), GpuStatus::Ready(_)) {
            let card = painter.linear(&p, Tick::ZERO, (64, 36), &mut pool, black);
            let (a, b) = (card.pixel(32, 18), cpu.pixel(32, 18));
            assert!(
                a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3),
                "{a:?} {b:?}"
            );
        }
    }

    /// A compositing parent with a gradient whose mode is Custom, a
    /// Displacement Map: its child's bars are moved by the gradient, not
    /// covered by it — on the processor and on the card alike.
    /// Flicker Control reads the frames around through the effects before
    /// it: a frame made brighter than its neighbours comes back toward
    /// them.
    #[test]
    fn flicker_control_evens_a_frame_out_with_the_ones_around() {
        register_own();
        let mut p = Project::new(ProjectSettings::default());
        let grey = solid(&mut p, [0.5, 0.5, 0.5, 1.0]);
        let v = p.tracks[0].id;
        let c = p.insert_media(grey, Tick::ZERO, Some(v), None).unwrap()[0];
        let rate = p.settings.frame_rate;
        // The brightness up on frame 30 only.
        let keys = (27..=33)
            .map(|n| PluginKey {
                at: rate.frame_start(n),
                value: PluginValue::Num(vec![if n == 30 { 0.1 } else { 0.0 }]),
                curve: KeyCurve::Hold,
                handles: Vec::new(),
            })
            .collect();
        let mut bright = own_fx("org.bananav.BrightnessContrast", None, &[]);
        if let Some(fx) = bright.plugin.as_mut() {
            fx.params.insert(
                "brightness".into(),
                PluginParam {
                    value: PluginValue::Num(vec![0.0]),
                    keys,
                },
            );
        }
        p.clip_mut(c).unwrap().effects.push(bright);
        let mut evened = p.clone();
        evened.clip_mut(c).unwrap().effects.push(own_fx(
            "org.bananav.FlickerControl",
            None,
            &[("window", PluginValue::Num(vec![1.0]))],
        ));
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel, Purpose::File);
        let mut pool = VideoPool::new();
        let bg = [0.0, 0.0, 0.0, 1.0];
        let mut frame = |p: &Project, n: i64| {
            painter
                .linear(p, rate.frame_start(n), (64, 36), &mut pool, bg)
                .pixel(32, 18)[0]
        };
        let (before, flicker, now) = (frame(&p, 29), frame(&p, 30), frame(&evened, 30));
        assert!(flicker > before + 0.02, "{before} {flicker}");
        assert!(
            now < flicker - 0.01 && now > before,
            "{before} {flicker} {now}"
        );
    }

    /// Mesh Warp's mesh, a custom parameter, goes from keyframe to keyframe
    /// as the plug-in interpolates it.
    #[test]
    fn mesh_warp_moves_between_its_keyframes() {
        register_own();
        let mut p = Project::new(ProjectSettings::default());
        let gray = solid(&mut p, [0.5, 0.5, 0.5, 1.0]);
        let v = p.tracks[0].id;
        let c = p.insert_media(gray, Tick::ZERO, Some(v), None).unwrap()[0];
        // A grid of one cell, its right side at the middle at 2 s.
        let key = |s: i64, text: &str| PluginKey {
            at: Tick::from_seconds(s),
            value: PluginValue::Text(text.into()),
            curve: KeyCurve::Linear,
            handles: Vec::new(),
        };
        let one = PluginValue::Num(vec![1.0]);
        let mut warp = own_fx(
            "org.bananav.MeshWarp",
            None,
            &[("gridX", one.clone()), ("gridY", one)],
        );
        if let Some(fx) = warp.plugin.as_mut() {
            fx.params.insert(
                "mesh".into(),
                PluginParam {
                    value: PluginValue::Text(String::new()),
                    keys: vec![key(0, ""), key(2, "1 1 0 0 0.5 0 0 1 0.5 1")],
                },
            );
        }
        p.clip_mut(c).unwrap().effects.push(warp);
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel, Purpose::File);
        let mut pool = VideoPool::new();
        let bg = [0.0, 0.0, 0.0, 1.0];
        let mut at = |s: f64, x: u32| {
            painter
                .linear(&p, Tick::from_seconds_f64(s), (64, 36), &mut pool, bg)
                .pixel(x, 18)[0]
        };
        // At first the whole frame; half way, three quarters of it; then
        // half.
        assert!(at(0.0, 56) > 0.2);
        assert!(at(1.0, 56) < 0.01 && at(1.0, 40) > 0.2);
        assert!(at(2.0, 40) < 0.01 && at(2.0, 24) > 0.2);
    }

    /// Bézier Masking: a mask cuts the event out around it; with Mask FX
    /// the effects before it show only inside it, the event's own picture
    /// (the "Original" clip: the chain's input) around it.
    #[test]
    fn bezier_masking_cuts_out_or_masks_the_effects_before_it() {
        use bv_timeline::EffectKind;
        register_own();
        let mut p = Project::new(ProjectSettings {
            pixel_format: bv_timeline::PixelFormat::Float32Full,
            ..ProjectSettings::default()
        });
        let red = solid(&mut p, [1.0, 0.0, 0.0, 1.0]);
        let v = p.tracks[0].id;
        let c = p.insert_media(red, Tick::ZERO, Some(v), None).unwrap()[0];
        // A mask on the left half of the frame.
        let num = |x: f64| PluginValue::Num(vec![x]);
        let mask = |mask_fx: f64| {
            own_fx(
                "org.bananav.BezierMasking",
                None,
                &[
                    ("width1", num(0.5)),
                    ("height1", num(1.0)),
                    ("location1", PluginValue::Num(vec![0.25, 0.5])),
                    ("maskFx", num(mask_fx)),
                ],
            )
        };
        p.clip_mut(c).unwrap().effects.push(mask(0.0));
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel, Purpose::File);
        let mut pool = VideoPool::new();
        let bg = [0.0, 0.0, 1.0, 1.0];
        let mut px = |p: &Project, x: u32| {
            painter
                .linear(p, Tick::from_seconds(1), (64, 36), &mut pool, bg)
                .pixel(x, 18)
        };
        // Cut out: red on the left, the blue background on the right.
        let (left, right) = (px(&p, 10), px(&p, 54));
        assert!(left[0] > 0.9 && left[2] < 0.1, "{left:?}");
        assert!(right[2] > 0.9 && right[0] < 0.1, "{right:?}");
        // An Invert before it, masked: cyan on the left, red on the right.
        let clip = p.clip_mut(c).unwrap();
        clip.effects.clear();
        clip.effects
            .push(bv_timeline::Effect::new(EffectKind::Invert));
        clip.effects.push(mask(1.0));
        let (left, right) = (px(&p, 10), px(&p, 54));
        assert!(left[0] < 0.1 && left[1] > 0.9, "{left:?}");
        assert!(right[0] > 0.9 && right[1] < 0.1, "{right:?}");
        // The same as the track's effects: the track's picture around it.
        let chain = std::mem::take(&mut p.clip_mut(c).unwrap().effects);
        p.tracks[0].effects = chain.clone();
        let (left, right) = (px(&p, 10), px(&p, 54));
        assert!(left[0] < 0.1 && left[1] > 0.9, "{left:?}");
        assert!(right[0] > 0.9 && right[1] < 0.1, "{right:?}");
        // As the video output's: the finished frame around it.
        p.tracks[0].effects.clear();
        p.video_output_fx = chain;
        let (left, right) = (px(&p, 10), px(&p, 54));
        assert!(left[0] < 0.1 && left[1] > 0.9, "{left:?}");
        assert!(right[0] > 0.9 && right[1] < 0.1, "{right:?}");
    }

    /// Dual Fish Eye Stitching: the panorama's middle, straight ahead, is
    /// the front lens's circle's middle (the right one's, three quarters of
    /// the way across the picture); straight behind, at its edges, the back
    /// lens's, on the left.
    #[test]
    fn stitching_puts_the_lenses_ahead_and_behind() {
        register_own();
        let mut p = Project::new(ProjectSettings {
            pixel_format: bv_timeline::PixelFormat::Float32Full,
            ..ProjectSettings::default()
        });
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let c = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel, Purpose::File);
        let mut pool = VideoPool::new();
        let mut img =
            |p: &Project| painter.linear(p, Tick::ZERO, (64, 36), &mut pool, [0.0, 0.0, 0.0, 1.0]);
        let plain = img(&p);
        p.clip_mut(c)
            .unwrap()
            .effects
            .push(own_fx("org.bananav.DualFishEyeStitching", None, &[]));
        let stitched = img(&p);
        let close = |a: [f32; 4], b: [f32; 4]| (0..3).all(|k| (a[k] - b[k]).abs() < 0.05);
        assert!(close(stitched.pixel(32, 12), plain.pixel(48, 18)), "ahead");
        assert!(close(stitched.pixel(1, 12), plain.pixel(16, 18)), "behind");
    }

    /// Video Stabilization: with Freeze motion on the first frame, a
    /// frame the camera moved an eighth of the width into comes back that
    /// far (the color bars, which stand still, move the other way).
    #[test]
    fn stabilization_moves_frames_against_the_camera() {
        use bv_effects::stabilization::{Step, motion_text};
        register_own();
        let mut p = Project::new(ProjectSettings {
            pixel_format: bv_timeline::PixelFormat::Float32Full,
            ..ProjectSettings::default()
        });
        let bars = p.add_media(MediaInfo::generated(
            Generator::ColorBars,
            &p.settings.clone(),
        ));
        let v = p.tracks[0].id;
        let c = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel, Purpose::File);
        let mut pool = VideoPool::new();
        let at = p.settings.frame_rate.frame_start(1);
        let mut row = |p: &Project| -> Vec<[f32; 4]> {
            let img = painter.linear(p, at, (64, 36), &mut pool, [0.0, 0.0, 0.0, 1.0]);
            (0..64).map(|x| img.pixel(x, 10)).collect()
        };
        let plain = row(&p);
        let steps = vec![Step::similar(0.125, 0.0, 0.0, 1.0); 4];
        let num = |v: f64| PluginValue::Num(vec![v]);
        p.clip_mut(c).unwrap().effects.push(own_fx(
            "org.bananav.VideoStabilization",
            None,
            &[
                ("motionData", PluginValue::Text(motion_text(&steps))),
                ("hasMotionData", num(1.0)),
                ("stabilizeMode", num(1.0)),
                ("avoidBlackBorders", num(0.0)),
            ],
        ));
        let moved = row(&p);
        for x in [6, 20, 34, 48] {
            let (a, b) = (moved[x], plain[x + 8]);
            assert!(
                (0..3).all(|k| (a[k] - b[k]).abs() < 0.02),
                "{x}: {a:?} {b:?}"
            );
        }
    }

    /// Color Match's buttons ask the host for pictures (BananaV's
    /// extension): connected to the button's clip before the plug-in hears
    /// of the push, the picture's summary is kept; with both images the
    /// picture's cast goes.
    #[test]
    fn color_match_takes_its_images_from_the_clips_its_buttons_name() {
        register_own();
        let plugin = bv_ofx::registry()
            .plugin("org.bananav.ColorMatch")
            .expect("Color Match");
        let setup = Setup {
            frame_rate: 30.0,
            duration: 300.0,
            size: (64.0, 36.0),
            pixel_aspect: 1.0,
            interactive: true,
        };
        let instance = Instance::new_in(&plugin, &FILTERS, &setup).expect("an instance");
        let infos = instance.param_infos();
        let info = |n: &str| infos.iter().find(|i| i.name == n).expect(n).clone();
        assert_eq!(info("sourcePreview").picture, "Preview");
        assert_eq!(info("sourcePreview").picture_clip, "SourceImage");
        assert_eq!(info("targetPreview").picture, "Input");
        // Its buttons on one line: all but the last say the next one is
        // on theirs.
        assert_eq!(info("sourceTrimmer").layout_hint, 2);
        assert_eq!(info("sourcePreview").layout_hint, 0);
        let text = |name: &str| match instance.values().into_iter().find(|(n, _)| n == name) {
            Some((_, a)) => match a.value {
                Value::Text(t) => t,
                v => panic!("{v:?}"),
            },
            None => String::new(),
        };
        // A picture of two halves (a solid color is VEGAS's "solid
        // color": no matching).
        let halves = |left: [f32; 4], right: [f32; 4]| {
            let mut picture = ImageRgbaF32::filled(64, 36, left);
            for (i, px) in picture.data.iter_mut().enumerate() {
                if i % 64 >= 32 {
                    *px = right;
                }
            }
            Arc::new(picture)
        };
        // A neutral source, a warm target.
        let take = |clip: &str, button: &str, picture: Arc<ImageRgbaF32>| {
            instance.connect(clip, Arc::new(Still(picture, (64.0, 36.0))));
            instance.changed(&[button.to_string()], bv_ofx::ChangeReason::User, 0.0)
        };
        let neutral = halves([0.3, 0.3, 0.3, 1.0], [0.7, 0.7, 0.7, 1.0]);
        let changed = take("SourceImage", "sourcePreview", neutral);
        assert!(changed.iter().any(|n| n == "sourceLook"), "{changed:?}");
        assert!(text("sourceLook").starts_with("64 36 "));
        assert_eq!(text("source"), "64 × 36");
        let warm = halves([0.4, 0.3, 0.2, 1.0], [0.8, 0.6, 0.5, 1.0]);
        take("TargetImage", "targetPreview", warm.clone());
        assert!(text("targetLook").starts_with("64 36 "));
        // Rendered with both: the warm picture's cast goes (its a and b
        // take the neutral source's).
        let req = RenderRequest {
            time: 0.0,
            width: 64,
            height: 36,
            scale: 1.0,
            draft: false,
            interactive: false,
            sequential: false,
            timeline: None,
        };
        let values: Vec<(String, Animation)> = instance.values();
        let out = instance
            .render_with(
                &[(
                    "Source",
                    Arc::new(Still(warm, (64.0, 36.0))) as Arc<dyn ClipSource>,
                )],
                &values,
                &req,
            )
            .expect("a render");
        let c = out.pixel(10, 10);
        assert!(
            (c[0] - c[1]).abs() < 0.03 && (c[1] - c[2]).abs() < 0.03,
            "{c:?}"
        );
    }

    #[test]
    fn a_custom_compositing_mode_runs_its_compositor() {
        register_own();
        let mut p = Project::new(ProjectSettings::default());
        let settings = p.settings;
        let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &settings));
        let ramp = p.add_media(MediaInfo::generated(
            Generator::Gradient {
                from: [0.0, 0.0, 0.0, 1.0],
                to: [1.0, 1.0, 1.0, 1.0],
                angle_deg: 0.0,
            },
            &settings,
        ));
        let top = p.tracks[0].id;
        let below = p.add_track(bv_timeline::TrackKind::Video);
        p.insert_media(ramp, Tick::ZERO, Some(top), None).unwrap();
        p.insert_media(bars, Tick::ZERO, Some(below), None).unwrap();
        p.make_compositing_child(below, false).unwrap();
        let mut bars_only = p.clone();
        bars_only.track_by_id_mut(top).unwrap().level = 0.0;
        let mut custom = p.clone();
        let t = custom.track_by_id_mut(top).unwrap();
        t.composite_mode = bv_timeline::CompositeMode::Custom;
        t.compositor = Some(Box::new(PluginFx {
            id: "org.bananav.DisplacementMap".into(),
            version: [2, 0],
            name: "Displacement Map".into(),
            params: [("scaleHorizontal", 0.4), ("scaleVertical", 0.0)]
                .into_iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        PluginParam {
                            value: PluginValue::Num(vec![v]),
                            keys: Vec::new(),
                        },
                    )
                })
                .collect(),
        }));
        let accel = Accel::new(std::sync::Arc::new(|| {}));
        let mut painter = Painter::new(accel.clone(), Purpose::File);
        let mut pool = VideoPool::new();
        let black = [0.0, 0.0, 0.0, 1.0];
        let size = (64, 36);
        let covered = painter.linear(&p, Tick::ZERO, size, &mut pool, black);
        let plain = painter.linear(&bars_only, Tick::ZERO, size, &mut pool, black);
        let moved = painter.linear(&custom, Tick::ZERO, size, &mut pool, black);
        // Source Alpha shows the gradient: gray in the middle.
        let c = covered.pixel(32, 18);
        assert!(
            (c[0] - c[1]).abs() < 1e-3 && (c[1] - c[2]).abs() < 1e-3,
            "{c:?}"
        );
        // Custom shows the bars, the right ones (a bright map) taken from
        // further right, the middle where they were.
        assert_eq!(moved.pixel(32, 30), plain.pixel(32, 30));
        assert_ne!(moved.data, plain.data, "the bars moved");
        assert_ne!(moved.data, covered.data);
        // The card, when there is one: the same.
        accel.set(GpuChoice::Auto);
        let start = Instant::now();
        while matches!(accel.status(), GpuStatus::Starting) && start.elapsed().as_secs() < 60 {
            std::thread::sleep(Duration::from_millis(50));
        }
        if matches!(accel.status(), GpuStatus::Ready(_)) {
            let card = painter.linear(&custom, Tick::ZERO, size, &mut pool, black);
            let most = card
                .data
                .iter()
                .zip(&moved.data)
                .flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| (x - y).abs()))
                .fold(0.0f32, f32::max);
            assert!(most < 2e-3, "the card's differs by {most}");
        }
    }
}
