//! Image effects (a plug-in's descriptors and its instances), their clips
//! and the images plug-ins fetch from them, and the image effect suite.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use bv_core::ImageRgbaF32;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::param::ParamSet;
use crate::pixels::{
    Buffer, Components, Depth, DevicePicture, Format, OfxPicture, Premult, to_ofx,
};
use crate::props::{PropSet, SetKind, cstring};
use crate::suites::Message;
use crate::templates;

const EFFECT_MAGIC: u64 = u64::from_le_bytes(*b"bvOFXeff");
const CLIP_MAGIC: u64 = u64::from_le_bytes(*b"bvOFXclp");
const MEMORY_MAGIC: u64 = u64::from_le_bytes(*b"bvOFXmem");

/// Where a clip's pictures come from: the host connects one to each input
/// clip before it renders.
pub trait ClipSource: Send + Sync {
    /// The picture at `time` (frames, the effect's time): premultiplied,
    /// top row first, at the render's scale. `None`: nothing there.
    fn picture(&self, time: f64) -> Option<Arc<ImageRgbaF32>>;

    /// The picture's size at full resolution (canonical coordinates).
    fn full_size(&self) -> (f64, f64);

    /// The picture at `time` already in the graphics card's memory, in the
    /// clip's layout, for a render on the card
    /// ([`crate::Instance::render_on_card`]): handed to the plug-in as it
    /// is. `None`: [`Self::picture`]'s is sent there.
    fn on_card(&self, _time: f64) -> Option<DevicePicture> {
        None
    }
}

/// A render under way.
#[derive(Debug)]
pub(crate) struct RenderState {
    pub scale: [f64; 2],
    /// The output's pixels: x1, y1, x2, y2.
    pub window: OfxRectI,
    pub output: Option<Arc<OfxPicture>>,
    /// A render on the graphics card: every picture is in its memory.
    pub card: Option<CardRender>,
}

/// A render on the graphics card (OpenFX's CUDA render).
#[derive(Debug)]
pub(crate) struct CardRender {
    pub device: &'static bv_cuda::Device,
    /// Where the plug-in draws.
    pub output: Arc<DevicePicture>,
    /// The plug-in fetched it.
    pub fetched: bool,
}

/// What an instance needs while plug-ins call it.
#[derive(Default)]
pub(crate) struct Run {
    pub sources: RwLock<HashMap<String, Arc<dyn ClipSource>>>,
    pub render: Mutex<Option<RenderState>>,
    /// Each clip's layout (from the clip preferences).
    pub formats: RwLock<HashMap<String, (Format, Premult)>>,
    /// Images handed out and not released yet (their addresses).
    pub images: Mutex<Vec<usize>>,
    pub abort: AtomicBool,
    pub persistent: Mutex<Option<Message>>,
    pub messages: Mutex<Vec<Message>>,
    pub progress: Mutex<Option<(String, f64)>>,
    /// The timeline's time, as the time line suite tells it (frames).
    pub timeline_time: AtomicU64,
    /// The output's size at full resolution (canonical coordinates).
    pub output_size: Mutex<(f64, f64)>,
}

impl std::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run").finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Run {
    /// A clip's layout: float RGBA, premultiplied, until the clip
    /// preferences say.
    pub fn format_of(&self, clip: &str) -> (Format, Premult) {
        self.formats
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(clip)
            .copied()
            .unwrap_or((
                Format {
                    depth: Depth::Float,
                    components: Components::Rgba,
                },
                Premult::PreMultiplied,
            ))
    }

    /// Frees the images the plug-in did not release.
    pub fn free_images(&self) {
        let left = std::mem::take(&mut *lock(&self.images));
        for addr in left {
            // SAFETY: an image this host made and the plug-in did not free.
            drop(unsafe { Box::from_raw(addr as *mut Image) });
        }
    }

    pub fn push_message(&self, m: Message) {
        let mut list = lock(&self.messages);
        if list.len() >= 64 {
            list.remove(0);
        }
        list.push(m);
    }
}

