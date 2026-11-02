//! An instance of a plug-in: an effect applied somewhere, with its own
//! parameter values, that renders pictures and answers changes.
#![allow(unsafe_code)]

use std::collections::{BTreeSet, HashMap};
use std::ffi::{CStr, c_void};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx_sys::names::*;
use bv_ofx_sys::{OfxRectI, OfxStatus};

use crate::effect::{CardRender, ClipSource, Effect, RenderState, Run};
use crate::info::{Context, Error, ParamInfo};
use crate::interact::{Interact, Mark, PenAction, View};
use crate::param::ParamSet;
use crate::pixels::{
    Components, Depth, DevicePicture, Format, OfxPicture, Premult, from_ofx, to_ofx,
};
use crate::plugin::{Plugin, succeeded};
use crate::props::{PropSet, SetKind, cstring};
use crate::suites::Message;
use crate::value::Animation;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Where an instance works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    pub frame_rate: f64,
    /// The effect's length, in frames (its time runs from 0).
    pub duration: f64,
    /// The pictures' full size (canonical coordinates): the project's
    /// frame, or the media's for an event's effects.
    pub size: (f64, f64),
    pub pixel_aspect: f64,
    /// It renders for someone watching (the preview) rather than a file.
    pub interactive: bool,
}

/// One frame to render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderRequest {
    /// Frames, the effect's time.
    pub time: f64,
    /// The output's size in pixels.
    pub width: u32,
    pub height: u32,
    /// The output's pixels over its full size (1 at full resolution).
    pub scale: f64,
    /// A quick render will do (a preview at draft quality).
    pub draft: bool,
    pub interactive: bool,
    /// Frames come in order (rendering a file).
    pub sequential: bool,
    /// The frame's place on the timeline, as its ruler shows it (seconds),
    /// for plug-ins that show it (`kBananaVImageEffectPropTimelineTime`).
    pub timeline: Option<f64>,
}

/// What the overlay did with the pen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PenAnswer {
    /// The overlay took it (the view should not use it too).
    pub taken: bool,
    /// The parameters the plug-in changed (by the pen, and in turn).
    pub changed: Vec<String>,
}

/// Why parameters changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeReason {
    /// The user changed them.
    User,
    /// The plug-in did.
    Plugin,
    /// The time changed (the cursor moved).
    Time,
}

impl ChangeReason {
    fn ofx_name(self) -> &'static CStr {
        match self {
            ChangeReason::User => kOfxChangeUserEdited,
            ChangeReason::Plugin => kOfxChangePluginEdited,
            ChangeReason::Time => kOfxChangeTime,
        }
    }
}

/// An instance of a plug-in.
#[derive(Debug)]
pub struct Instance {
    plugin: Arc<Plugin>,
    context: Context,
    effect: Box<Effect>,
    /// One action at a time: renders and changes wait for each other.
    busy: Mutex<()>,
    /// The frame range of a sequence being rendered, and whether its
    /// pictures are on the graphics card (CUDA's).
    sequence: Mutex<Option<(f64, f64, bool)>>,
    /// Its overlay (on-screen controls), made when first asked for.
    interact: OnceLock<Option<Interact>>,
    /// Its parameters' own interfaces (BananaV's extension), each made
    /// when first asked for.
    param_interacts: Mutex<HashMap<String, Option<Interact>>>,
    created: bool,
}

/// The render arguments every render and sequence action has: the
/// graphics card interface the pictures are in — CUDA's in a render on the
/// card, none otherwise (the processor's memory) —, a whole render that is
/// not a thumbnail.
fn render_flags(args: &PropSet, cuda: bool) {
    for gpu in [
        kOfxImageEffectPropOpenGLEnabled,
        kOfxImageEffectPropMetalEnabled,
        kOfxImageEffectPropOpenCLEnabled,
    ] {
        args.set_int(gpu, &[0]);
    }
    args.set_int(kOfxImageEffectPropCudaEnabled, &[cuda as i32]);
    args.set_cstr(kOfxImageEffectPropNoSpatialAwareness, c"false");
    args.set_cstr(kOfxImageEffectPropThumbnailRender, c"false");
}

/// What a render drew.
enum Drawn {
    /// The picture, in BananaV's layout.
    Host(ImageRgbaF32),
    /// Into the output picture on the graphics card.
    Card,
}

