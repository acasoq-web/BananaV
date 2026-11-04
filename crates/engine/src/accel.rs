//! Hardware acceleration: the graphics card that composites video
//! (Options › Preferences › Video, "GPU acceleration of video processing")
//! and the one that decodes it (File I/O, "Enable Hardware Decoding").
//!
//! The card opens on a thread of its own, so nothing waits for it; until
//! it is ready, and if it ever fails, the processor composites (the same
//! frames: `bv-gpu`'s tests check it against `bv-render`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use bv_core::{ImageRgbaF32, Tick, TransferCharacteristic};
use bv_gpu::{Compositor, Gpu, GpuError, Output, Rendered};
use bv_media::{Need, Want};
use bv_render::VideoPool;
use bv_timeline::scene::Plugins;
use bv_timeline::{Effect, Generator, MediaSource, Project, RenderQuality};
use serde::{Deserialize, Serialize};

use crate::RepaintFn;
use crate::plugins::{OfxEffects, Purpose};

/// Options › Preferences › General: "Use GPU processing for ACES color
/// management". Off, frames under ACES are drawn on the processor.
static ACES_ON_GPU: AtomicBool = AtomicBool::new(true);

/// Sets whether the graphics card draws frames under ACES.
pub fn set_aces_on_gpu(on: bool) {
    ACES_ON_GPU.store(on, Ordering::Relaxed);
}

/// Which graphics card composites video.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuChoice {
    /// The processor does it.
    Off,
    /// The best card of the computer.
    #[default]
    Auto,
    /// The card of this name (as [`gpu_adapters`] lists it).
    Named(String),
}

/// Where video is decoded (Options › Preferences › File I/O).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecodeChoice {
    /// On the processor.
    Off,
    /// On the fastest graphics card (the processor for what it cannot).
    #[default]
    Auto,
    /// On the card of this name (as `Engine::decode_adapters` lists it).
    Card(String),
}

impl DecodeChoice {
    fn to_mf(&self) -> bv_mf::HardwareDecoding {
        match self {
            DecodeChoice::Off => bv_mf::HardwareDecoding::Off,
            DecodeChoice::Auto => bv_mf::HardwareDecoding::Auto,
            DecodeChoice::Card(n) => bv_mf::HardwareDecoding::Card(n.clone()),
        }
    }
}

/// What the card compositing video is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuStatus {
    /// Turned off: the processor composites.
    Off,
    /// Opening (the processor composites meanwhile).
    Starting,
    /// Compositing, on the card of this name.
    Ready(String),
    /// It failed (the message): the processor composites.
    Failed(String),
}

/// The graphics cards that can composite video, the best first.
pub fn gpu_adapters() -> Vec<bv_gpu::Adapter> {
    bv_gpu::adapters()
}

struct State {
    choice: GpuChoice,
    gpu: Option<Arc<Gpu>>,
    status: GpuStatus,
    /// Counts choices: a card that opens after another choice is dropped.
    serial: u64,
}

/// The card compositing video, shared by the preview and the renders.
pub(crate) struct Accel {
    state: Mutex<State>,
    repaint: RepaintFn,
    /// Counts changes of how video is decoded: readers opened before are
    /// opened again.
    decoding: AtomicU64,
}

impl Accel {
    pub(crate) fn new(repaint: RepaintFn) -> Arc<Accel> {
        Arc::new(Accel {
            state: Mutex::new(State {
                choice: GpuChoice::Off,
                gpu: None,
                status: GpuStatus::Off,
                serial: 0,
            }),
            repaint,
            decoding: AtomicU64::new(0),
        })
    }

    /// Decodes video on the graphics card (`hw`) or on the processor from
    /// now on; the preview's readers open again.
    pub(crate) fn set_decoding(&self, choice: &DecodeChoice) {
        let hw = choice.to_mf();
        if bv_mf::hardware_decoding() != hw {
            bv_mf::set_hardware_decoding(hw);
            self.decoding.fetch_add(1, Ordering::Relaxed);
            (self.repaint)();
        }
    }