/// An image effect: a plug-in's descriptor, a context's descriptor, or an
/// instance.
#[repr(C)]
#[derive(Debug)]
pub struct Effect {
    magic: u64,
    pub props: PropSet,
    pub params: ParamSet,
    // Boxes keep each clip where it is while the list grows: plug-ins
    // hold their addresses.
    #[allow(clippy::vec_box)]
    clips: RwLock<Vec<Box<Clip>>>,
    pub(crate) run: Option<Run>,
}

impl Effect {
    /// The plug-in's descriptor, before it describes itself.
    pub fn descriptor(file: &CStr) -> Box<Effect> {
        Box::new(Effect {
            magic: EFFECT_MAGIC,
            props: templates::effect_descriptor(SetKind::EffectDescriptor, file),
            params: ParamSet::new(SetKind::ParamDescriptor),
            clips: RwLock::new(Vec::new()),
            run: None,
        })
    }

    /// A context's descriptor: the plug-in's, for describing in `context`.
    pub fn for_context(base: &Effect) -> Box<Effect> {
        let props = PropSet::new(SetKind::EffectDescriptor);
        props.copy_from(&base.props);
        Box::new(Effect {
            magic: EFFECT_MAGIC,
            props,
            params: ParamSet::new(SetKind::ParamDescriptor),
            clips: RwLock::new(Vec::new()),
            run: None,
        })
    }

    /// An instance of a context's descriptor.
    pub fn instance(described: &Effect, context: &CStr) -> Box<Effect> {
        let props = PropSet::new(SetKind::EffectInstance);
        props.copy_from(&described.props);
        templates::effect_instance(&props, context);
        let effect = Box::new(Effect {
            magic: EFFECT_MAGIC,
            props,
            params: ParamSet::new(SetKind::ParamInstance),
            clips: RwLock::new(Vec::new()),
            run: Some(Run::default()),
        });
        // In its box now: its parameters may keep its set's address.
        effect.params.instantiate(&described.params);
        effect.props.set_ptr(
            kOfxImageEffectPropPluginHandle,
            effect.handle() as *const c_void,
        );
        for d in described.clips() {
            let name = cstring(&d.name);
            if let Some(c) = effect.define_clip(&name) {
                c.props.copy_from(&d.props);
                templates::clip_instance(&c.props);
            }
        }
        effect
    }

    pub fn handle(&self) -> OfxImageEffectHandle {
        self as *const Effect as OfxImageEffectHandle
    }

    /// # Safety
    /// `h` is null or readable, and an effect it points to lives on.
    pub unsafe fn from_handle<'a>(h: *const c_void) -> Option<&'a Effect> {
        if h.is_null() {
            return None;
        }
        let e = h as *const Effect;
        // SAFETY: as the caller promises; the magic number tells.
        unsafe { ((*e).magic == EFFECT_MAGIC).then(|| &*e) }
    }

    /// Adds a clip; `None` when the name is taken.
    pub fn define_clip(&self, name: &CStr) -> Option<&Clip> {
        let mut list = self.clips.write().unwrap_or_else(|e| e.into_inner());
        if list.iter().any(|c| c.name.as_bytes() == name.to_bytes()) {
            return None;
        }
        let clip = Box::new(Clip {
            magic: CLIP_MAGIC,
            props: templates::clip_descriptor(name),
            name: name.to_string_lossy().into_owned(),
            effect: OnceLock::new(),
        });
        let _ = clip.effect.set(self as *const Effect as usize);
        let ptr: *const Clip = &*clip;
        list.push(clip);
        // SAFETY: the box never moves nor goes while the effect lives.
        Some(unsafe { &*ptr })
    }

    pub fn clip(&self, name: &str) -> Option<&Clip> {
        let list = self.clips.read().unwrap_or_else(|e| e.into_inner());
        let ptr: *const Clip = &**list.iter().find(|c| c.name == name)?;
        // SAFETY: as in `define_clip`.
        Some(unsafe { &*ptr })
    }