/// The picture `source` gives at `time` (an effect that changes nothing
/// names it) copied into `output` on the card; `false` when its size or
/// layout is not the output's.
fn copy_identity(
    device: &'static bv_cuda::Device,
    source: &dyn ClipSource,
    time: f64,
    output: &DevicePicture,
) -> bool {
    let same = |w: usize, h: usize, row_bytes: usize, format: Format| {
        (w, h, row_bytes, format) == (output.width, output.height, output.row_bytes, output.format)
    };
    if let Some(p) = source.on_card(time) {
        // SAFETY: both pictures' memory, as big as the output.
        return same(p.width, p.height, p.row_bytes, p.format)
            && unsafe { device.copy(output.address, p.address, output.len()) }.is_ok();
    }
    let Some(picture) = source.picture(time) else {
        return false;
    };
    let pic = to_ofx(&picture, output.format);
    // SAFETY: the output's memory, as big as the picture.
    same(pic.width, pic.height, pic.row_bytes, pic.format)
        && unsafe { device.upload(output.address, pic.buffer.bytes()) }.is_ok()
}

/// The best channels a clip takes: RGBA when it does.
fn best_components(supported: &[String]) -> Components {
    if supported.is_empty() {
        return Components::Rgba;
    }
    [Components::Rgba, Components::Rgb, Components::Alpha]
        .into_iter()
        .find(|c| {
            supported
                .iter()
                .any(|s| Components::from_ofx_name(s) == Some(*c))
        })
        .unwrap_or(Components::Rgba)
}

impl Instance {
    /// Creates an instance of `plugin` in `context`.
    pub fn new(plugin: &Arc<Plugin>, context: Context, setup: &Setup) -> Result<Instance, Error> {
        let described = plugin.described(context).ok_or(Error::Context)?;
        let effect = Effect::instance(described, context.ofx_name());
        effect.params.denormalise_defaults(setup.size);
        let props = &effect.props;
        let (w, h) = setup.size;
        props.set_dbl(kOfxImageEffectPropProjectSize, &[w, h]);
        props.set_dbl(kOfxImageEffectPropProjectExtent, &[w, h]);
        props.set_dbl(kOfxImageEffectPropProjectOffset, &[0.0, 0.0]);
        props.set_dbl(
            kOfxImageEffectPropProjectPixelAspectRatio,
            &[setup.pixel_aspect],
        );
        props.set_dbl(kOfxImageEffectInstancePropEffectDuration, &[setup.duration]);
        props.set_dbl(kOfxImageEffectPropFrameRate, &[setup.frame_rate]);
        props.set_int(kOfxPropIsInteractive, &[setup.interactive as i32]);
        for c in effect.clips() {
            c.props
                .set_dbl(kOfxImageEffectPropFrameRate, &[setup.frame_rate]);
            c.props
                .set_dbl(kOfxImageEffectPropUnmappedFrameRate, &[setup.frame_rate]);
            c.props
                .set_dbl(kOfxImageEffectPropFrameRange, &[0.0, setup.duration]);
            c.props.set_dbl(
                kOfxImageEffectPropUnmappedFrameRange,
                &[0.0, setup.duration],
            );
            c.props
                .set_dbl(kOfxImagePropPixelAspectRatio, &[setup.pixel_aspect]);
            c.props
                .set_int(kOfxImageClipPropConnected, &[c.is_output() as i32]);
        }
        if let Some(run) = &effect.run {
            *lock(&run.output_size) = setup.size;
        }
        let mut instance = Instance {
            plugin: Arc::clone(plugin),
            context,
            effect,
            busy: Mutex::new(()),
            sequence: Mutex::new(None),
            interact: OnceLock::new(),
            param_interacts: Mutex::new(HashMap::new()),
            created: false,
        };
        let status = instance.call(kOfxActionCreateInstance, None, None);
        instance.run().free_images();
        if !succeeded(status) {
            return Err(Error::Action {
                action: "CreateInstance".into(),
                status,
            });
        }
        instance.created = true;
        instance.clip_preferences();
        Ok(instance)
    }

    /// An instance in the first of `contexts` the plug-in has that it
    /// makes one in: some plug-ins fail in one of their contexts (Natron's
    /// tracker asks for its mask clip, which it describes only in the
    /// general context).
    pub fn new_in(
        plugin: &Arc<Plugin>,
        contexts: &[Context],
        setup: &Setup,
    ) -> Result<Instance, Error> {
        let mut first = None;
        for &c in contexts.iter().filter(|c| plugin.info.contexts.contains(c)) {
            match Instance::new(plugin, c, setup) {
                Ok(i) => return Ok(i),
                Err(e) => {
                    first.get_or_insert(e);
                }
            }
        }
        Err(first.unwrap_or(Error::Context))
    }

    fn run(&self) -> &Run {
        self.effect.run.as_ref().expect("an instance runs")
    }

    fn call(
        &self,
        action: &CStr,
        in_args: Option<&PropSet>,
        out_args: Option<&PropSet>,
    ) -> OfxStatus {
        self.plugin.call(
            action,
            self.effect.handle() as *const c_void,
            in_args,
            out_args,
        )
    }

    pub fn plugin(&self) -> &Arc<Plugin> {
        &self.plugin
    }

    pub fn context(&self) -> Context {
        self.context
    }