    /// Changes so far of how video is decoded.
    pub(crate) fn decoding_serial(&self) -> u64 {
        self.decoding.load(Ordering::Relaxed)
    }

    /// Uses `choice` from now on (opening the card in the background). A
    /// card that failed is tried again when chosen again.
    pub(crate) fn set(self: &Arc<Accel>, choice: GpuChoice) {
        let serial = {
            let mut st = self.state.lock().expect("acceleration");
            let failed = matches!(st.status, GpuStatus::Failed(_));
            if st.choice == choice && !failed {
                return;
            }
            st.choice = choice.clone();
            st.gpu = None;
            st.serial += 1;
            if choice == GpuChoice::Off {
                st.status = GpuStatus::Off;
                return;
            }
            st.status = GpuStatus::Starting;
            st.serial
        };
        let me = self.clone();
        let spawned = std::thread::Builder::new()
            .name("bv-gpu-open".into())
            .spawn(move || {
                let name = match &choice {
                    GpuChoice::Named(n) => Some(n.as_str()),
                    _ => None,
                };
                let opened = Gpu::open(name);
                let gpu = {
                    let mut st = me.state.lock().expect("acceleration");
                    if st.serial != serial {
                        return;
                    }
                    match opened {
                        Ok(g) => {
                            st.status = GpuStatus::Ready(g.adapter().name.clone());
                            st.gpu = Some(g.clone());
                            Some(g)
                        }
                        Err(e) => {
                            st.status = GpuStatus::Failed(e.to_string());
                            None
                        }
                    }
                };
                (me.repaint)();
                // Frames draw on the card from now on, waiting only for
                // the kernels they use; the others are compiled meanwhile.
                if let Some(g) = gpu
                    && let Err(e) = g.warm_up()
                {
                    me.failed(&g, &e);
                }
            });
        if spawned.is_err() {
            self.state.lock().expect("acceleration").status =
                GpuStatus::Failed("could not start".into());
        }
    }

    pub(crate) fn status(&self) -> GpuStatus {
        self.state.lock().expect("acceleration").status.clone()
    }

    /// The card, when it is ready.
    fn gpu(&self) -> Option<Arc<Gpu>> {
        self.state.lock().expect("acceleration").gpu.clone()
    }

    /// `gpu` failed a frame: the processor takes over until the card is
    /// chosen again.
    fn failed(&self, gpu: &Arc<Gpu>, e: &GpuError) {
        let mut st = self.state.lock().expect("acceleration");
        if st.gpu.as_ref().is_some_and(|g| Arc::ptr_eq(g, gpu)) {
            st.gpu = None;
            st.status = GpuStatus::Failed(e.to_string());
            drop(st);
            (self.repaint)();
        }
    }
}

/// Draws frames on the card when there is one, on the processor otherwise.
pub(crate) struct Painter {
    accel: Arc<Accel>,
    compositor: Option<Compositor>,
    /// The plug-in effects' instances.
    plugins: Arc<OfxEffects>,
    /// The thread readying the plug-ins of frames coming soon (see
    /// [`Painter::ready`]), and the frames given it for the project (by
    /// its address) and size it was given last.
    readier: Option<Sender<Readying>>,
    readied: (usize, (u32, u32), Vec<Tick>),
}

/// Frames of a project whose plug-ins to ready, and their size.
type Readying = (Arc<Project>, Vec<Tick>, (u32, u32));

impl Painter {
    /// A painter of frames for `purpose`.
    pub(crate) fn new(accel: Arc<Accel>, purpose: Purpose) -> Painter {
        Painter::sharing(accel, OfxEffects::new(purpose))
    }

    /// A painter drawing with `plugins`' instances.
    fn sharing(accel: Arc<Accel>, plugins: Arc<OfxEffects>) -> Painter {
        Painter {
            accel,
            compositor: None,
            plugins,
            readier: None,
            readied: (0, (0, 0), Vec::new()),
        }
    }