    /// Every clip, in the order its plug-in defined them.
    pub fn clips(&self) -> Vec<&Clip> {
        let list = self.clips.read().unwrap_or_else(|e| e.into_inner());
        list.iter()
            .map(|c| {
                let ptr: *const Clip = &**c;
                // SAFETY: as in `define_clip`.
                unsafe { &*ptr }
            })
            .collect()
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        if let Some(run) = &self.run {
            run.free_images();
        }
    }
}

/// A clip of an effect.
#[repr(C)]
#[derive(Debug)]
pub struct Clip {
    magic: u64,
    pub props: PropSet,
    pub name: String,
    effect: OnceLock<usize>,
}

impl Clip {
    pub fn handle(&self) -> OfxImageClipHandle {
        self as *const Clip as OfxImageClipHandle
    }

    /// # Safety
    /// `h` is null or readable, and a clip it points to lives on.
    pub unsafe fn from_handle<'a>(h: OfxImageClipHandle) -> Option<&'a Clip> {
        if h.is_null() {
            return None;
        }
        let c = h as *const Clip;
        // SAFETY: as the caller promises; the magic number tells.
        unsafe { ((*c).magic == CLIP_MAGIC).then(|| &*c) }
    }

    pub fn effect(&self) -> Option<&Effect> {
        let e = *self.effect.get()? as *const Effect;
        // SAFETY: an effect outlives its clips.
        Some(unsafe { &*e })
    }

    pub fn is_output(&self) -> bool {
        self.name.as_bytes() == kOfxImageEffectOutputClipName.to_bytes()
    }
}

/// An image's pixels: in the processor's memory, or in the graphics
/// card's in a render on the card.
#[derive(Debug)]
pub(crate) enum Pixels {
    Host(Arc<OfxPicture>),
    Card(Arc<DevicePicture>),
}

impl Pixels {
    fn format(&self) -> Format {
        match self {
            Pixels::Host(p) => p.format,
            Pixels::Card(p) => p.format,
        }
    }

    fn size(&self) -> (usize, usize) {
        match self {
            Pixels::Host(p) => (p.width, p.height),
            Pixels::Card(p) => (p.width, p.height),
        }
    }

    fn row_bytes(&self) -> usize {
        match self {
            Pixels::Host(p) => p.row_bytes,
            Pixels::Card(p) => p.row_bytes,
        }
    }

    fn data(&self) -> *const c_void {
        match self {
            Pixels::Host(p) => p.buffer.as_ptr() as *const c_void,
            Pixels::Card(p) => p.address as usize as *const c_void,
        }
    }
}

/// An image a plug-in fetched: its handle is its property set's.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct Image {
    props: PropSet,
    pixels: Pixels,
    owner: usize,
}

static IMAGE_COUNT: AtomicU64 = AtomicU64::new(0);

fn new_image(
    run: &Run,
    owner: &Effect,
    pixels: Pixels,
    bounds: OfxRectI,
    premult: Premult,
    scale: [f64; 2],
) -> OfxPropertySetHandle {
    let props = templates::image();
    let f = pixels.format();
    props.set_cstr(kOfxImageEffectPropPixelDepth, f.depth.ofx_name());
    props.set_cstr(kOfxImageEffectPropComponents, f.components.ofx_name());
    props.set_cstr(kOfxImageEffectPropPreMultiplication, premult.ofx_name());
    props.set_dbl(kOfxImageEffectPropRenderScale, &scale);
    let par = owner
        .props
        .dbl(kOfxImageEffectPropProjectPixelAspectRatio, 0)
        .unwrap_or(1.0);
    props.set_dbl(kOfxImagePropPixelAspectRatio, &[par]);
    props.set_ptr(kOfxImagePropData, pixels.data());
    let b = [bounds.x1, bounds.y1, bounds.x2, bounds.y2];
    props.set_int(kOfxImagePropBounds, &b);
    props.set_int(kOfxImagePropRegionOfDefinition, &b);
    props.set_int(kOfxImagePropRowBytes, &[pixels.row_bytes() as i32]);
    props.set_cstr(kOfxImagePropField, kOfxImageFieldNone);
    let id = IMAGE_COUNT.fetch_add(1, Ordering::Relaxed);
    props.set_str(kOfxImagePropUniqueIdentifier, &[&format!("bananav-{id}")]);
    let image = Box::into_raw(Box::new(Image {
        props,
        pixels,
        owner: owner as *const Effect as usize,
    }));
    lock(&run.images).push(image as usize);
    image as OfxPropertySetHandle
}