    /// Tells the instance its effect's length in frames (a generator's:
    /// its event's, which can change; [`Setup::duration`] at first).
    pub fn set_duration(&self, frames: f64) {
        let _busy = lock(&self.busy);
        self.effect
            .props
            .set_dbl(kOfxImageEffectInstancePropEffectDuration, &[frames]);
        for c in self.effect.clips() {
            c.props
                .set_dbl(kOfxImageEffectPropFrameRange, &[0.0, frames]);
            c.props
                .set_dbl(kOfxImageEffectPropUnmappedFrameRange, &[0.0, frames]);
        }
    }

    /// The instance's effect (its properties, parameters and clips).
    pub fn effect(&self) -> &Effect {
        &self.effect
    }

    pub fn params(&self) -> &ParamSet {
        &self.effect.params
    }

    /// Every parameter, as the plug-in describes it now.
    pub fn param_infos(&self) -> Vec<ParamInfo> {
        self.effect
            .params
            .all()
            .into_iter()
            .map(ParamInfo::of)
            .collect()
    }

    /// The values to keep (in the project): every parameter that has one
    /// and that the plug-in wants kept.
    pub fn values(&self) -> Vec<(String, Animation)> {
        self.effect
            .params
            .all()
            .into_iter()
            .filter(|p| p.ty.has_value() && p.props.flag(kOfxParamPropPersistant).unwrap_or(true))
            .map(|p| (p.name.clone(), p.animation()))
            .collect()
    }

    /// Sets parameters' values (from the project): the plug-in is not told.
    pub fn set_values(&self, values: &[(String, Animation)]) {
        for (name, anim) in values {
            if let Some(p) = self.effect.params.get(name)
                && p.ty.has_value()
            {
                p.set_animation(anim.clone());
            }
        }
    }

    /// Connects an input clip to its pictures.
    pub fn connect(&self, clip: &str, source: Arc<dyn ClipSource>) {
        self.run()
            .sources
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(clip.to_string(), source);
        if let Some(c) = self.effect.clip(clip) {
            c.props.set_int(kOfxImageClipPropConnected, &[1]);
        }
    }

    pub fn disconnect(&self, clip: &str) {
        self.run()
            .sources
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(clip);
        if let Some(c) = self.effect.clip(clip) {
            c.props.set_int(kOfxImageClipPropConnected, &[0]);
        }
    }

    /// Asks the plug-in how its clips' pictures should be (the clip
    /// preferences action) and sets them.
    pub fn clip_preferences(&self) {
        let depth = self.plugin.depth();
        let par = self
            .effect
            .props
            .dbl(kOfxImageEffectPropProjectPixelAspectRatio, 0)
            .unwrap_or(1.0);
        let rate = self
            .effect
            .props
            .dbl(kOfxImageEffectPropFrameRate, 0)
            .unwrap_or(25.0);
        let out = PropSet::new(SetKind::Arguments);
        let clips = self.effect.clips();
        let name = |prefix: &str, clip: &str| cstring(&format!("{prefix}{clip}"));
        for c in &clips {
            let supported = c.props.strs(kOfxImageEffectPropSupportedComponents);
            let comps = best_components(&supported);
            out.define_str(
                &name("OfxImageClipPropComponents_", &c.name),
                Some(1),
                &[comps.ofx_name()],
            );
            out.define_str(
                &name("OfxImageClipPropDepth_", &c.name),
                Some(1),
                &[depth.ofx_name()],
            );
            out.define_dbl(&name("OfxImageClipPropPAR_", &c.name), Some(1), &[par]);
        }
        out.define_dbl(kOfxImageEffectPropFrameRate, Some(1), &[rate]);
        out.define_str(
            kOfxImageEffectPropPreMultiplication,
            Some(1),
            &[kOfxImagePreMultiplied],
        );
        out.define_str(kOfxImageClipPropFieldOrder, Some(1), &[kOfxImageFieldNone]);
        out.define_int(kOfxImageClipPropContinuousSamples, Some(1), &[0]);
        out.define_int(kOfxImageEffectFrameVarying, Some(1), &[0]);
        let _ = self.call(kOfxImageEffectActionGetClipPreferences, None, Some(&out));
        self.run().free_images();
        let output_premult = out
            .str(kOfxImageEffectPropPreMultiplication, 0)
            .and_then(|s| Premult::from_ofx_name(&s))
            .unwrap_or(Premult::PreMultiplied);
        let mut formats = HashMap::new();
        for c in &clips {
            let supported = c.props.strs(kOfxImageEffectPropSupportedComponents);
            let comps = out
                .str(&name("OfxImageClipPropComponents_", &c.name), 0)
                .and_then(|s| Components::from_ofx_name(&s))
                .filter(|comp| {
                    supported.is_empty()
                        || supported
                            .iter()
                            .any(|s| Components::from_ofx_name(s) == Some(*comp))
                })
                .unwrap_or_else(|| best_components(&supported));
            let premult = if c.is_output() {
                output_premult
            } else {
                Premult::PreMultiplied
            };
            let format = Format {
                depth,
                components: comps,
            };
            c.props
                .set_cstr(kOfxImageEffectPropPixelDepth, depth.ofx_name());
            c.props
                .set_cstr(kOfxImageEffectPropComponents, comps.ofx_name());
            c.props
                .set_cstr(kOfxImageClipPropUnmappedPixelDepth, Depth::Float.ofx_name());
            c.props.set_cstr(
                kOfxImageClipPropUnmappedComponents,
                Components::Rgba.ofx_name(),
            );
            c.props
                .set_cstr(kOfxImageEffectPropPreMultiplication, premult.ofx_name());
            formats.insert(c.name.clone(), (format, premult));
        }
        *self
            .run()
            .formats
            .write()
            .unwrap_or_else(|e| e.into_inner()) = formats;
    }

