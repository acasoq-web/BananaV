//! Draws a frame scene on the graphics card, step by step as `bv-render`
//! does on the processor.

use std::collections::HashMap;
use std::sync::Arc;

use bv_core::{ImageRgbaF32, PixelFormat, PlaneData, TransferCharacteristic, VideoFrame};
use bv_media::{Ask, Picture, VideoPool, Want};
use bv_timeline::fx::grade;
use bv_timeline::scene::{
    CardPicture, EventLayer, FrameScene, Framing, MediaLayer, PluginCall, Plugins, TrackContent,
    fit_rect_par,
};
use bv_timeline::{
    Direction, Effect, EffectKind, OutputRotation, RenderQuality, Transform, Transition,
};

use crate::kernels::{Grid, K, Params};
use crate::shared::{self, Shared};

mod color;
mod own;
use crate::{Gpu, GpuError};
pub use color::draws;

/// The color shown where media cannot be read (`bv_render::OFFLINE`).
const OFFLINE: [f32; 4] = [0.25, 0.0, 0.02, 1.0];

/// `c` (linear Rec. 709) in the scene's compositing space.
fn entered(scene: &FrameScene<'_>, c: [f32; 4]) -> [f32; 4] {
    match &scene.color {
        Some(p) => {
            let e = p.enter([c[0], c[1], c[2]]);
            [e[0], e[1], e[2], c[3]]
        }
        None => c,
    }
}

/// Bytes of free buffers kept for the next frames (the card's memory is
/// shared with the screen and the decoders).
const KEEP_FREE: u64 = 384 << 20;

/// How a finished frame comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// RGBA, 8 bits, encoded with a transfer curve: for the screen.
    Display(TransferCharacteristic),
    /// Linear light, 32-bit float: for files.
    Linear,
    /// Y′CbCr 4:2:0 in BT.709 colors and limited range, packed for the
    /// video encoders as `bv_color::linear_to_420_levels` makes it: NV12,
    /// or P010 with `ten_bit`; with `video_levels` the frame is studio RGB,
    /// its levels kept. The frame must be opaque, its sides even.
    Yuv420 { ten_bit: bool, video_levels: bool },
}

/// A frame drawn by the card.
#[derive(Debug)]
pub enum Rendered {
    Display {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Linear(ImageRgbaF32),
    Yuv420 {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
}

/// An image on the card: RGBA in linear light, premultiplied, row after
/// row (an `ImageRgbaF32`).
#[derive(Clone, Debug)]
struct Img {
    buf: wgpu::Buffer,
    w: u32,
    h: u32,
}

/// Draws frames on a graphics card. Keeps its buffers and the pictures it
/// uploaded from one frame to the next.
pub struct Compositor {
    gpu: Arc<Gpu>,
    free: Vec<wgpu::Buffer>,
    /// Buffers of the frame being drawn (free again when it is done).
    used: Vec<wgpu::Buffer>,
    /// Pictures uploaded already, by event: stills and generated media
    /// give the same picture frame after frame.
    uploaded: HashMap<u64, (Arc<ImageRgbaF32>, Img)>,
    /// The events whose pictures this frame used.
    seen: Vec<u64>,
    /// Transfer tables on the card: (encode, curve).
    luts: HashMap<(bool, TransferCharacteristic), wgpu::Buffer>,
    readback: Option<wgpu::Buffer>,
    /// Buffers CUDA shares, all of one size: a frame's plug-ins that
    /// render with CUDA use them one after another.
    shared: Vec<Shared>,
    /// Buffers pictures for plug-ins come out of the card through, kept
    /// for the next frames (a 4K picture's are 133 MB).
    plugin_readbacks: Vec<wgpu::Buffer>,
}

impl std::fmt::Debug for Compositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compositor")
            .field("gpu", &self.gpu)
            .finish()
    }
}

fn direction_index(d: Direction) -> u32 {
    match d {
        Direction::Left => 0,
        Direction::Right => 1,
        Direction::Up => 2,
        Direction::Down => 3,
    }
}

/// Box radii whose passes add up to a Gaussian of `sigma` (as
/// `bv_render::effects::blur`).
fn box_sizes(sigma: f32, n: usize) -> Vec<usize> {
    let ideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma
        - (n as i32 * wl * wl) as f32
        - (4 * n as i32 * wl) as f32
        - 3.0 * n as f32)
        / (-4.0 * wl as f32 - 4.0))
        .round() as i32;
    (0..n as i32)
        .map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize)
        .collect()
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

impl Compositor {
    pub fn new(gpu: Arc<Gpu>) -> Compositor {
        Compositor {
            gpu,
            free: Vec::new(),
            used: Vec::new(),
            uploaded: HashMap::new(),
            seen: Vec::new(),
            luts: HashMap::new(),
            readback: None,
            shared: Vec::new(),
            plugin_readbacks: Vec::new(),
        }
    }

    pub fn gpu(&self) -> &Arc<Gpu> {
        &self.gpu
    }

    /// Draws the frame `scene` describes over `background`, turned by
    /// `rotation`, and brings it back as `output`.
    pub fn render(
        &mut self,
        scene: &FrameScene<'_>,
        rotation: OutputRotation,
        pool: &mut VideoPool,
        background: [f32; 4],
        output: Output,
    ) -> Result<Rendered, GpuError> {
        let pixels = scene.width as u64 * scene.height as u64;
        if pixels == 0 || pixels * 16 > self.gpu.max_buffer() {
            return Err(GpuError::Failed(
                "the frame is too large for the card".into(),
            ));
        }
        self.run(output, |c, enc| {
            let out = c.draw(enc, scene, pool, background);
            c.rotated(enc, out, rotation)
        })
    }

    /// A decoded picture in linear light, reduced for `min_size` as the
    /// processor's readers reduce it.
    pub fn linear(
        &mut self,
        frame: &VideoFrame,
        min_size: (u32, u32),
    ) -> Result<ImageRgbaF32, GpuError> {
        self.linear_levels(frame, min_size, false)
    }

    /// As [`Compositor::linear`]; with `video_levels`, Y′CbCr as studio
    /// RGB (`bv_color::frame_to_linear_levels`).
    pub fn linear_levels(
        &mut self,
        frame: &VideoFrame,
        min_size: (u32, u32),
        video_levels: bool,
    ) -> Result<ImageRgbaF32, GpuError> {
        match self.run(Output::Linear, |c, enc| {
            c.decoded(enc, frame, min_size, None, video_levels)
        })? {
            Rendered::Linear(img) => Ok(img),
            Rendered::Display { .. } | Rendered::Yuv420 { .. } => {
                unreachable!("asked in linear light")
            }
        }
    }