    /// Who draws the plug-in effects.
    fn plugins(&self) -> Option<Plugins<'_>> {
        Some(Plugins(&*self.plugins))
    }

    /// Whether it draws on the graphics card (else on the processor, whose
    /// threads the frame keeps busy).
    pub(crate) fn on_the_card(&self) -> bool {
        self.accel.gpu().is_some()
    }

    /// Another painter for `purpose` on the same card, with plug-in
    /// instances of its own (a render's second drawing thread).
    pub(crate) fn another(&self, purpose: Purpose) -> Painter {
        Painter::new(self.accel.clone(), purpose)
    }

    /// Lets go of the plug-in instances no frame used lately.
    pub(crate) fn sweep_plugins(&self) {
        self.plugins.sweep(std::time::Duration::from_secs(30));
    }

    /// The card's compositor, if the card is ready.
    fn compositor(&mut self) -> Option<&mut Compositor> {
        let Some(gpu) = self.accel.gpu() else {
            self.compositor = None;
            return None;
        };
        if self
            .compositor
            .as_ref()
            .is_none_or(|c| !Arc::ptr_eq(c.gpu(), &gpu))
        {
            self.compositor = Some(Compositor::new(gpu));
        }
        self.compositor.as_mut()
    }

    /// Draws on the card (pictures scaled at `quality`, `None`: the
    /// project's); `None` (the processor must) when there is no card or it
    /// failed.
    #[allow(clippy::too_many_arguments)]
    fn on_card(
        &mut self,
        project: &Project,
        t: Tick,
        held: Option<Tick>,
        (width, height): (u32, u32),
        pool: &mut VideoPool,
        background: [f32; 4],
        output: Output,
        quality: Option<RenderQuality>,
    ) -> Option<Rendered> {
        let plugins = Arc::clone(&self.plugins);
        let c = self.compositor()?;
        let rotation = project.settings.rotation;
        let (w, h) = if rotation.is_quarter_turn() {
            (height, width)
        } else {
            (width, height)
        };
        let mut scene = match held {
            Some(held) => bv_timeline::scene::frame_scene_held(project, t, held, w, h),
            None => bv_timeline::scene::frame_scene(project, t, w, h),
        };
        // 3D compositing and Optical Flow's frames are drawn on the
        // processor.
        if scene
            .tracks
            .iter()
            .any(|t| t.space.is_some() || t.parent_space.is_some())
            || scene
                .media_layers()
                .any(|(_, m)| m.flow && m.blend.is_some())
        {
            return None;
        }
        // ACES on the processor when the preferences ask (or for a step
        // the card has no kernel for: in the view or in a media's color
        // space).
        if let Some(p) = &scene.color {
            let media_drawn = scene.media_layers().all(|(_, m)| {
                m.info
                    .color_space
                    .and_then(|s| p.media_input(s))
                    .is_none_or(|q| bv_gpu::draws(&q))
            });
            if !ACES_ON_GPU.load(Ordering::Relaxed) || !bv_gpu::draws(&p.view) || !media_drawn {
                return None;
            }
        }
        if let Some(q) = quality {
            scene.set_quality(q);
        }
        scene.plugins = Some(Plugins(&*plugins));
        match c.render(&scene, rotation, pool, background, output) {
            Ok(r) => Some(r),
            Err(e) => {
                let gpu = c.gpu().clone();
                self.compositor = None;
                self.accel.failed(&gpu, &e);
                None
            }
        }
    }

    /// Opens and positions readers for the events that start in `(from,
    /// to]` (while playing) — also those open already but left elsewhere
    /// (played before, then played again) —, and readies their plug-ins
    /// ([`Painter::ready`]): when playback gets there, their first frames
    /// are decoded, their plug-ins made, and nothing stalls at the cut or
    /// the transition.
    pub(crate) fn warm(
        &mut self,
        project: &Arc<Project>,
        (from, to): (Tick, Tick),
        (width, height): (u32, u32),
        pool: &mut VideoPool,
    ) {
        let starts = starts_in(project, (from, to));
        if starts.is_empty() {
            return;
        }
        self.ready(project, (from, to), (width, height));
        let want = if self.compositor().is_some() {
            Want::Decoded
        } else {
            Want::Linear
        };
        let (w, h) = if project.settings.rotation.is_quarter_turn() {
            (height, width)
        } else {
            (width, height)
        };
        for s in starts {
            let scene = bv_timeline::scene::frame_scene(project, s, w, h);
            // The events starting there (those playing on there are being
            // read: asking their readers for a later picture would send
            // them there and back).
            let needs: Vec<Need<'_>> = scene
                .media_layers()
                .filter(|(clip, m)| m.clip.start == s && !pool.ready_at(clip.0, m.info.id, m.time))
                .flat_map(|(clip, m)| bv_media::needs_of(clip.0, m, want))
                .collect();
            if !needs.is_empty() {
                pool.prefetch(&needs);
            }
        }
    }

    /// Readies the plug-ins of `project`'s frames where events start in
    /// `(from, to]` — the events' effects and transitions, as playback will
    /// draw them, `size` big —, on a thread of its own: their instances
    /// made (a plug-in may take a second to make one: Sapphire's do) and
    /// each drawn once at that size (a plug-in's first frame of a size takes
    /// longer than the next ones: Sapphire's set themselves up for it), the
    /// media standing in as black (nothing decoded). When playback gets
    /// there, nothing waits for them.
    pub(crate) fn ready(
        &mut self,
        project: &Arc<Project>,
        (from, to): (Tick, Tick),
        size: (u32, u32),
    ) {
        if !has_plugins(project) {
            return;
        }
        let address = Arc::as_ptr(project) as usize;
        if (self.readied.0, self.readied.1) != (address, size) {
            self.readied = (address, size, Vec::new());
        }
        let starts: Vec<Tick> = starts_in(project, (from, to))
            .into_iter()
            .filter(|s| !self.readied.2.contains(s))
            .collect();
        if starts.is_empty() {
            return;
        }
        self.readied.2.extend(&starts);
        let (accel, plugins) = (&self.accel, &self.plugins);
        let jobs = self
            .readier
            .get_or_insert_with(|| readier(accel.clone(), plugins.clone()));
        if jobs.send((project.clone(), starts, size)).is_err() {
            self.readier = None;
        }
    }

    /// The finished frame in linear light, as `bv_render::render_output_on`
    /// (the video bus's motion blur averaged in).
    pub(crate) fn linear(
        &mut self,
        project: &Project,
        t: Tick,
        size: (u32, u32),
        pool: &mut VideoPool,
        background: [f32; 4],
    ) -> ImageRgbaF32 {
        self.linear_as(project, t, size, pool, background, None)
    }

    /// [`Painter::linear`] with the pictures scaled at `quality` (`None`:
    /// the project's).
    pub(crate) fn linear_as(
        &mut self,
        project: &Project,
        t: Tick,
        size: (u32, u32),
        pool: &mut VideoPool,
        background: [f32; 4],
        quality: Option<RenderQuality>,
    ) -> ImageRgbaF32 {
        if let Some(samples) = project.motion_blur_at(t) {
            return self.blurred(project, t, &samples, size, pool, background, quality);
        }
        self.linear_one(project, t, None, size, pool, background, quality)
    }

    /// One finished frame in linear light, on the card or the processor.
    /// (`held`: a sub-frame's frame time, which the tracks bypassing
    /// motion blur show.)
    #[allow(clippy::too_many_arguments)]
    fn linear_one(
        &mut self,
        project: &Project,
        t: Tick,
        held: Option<Tick>,
        size: (u32, u32),
        pool: &mut VideoPool,
        background: [f32; 4],
        quality: Option<RenderQuality>,
    ) -> ImageRgbaF32 {
        if let Some(Rendered::Linear(img)) = self.on_card(
            project,
            t,
            held,
            size,
            pool,
            background,
            Output::Linear,
            quality,
        ) {
            return img;
        }
        bv_render::render_output_held(
            project,
            t,
            held,
            size,
            pool,
            background,
            quality,
            self.plugins(),
        )
    }

    /// The video bus's motion blur (and supersampling): the frames at
    /// `samples`' times in linear light, weighed and summed (the tracks
    /// bypassing it at the frame's time, `t`).
    #[allow(clippy::too_many_arguments)]
    fn blurred(
        &mut self,
        project: &Project,
        t: Tick,
        samples: &[(Tick, f32)],
        size: (u32, u32),
        pool: &mut VideoPool,
        background: [f32; 4],
        quality: Option<RenderQuality>,
    ) -> ImageRgbaF32 {
        let mut out: Option<ImageRgbaF32> = None;
        for &(at, w) in samples {
            let img = self.linear_one(project, at, Some(t), size, pool, background, quality);
            match &mut out {
                None => {
                    let mut first = img;
                    for px in &mut first.data {
                        *px = px.map(|v| v * w);
                    }
                    out = Some(first);
                }
                Some(sum) if sum.data.len() == img.data.len() => {
                    for (s, px) in sum.data.iter_mut().zip(&img.data) {
                        for c in 0..4 {
                            s[c] += px[c] * w;
                        }
                    }
                }
                Some(_) => {}
            }
        }
        out.unwrap_or_else(|| ImageRgbaF32::filled(size.0, size.1, background))
    }

    /// The finished frame packed for a video encoder (NV12, or P010 with
    /// `ten_bit`) as `bv_color::linear_to_420` makes it: converted on the
    /// card, which brings back a tenth of the bytes of a linear frame, or
    /// on the processor.
    pub(crate) fn yuv420(
        &mut self,
        project: &Project,
        t: Tick,
        size: (u32, u32),
        pool: &mut VideoPool,
        ten_bit: bool,
    ) -> Vec<u8> {
        let black = [0.0, 0.0, 0.0, 1.0];
        let video_levels = project.settings.pixel_format.is_video_levels();
        if let Some(samples) = project.motion_blur_at(t) {
            let img = self.blurred(project, t, &samples, size, pool, black, None);
            return bv_color::linear_to_420_levels(&img, ten_bit, video_levels);
        }
        let output = Output::Yuv420 {
            ten_bit,
            video_levels,
        };
        if let Some(Rendered::Yuv420 { data, .. }) =
            self.on_card(project, t, None, size, pool, black, output, None)
        {
            return data;
        }
        let img =
            bv_render::render_output_with(project, t, size, pool, black, None, self.plugins());
        bv_color::linear_to_420_levels(&img, ten_bit, video_levels)
    }

    /// The finished frame for the screen (RGBA, 8 bits, BT.709) with the
    /// pictures scaled at `quality` (`None`: the project's), over
    /// `background`.
    pub(crate) fn display_as(
        &mut self,
        project: &Project,
        t: Tick,
        size: (u32, u32),
        pool: &mut VideoPool,
        quality: Option<RenderQuality>,
        background: [f32; 4],
    ) -> Vec<u8> {
        let tc = TransferCharacteristic::Bt709;
        if let Some(samples) = project.motion_blur_at(t) {
            let img = self.blurred(project, t, &samples, size, pool, background, quality);
            return bv_color::linear_to_display_rgba8(&img, tc);
        }
        let output = Output::Display(tc);
        if let Some(Rendered::Display { rgba, .. }) =
            self.on_card(project, t, None, size, pool, background, output, quality)
        {
            return rgba;
        }
        let img = bv_render::render_output_with(
            project,
            t,
            size,
            pool,
            background,
            quality,
            self.plugins(),
        );
        bv_color::linear_to_display_rgba8(&img, tc)
    }
}