    /// Whether the plug-in renders with CUDA (OpenFX 1.5's CUDA render,
    /// "true" or "needed"): as the instance says, else as its plug-in does.
    pub fn renders_with_cuda(&self) -> bool {
        matches!(self.cuda_support().as_deref(), Some("true" | "needed"))
    }

    /// Whether it renders only with CUDA ("needed").
    fn needs_cuda(&self) -> bool {
        self.cuda_support().as_deref() == Some("needed")
    }

    fn cuda_support(&self) -> Option<String> {
        let said = |p: &PropSet| p.str(kOfxImageEffectPropCudaRenderSupported, 0);
        said(&self.effect.props).or_else(|| said(&self.plugin.descriptor().props))
    }

    /// The layout of clip `clip`'s pictures, as its clip preferences say.
    pub fn clip_format(&self, clip: &str) -> (Format, Premult) {
        self.run().format_of(clip)
    }

    /// The action arguments every render action shares.
    fn render_args(&self, req: &RenderRequest, cuda: bool) -> PropSet {
        let args = PropSet::new(SetKind::Arguments);
        args.set_dbl(kOfxPropTime, &[req.time]);
        args.set_cstr(kOfxImageEffectPropFieldToRender, kOfxImageFieldNone);
        args.set_int(
            kOfxImageEffectPropRenderWindow,
            &[0, 0, req.width as i32, req.height as i32],
        );
        args.set_dbl(kOfxImageEffectPropRenderScale, &[req.scale, req.scale]);
        args.set_int(
            kOfxImageEffectPropSequentialRenderStatus,
            &[req.sequential as i32],
        );
        args.set_int(
            kOfxImageEffectPropInteractiveRenderStatus,
            &[req.interactive as i32],
        );
        args.set_int(kOfxImageEffectPropRenderQualityDraft, &[req.draft as i32]);
        if let Some(t) = req.timeline {
            args.set_dbl(
                bv_ofx_sys::extensions::kBananaVImageEffectPropTimelineTime,
                &[t],
            );
        }
        render_flags(&args, cuda);
        args
    }

    fn sequence_args(&self, first: f64, last: f64, req: &RenderRequest, cuda: bool) -> PropSet {
        let args = PropSet::new(SetKind::Arguments);
        args.set_dbl(kOfxImageEffectPropFrameRange, &[first, last]);
        args.set_dbl(kOfxImageEffectPropFrameStep, &[1.0]);
        args.set_int(kOfxPropIsInteractive, &[req.interactive as i32]);
        args.set_dbl(kOfxImageEffectPropRenderScale, &[req.scale, req.scale]);
        args.set_int(
            kOfxImageEffectPropSequentialRenderStatus,
            &[req.sequential as i32],
        );
        args.set_int(
            kOfxImageEffectPropInteractiveRenderStatus,
            &[req.interactive as i32],
        );
        render_flags(&args, cuda);
        args
    }

    /// Starts rendering frames `first`..=`last` in order (a file): the
    /// plug-in may prepare for it.
    pub fn begin_sequence(&self, first: f64, last: f64, req: &RenderRequest) {
        let _busy = lock(&self.busy);
        let args = self.sequence_args(first, last, req, false);
        let _ = self.call(kOfxImageEffectActionBeginSequenceRender, Some(&args), None);
        *lock(&self.sequence) = Some((first, last, false));
    }

    pub fn end_sequence(&self, req: &RenderRequest) {
        let _busy = lock(&self.busy);
        if let Some((first, last, cuda)) = lock(&self.sequence).take() {
            let args = self.sequence_args(first, last, req, cuda);
            let _ = self.call(kOfxImageEffectActionEndSequenceRender, Some(&args), None);
        }
    }