    /// Records the work `draw` describes, runs it and brings its image
    /// back as `output`.
    fn run(
        &mut self,
        output: Output,
        draw: impl FnOnce(&mut Compositor, &mut wgpu::CommandEncoder) -> Img,
    ) -> Result<Rendered, GpuError> {
        let gpu = self.gpu.clone();
        if gpu.is_lost() {
            return Err(GpuError::Lost);
        }
        let memory = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let out = draw(self, &mut enc);
        let (result_buf, bytes) = match output {
            Output::Display(tc) => {
                let words = self.buffer(out.w as u64 * out.h as u64 * 4);
                let lut = self.lut(true, tc);
                let top = bv_color::transfer::EncodeLut::shared(tc).max();
                self.kernel(
                    &mut enc,
                    K::EncodeRgba8,
                    Params::sized(out.w, out.h).with_f(&[top]),
                    &[&out.buf, &lut, &words],
                    Grid::Pixels(out.w, out.h),
                );
                (words, out.w as u64 * out.h as u64 * 4)
            }
            Output::Linear => (out.buf.clone(), out.w as u64 * out.h as u64 * 16),
            Output::Yuv420 {
                ten_bit,
                video_levels,
            } => {
                let (w, h) = (out.w, out.h);
                // Rows of whole words: four NV12 samples, or two P010.
                let stride = if ten_bit {
                    w.div_ceil(2)
                } else {
                    w.div_ceil(4)
                };
                let bytes = stride as u64 * (h + h / 2) as u64 * 4;
                let words = self.buffer(bytes);
                let tc = TransferCharacteristic::Bt709;
                let lut = self.lut(true, tc);
                let top = bv_color::transfer::EncodeLut::shared(tc).max();
                let depth = if ten_bit { 10 } else { 8 };
                let range = if video_levels {
                    bv_core::ColorRange::Full
                } else {
                    bv_core::ColorRange::Limited
                };
                let [y_off, y_range, c_off, c_range] =
                    bv_color::matrix::Normalizer::new(range, depth).inverse_params();
                let (kr, kb) = bv_color::matrix::kr_kb(bv_core::MatrixCoefficients::Bt709)
                    .unwrap_or((0.2126, 0.0722));
                let max = ((1u32 << depth) - 1) as f32;
                let mut params =
                    Params::sized(w, h).with_f(&[top, kr, kb, y_off, y_range, c_off, c_range, max]);
                params.j[0] = stride;
                let (k, grid) = if ten_bit {
                    (K::EncodeP010, Grid::Pixels(w.div_ceil(2), h.div_ceil(2)))
                } else {
                    (K::EncodeNv12, Grid::Pixels(w.div_ceil(4), h.div_ceil(2)))
                };
                self.kernel(&mut enc, k, params, &[&out.buf, &lut, &words], grid);
                (words, bytes)
            }
        };
        let readback = match self.readback.take() {
            Some(b) if b.size() == bytes => b,
            _ => gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };
        enc.copy_buffer_to_buffer(&result_buf, 0, &readback, 0, bytes);
        let submitted = gpu.queue.submit([enc.finish()]);
        let failed = pollster::block_on(validation.pop())
            .or(pollster::block_on(memory.pop()))
            .map(|e| e.to_string())
            .or_else(|| gpu.take_stray());
        let result = match failed {
            Some(e) => Err(GpuError::Failed(e)),
            None => self.read(&readback, submitted, bytes, (out.w, out.h), output),
        };
        self.readback = Some(readback);
        self.end_frame();
        if gpu.is_lost() {
            return Err(GpuError::Lost);
        }
        result
    }