/// Where `project`'s visible video events start in `(from, to]`, in order.
fn starts_in(project: &Project, (from, to): (Tick, Tick)) -> Vec<Tick> {
    let mut starts: Vec<Tick> = project
        .visible_video_tracks()
        .flat_map(|t| t.clips.iter().map(|c| c.start))
        .filter(|&s| s > from && s <= to)
        .collect();
    starts.sort();
    starts.dedup();
    starts
}

/// Whether any plug-in draws `project`'s video: an event's, a media's, a
/// track's or the output's effect, a transition.
pub(crate) fn has_plugins(project: &Project) -> bool {
    let plugin = |e: &Effect| e.enabled && e.plugin.is_some();
    project.tracks.iter().any(|t| {
        t.effects.iter().any(plugin)
            || t.clips
                .iter()
                .any(|c| c.transition_fx.is_some() || c.effects.iter().any(plugin))
    }) || project.media.iter().any(|m| m.effects.iter().any(plugin))
        || project.video_output_fx.iter().any(plugin)
}

/// `project` with its media standing in as black (their sizes, rates and
/// lengths kept): what readying its plug-ins draws, decoding nothing.
fn standing_in(project: &Project) -> Project {
    let mut p = project.clone();
    for m in p.media.iter_mut().filter(|m| m.video.is_some()) {
        m.source = MediaSource::Generated(Generator::SolidColor {
            rgba: [0.0, 0.0, 0.0, 1.0],
        });
        m.sequence = None;
        m.layer = None;
    }
    p.prerenders.clear();
    p
}