    /// Renders one frame, its inputs from the connected clips.
    pub fn render(&self, req: &RenderRequest) -> Result<ImageRgbaF32, Error> {
        self.render_with(&[], &[], req)
    }

    /// Renders one frame with `inputs` connected and `values` set for it,
    /// as one action: another thread rendering the same instance waits.
    /// The inputs stay connected (their clips show as connected) but their
    /// pictures are let go of. A plug-in that renders only with CUDA gets
    /// them sent to CUDA's first device, and its picture back.
    pub fn render_with(
        &self,
        inputs: &[(&str, Arc<dyn ClipSource>)],
        values: &[(String, Animation)],
        req: &RenderRequest,
    ) -> Result<ImageRgbaF32, Error> {
        if self.needs_cuda()
            && let Some(device) = bv_cuda::devices().first()
        {
            return self.render_through_card(inputs, values, req, device);
        }
        match self.render_inputs(inputs, values, req, None)? {
            Drawn::Host(img) => Ok(img),
            Drawn::Card => Err(Error::NoOutput),
        }
    }

    /// [`Self::render_with`] on the graphics card, for a plug-in that
    /// renders with CUDA ([`Self::renders_with_cuda`]): its pictures are in
    /// CUDA device `device`'s memory — each input's [`ClipSource::on_card`]
    /// picture, or its picture sent there — and it draws into `output`, in
    /// the output clip's layout and the request's size. Its context is
    /// current meanwhile, and what the plug-in queued is done when this
    /// returns.
    pub fn render_on_card(
        &self,
        inputs: &[(&str, Arc<dyn ClipSource>)],
        values: &[(String, Animation)],
        req: &RenderRequest,
        device: &'static bv_cuda::Device,
        output: DevicePicture,
    ) -> Result<(), Error> {
        let card = CardRender {
            device,
            output: Arc::new(output),
            fetched: false,
        };
        self.render_inputs(inputs, values, req, Some(card))
            .map(|_| ())
    }

    /// [`Self::render_on_card`] into memory of its own, the picture then
    /// brought back.
    fn render_through_card(
        &self,
        inputs: &[(&str, Arc<dyn ClipSource>)],
        values: &[(String, Animation)],
        req: &RenderRequest,
        device: &'static bv_cuda::Device,
    ) -> Result<ImageRgbaF32, Error> {
        let name = kOfxImageEffectOutputClipName.to_string_lossy();
        let (format, premult) = self.run().format_of(&name);
        let mut pic = OfxPicture::new(req.width as usize, req.height as usize, format);
        let memory = device
            .alloc(pic.buffer.len())
            .map_err(|e| Error::Cuda(e.to_string()))?;
        let output = DevicePicture::borrowed(
            memory.address(),
            pic.width,
            pic.height,
            pic.row_bytes,
            format,
        );
        self.render_on_card(inputs, values, req, device, output)?;
        // SAFETY: the memory the plug-in drew into, as big as the picture.
        unsafe { device.download(pic.buffer.bytes_mut(), memory.address()) }
            .map_err(|e| Error::Cuda(e.to_string()))?;
        // SAFETY: the picture's own layout, filled by the plug-in.
        Ok(unsafe {
            from_ofx(
                pic.buffer.as_ptr(),
                pic.width,
                pic.height,
                pic.row_bytes as isize,
                format,
                premult,
            )
        })
    }

    fn render_inputs(
        &self,
        inputs: &[(&str, Arc<dyn ClipSource>)],
        values: &[(String, Animation)],
        req: &RenderRequest,
        card: Option<CardRender>,
    ) -> Result<Drawn, Error> {
        let _alone = self
            .plugin
            .renders_alone()
            .then(|| lock(&self.plugin.exclusive));
        let _busy = lock(&self.busy);
        self.set_values(values);
        for (clip, source) in inputs {
            self.connect(clip, Arc::clone(source));
        }
        let result = self.render_locked(req, card);
        let run = self.run();
        let mut sources = run.sources.write().unwrap_or_else(|e| e.into_inner());
        for (clip, _) in inputs {
            sources.remove(*clip);
        }
        result
    }

    fn render_locked(&self, req: &RenderRequest, card: Option<CardRender>) -> Result<Drawn, Error> {
        let run = self.run();
        run.abort.store(false, Ordering::Relaxed);
        run.timeline_time
            .store(req.time.to_bits(), Ordering::Relaxed);
        let params = &self.effect.params;
        params.set_time(req.time);
        params.set_writable(false);
        *lock(&run.render) = Some(RenderState {
            scale: [req.scale, req.scale],
            window: OfxRectI {
                x1: 0,
                y1: 0,
                x2: req.width as i32,
                y2: req.height as i32,
            },
            output: None,
            card,
        });
        let result = self.render_frame(run, req);
        *lock(&run.render) = None;
        run.free_images();
        params.set_writable(true);
        result
    }