    /// Waits for the frame (the work `submitted` copies it into
    /// `readback`) and copies it out of the card. Only that work is waited
    /// for: another compositor's on the same card goes on meanwhile.
    fn read(
        &self,
        readback: &wgpu::Buffer,
        submitted: wgpu::SubmissionIndex,
        bytes: u64,
        (w, h): (u32, u32),
        output: Output,
    ) -> Result<Rendered, GpuError> {
        let (tx, rx) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.gpu
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: None,
            })
            .map_err(|e| GpuError::Failed(e.to_string()))?;
        rx.recv()
            .map_err(|_| GpuError::Lost)?
            .map_err(|e| GpuError::Failed(e.to_string()))?;
        let rendered = {
            let view = readback
                .get_mapped_range(..bytes)
                .map_err(|e| GpuError::Failed(e.to_string()))?;
            match output {
                Output::Display(_) => {
                    let mut rgba = vec![0u8; view.len()];
                    copy_in_bands(&mut rgba, &view);
                    Rendered::Display {
                        width: w,
                        height: h,
                        rgba,
                    }
                }
                Output::Linear => {
                    let mut data = vec![[0.0f32; 4]; view.len() / 16];
                    copy_in_bands(bytemuck::cast_slice_mut(&mut data), &view);
                    Rendered::Linear(ImageRgbaF32 {
                        width: w,
                        height: h,
                        data,
                    })
                }
                Output::Yuv420 { ten_bit, .. } => {
                    // Rows of whole words on the card, tight for the
                    // encoder.
                    let sample = if ten_bit { 2 } else { 1 };
                    let row = w as usize * sample;
                    let padded = bytes as usize / (h + h / 2) as usize;
                    let data = if padded == row {
                        view.to_vec()
                    } else {
                        view.chunks(padded)
                            .flat_map(|r| &r[..row])
                            .copied()
                            .collect()
                    };
                    Rendered::Yuv420 {
                        width: w,
                        height: h,
                        data,
                    }
                }
            }
        };
        readback.unmap();
        Ok(rendered)
    }

    /// Buffers of the frame are free again; pictures not used are let go.
    fn end_frame(&mut self) {
        self.free.append(&mut self.used);
        // The oldest go first.
        let mut kept: u64 = self.free.iter().map(|b| b.size()).sum();
        while kept > KEEP_FREE && !self.free.is_empty() {
            kept -= self.free.remove(0).size();
        }
        let seen = std::mem::take(&mut self.seen);
        self.uploaded.retain(|owner, _| seen.contains(owner));
    }

    fn buffer(&mut self, size: u64) -> wgpu::Buffer {
        let size = size.max(16).next_multiple_of(4);
        let b = match self.free.iter().position(|b| b.size() == size) {
            Some(i) => self.free.swap_remove(i),
            None => self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image"),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };
        self.used.push(b.clone());
        b
    }

    fn image(&mut self, w: u32, h: u32) -> Img {
        Img {
            buf: self.buffer(w as u64 * h as u64 * 16),
            w,
            h,
        }
    }

    fn transparent(&mut self, enc: &mut wgpu::CommandEncoder, w: u32, h: u32) -> Img {
        let img = self.image(w, h);
        enc.clear_buffer(&img.buf, 0, None);
        img
    }

    fn copy(&mut self, enc: &mut wgpu::CommandEncoder, src: &Img) -> Img {
        let img = self.image(src.w, src.h);
        enc.copy_buffer_to_buffer(&src.buf, 0, &img.buf, 0, src.w as u64 * src.h as u64 * 16);
        img
    }

    /// An image from the processor (kept for later frames when `owner` is
    /// an event).
    fn upload(&mut self, owner: Option<u64>, img: &Arc<ImageRgbaF32>) -> Img {
        if let Some(o) = owner {
            self.seen.push(o);
            if let Some((a, i)) = self.uploaded.get(&o)
                && Arc::ptr_eq(a, img)
            {
                return i.clone();
            }
        }
        let bytes: &[u8] = bytemuck::cast_slice(&img.data);
        let buf = match owner {
            // Kept apart from the frame's buffers.
            Some(_) => self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("picture"),
                size: (bytes.len() as u64).max(16),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            None => self.buffer(bytes.len() as u64),
        };
        self.gpu.queue.write_buffer(&buf, 0, bytes);
        let i = Img {
            buf,
            w: img.width,
            h: img.height,
        };
        if let Some(o) = owner {
            self.uploaded.insert(o, (img.clone(), i.clone()));
        }
        i
    }

    /// A transfer curve's table on the card.
    fn lut(&mut self, encode: bool, tc: TransferCharacteristic) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        let device = &self.gpu.device;
        self.luts
            .entry((encode, tc))
            .or_insert_with(|| {
                let table = if encode {
                    bv_color::transfer::EncodeLut::shared(tc).table()
                } else {
                    bv_color::transfer::DecodeLut::shared(tc).table()
                };
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("transfer table"),
                    contents: bytemuck::cast_slice(table),
                    usage: wgpu::BufferUsages::STORAGE,
                })
            })
            .clone()
    }

    fn kernel(
        &self,
        enc: &mut wgpu::CommandEncoder,
        k: K,
        params: Params,
        buffers: &[&wgpu::Buffer],
        grid: Grid,
    ) {
        self.gpu
            .kernels
            .run(&self.gpu.device, enc, k, &params, buffers, grid);
    }

    /// The frame, before the output rotation (as `bv_render::render_scene`).
    fn draw(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        pool: &mut VideoPool,
        background: [f32; 4],
    ) -> Img {
        let (w, h) = (scene.width, scene.height);
        let out = self.image(w, h);
        self.kernel(
            enc,
            K::Fill,
            Params::sized(w, h).with_f(&entered(scene, background)),
            &[&out.buf],
            Grid::Pixels(w, h),
        );
        if scene.muted {
            if let Some(p) = &scene.color {
                self.leave_through_view(enc, &out, p);
            }
            return out;
        }
        pool.prefetch_scene(scene, Want::Decoded);
        let units = bv_timeline::scene::composite_units(&scene.tracks);
        self.draw_units(enc, scene, &units, &out, pool);
        // Tools › Video › Video Output FX: applied to the finished frame.
        let mut out = out;
        let start = self.chain_start(enc, scene, None, scene.output_fx, 0, &out);
        for (i, e) in scene
            .output_fx
            .iter()
            .enumerate()
            .filter(|(_, e)| e.enabled)
        {
            match scene.frame_call(None, i, e) {
                Some(call) => self.plugin_from(enc, scene, &call, &mut out, start.as_ref()),
                None => self.effect(enc, &mut out, e, scene.scale),
            }
            self.stored(enc, scene, &out);
        }
        // The video bus's Fade to Color, over everything.
        if let Some((c, v)) = scene.output_fade {
            self.kernel(
                enc,
                K::FadeToColor,
                Params::sized(w, h).with_f(&[c[0], c[1], c[2], v]),
                &[&out.buf],
                Grid::Pixels(w, h),
            );
        }
        // Back to light (under ACES the view takes the values as they are).
        if scene.video_gamma && scene.color.is_none() {
            let mut p = Params::sized(w, h);
            p.j[0] = scene.clips as u32;
            self.kernel(enc, K::FromVideoGamma, p, &[&out.buf], Grid::Pixels(w, h));
        }
        if let Some(p) = &scene.color {
            self.leave_through_view(enc, &out, p);
        }
        out
    }

    fn rotated(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: Img,
        rotation: OutputRotation,
    ) -> Img {
        let (w, h, turn) = match rotation {
            OutputRotation::None => return img,
            OutputRotation::Clockwise90 => (img.h, img.w, 1),
            OutputRotation::Counterclockwise90 => (img.h, img.w, 2),
            OutputRotation::Inverted180 => (img.w, img.h, 3),
        };
        let out = self.image(w, h);
        let mut p = Params::sized(w, h);
        (p.sw, p.sh) = (img.w, img.h);
        p.j[0] = turn;
        self.kernel(enc, K::Rotate, p, &[&img.buf, &out.buf], Grid::Pixels(w, h));
        out
    }

    /// A track's picture as it composites (its level is the blend's):
    /// its content through its effects, toward its fade color.
    fn finished_layer(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        track: &bv_timeline::scene::TrackScene<'_>,
        pool: &mut VideoPool,
        below: &Img,
    ) -> Img {
        let (w, h) = (scene.width, scene.height);
        // An adjustment track's picture is what is below it (its events over
        // it: `adjust`), through its effects (its level mixes it in).
        let mut layer = if track.adjustment {
            self.copy(enc, below)
        } else {
            self.track_layer(enc, scene, &track.content, pool, below)
        };
        let pre = &track.effects[..track.post_from];
        let start = self.chain_start(enc, scene, Some(track.track), pre, 0, &layer);
        for (i, e) in pre.iter().enumerate().filter(|(_, e)| e.enabled) {
            match scene.frame_call(Some(track.track), i, e) {
                Some(call) => self.plugin_from(enc, scene, &call, &mut layer, start.as_ref()),
                None => self.effect(enc, &mut layer, e, scene.scale),
            }
            self.stored(enc, scene, &layer);
        }
        if let Some((c, v)) = track.fade {
            self.kernel(
                enc,
                K::FadeToColor,
                Params::sized(w, h).with_f(&[c[0], c[1], c[2], v]),
                &[&layer.buf],
                Grid::Pixels(w, h),
            );
        }
        match &track.motion {
            Some(m) => self.moved(enc, &layer, m),
            None => layer,
        }
    }

    /// A picture moved as a Track Motion says, its 2D shadow and glow
    /// under it (as `bv_render`'s `moved`).
    fn moved(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        layer: &Img,
        m: &bv_timeline::MotionAt,
    ) -> Img {
        let (w, h) = (layer.w, layer.h);
        let picture = self.place(enc, layer, (w, h), &m.transform, m.quality, 1.0);
        if m.shadow.is_none() && m.glow.is_none() {
            return picture;
        }
        let out = self.transparent(enc, w, h);
        for look in [m.shadow, m.glow].into_iter().flatten() {
            let s = look.size.max(0.01);
            let at = Transform {
                x: m.transform.x * (1.0 - s) + look.x,
                y: m.transform.y * (1.0 - s) + look.y,
                scale: s,
                ..Transform::default()
            };
            let mut shape = self.place(enc, &picture, (w, h), &at, m.quality, 1.0);
            let radius = look.blur.clamp(0.0, 1.0) * bv_timeline::motion::HALO_BLUR * w as f32;
            if radius >= 0.5 {
                shape = self.blur(enc, &shape, radius);
            }
            let c = look.color;
            let k = look.intensity.clamp(0.0, 1.0) * c[3];
            self.kernel(
                enc,
                K::Shade,
                Params::sized(w, h).with_f(&[c[0], c[1], c[2], k]),
                &[&shape.buf],
                Grid::Pixels(w, h),
            );
            self.over(enc, &shape, &out);
        }
        self.over(enc, &picture, &out);
        out
    }

    /// `top` over `under` (in place).
    fn over(&self, enc: &mut wgpu::CommandEncoder, top: &Img, under: &Img) {
        self.kernel(
            enc,
            K::Composite,
            Params::sized(under.w, under.h).with_f(&[1.0]),
            &[&top.buf, &under.buf],
            Grid::Pixels(under.w, under.h),
        );
    }

    /// `top` at `level` combined with `under` (in place) as `mode` says.
    fn blend(
        &self,
        enc: &mut wgpu::CommandEncoder,
        top: &Img,
        under: &Img,
        mode: bv_timeline::CompositeMode,
        level: f32,
    ) {
        let mut p = Params::sized(under.w, under.h).with_f(&[level]);
        p.j[0] = mode.index();
        self.kernel(
            enc,
            K::Blend,
            p,
            &[&top.buf, &under.buf],
            Grid::Pixels(under.w, under.h),
        );
    }

    /// Draws `units` (top first) over `under`, the bottom one first, as
    /// `bv_render`'s `draw_units`: compositing groups and modes.
    fn draw_units(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        units: &[bv_timeline::scene::CompositeUnit],
        under: &Img,
        pool: &mut VideoPool,
    ) {
        for u in units.iter().rev() {
            let t = &scene.tracks[u.track];
            if u.children.is_empty() {
                if t.adjustment {
                    self.adjust(enc, scene, t, pool, under);
                    continue;
                }
                let layer = self.finished_layer(enc, scene, t, pool, under);
                self.blend_track(enc, scene, t, &layer, under);
                self.post_composite(enc, scene, t, under);
            } else {
                let group = self.transparent(enc, scene.width, scene.height);
                self.draw_units(enc, scene, &u.children, &group, pool);
                if t.adjustment {
                    self.adjust(enc, scene, t, pool, &group);
                } else {
                    let layer = self.finished_layer(enc, scene, t, pool, &group);
                    self.blend_track(enc, scene, t, &layer, &group);
                    self.post_composite(enc, scene, t, &group);
                }
                // Parent Motion: the whole group moved.
                let group = match &t.parent_motion {
                    Some(m) => self.moved(enc, &group, m),
                    None => group,
                };
                if !self.composite(enc, scene, t, true, &group, under, 1.0) {
                    self.blend(enc, &group, under, t.parent_mode, 1.0);
                }
            }
        }
    }

    /// A track's layer onto what is below it through its compositing mode.
    fn blend_track(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        t: &bv_timeline::scene::TrackScene<'_>,
        layer: &Img,
        under: &Img,
    ) {
        if !self.composite(enc, scene, t, false, layer, under, t.level) {
            self.blend(enc, layer, under, t.mode, t.level);
        }
    }

    /// An adjustment track over `under` (in place), as `bv_render`'s
    /// `adjust_into`: its events over it, then its effects on all of
    /// that, mixed in by its level (their strength).
    fn adjust(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        t: &bv_timeline::scene::TrackScene<'_>,
        pool: &mut VideoPool,
        under: &Img,
    ) {
        if !matches!(t.content, TrackContent::Nothing) {
            let own = self.track_layer(enc, scene, &t.content, pool, under);
            self.over(enc, &own, under);
        }
        if t.level > 0.0 {
            let layer = self.finished_layer(enc, scene, t, pool, under);
            self.mix(enc, &layer, under, t.level);
        }
    }

    /// A track's effects after its Composite button on its group's
    /// picture `group` (in place), as `bv_render`'s `post_composite`.
    fn post_composite(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        t: &bv_timeline::scene::TrackScene<'_>,
        group: &Img,
    ) {
        if !t.effects.iter().skip(t.post_from).any(|e| e.enabled) {
            return;
        }
        let mut img = self.copy(enc, group);
        let start = self.chain_start(enc, scene, Some(t.track), t.effects, t.post_from, &img);
        for (i, e) in t
            .effects
            .iter()
            .enumerate()
            .skip(t.post_from)
            .filter(|(_, e)| e.enabled)
        {
            match scene.frame_call(Some(t.track), i, e) {
                Some(call) => self.plugin_from(enc, scene, &call, &mut img, start.as_ref()),
                None => self.effect(enc, &mut img, e, scene.scale),
            }
            self.stored(enc, scene, &img);
        }
        let bytes = group.w as u64 * group.h as u64 * 16;
        enc.copy_buffer_to_buffer(&img.buf, 0, &group.buf, 0, bytes);
    }

    /// `under` moved `level` of the way to `top` (in place).
    fn mix(&self, enc: &mut wgpu::CommandEncoder, top: &Img, under: &Img, level: f32) {
        let mut p = Params::sized(under.w, under.h).with_f(&[level]);
        p.j[0] = 100;
        self.kernel(
            enc,
            K::Blend,
            p,
            &[&top.buf, &under.buf],
            Grid::Pixels(under.w, under.h),
        );
    }

    /// A Custom compositing mode's compositor (`parent`: the Parent
    /// Composite Mode's) drawing `under` as `top` guides it, `level` of the
    /// way: BananaV's own as kernels; others' with both pictures out of the
    /// card, through the plug-in and the result back. `false` when there is
    /// none or it could not draw (Source Alpha stands in).
    #[allow(clippy::too_many_arguments)]
    fn composite(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        t: &bv_timeline::scene::TrackScene<'_>,
        parent: bool,
        top: &Img,
        under: &Img,
        level: f32,
    ) -> bool {
        let (Some(call), Some(plugins)) = (scene.compositor_call(t, parent), scene.plugins) else {
            return false;
        };
        if let Some(out) = self.own_compositor(enc, plugins, &call, top, under) {
            self.mix(enc, &out, under, level);
            return true;
        }
        let (Some(source), Some(map)) = (self.download(enc, under), self.download(enc, top)) else {
            return false;
        };
        let Some(out) = plugins.0.composite(&call, Arc::new(source), Arc::new(map)) else {
            return false;
        };
        let out = self.upload(None, &Arc::new(out));
        self.mix(enc, &out, under, level);
        true
    }

    fn track_layer(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        content: &TrackContent<'_>,
        pool: &mut VideoPool,
        below: &Img,
    ) -> Img {
        match content {
            TrackContent::Nothing => self.transparent(enc, scene.width, scene.height),
            TrackContent::Event(e) => self.event_layer(enc, scene, e, pool, below),
            TrackContent::Transition {
                kind,
                plugin,
                from,
                to,
                progress,
            } => {
                let b = self.event_layer(enc, scene, to, pool, below);
                let a = match from {
                    Some(a) => self.event_layer(enc, scene, a, pool, below),
                    None => self.transparent(enc, scene.width, scene.height),
                };
                if let (Some(call), Some(plugins)) = (plugin, scene.plugins) {
                    // BananaV's own on the card; others' (and its own
                    // without a kernel) through the processor.
                    if let Some(out) = self.own_transition(enc, plugins, call, (&a, &b), *progress)
                    {
                        return out;
                    }
                    if let Some(out) = self.plugin_transition(enc, plugins, call, &a, &b, *progress)
                    {
                        return out;
                    }
                }
                self.transition(enc, *kind, &a, &b, *progress)
            }
        }
    }

    fn event_layer(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        e: &EventLayer<'_>,
        pool: &mut VideoPool,
        below: &Img,
    ) -> Img {
        let img = match &e.source {
            Some(m) if m.info.is_adjustment() => self.adjusted(enc, scene, m, below),
            Some(m) => self.media_layer(enc, scene, e.clip.0, m, pool),
            None => return self.transparent(enc, scene.width, scene.height),
        };
        self.scale(enc, &img, e.level);
        if let Some(f) = e.fade {
            self.scale(enc, &img, f);
        }
        img
    }

    /// An adjustment event's picture: what is below it, through its effects
    /// (as `bv_render`'s `adjusted`).
    fn adjusted(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        m: &MediaLayer<'_>,
        below: &Img,
    ) -> Img {
        let mut img = self.copy(enc, below);
        let framing = Framing {
            framed_at: 0,
            media_size: (img.w, img.h),
            media_fx_scale: 1.0,
            par: 1.0,
            quality: m.quality,
        };
        for (k, e) in m.effects.iter().enumerate() {
            match scene.media_call(m, k, scene.scale, Some(framing)) {
                Some(call) => self.plugin(enc, scene, &call, &mut img),
                None => self.effect(enc, &mut img, e.fx, scene.scale),
            }
            self.stored(enc, scene, &img);
        }
        img
    }

    fn scale(&self, enc: &mut wgpu::CommandEncoder, img: &Img, k: f32) {
        if (k - 1.0).abs() < 1e-6 {
            return;
        }
        self.kernel(
            enc,
            K::Scale,
            Params::sized(img.w, img.h).with_f(&[k]),
            &[&img.buf],
            Grid::Pixels(img.w, img.h),
        );
    }

    /// A picture of a media deinterlaced as `how` says, as `bv_render`'s
    /// `deinterlace` (a new picture).
    fn deinterlace(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        how: (bv_timeline::DeinterlaceMethod, bv_core::FieldOrder),
    ) -> Img {
        let (kernel, keep) = match how.0 {
            bv_timeline::DeinterlaceMethod::None => return self.copy(enc, img),
            bv_timeline::DeinterlaceMethod::Interpolate
            | bv_timeline::DeinterlaceMethod::SmartAdaptive => (
                K::InterpolateFields,
                u32::from(how.1 == bv_core::FieldOrder::LowerFirst),
            ),
            _ => (K::FlickerFilter, 0),
        };
        let out = self.image(img.w, img.h);
        let mut p = Params::sized(img.w, img.h);
        p.j[0] = keep;
        // Smart adaptive: the kernel's mix by the comb's size.
        if how.0 == bv_timeline::DeinterlaceMethod::SmartAdaptive {
            p.j[1] = 1;
            p.f[0] = bv_timeline::SMART_THRESHOLDS[0];
            p.f[1] = bv_timeline::SMART_THRESHOLDS[1];
        }
        self.kernel(
            enc,
            kernel,
            p,
            &[&img.buf, &out.buf],
            Grid::Pixels(img.w, img.h),
        );
        out
    }

    /// The media's picture `ask` asks `owner`'s reader for, in the
    /// composite's space; `None` when it cannot be read.
    fn media_picture(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        m: &MediaLayer<'_>,
        pool: &mut VideoPool,
        owner: u64,
        ask: Ask,
    ) -> Option<Img> {
        // Under ACES, the picture's light enters its working space (decoded
        // ones as they are converted); a media in a color space of its own
        // (Media Properties) enters from its file's values, below.
        let space = scene
            .color
            .as_ref()
            .zip(m.info.color_space)
            .and_then(|(p, s)| p.media_input(s));
        let light = scene
            .color
            .as_ref()
            .filter(|_| !scene.video_gamma && space.is_none());
        let how = |img: &Img| m.deinterlaced(img.h);
        // BananaV's own generators draw here, on the card, as their
        // plug-ins draw them for the processor.
        let generated = self.own_generator(enc, m);
        let src = match generated {
            Some(img) => {
                if let Some(p) = light {
                    self.enter_working_space(enc, &img, p);
                }
                img
            }
            None => match pool.picture(owner, m.info, ask) {
                Ok(Picture::Decoded(f)) => {
                    self.decoded(enc, &f, m.min_size, light.map(|p| p.input), m.video_levels)
                }
                Ok(Picture::Linear(img)) => {
                    let up = self.upload(Some(owner), &img);
                    match light {
                        // On a copy: the upload is kept for the next frames.
                        Some(p) => {
                            let img = self.copy(enc, &up);
                            self.enter_working_space(enc, &img, p);
                            img
                        }
                        None => up,
                    }
                }
                Err(_) => return None,
            },
        };
        // 8-bit formats and the 2.222 compositing gamma work on the encoded
        // values (on a copy: the picture may be kept for the next frame);
        // under ACES those values enter its working space, or a media's
        // file values through its color space's conversion.
        let img = if scene.video_gamma || space.is_some() {
            let img = self.copy(enc, &src);
            self.kernel(
                enc,
                K::ToVideoGamma,
                Params::sized(img.w, img.h),
                &[&img.buf],
                Grid::Pixels(img.w, img.h),
            );
            match (&space, &scene.color) {
                (Some(q), _) => self.enter_from_space(enc, &img, q),
                (None, Some(p)) => self.enter_working_space(enc, &img, p),
                (None, None) => {}
            }
            img
        } else {
            src
        };
        // Interlaced media: whole frames, as the Deinterlace method says.
        Some(match how(&img) {
            Some(h) => self.deinterlace(enc, &img, h),
            None => img,
        })
    }

    /// The media's picture placed in the frame, with its effects; the
    /// offline color where it cannot be read.
    fn media_layer(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        owner: u64,
        m: &MediaLayer<'_>,
        pool: &mut VideoPool,
    ) -> Img {
        let Some(mut src) =
            self.media_picture(enc, scene, m, pool, owner, Ask::of(m, Want::Decoded))
        else {
            return self.offline(scene, m);
        };
        // Frame Blend (the Resample mode): the media's next frame mixed in,
        // as `bv_render`'s (on a copy: the picture may be kept).
        if let Some((ask, k)) = Ask::blend(m, Want::Decoded)
            && let Some(next) =
                self.media_picture(enc, scene, m, pool, bv_media::blend_owner(owner), ask)
            && (next.w, next.h) == (src.w, src.h)
        {
            let img = self.copy(enc, &src);
            self.mix(enc, &next, &img, k);
            src = img;
        }
        let tf = &m.transform;
        // The media's effects work on its own picture, on pixels that
        // show; then the event's Pan/Crop frames it, and the event's
        // effects work on the framed picture (VEGAS's default: Pan/Crop
        // first in the event's chain).
        let framed_at = m.framed_from();
        let (src, media_fx_scale) = if framed_at == 0 {
            (src, 1.0)
        } else {
            let mut src = self.reduce_for_display(enc, scene, &src, tf.scale, m.par);
            let (pw, ph) = scene.project_size;
            let fit = fit_rect_par(src.w, src.h, m.par, pw, ph);
            let fx_scale = src.h as f32 / (fit.h * tf.scale.max(1e-4));
            for (k, e) in m.effects[..framed_at].iter().enumerate() {
                match scene.media_call(m, k, fx_scale, None) {
                    Some(call) => self.plugin(enc, scene, &call, &mut src),
                    None => self.effect(enc, &mut src, e.fx, fx_scale),
                }
                self.stored(enc, scene, &src);
            }
            (src, fx_scale)
        };
        let mut img = self.place(enc, &src, (scene.width, scene.height), tf, m.quality, m.par);
        // Reduce Interlace Flicker: on the framed picture, before the
        // event's effects.
        if m.clip.reduce_flicker {
            let out = self.image(img.w, img.h);
            self.kernel(
                enc,
                K::FlickerFilter,
                Params::sized(img.w, img.h),
                &[&img.buf, &out.buf],
                Grid::Pixels(img.w, img.h),
            );
            img = out;
        }
        // Its Pan/Crop masks (as `bv_render`'s): worked out on the
        // processor at the picture's size, framed here as the picture is;
        // kept on the card while they do not change.
        let mask = m.mask_picture(src.w, src.h).map(|c| {
            let up = self.upload(Some(owner.rotate_left(17) ^ 0x6d61_736b), &c);
            self.place(enc, &up, (scene.width, scene.height), tf, m.quality, m.par)
        });
        let unaffected = match &mask {
            Some(_) if m.clip.mask.as_ref().is_some_and(|k| k.apply_to_fx) => {
                Some(self.copy(enc, &img))
            }
            _ => None,
        };
        let framing = Framing {
            framed_at,
            media_size: (src.w, src.h),
            media_fx_scale,
            par: m.par,
            quality: m.quality,
        };
        // The event's picture before its effects, kept for those that
        // draw over it (Bézier Masking's Mask FX).
        let start = m
            .effects
            .iter()
            .skip(framed_at)
            .any(|e| {
                e.fx.enabled
                    && e.fx
                        .plugin
                        .as_ref()
                        .is_some_and(|p| bv_effects::accel::takes_start(&p.id))
            })
            .then(|| self.copy(enc, &img));
        for (k, e) in m.effects.iter().enumerate().skip(framed_at) {
            match scene.media_call(m, k, scene.scale, Some(framing)) {
                Some(call) => self.plugin_with(enc, scene, &call, &mut img, start.as_ref()),
                None => self.effect(enc, &mut img, e.fx, scene.scale),
            }
            self.stored(enc, scene, &img);
        }
        if let Some(mask) = &mask {
            // Cut: the picture times the mask. Apply to FX: the effects'
            // picture where it keeps, the unaffected one elsewhere.
            self.by_mask(enc, mask, &img, 101);
            if let Some(plain) = unaffected {
                self.by_mask(enc, mask, &plain, 102);
                let mut p = Params::sized(plain.w, plain.h).with_f(&[1.0]);
                p.j[0] = 2;
                self.kernel(
                    enc,
                    K::Blend,
                    p,
                    &[&img.buf, &plain.buf],
                    Grid::Pixels(plain.w, plain.h),
                );
                img = plain;
            }
        }
        img
    }

    /// `img` times `mask`'s alpha (`mode` 101) or what it leaves (102).
    fn by_mask(&self, enc: &mut wgpu::CommandEncoder, mask: &Img, img: &Img, mode: u32) {
        let mut p = Params::sized(img.w, img.h);
        p.j[0] = mode;
        self.kernel(
            enc,
            K::Blend,
            p,
            &[&mask.buf, &img.buf],
            Grid::Pixels(img.w, img.h),
        );
    }

    /// What an effect left, as the project's pixel format keeps it (as
    /// `bv_render::effects::stored`): 8-bit formats clip it to 0–1.
    fn stored(&self, enc: &mut wgpu::CommandEncoder, scene: &FrameScene<'_>, img: &Img) {
        if scene.clips {
            self.kernel(
                enc,
                K::ClipRange,
                Params::sized(img.w, img.h),
                &[&img.buf],
                Grid::Pixels(img.w, img.h),
            );
        }
    }

    /// The offline color where the media shows.
    fn offline(&mut self, scene: &FrameScene<'_>, m: &MediaLayer<'_>) -> Img {
        let (w, h) = m
            .info
            .video
            .map_or((scene.width, scene.height), |v| (v.width, v.height));
        let r = fit_rect_par(w, h, m.par, scene.width, scene.height);
        let mut img = ImageRgbaF32::transparent(scene.width, scene.height);
        let color = entered(scene, OFFLINE);
        for y in r.y as u32..(r.y + r.h) as u32 {
            for x in r.x as u32..(r.x + r.w) as u32 {
                *img.pixel_mut(x, y) = color;
            }
        }
        self.upload(None, &Arc::new(img))
    }

    /// A decoded picture in linear light, reduced as the processor's
    /// readers reduce it for `min_size`; then through `enter` (ACES's
    /// working space) when given.
    fn decoded(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        f: &VideoFrame,
        min_size: (u32, u32),
        enter: Option<[[f32; 3]; 3]>,
        video_levels: bool,
    ) -> Img {
        let factor = bv_media::reduction(f.width, f.height, min_size.0, min_size.1);
        let planar = matches!(
            f.format,
            PixelFormat::Yuv420P8
                | PixelFormat::Yuv422P8
                | PixelFormat::Yuv444P8
                | PixelFormat::Yuv420P10
                | PixelFormat::Yuv422P10
                | PixelFormat::Yuv444P10
        ) && f.planes.len() == 3;
        if !planar {
            let img = bv_color::frame_to_linear_levels(f, factor, video_levels);
            let up = self.upload(None, &Arc::new(img));
            if let Some(m) = enter {
                self.kernel(
                    enc,
                    K::ColorEnter,
                    Params::sized(up.w, up.h).with_f(&m.concat()),
                    &[&up.buf],
                    Grid::Pixels(up.w, up.h),
                );
            }
            return up;
        }
        // The three planes one after another, each from a word boundary.
        let mut bytes = Vec::new();
        let mut starts = [0u32; 3];
        for (n, p) in f.planes.iter().enumerate() {
            starts[n] = bytes.len() as u32;
            match &p.data {
                PlaneData::U8(d) => bytes.extend_from_slice(d),
                PlaneData::U16(d) => {
                    for v in d {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
            while bytes.len() % 4 != 0 {
                bytes.push(0);
            }
        }
        let planes = self.buffer(bytes.len() as u64);
        self.gpu.queue.write_buffer(&planes, 0, &bytes);
        let lut = self.lut(false, f.color.transfer);
        let (ow, oh) = (f.width.div_ceil(factor), f.height.div_ceil(factor));
        let out = self.image(ow, oh);
        let depth = f.format.bit_depth();
        let (sx, sy) = f.format.chroma_shift();
        let coeffs = bv_color::matrix::kr_kb(f.color.matrix);
        let gamut = (f.color.primaries != bv_color::convert::WORKING_PRIMARIES).then(|| {
            bv_color::primaries::conversion(f.color.primaries, bv_color::convert::WORKING_PRIMARIES)
        });
        // ACES's working space after the working primaries, one matrix.
        let gamut = match (gamut, enter) {
            (g, None) => g,
            (None, Some(e)) => Some(e),
            (Some(g), Some(e)) => Some(std::array::from_fn(|i| {
                std::array::from_fn(|j| (0..3).map(|k| e[i][k] * g[k][j]).sum())
            })),
        };
        // The video levels formats: Y′CbCr as studio RGB.
        let range = if video_levels {
            bv_core::ColorRange::Full
        } else {
            f.color.range
        };
        let n = bv_color::matrix::Normalizer::new(range, depth).params();
        let (kr, kb) = coeffs.unwrap_or((0.0, 0.0));
        let mut p = Params::sized(ow, oh);
        (p.sw, p.sh) = (f.width, f.height);
        p.i = [
            f.planes[0].stride as i32,
            f.planes[1].stride as i32,
            f.planes[1].width as i32,
            f.planes[1].height as i32,
        ];
        p.j = [
            factor,
            u32::from(depth > 8)
                | (u32::from(coeffs.is_none()) << 1)
                | (u32::from(gamut.is_some()) << 2),
            sx as u32,
            sy as u32,
        ];
        p.o = [starts[0], starts[1], starts[2], 0];
        p.f[..4].copy_from_slice(&n);
        p.f[4] = kr;
        p.f[5] = kb;
        if let Some(m) = gamut {
            for (r, row) in m.iter().enumerate() {
                p.f[8 + r * 3..8 + r * 3 + 3].copy_from_slice(row);
            }
        }
        self.kernel(
            enc,
            K::YuvToLinear,
            p,
            &[&planes, &lut, &out.buf],
            Grid::Pixels(ow, oh),
        );
        out
    }

    /// Halves the source while it is more than twice the size it shows at
    /// (always a copy: effects then change it).
    fn reduce_for_display(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        src: &Img,
        scale: f32,
        par: f32,
    ) -> Img {
        let fit = fit_rect_par(src.w, src.h, par, scene.width, scene.height);
        let mut img = src.clone();
        let mut halved = false;
        while img.w as f32 >= fit.w * scale * 2.0
            && img.h as f32 >= fit.h * scale * 2.0
            && img.w > 1
        {
            img = self.half(enc, &img);
            halved = true;
        }
        if halved { img } else { self.copy(enc, &img) }
    }

    fn half(&mut self, enc: &mut wgpu::CommandEncoder, src: &Img) -> Img {
        let (w, h) = ((src.w / 2).max(1), (src.h / 2).max(1));
        let out = self.image(w, h);
        let mut p = Params::sized(w, h);
        (p.sw, p.sh) = (src.w, src.h);
        self.kernel(
            enc,
            K::HalfSize,
            p,
            &[&src.buf, &out.buf],
            Grid::Pixels(w, h),
        );
        out
    }

    /// `src` placed in a `w` × `h` layer, framed by `tf` (as
    /// `bv_render::place_with`).
    fn place(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        src: &Img,
        (w, h): (u32, u32),
        tf: &Transform,
        quality: RenderQuality,
        par: f32,
    ) -> Img {
        let out = self.image(w, h);
        let fit = fit_rect_par(src.w, src.h, par, w, h);
        let scale = tf.scale.max(1e-4);
        if tf.is_identity() && fit.w as u32 == src.w && fit.h as u32 == src.h {
            let mut p = Params::sized(w, h);
            (p.sw, p.sh) = (src.w, src.h);
            p.i = [fit.x as i32, fit.y as i32, 0, 0];
            self.kernel(
                enc,
                K::CopyInto,
                p,
                &[&src.buf, &out.buf],
                Grid::Pixels(w, h),
            );
            return out;
        }
        // Large reductions go through 2 × 2 averages first (Good and Best).
        let stretch = tf.stretch.max(1e-3);
        let prefilter = matches!(quality, RenderQuality::Good | RenderQuality::Best);
        let mut s = src.clone();
        while prefilter
            && (s.w as f32) >= fit.w * scale * 2.0
            && (s.h as f32) >= fit.h * scale * stretch * 2.0
            && s.w > 1
        {
            s = self.half(enc, &s);
        }
        let (sw, sh) = (s.w as f32, s.h as f32);
        let crop = [
            tf.crop_left.clamp(0.0, 1.0) * sw,
            tf.crop_top.clamp(0.0, 1.0) * sh,
            (1.0 - tf.crop_right.clamp(0.0, 1.0)) * sw,
            (1.0 - tf.crop_bottom.clamp(0.0, 1.0)) * sh,
        ];
        let center_x = w as f32 / 2.0 + tf.x * w as f32;
        let center_y = h as f32 / 2.0 - tf.y * h as f32;
        let (sin, cos) = (-tf.rotation_deg.to_radians()).sin_cos();
        let to_src_x = sw / (fit.w * scale);
        let to_src_y = sh / (fit.h * scale * stretch);
        let fx = if tf.flip_h { -1.0 } else { 1.0 };
        let fy = if tf.flip_v { -1.0 } else { 1.0 };
        let mut p = Params::sized(w, h).with_f(&[
            crop[0], crop[1], crop[2], crop[3], center_x, center_y, sin, cos, to_src_x, to_src_y,
            fx, fy,
        ]);
        (p.sw, p.sh) = (s.w, s.h);
        p.j[0] = match quality {
            RenderQuality::Draft => 0,
            RenderQuality::Preview | RenderQuality::Good => 1,
            RenderQuality::Best => 2,
        };
        self.kernel(enc, K::Place, p, &[&s.buf, &out.buf], Grid::Pixels(w, h));
        out
    }

    fn transition(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        kind: Transition,
        a: &Img,
        b: &Img,
        p: f32,
    ) -> Img {
        let p = p.clamp(0.0, 1.0);
        let (w, h) = (a.w, a.h);
        let (wf, hf) = (w as f32, h as f32);
        let out = self.image(w, h);
        let mut params = Params::sized(w, h).with_f(&[p]);
        let k = match kind {
            Transition::Dissolve => K::Dissolve,
            Transition::FadeThroughBlack => K::FadeThroughBlack,
            Transition::Wipe(d) => {
                params.j[0] = direction_index(d);
                K::Wipe
            }
            Transition::Push(d) | Transition::Slide(d) => {
                let (dx, dy) = match d {
                    Direction::Left => (-1, 0),
                    Direction::Right => (1, 0),
                    Direction::Up => (0, -1),
                    Direction::Down => (0, 1),
                };
                let shift_x = (dx as f32 * p * wf).round() as i32;
                let shift_y = (dy as f32 * p * hf).round() as i32;
                params.i = [shift_x, shift_y, dx, dy];
                params.j[0] = matches!(kind, Transition::Push(_)) as u32;
                K::PushSlide
            }
            Transition::Iris => {
                params.f[1] = (wf * wf + hf * hf).sqrt() / 2.0;
                K::Iris
            }
            Transition::Clock => K::Clock,
            Transition::Zoom => {
                params.f[1] = 0.05 + 0.95 * p;
                K::Zoom
            }
        };
        self.kernel(
            enc,
            k,
            params,
            &[&a.buf, &b.buf, &out.buf],
            Grid::Pixels(w, h),
        );
        out
    }

    /// A Gaussian blur of `radius` (three box passes each way).
    fn blur(&mut self, enc: &mut wgpu::CommandEncoder, img: &Img, radius: f32) -> Img {
        let sigma = (radius / 2.0).max(0.3);
        let a = self.copy(enc, img);
        let b = self.image(img.w, img.h);
        for r in box_sizes(sigma, 3) {
            if r == 0 {
                continue;
            }
            let mut p = Params::sized(img.w, img.h).with_f(&[1.0 / (2 * r + 1) as f32]);
            p.j[0] = r as u32;
            self.kernel(enc, K::BoxH, p, &[&a.buf, &b.buf], Grid::Lines(img.h));
            self.kernel(enc, K::BoxV, p, &[&b.buf, &a.buf], Grid::Lines(img.w));
        }
        a
    }

    fn color(&self, enc: &mut wgpu::CommandEncoder, img: &Img, k: K, values: &[f32]) {
        let p = Params::sized(img.w, img.h).with_f(values);
        self.kernel(enc, k, p, &[&img.buf], Grid::Pixels(img.w, img.h));
    }

    /// One effect (as `bv_render::effects::apply`); `scale` turns its sizes
    /// in the project's pixels into the image's.
    fn effect(&mut self, enc: &mut wgpu::CommandEncoder, img: &mut Img, e: &Effect, scale: f32) {
        let p = |i: usize| e.param(i);
        let (w, h) = (img.w, img.h);
        match e.kind {
            EffectKind::BrightnessContrast => {
                self.color(enc, img, K::BrightnessContrast, &[p(0), p(1) + 1.0]);
            }
            EffectKind::ColorGrade => {
                let temp = p(grade::TEMPERATURE);
                let tint = p(grade::TINT);
                let mut v = vec![
                    2f32.powf(p(grade::EXPOSURE)),
                    1.0 + p(grade::CONTRAST),
                    p(grade::SATURATION),
                    0.18f32.powf(1.0 / 2.2),
                    1.0 + 0.3 * temp,
                    1.0 - 0.3 * tint,
                    1.0 - 0.3 * temp,
                ];
                for base in [grade::LIFT, grade::GAMMA, grade::GAIN] {
                    v.extend((0..3).map(|i| p(base + i)));
                }
                self.color(enc, img, K::ColorGrade, &v);
            }
            EffectKind::Levels => {
                let (ib, iw, g, ob, ow) = (p(0), p(1), p(2), p(3), p(4));
                self.color(enc, img, K::Levels, &[ib, (iw - ib).max(1e-4), g, ob, ow]);
            }
            EffectKind::BlackAndWhite => self.color(enc, img, K::BlackAndWhite, &[p(0)]),
            EffectKind::Sepia => self.color(enc, img, K::Sepia, &[p(0)]),
            EffectKind::Invert => self.color(enc, img, K::Invert, &[p(0)]),
            EffectKind::GaussianBlur => {
                let r = p(0) * scale;
                if r >= 0.5 {
                    *img = self.blur(enc, img, r);
                }
            }
            EffectKind::Sharpen => {
                let (amount, r) = (p(0), (p(1) * scale).max(0.5));
                let soft = self.blur(enc, img, r);
                self.kernel(
                    enc,
                    K::Sharpen,
                    Params::sized(w, h).with_f(&[amount]),
                    &[&soft.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            EffectKind::Vignette => self.kernel(
                enc,
                K::Vignette,
                Params::sized(w, h).with_f(&[p(0), p(1), p(2)]),
                &[&img.buf],
                Grid::Pixels(w, h),
            ),
            EffectKind::Mirror => {
                let out = self.image(w, h);
                let mut params = Params::sized(w, h);
                params.j = [(p(0) >= 0.5) as u32, (p(1) >= 0.5) as u32, 0, 0];
                self.kernel(
                    enc,
                    K::Mirror,
                    params,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            EffectKind::Pixelate => {
                let block = ((p(0) * scale).round() as u32).max(1);
                if block > 1 {
                    let (bw, bh) = (w.div_ceil(block), h.div_ceil(block));
                    let blocks = self.image(bw, bh);
                    let mut params = Params::sized(bw, bh);
                    (params.sw, params.sh) = (w, h);
                    params.i[0] = block as i32;
                    self.kernel(
                        enc,
                        K::BlockAverage,
                        params,
                        &[&img.buf, &blocks.buf],
                        Grid::Pixels(bw, bh),
                    );
                    let mut params = Params::sized(w, h);
                    (params.sw, params.sh) = (bw, bh);
                    params.i[0] = block as i32;
                    self.kernel(
                        enc,
                        K::BlockFill,
                        params,
                        &[&blocks.buf, &img.buf],
                        Grid::Pixels(w, h),
                    );
                }
            }
            EffectKind::ChromaKey => {
                let key = [p(0), p(1), p(2)];
                let (tol, soft, spill) = (p(3), p(4).max(1e-3), p(5));
                let y = luma(key);
                let k = [(key[2] - y) / 1.8556, (key[0] - y) / 1.5748];
                let k_len = (k[0] * k[0] + k[1] * k[1]).sqrt().max(1e-4);
                let dom = if key[1] >= key[0] && key[1] >= key[2] {
                    1
                } else if key[2] >= key[0] {
                    2
                } else {
                    0
                };
                let mut params = Params::sized(w, h).with_f(&[tol, soft, spill, k[0], k[1], k_len]);
                params.j[0] = dom;
                self.kernel(enc, K::ChromaKey, params, &[&img.buf], Grid::Pixels(w, h));
            }
            EffectKind::Glow => {
                let (threshold, intensity, r) = (p(0), p(1), (p(2) * scale).max(0.5));
                let bright = self.copy(enc, img);
                self.kernel(
                    enc,
                    K::GlowBright,
                    Params::sized(w, h).with_f(&[threshold]),
                    &[&bright.buf],
                    Grid::Pixels(w, h),
                );
                let glow = self.blur(enc, &bright, r);
                self.kernel(
                    enc,
                    K::AddGlow,
                    Params::sized(w, h).with_f(&[intensity]),
                    &[&glow.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            // Audio effects do not touch the picture; plug-ins' go through
            // `plugin`.
            EffectKind::Gain
            | EffectKind::Eq3
            | EffectKind::HighPass
            | EffectKind::LowPass
            | EffectKind::Compressor
            | EffectKind::Limiter
            | EffectKind::NoiseGate
            | EffectKind::Delay
            | EffectKind::Reverb
            | EffectKind::ModernEq
            | EffectKind::Vst
            | EffectKind::Plugin => {}
        }
    }

    /// The picture a chain of a track's or the output's effects starts
    /// from (`effects` from the `from`th), kept on the card when one of
    /// them takes it (Bézier Masking's Mask FX).
    fn chain_start(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        track: Option<bv_timeline::TrackId>,
        effects: &[bv_timeline::Effect],
        from: usize,
        img: &Img,
    ) -> Option<Img> {
        let plugins = scene.plugins?;
        let takes = effects.iter().enumerate().skip(from).any(|(i, e)| {
            e.enabled
                && scene
                    .frame_call(track, i, e)
                    .is_some_and(|c| plugins.0.takes_chain_input(&c))
        });
        takes.then(|| self.copy(enc, img))
    }

    /// [`Self::plugin`], the chain's starting picture given to the
    /// plug-ins that take it.
    fn plugin_from(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
        start: Option<&Img>,
    ) {
        let Some(plugins) = scene.plugins else {
            return;
        };
        let Some(start) = start.filter(|_| plugins.0.takes_chain_input(call)) else {
            return self.plugin(enc, scene, call, img);
        };
        if self.own_effect_from(enc, plugins, call, img, Some(start)) {
            return;
        }
        let (Some(input), Some(start)) = (self.download(enc, img), self.download(enc, start))
        else {
            return;
        };
        if let Some(out) = plugins
            .0
            .apply_in_chain(call, Arc::new(input), Arc::new(start))
        {
            *img = self.upload(None, &Arc::new(out));
        }
    }

    /// [`Self::plugin`], BananaV's own effects given the picture their
    /// chain started from when the card keeps it (`start`: an event's
    /// before its effects).
    fn plugin_with(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
        start: Option<&Img>,
    ) {
        if let (Some(plugins), Some(start)) = (scene.plugins, start)
            && self.own_effect_from(enc, plugins, call, img, Some(start))
        {
            return;
        }
        self.plugin(enc, scene, call, img);
    }

    /// A plug-in's effect: BananaV's own are drawn here, on the card
    /// ([`own`]); others' run on the processor, so the picture comes out of
    /// the card, goes through the plug-in and back.
    fn plugin(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        scene: &FrameScene<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
    ) {
        let Some(plugins) = scene.plugins else {
            return;
        };
        if self.own_effect(enc, plugins, call, img) || self.plugin_on_card(enc, plugins, call, img)
        {
            return;
        }
        let Some(input) = self.download(enc, img) else {
            return;
        };
        if let Some(out) = plugins.0.apply(call, Arc::new(input)) {
            *img = self.upload(None, &Arc::new(out));
        }
    }

    /// A plug-in that renders with CUDA draws the picture where it is
    /// ([`bv_timeline::scene::PluginEffects::on_card`]): it goes to a
    /// buffer CUDA shares, its rows turned (OpenFX's go up), the plug-in
    /// draws into another, and its picture comes back turned. `false`:
    /// the processor's way.
    fn plugin_on_card(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
    ) -> bool {
        // Asked first: CUDA's driver starts only for such plug-ins.
        if !plugins.0.on_card(call) {
            return false;
        }
        let Some(cuda) = self.gpu.cuda() else {
            return false;
        };
        let (w, h) = (img.w, img.h);
        if !self.shared_ready(cuda, w, h, 2) {
            return false;
        }
        self.turn_rows(enc, &img.buf, &self.shared[0].buffer, w, h);
        if !self.finish_for_cuda(enc) {
            return false;
        }
        let (input, output) = (self.card_picture(0, w, h), self.card_picture(1, w, h));
        if !plugins.0.apply_on_card(call, input, output) {
            return false;
        }
        let out = self.image(w, h);
        self.turn_rows(enc, &self.shared[1].buffer, &out.buf, w, h);
        *img = out;
        true
    }

    /// [`Self::plugin_on_card`] for a transition.
    fn transition_on_card(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        (from, to): (&Img, &Img),
        progress: f32,
    ) -> Option<Img> {
        let (w, h) = (to.w, to.h);
        if (from.w, from.h) != (w, h) || !plugins.0.on_card(call) {
            return None;
        }
        let cuda = self.gpu.cuda()?;
        if !self.shared_ready(cuda, w, h, 3) {
            return None;
        }
        self.turn_rows(enc, &from.buf, &self.shared[0].buffer, w, h);
        self.turn_rows(enc, &to.buf, &self.shared[1].buffer, w, h);
        if !self.finish_for_cuda(enc) {
            return None;
        }
        let pictures = [0, 1, 2].map(|i| self.card_picture(i, w, h));
        if !plugins
            .0
            .transition_on_card(call, pictures[0], pictures[1], pictures[2], progress)
        {
            return None;
        }
        let out = self.image(w, h);
        self.turn_rows(enc, &self.shared[2].buffer, &out.buf, w, h);
        Some(out)
    }

    /// At least `n` buffers CUDA shares, each a `w` × `h` picture's.
    fn shared_ready(&mut self, cuda: &'static bv_cuda::Device, w: u32, h: u32, n: usize) -> bool {
        let bytes = (w as u64 * h as u64 * 16).max(16);
        if self.shared.first().is_some_and(|s| s.bytes != bytes) {
            self.shared.clear();
        }
        while self.shared.len() < n {
            match shared::make(&self.gpu.device, cuda, bytes) {
                Some(s) => self.shared.push(s),
                None => return false,
            }
        }
        true
    }

    /// The `i`th shared buffer as a `w` × `h` picture for a plug-in.
    fn card_picture(&self, i: usize, w: u32, h: u32) -> CardPicture {
        let s = &self.shared[i];
        CardPicture {
            device: s.cuda.device().ordinal(),
            address: s.cuda.address(),
            width: w,
            height: h,
            row_bytes: w * 16,
        }
    }

    /// `src`'s rows into `dst` upside down: BananaV's go down, OpenFX's up.
    fn turn_rows(
        &self,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::Buffer,
        dst: &wgpu::Buffer,
        w: u32,
        h: u32,
    ) {
        let mut params = Params::sized(w, h);
        params.j = [0, 1, 0, 0];
        self.kernel(enc, K::Mirror, params, &[src, dst], Grid::Pixels(w, h));
    }

    /// Sends the work recorded so far and waits for the card to finish it,
    /// so CUDA finds what it wrote; recording goes on in a new encoder.
    fn finish_for_cuda(&mut self, enc: &mut wgpu::CommandEncoder) -> bool {
        let next = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let done = std::mem::replace(enc, next);
        let submitted = self.gpu.queue.submit([done.finish()]);
        self.gpu
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: None,
            })
            .is_ok()
    }

    /// A plug-in's transition: both pictures come out of the card, go
    /// through the plug-in, and the result goes back (`None` when the
    /// plug-in could not draw it).
    fn plugin_transition(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        from: &Img,
        to: &Img,
        progress: f32,
    ) -> Option<Img> {
        if let Some(out) = self.transition_on_card(enc, plugins, call, (from, to), progress) {
            return Some(out);
        }
        let a = Arc::new(self.download(enc, from)?);
        let b = Arc::new(self.download(enc, to)?);
        let out = plugins.0.transition(call, a, b, progress)?;
        Some(self.upload(None, &Arc::new(out)))
    }

    /// An image's pixels out of the card: the work recorded so far is sent
    /// first, and recording goes on in a new encoder.
    fn download(&mut self, enc: &mut wgpu::CommandEncoder, img: &Img) -> Option<ImageRgbaF32> {
        let bytes = img.w as u64 * img.h as u64 * 16;
        let size = bytes.max(16);
        let staging = match self.plugin_readbacks.iter().position(|b| b.size() == size) {
            Some(i) => self.plugin_readbacks.swap_remove(i),
            None => self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("plug-in picture"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };
        enc.copy_buffer_to_buffer(&img.buf, 0, &staging, 0, bytes);
        let next = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let done = std::mem::replace(enc, next);
        let submitted = self.gpu.queue.submit([done.finish()]);
        let picture = match self.read(&staging, submitted, bytes, (img.w, img.h), Output::Linear) {
            Ok(Rendered::Linear(i)) => Some(i),
            _ => None,
        };
        // A transition's two pictures, at most, of the last size (one that
        // failed may still be mapped: never again).
        self.plugin_readbacks.retain(|b| b.size() == size);
        self.plugin_readbacks.truncate(1);
        if picture.is_some() {
            self.plugin_readbacks.push(staging);
        }
        picture
    }
}

/// `src` copied into `dst` (as much as both hold) in bands on a few
/// threads: frames are big, and copies are bound by memory.
fn copy_in_bands(dst: &mut [u8], src: &[u8]) {
    let n = dst.len().min(src.len());
    let (dst, src) = (&mut dst[..n], &src[..n]);
    let threads = std::thread::available_parallelism()
        .map_or(1, |t| t.get())
        .min(8);
    let per = n.div_ceil(threads).max(1 << 20);
    if per >= n {
        dst.copy_from_slice(src);
        return;
    }
    std::thread::scope(|s| {
        for (d, from) in dst.chunks_mut(per).zip(src.chunks(per)) {
            s.spawn(move || d.copy_from_slice(from));
        }
    });
}