thread_local! {
    /// This thread readies plug-ins (its frames are no one's to watch).
    static READYING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread readies plug-ins ([`Painter::ready`]).
pub(crate) fn readying() -> bool {
    READYING.get()
}

/// The thread readying plug-ins for a painter: a painter of its own
/// drawing with `plugins`' instances the frames it is given, over the media
/// standing in.
fn readier(accel: Arc<Accel>, plugins: Arc<OfxEffects>) -> Sender<Readying> {
    let (jobs, rx) = channel::<Readying>();
    let started = std::thread::Builder::new()
        .name("bv-plugins-ready".into())
        // Plug-in effects run on it.
        .stack_size(bv_ofx::PLUGIN_STACK)
        .spawn(move || {
            READYING.set(true);
            let mut painter = Painter::sharing(accel, plugins);
            let mut pool = VideoPool::new();
            let mut stand_in: Option<(usize, Arc<Project>)> = None;
            while let Ok((project, times, size)) = rx.recv() {
                let address = Arc::as_ptr(&project) as usize;
                let p = match &stand_in {
                    Some((a, p)) if *a == address => p.clone(),
                    _ => {
                        let p = Arc::new(standing_in(&project));
                        stand_in = Some((address, p.clone()));
                        p
                    }
                };
                for t in times {
                    let black = [0.0, 0.0, 0.0, 1.0];
                    painter.display_as(&p, t, size, &mut pool, None, black);
                }
            }
        });
    if started.is_err() {
        eprintln!("no thread to ready plug-ins on");
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Generator, MediaInfo, ProjectSettings};

    /// The video bus's motion blur averages the frames around: a cut from
    /// black to white blurred over three frames each way, evenly (Box) or
    /// only from before (Box, asymmetric).
    #[test]
    fn motion_blur_averages_the_frames_around() {
        use bv_timeline::{EnvelopeKind, EnvelopeOwner, KeyCurve, MotionBlurType};
        let mut p = Project::new(ProjectSettings {
            pixel_format: bv_timeline::PixelFormat::Float32Full,
            ..ProjectSettings::default()
        });
        let v = p.tracks[0].id;
        let solid = |p: &mut Project, x: f32| {
            p.add_media(MediaInfo::generated(
                Generator::SolidColor {
                    rgba: [x, x, x, 1.0],
                },
                &p.settings,
            ))
        };
        let (black, white) = (solid(&mut p, 0.0), solid(&mut p, 1.0));
        let s = Tick::from_seconds;
        let a = p.insert_media(black, Tick::ZERO, Some(v), None).unwrap()[0];
        p.trim_end(a, s(1)).unwrap();
        p.insert_media(white, s(1), Some(v), None).unwrap();
        // The cut lands on a frame: its start.
        let at = p
            .settings
            .frame_rate
            .frame_start(p.settings.frame_rate.frame_at(s(1)) + 1);
        let mut painter = Painter::new(Accel::new(Arc::new(|| {})), Purpose::File);
        let mut pool = VideoPool::new();
        let frame = |painter: &mut Painter, p: &Project, pool: &mut VideoPool| {
            painter
                .linear(p, at, (32, 18), pool, [0.0, 0.0, 0.0, 1.0])
                .data[0][0]
        };
        assert!(frame(&mut painter, &p, &mut pool) > 0.99);
        p.toggle_video_bus_envelope(EnvelopeKind::MotionBlur, KeyCurve::Linear)
            .unwrap();
        p.set_envelope_point(
            EnvelopeOwner::VideoBus,
            EnvelopeKind::MotionBlur,
            Tick::ZERO,
            0.21,
        )
        .unwrap();
        p.settings.motion_blur = MotionBlurType::Box;
        let even = frame(&mut painter, &p, &mut pool);
        assert!((even - 4.0 / 7.0).abs() < 0.01, "{even}");
        p.settings.motion_blur = MotionBlurType::BoxAsymmetric;
        let trailing = frame(&mut painter, &p, &mut pool);
        assert!((trailing - 0.25).abs() < 0.01, "{trailing}");
    }

    #[test]
    fn events_about_to_start_get_their_readers() {
        let mut p = Project::new(ProjectSettings::default());
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        let s = Tick::from_seconds;
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let b = p.insert_media(m, s(3), Some(v), None).unwrap()[0];
        let p = Arc::new(p);
        let mut painter = Painter::new(Accel::new(Arc::new(|| {})), Purpose::Preview);
        let mut pool = VideoPool::new();
        painter.warm(&p, (s(1), s(2)), (64, 36), &mut pool);
        assert!(!pool.has_reader(b.0, m), "not within the second yet");
        painter.warm(&p, (s(2), s(3)), (64, 36), &mut pool);
        assert!(pool.has_reader(b.0, m), "opened before its start");
        // Played on, then played again from before its start: its reader,
        // left a second in, is asked for its start again.
        let black = [0.0, 0.0, 0.0, 1.0];
        painter.display_as(&p, s(4), (64, 36), &mut pool, None, black);
        assert!(!pool.ready_at(b.0, m, Tick::ZERO), "left further on");
        painter.warm(&p, (s(2), s(3)), (64, 36), &mut pool);
        assert!(pool.ready_at(b.0, m, Tick::ZERO), "back at its start");
        // An event playing on is left where it is: its reader is not sent
        // to where the next one starts.
        painter.display_as(&p, s(1), (64, 36), &mut pool, None, black);
        assert!(pool.ready_at(a.0, m, s(1)));
        painter.warm(&p, (s(1), s(3)), (64, 36), &mut pool);
        assert!(pool.ready_at(a.0, m, s(1)), "left alone");
    }

    /// The plug-ins of an event about to start are made before it does, on
    /// a thread of their own (drawing its first frame tiny, its media
    /// standing in).
    #[test]
    fn plugins_of_events_about_to_start_are_readied() {
        crate::plugins::tests::test_plugins();
        let mut p = Project::new(ProjectSettings::default());
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        let s = Tick::from_seconds;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let b = p.insert_media(m, s(3), Some(v), None).unwrap()[0];
        p.clip_mut(b)
            .unwrap()
            .effects
            .push(Effect::plugin(bv_timeline::PluginFx {
                id: "org.bananav.test.Invert".into(),
                version: [1, 2],
                name: "Invert (test)".into(),
                params: Default::default(),
            }));
        let p = Arc::new(p);
        let mut painter = Painter::new(Accel::new(Arc::new(|| {})), Purpose::Preview);
        assert_eq!(painter.plugins.instances(), 0);
        painter.ready(&p, (s(1), s(2)), (64, 36));
        painter.ready(&p, (s(2), s(3)), (64, 36));
        let start = std::time::Instant::now();
        while painter.plugins.instances() == 0 {
            assert!(start.elapsed().as_secs() < 10, "readied");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Without plug-ins, nothing to ready: no thread.
        let plain = Arc::new(Project::new(ProjectSettings::default()));
        let mut other = Painter::new(Accel::new(Arc::new(|| {})), Purpose::Preview);
        other.ready(&plain, (s(0), s(10)), (64, 36));
        assert!(other.readier.is_none());
    }
}