    fn render_frame(&self, run: &Run, req: &RenderRequest) -> Result<Drawn, Error> {
        let card = lock(&run.render)
            .as_ref()
            .and_then(|r| r.card.as_ref())
            .map(|c| (c.device, Arc::clone(&c.output)));
        // Plug-ins rendering with CUDA find the device's context current,
        // through the sequence's actions too.
        let _current = match &card {
            Some((device, _)) => Some(device.enter().map_err(|e| Error::Cuda(e.to_string()))?),
            None => None,
        };
        // An effect that changes nothing names the input to show.
        let args = PropSet::new(SetKind::Arguments);
        args.set_dbl(kOfxPropTime, &[req.time]);
        args.set_cstr(kOfxImageEffectPropFieldToRender, kOfxImageFieldNone);
        args.set_int(
            kOfxImageEffectPropRenderWindow,
            &[0, 0, req.width as i32, req.height as i32],
        );
        args.set_dbl(kOfxImageEffectPropRenderScale, &[req.scale, req.scale]);
        let out = PropSet::new(SetKind::Arguments);
        out.define_str(kOfxPropName, Some(1), &[c""]);
        out.define_dbl(kOfxPropTime, Some(1), &[req.time]);
        if self.call(kOfxImageEffectActionIsIdentity, Some(&args), Some(&out)) == kOfxStatOK {
            let name = out.str(kOfxPropName, 0).unwrap_or_default();
            let time = out.dbl(kOfxPropTime, 0).unwrap_or(req.time);
            let source = run
                .sources
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(&name)
                .cloned();
            match (source, &card) {
                (Some(s), None) => {
                    if let Some(picture) = s.picture(time) {
                        return Ok(Drawn::Host((*picture).clone()));
                    }
                }
                (Some(s), Some((device, output))) => {
                    if copy_identity(device, &*s, time, output) {
                        return Ok(Drawn::Card);
                    }
                }
                (None, _) => {}
            }
        }
        let cuda = card.is_some();
        let sequence = *lock(&self.sequence);
        let in_sequence = match sequence {
            // The pictures moved between the processor and the card: the
            // plug-in hears of a sequence of the other kind.
            Some((first, last, was)) if was != cuda => {
                let end = self.sequence_args(first, last, req, was);
                let _ = self.call(kOfxImageEffectActionEndSequenceRender, Some(&end), None);
                let begin = self.sequence_args(first, last, req, cuda);
                let _ = self.call(kOfxImageEffectActionBeginSequenceRender, Some(&begin), None);
                *lock(&self.sequence) = Some((first, last, cuda));
                true
            }
            Some(_) => true,
            None => false,
        };
        if !in_sequence {
            let seq = self.sequence_args(req.time, req.time, req, cuda);
            let _ = self.call(kOfxImageEffectActionBeginSequenceRender, Some(&seq), None);
        }
        let args = self.render_args(req, cuda);
        let mut status = self.call(kOfxImageEffectActionRender, Some(&args), None);
        // What the plug-in queued on the card is done before its pictures
        // are used (or freed).
        if let Some((device, _)) = &card
            && device.synchronize().is_err()
        {
            status = kOfxStatFailed;
        }
        if !in_sequence {
            let seq = self.sequence_args(req.time, req.time, req, cuda);
            let _ = self.call(kOfxImageEffectActionEndSequenceRender, Some(&seq), None);
        }
        if !succeeded(status) {
            return Err(Error::Action {
                action: "Render".into(),
                status,
            });
        }
        if card.is_some() {
            let fetched = lock(&run.render)
                .as_ref()
                .and_then(|r| r.card.as_ref())
                .is_some_and(|c| c.fetched);
            return if fetched {
                Ok(Drawn::Card)
            } else {
                Err(Error::NoOutput)
            };
        }
        let picture = lock(&run.render)
            .as_mut()
            .and_then(|r| r.output.take())
            .ok_or(Error::NoOutput)?;
        let (format, premult) = run.format_of(&kOfxImageEffectOutputClipName.to_string_lossy());
        // SAFETY: the output picture's own layout, filled by the plug-in.
        Ok(Drawn::Host(unsafe {
            from_ofx(
                picture.buffer.as_ptr(),
                picture.width,
                picture.height,
                picture.row_bytes as isize,
                format,
                premult,
            )
        }))
    }