/// The picture for clip `clip` at `time`, in the clip's layout, and its
/// pixel bounds, for the image effect suite. In a render on the graphics
/// card every picture is in the card's memory: the source's own there, or
/// its picture sent there.
fn fetch(
    effect: &Effect,
    run: &Run,
    clip: &Clip,
    time: f64,
) -> Result<OfxPropertySetHandle, OfxStatus> {
    let (format, premult) = run.format_of(&clip.name);
    if clip.is_output() {
        let mut render = lock(&run.render);
        let Some(r) = render.as_mut() else {
            return Err(kOfxStatFailed);
        };
        let (window, scale) = (r.window, r.scale);
        let out = match r.card.as_mut() {
            Some(card) => {
                card.fetched = true;
                Pixels::Card(card.output.clone())
            }
            None => {
                let w = (r.window.x2 - r.window.x1).max(0) as usize;
                let h = (r.window.y2 - r.window.y1).max(0) as usize;
                Pixels::Host(
                    r.output
                        .get_or_insert_with(|| Arc::new(OfxPicture::new(w, h, format)))
                        .clone(),
                )
            }
        };
        drop(render);
        return Ok(new_image(run, effect, out, window, premult, scale));
    }
    let (scale, card) = lock(&run.render).as_ref().map_or(([1.0, 1.0], None), |r| {
        (r.scale, r.card.as_ref().map(|c| c.device))
    });
    let source = run
        .sources
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&clip.name)
        .cloned()
        .ok_or(kOfxStatFailed)?;
    let pixels = match card {
        Some(device) => match source.on_card(time).filter(|p| p.format == format) {
            Some(on_card) => Pixels::Card(Arc::new(on_card)),
            None => {
                let picture = source.picture(time).ok_or(kOfxStatFailed)?;
                let sent = DevicePicture::upload(device, &picture, format)
                    .map_err(|_| kOfxStatErrMemory)?;
                Pixels::Card(Arc::new(sent))
            }
        },
        None => {
            let picture = source.picture(time).ok_or(kOfxStatFailed)?;
            Pixels::Host(Arc::new(to_ofx(&picture, format)))
        }
    };
    let (w, h) = pixels.size();
    let bounds = OfxRectI {
        x1: 0,
        y1: 0,
        x2: w as c_int,
        y2: h as c_int,
    };
    // Inputs are always premultiplied.
    let _ = premult;
    Ok(new_image(
        run,
        effect,
        pixels,
        bounds,
        Premult::PreMultiplied,
        scale,
    ))
}

// The image effect suite.

unsafe extern "C" fn get_property_set(
    effect: OfxImageEffectHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(e) = (unsafe { Effect::from_handle(effect as *const c_void) }) else {
        return kOfxStatErrBadHandle;
    };
    if props.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the handle.
    unsafe { *props = e.props.handle() };
    kOfxStatOK
}

unsafe extern "C" fn get_param_set(
    effect: OfxImageEffectHandle,
    set: *mut OfxParamSetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(e) = (unsafe { Effect::from_handle(effect as *const c_void) }) else {
        return kOfxStatErrBadHandle;
    };
    if set.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the handle.
    unsafe { *set = e.params.handle() };
    kOfxStatOK
}