    /// The plug-in's overlay entry point, and whether it draws with OpenGL
    /// itself: the second kind of overlays (the draw suite's) when it has
    /// one, else, on Windows, the first (`gl.rs`).
    fn overlay_entry(&self) -> Option<(bv_ofx_sys::OfxPluginEntryPoint, bool)> {
        let sets = [&self.effect.props, &self.plugin.descriptor().props];
        let find = |name: &std::ffi::CStr| {
            sets.iter()
                .find_map(|s| s.ptr(name, 0))
                .filter(|p| !p.is_null())
        };
        let (p, opengl) = match find(kOfxImageEffectPluginPropOverlayInteractV2) {
            Some(p) => (p, false),
            None if cfg!(windows) => (find(kOfxImageEffectPluginPropOverlayInteractV1)?, true),
            None => return None,
        };
        // SAFETY: the plug-in gave it as its overlay's entry point.
        let entry =
            unsafe { std::mem::transmute::<*mut c_void, bv_ofx_sys::OfxPluginEntryPoint>(p) };
        Some((entry, opengl))
    }

    /// Whether the plug-in has on-screen controls BananaV can show.
    pub fn has_overlay(&self) -> bool {
        self.overlay_entry().is_some()
    }

    /// The overlay, made the first time (under the busy lock).
    fn interact(&self) -> Option<&Interact> {
        self.interact
            .get_or_init(|| {
                let (entry, opengl) = self.overlay_entry()?;
                let i = Interact::new(
                    self.plugin.binary().clone(),
                    entry,
                    self.effect.handle() as *const c_void,
                    opengl,
                );
                self.run().free_images();
                i
            })
            .as_ref()
    }

    /// What the overlay draws for `view`.
    pub fn overlay_draw(&self, view: &View) -> Vec<Mark> {
        let _busy = lock(&self.busy);
        let params = &self.effect.params;
        params.set_time(view.time);
        params.set_writable(false);
        let marks = self.interact().map(|i| i.draw(view)).unwrap_or_default();
        params.set_writable(true);
        self.run().free_images();
        marks
    }

    /// The pen at `at` (canonical coordinates; `screen` in the view's
    /// pixels): whether the overlay took it, and what it changed.
    pub fn overlay_pen(
        &self,
        action: PenAction,
        view: &View,
        at: [f64; 2],
        screen: [i32; 2],
        pressure: f64,
    ) -> PenAnswer {
        let busy = lock(&self.busy);
        let params = &self.effect.params;
        params.set_time(view.time);
        params.set_writable(true);
        let taken = self
            .interact()
            .is_some_and(|i| i.pen(action, view, at, screen, pressure));
        self.pen_answer(busy, taken, view.time)
    }

    /// What a pen action changed (the plug-in told of it, in turn), the
    /// busy lock held (and let go).
    fn pen_answer(&self, busy: MutexGuard<'_, ()>, taken: bool, time: f64) -> PenAnswer {
        self.run().free_images();
        let mut changed = self.effect.params.take_changed();
        if !changed.is_empty() {
            // The plug-in hears of its own changes, as of the user's.
            let more = self.changed_locked(busy, &changed, ChangeReason::Plugin, time);
            for n in more {
                if !changed.contains(&n) {
                    changed.push(n);
                }
            }
        }
        PenAnswer { taken, changed }
    }

    /// The entry point of parameter `name`'s own interface, when it draws
    /// one with the draw suite (BananaV's extension).
    fn param_interact_entry(&self, name: &str) -> Option<bv_ofx_sys::OfxPluginEntryPoint> {
        let p = self
            .effect
            .params
            .get(name)?
            .props
            .ptr(bv_ofx_sys::extensions::kBananaVParamPropInteract, 0)
            .filter(|p| !p.is_null())?;
        // SAFETY: the plug-in gave it as its interface's entry point.
        Some(unsafe { std::mem::transmute::<*mut c_void, bv_ofx_sys::OfxPluginEntryPoint>(p) })
    }

    /// Runs `f` on parameter `name`'s own interface, made the first time
    /// (under the busy lock).
    fn with_param_interact<R>(&self, name: &str, f: impl FnOnce(&Interact) -> R) -> Option<R> {
        let mut made = lock(&self.param_interacts);
        if !made.contains_key(name) {
            let i = self.param_interact_entry(name).and_then(|entry| {
                Interact::new(
                    self.plugin.binary().clone(),
                    entry,
                    self.effect.handle() as *const c_void,
                    false,
                )
            });
            self.run().free_images();
            made.insert(name.to_string(), i);
        }
        made.get(name)?.as_ref().map(f)
    }

    /// What parameter `name`'s own interface draws for `view` (its size
    /// in screen pixels, y up).
    pub fn param_draw(&self, name: &str, view: &View) -> Vec<Mark> {
        let _busy = lock(&self.busy);
        let params = &self.effect.params;
        params.set_time(view.time);
        params.set_writable(false);
        let marks = self
            .with_param_interact(name, |i| i.draw(view))
            .unwrap_or_default();
        params.set_writable(true);
        self.run().free_images();
        marks
    }