unsafe extern "C" fn clip_define(
    effect: OfxImageEffectHandle,
    name: *const c_char,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(e) = (unsafe { Effect::from_handle(effect as *const c_void) }) else {
        return kOfxStatErrBadHandle;
    };
    if name.is_null() {
        return kOfxStatErrValue;
    }
    if e.run.is_some() {
        // Instances do not define clips.
        return kOfxStatErrBadHandle;
    }
    // SAFETY: a C string from the plug-in.
    let name = unsafe { CStr::from_ptr(name) };
    match e.define_clip(name) {
        Some(c) => {
            if !props.is_null() {
                // SAFETY: where the plug-in wants the handle.
                unsafe { *props = c.props.handle() };
            }
            kOfxStatOK
        }
        None => kOfxStatErrExists,
    }
}

unsafe extern "C" fn clip_get_handle(
    effect: OfxImageEffectHandle,
    name: *const c_char,
    clip: *mut OfxImageClipHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(e) = (unsafe { Effect::from_handle(effect as *const c_void) }) else {
        return kOfxStatErrBadHandle;
    };
    if name.is_null() {
        return kOfxStatErrUnknown;
    }
    // SAFETY: a C string from the plug-in.
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    let Some(c) = e.clip(&name) else {
        if crate::suites::tracing() {
            eprintln!("ofx: clipGetHandle {name:?}: no such clip");
        }
        return kOfxStatErrUnknown;
    };
    // SAFETY: where the plug-in wants the handles.
    unsafe {
        if !clip.is_null() {
            *clip = c.handle();
        }
        if !props.is_null() {
            *props = c.props.handle();
        }
    }
    kOfxStatOK
}

unsafe extern "C" fn clip_get_property_set(
    clip: OfxImageClipHandle,
    props: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { Clip::from_handle(clip) }) else {
        return kOfxStatErrBadHandle;
    };
    if props.is_null() {
        return kOfxStatErrValue;
    }
    // SAFETY: where the plug-in wants the handle.
    unsafe { *props = c.props.handle() };
    kOfxStatOK
}

unsafe extern "C" fn clip_get_image(
    clip: OfxImageClipHandle,
    time: OfxTime,
    _region: *const OfxRectD,
    image: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { Clip::from_handle(clip) }) else {
        return kOfxStatErrBadHandle;
    };
    if image.is_null() {
        return kOfxStatErrValue;
    }
    let Some(effect) = c.effect() else {
        return kOfxStatErrBadHandle;
    };
    let Some(run) = &effect.run else {
        return kOfxStatErrBadHandle;
    };
    match fetch(effect, run, c, time) {
        Ok(h) => {
            // SAFETY: where the plug-in wants the image.
            unsafe { *image = h };
            kOfxStatOK
        }
        Err(e) => e,
    }
}

unsafe extern "C" fn clip_release_image(image: OfxPropertySetHandle) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(props) = (unsafe { PropSet::from_handle(image) }) else {
        return kOfxStatErrBadHandle;
    };
    if props.kind() != SetKind::Image {
        return kOfxStatErrBadHandle;
    }
    let img = image as *mut Image;
    // SAFETY: image sets only live inside images.
    let owner = unsafe { (*img).owner } as *const Effect;
    // SAFETY: the image's effect outlives its images.
    let Some(run) = (unsafe { &*owner }).run.as_ref() else {
        return kOfxStatErrBadHandle;
    };
    let mut live = lock(&run.images);
    match live.iter().position(|&a| a == img as usize) {
        Some(i) => {
            live.swap_remove(i);
            drop(live);
            // SAFETY: made by `new_image`, released once.
            drop(unsafe { Box::from_raw(img) });
            kOfxStatOK
        }
        None => kOfxStatErrBadHandle,
    }
}

unsafe extern "C" fn clip_get_region_of_definition(
    clip: OfxImageClipHandle,
    _time: OfxTime,
    bounds: *mut OfxRectD,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(c) = (unsafe { Clip::from_handle(clip) }) else {
        return kOfxStatErrBadHandle;
    };
    if bounds.is_null() {
        return kOfxStatErrValue;
    }
    let Some(run) = c.effect().and_then(|e| e.run.as_ref()) else {
        return kOfxStatErrBadHandle;
    };
    let (w, h) = if c.is_output() {
        *lock(&run.output_size)
    } else {
        match run
            .sources
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&c.name)
        {
            Some(s) => s.full_size(),
            None => (0.0, 0.0),
        }
    };
    // SAFETY: where the plug-in wants the rectangle.
    unsafe {
        *bounds = OfxRectD {
            x1: 0.0,
            y1: 0.0,
            x2: w,
            y2: h,
        }
    };
    kOfxStatOK
}

unsafe extern "C" fn abort(effect: OfxImageEffectHandle) -> c_int {
    // SAFETY: the plug-in's handle.
    match unsafe { Effect::from_handle(effect as *const c_void) }.and_then(|e| e.run.as_ref()) {
        Some(run) => run.abort.load(Ordering::Relaxed) as c_int,
        None => 0,
    }
}

/// Memory from the image effect suite's allocator.
#[repr(C)]
#[derive(Debug)]
struct ImageMemory {
    magic: u64,
    buffer: Buffer,
    locks: AtomicI32,
}

/// # Safety
/// `h` is null or readable, and memory it points to lives on.
unsafe fn memory<'a>(h: OfxImageMemoryHandle) -> Option<&'a ImageMemory> {
    if h.is_null() {
        return None;
    }
    let m = h as *const ImageMemory;
    // SAFETY: as the caller promises; the magic number tells.
    unsafe { ((*m).magic == MEMORY_MAGIC).then(|| &*m) }
}

unsafe extern "C" fn image_memory_alloc(
    _effect: OfxImageEffectHandle,
    n_bytes: usize,
    handle: *mut OfxImageMemoryHandle,
) -> OfxStatus {
    if handle.is_null() {
        return kOfxStatErrValue;
    }
    let m = Box::new(ImageMemory {
        magic: MEMORY_MAGIC,
        buffer: Buffer::new(n_bytes),
        locks: AtomicI32::new(0),
    });
    // SAFETY: where the plug-in wants the handle.
    unsafe { *handle = Box::into_raw(m) as OfxImageMemoryHandle };
    kOfxStatOK
}

unsafe extern "C" fn image_memory_free(h: OfxImageMemoryHandle) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    match unsafe { memory(h) } {
        Some(_) => {
            // SAFETY: made by `image_memory_alloc`, freed once.
            drop(unsafe { Box::from_raw(h as *mut ImageMemory) });
            kOfxStatOK
        }
        None => kOfxStatErrBadHandle,
    }
}

unsafe extern "C" fn image_memory_lock(
    h: OfxImageMemoryHandle,
    ptr: *mut *mut c_void,
) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    let Some(m) = (unsafe { memory(h) }) else {
        return kOfxStatErrBadHandle;
    };
    if ptr.is_null() {
        return kOfxStatErrValue;
    }
    m.locks.fetch_add(1, Ordering::Relaxed);
    // SAFETY: where the plug-in wants the memory.
    unsafe { *ptr = m.buffer.as_ptr() as *mut c_void };
    kOfxStatOK
}

unsafe extern "C" fn image_memory_unlock(h: OfxImageMemoryHandle) -> OfxStatus {
    // SAFETY: the plug-in's handle.
    match unsafe { memory(h) } {
        Some(m) => {
            m.locks.fetch_sub(1, Ordering::Relaxed);
            kOfxStatOK
        }
        None => kOfxStatErrBadHandle,
    }
}

/// The image effect suite plug-ins fetch.
pub static IMAGE_EFFECT_SUITE: OfxImageEffectSuiteV1 = OfxImageEffectSuiteV1 {
    get_property_set,
    get_param_set,
    clip_define,
    clip_get_handle,
    clip_get_property_set,
    clip_get_image,
    clip_release_image,
    clip_get_region_of_definition,
    abort,
    image_memory_alloc,
    image_memory_free,
    image_memory_lock,
    image_memory_unlock,
};