    /// The pen at `at` on parameter `name`'s own interface (its pixels, y
    /// up; `screen` the same, whole): whether it took it, and what changed.
    pub fn param_pen(
        &self,
        name: &str,
        action: PenAction,
        view: &View,
        at: [f64; 2],
        screen: [i32; 2],
        pressure: f64,
    ) -> PenAnswer {
        let busy = lock(&self.busy);
        let params = &self.effect.params;
        params.set_time(view.time);
        params.set_writable(true);
        let taken = self
            .with_param_interact(name, |i| i.pen(action, view, at, screen, pressure))
            .unwrap_or(false);
        self.pen_answer(busy, taken, view.time)
    }

    /// The overlay gained (`true`) or lost the keyboard's attention.
    pub fn overlay_focus(&self, gained: bool, view: &View) {
        let _busy = lock(&self.busy);
        self.effect.params.set_time(view.time);
        if let Some(i) = self.interact() {
            i.focus(gained, view);
        }
        self.run().free_images();
    }

    /// Tells the plug-in parameters changed (at `time`, frames); returns
    /// the parameters the plug-in changed in turn.
    pub fn changed(&self, names: &[String], reason: ChangeReason, time: f64) -> Vec<String> {
        let busy = lock(&self.busy);
        self.changed_locked(busy, names, reason, time)
    }

    /// [`changed`](Self::changed), the busy lock held (and let go).
    fn changed_locked(
        &self,
        _busy: MutexGuard<'_, ()>,
        names: &[String],
        reason: ChangeReason,
        time: f64,
    ) -> Vec<String> {
        let run = self.run();
        let params = &self.effect.params;
        params.set_time(time);
        params.set_writable(true);
        run.timeline_time.store(time.to_bits(), Ordering::Relaxed);
        let mut changed_by_plugin = BTreeSet::new();
        let mut pending: Vec<String> = names.to_vec();
        let mut why = reason.ofx_name();
        for _ in 0..4 {
            if pending.is_empty() {
                break;
            }
            let begin = PropSet::new(SetKind::Arguments);
            begin.set_cstr(kOfxPropChangeReason, why);
            let _ = self.call(kOfxActionBeginInstanceChanged, Some(&begin), None);
            for n in &pending {
                let args = PropSet::new(SetKind::Arguments);
                args.set_cstr(kOfxPropType, kOfxTypeParameter);
                args.set_str(kOfxPropName, &[n]);
                args.set_cstr(kOfxPropChangeReason, why);
                args.set_dbl(kOfxPropTime, &[time]);
                args.set_dbl(kOfxImageEffectPropRenderScale, &[1.0, 1.0]);
                let _ = self.call(kOfxActionInstanceChanged, Some(&args), None);
            }
            let _ = self.call(kOfxActionEndInstanceChanged, Some(&begin), None);
            run.free_images();
            pending = params.take_changed();
            changed_by_plugin.extend(pending.iter().cloned());
            why = kOfxChangePluginEdited;
        }
        let slaves = self
            .effect
            .props
            .strs(kOfxImageEffectPropClipPreferencesSlaveParam);
        let touched = names.iter().chain(changed_by_plugin.iter());
        if touched.into_iter().any(|n| slaves.contains(n)) {
            drop(_busy);
            self.clip_preferences();
        }
        changed_by_plugin.into_iter().collect()
    }

    /// Asks the plug-in to put what it keeps to itself into its parameters
    /// (before the project is saved).
    pub fn sync_private_data(&self) -> Vec<String> {
        let _busy = lock(&self.busy);
        let _ = self.call(kOfxActionSyncPrivateData, None, None);
        self.effect.params.take_changed()
    }

    /// The user opened its settings (`true`) or closed them.
    pub fn editing(&self, open: bool) {
        let _busy = lock(&self.busy);
        let action = if open {
            kOfxActionBeginInstanceEdit
        } else {
            kOfxActionEndInstanceEdit
        };
        let _ = self.call(action, None, None);
    }

    /// Asks the plug-in to free what it keeps for speed.
    pub fn purge_caches(&self) {
        let _busy = lock(&self.busy);
        let _ = self.call(kOfxActionPurgeCaches, None, None);
    }

    /// Asks a render under way to stop.
    pub fn abort(&self) {
        self.run().abort.store(true, Ordering::Relaxed);
    }

    /// The message the plug-in keeps showing (an error, a warning).
    pub fn persistent_message(&self) -> Option<Message> {
        lock(&self.run().persistent).clone()
    }

    /// The messages it sent lately.
    pub fn messages(&self) -> Vec<Message> {
        lock(&self.run().messages).clone()
    }

    /// What the plug-in says it is doing, and how far (0–1).
    pub fn progress(&self) -> Option<(String, f64)> {
        lock(&self.run().progress).clone()
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // The overlay and the parameters' interfaces go first: they belong
        // to the instance.
        drop(self.interact.take());
        self.param_interacts
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        if self.created {
            let _ = self.call(kOfxActionDestroyInstance, None, None);
        }
        self.run().free_images();
    }
}
