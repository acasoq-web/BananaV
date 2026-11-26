//! Banana Split's effects drawn on the card: their steps
//! (`bv_banana::graph`) run on the pictures where they are, which never
//! leave it.

use std::hash::{Hash as _, Hasher as _};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_banana::Canvas;
use bv_banana::graph::{Graph, Slot};
use bv_core::{Tick, VideoFrame};
use bv_effects::accel;
use bv_media::{Ask, Picture, VideoPool, Want};
use bv_timeline::scene::{FxOwner, FxSource, MediaLayer, PluginCall, Plugins, SceneFx};
use bv_timeline::{Generator, MediaSource, PluginValue};
use wgpu::util::DeviceExt as _;

use super::{Compositor, Img};
use crate::kernels::{Grid, K, Params};

/// Pictures effects got, or will get, at other times than the frame's,
/// kept on the card: an effect that reads the frames before finds them
/// here as they were drawn, and each frame read from the media — to come,
/// or before where playback began — is sent to the card once.
#[derive(Debug, Default)]
pub(super) struct Past {
    kept: Vec<Kept>,
    /// The frames drawn so far.
    frame: u64,
    /// The readers of those not drawn yet, and when they were used last.
    readers: Option<(VideoPool, Instant)>,
    /// What they read lately, as it was decoded (a tenth of the size): an
    /// effect that reads more frames than fit on the card reads each from
    /// its file once.
    decoded: Vec<Decoded>,
}

struct Decoded {
    /// The effect's reader and the media's instant.
    reader: u64,
    media: Tick,
    frame: Arc<VideoFrame>,
    made: Instant,
    used: Instant,
}

impl std::fmt::Debug for Decoded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoded")
            .field("media", &self.media)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Kept {
    /// What it is of ([`bv_timeline::scene::PluginEffects::input_key`]) and
    /// its time there.
    key: u64,
    at: Tick,
    img: Img,
    made: Instant,
    /// The frame that used it last.
    used: u64,
}

impl Past {
    /// At most this many bytes of the card's memory (13 pictures of 1080p),
    /// and this long: a media's file may change.
    const BYTES: u64 = 448 << 20;
    const LASTS: Duration = Duration::from_secs(10);

    /// At most this many bytes of decoded pictures (80 of 1080p).
    const DECODED: usize = 256 << 20;

    fn read(&mut self, reader: u64, media: Tick) -> Option<Arc<VideoFrame>> {
        let d = self
            .decoded
            .iter_mut()
            .find(|d| (d.reader, d.media) == (reader, media) && d.made.elapsed() < Self::LASTS)?;
        d.used = Instant::now();
        Some(Arc::clone(&d.frame))
    }

    fn keep_read(&mut self, reader: u64, media: Tick, frame: &Arc<VideoFrame>) {
        let size = |f: &VideoFrame| {
            f.planes
                .iter()
                .map(|p| super::plane_bytes(&p.data).len())
                .sum::<usize>()
        };
        let mut bytes = size(frame) + self.decoded.iter().map(|d| size(&d.frame)).sum::<usize>();
        while bytes > Self::DECODED
            && let Some(i) = (0..self.decoded.len()).min_by_key(|&i| self.decoded[i].used)
        {
            bytes -= size(&self.decoded.swap_remove(i).frame);
        }
        if bytes <= Self::DECODED {
            let now = Instant::now();
            self.decoded.push(Decoded {
                reader,
                media,
                frame: Arc::clone(frame),
                made: now,
                used: now,
            });
        }
    }

    /// How many pictures of `bytes` there is room for.
    fn room(bytes: u64) -> u64 {
        Self::BYTES / bytes.max(1)
    }

    fn get(&mut self, key: u64, at: Tick, (w, h): (u32, u32)) -> Option<Img> {
        let frame = self.frame;
        let k = self.kept.iter_mut().find(|k| {
            (k.key, k.at) == (key, at)
                && (k.img.w, k.img.h) == (w, h)
                && k.made.elapsed() < Self::LASTS
        })?;
        k.used = frame;
        Some(k.img.clone())
    }

    /// A buffer for a picture `w` × `h` of `key` at `at` from now on: a
    /// picture's no one needs any more, or a new one. `None`: no room but
    /// for pictures this frame uses.
    fn put(
        &mut self,
        device: &wgpu::Device,
        key: u64,
        at: Tick,
        (w, h): (u32, u32),
    ) -> Option<Img> {
        let frame = self.frame;
        let size = (u64::from(w) * u64::from(h) * 16).max(16);
        let old = |k: &Kept| k.made.elapsed() >= Self::LASTS;
        let mut spare: Option<wgpu::Buffer> = None;
        // Those too old, and the same one; then the ones used longest ago.
        loop {
            let kept: u64 = self.kept.iter().map(|k| k.img.buf.size()).sum();
            let over = kept + size > Self::BYTES;
            let gone = (0..self.kept.len())
                .filter(|&i| {
                    let k = &self.kept[i];
                    k.used != frame && (over || old(k) || (k.key, k.at) == (key, at))
                })
                .min_by_key(|&i| (!old(&self.kept[i]), self.kept[i].used));
            let Some(i) = gone else {
                if over {
                    return None;
                }
                break;
            };
            let k = self.kept.swap_remove(i);
            if k.img.buf.size() == size {
                spare = Some(k.img.buf);
            }
        }
        let buf = spare.unwrap_or_else(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("another frame"),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let img = Img { buf, w, h };
        self.kept.push(Kept {
            key,
            at,
            img: img.clone(),
            made: Instant::now(),
            used: frame,
        });
        Some(img)
    }

    /// The frame is drawn; pictures of a while ago are let go.
    pub(super) fn end_frame(&mut self) {
        self.frame += 1;
        self.kept.retain(|k| k.made.elapsed() < Self::LASTS);
        self.decoded.retain(|d| d.made.elapsed() < Self::LASTS);
        if self
            .readers
            .as_ref()
            .is_some_and(|(_, used)| used.elapsed() >= Self::LASTS)
        {
            self.readers = None;
        }
    }
}

/// A plug-in's values as the pack reads them.
fn plain(values: Vec<(String, PluginValue)>) -> Vec<(String, accel::Value)> {
    values
        .into_iter()
        .filter_map(|(name, v)| match v {
            PluginValue::Num(n) => Some((name, accel::Value::Num(n))),
            PluginValue::Text(t) => Some((name, accel::Value::Text(t))),
            _ => None,
        })
        .collect()
}

fn moment(call: &PluginCall<'_>) -> accel::Moment {
    accel::Moment {
        frames: bv_media::plugins::frames(call.time, call.rate),
        rate: call.rate.as_f64(),
        ruler: call.timeline.as_seconds_f64(),
        length: bv_media::plugins::frames(call.duration, call.rate),
    }
}

impl Compositor {
    /// Runs `graph` on `inputs` (its first pictures); what it comes to.
    fn banana(&mut self, enc: &mut wgpu::CommandEncoder, graph: &Graph, inputs: &[&Img]) -> Img {
        let last = graph.last_reads();
        let mut drawn: Vec<Option<Img>> = vec![None; graph.slots as usize];
        for (slot, img) in drawn.iter_mut().zip(inputs) {
            *slot = Some((*img).clone());
        }
        for d in &graph.data {
            let buf = self
                .gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("banana split numbers"),
                    contents: bytemuck::cast_slice(&d.pixels),
                    usage: wgpu::BufferUsages::STORAGE,
                });
            if let Some(slot) = drawn.get_mut(d.slot.0 as usize) {
                let [w, h] = d.size;
                *slot = Some(Img { buf, w, h });
            }
        }
        // A generator alone gets no picture: a pixel stands for it.
        let none = match inputs.first() {
            Some(img) => (*img).clone(),
            None => self.transparent(enc, 1, 1),
        };
        // Pictures no later step reads, drawn over by the next ones.
        let mut spare: Vec<Img> = Vec::new();
        for (n, pass) in graph.passes.iter().enumerate() {
            let (w, h) = (pass.params.w, pass.params.h);
            let out = match spare
                .iter()
                .position(|i| i.buf.size() == (u64::from(w) * u64::from(h) * 16).max(16))
            {
                Some(i) => Img {
                    w,
                    h,
                    ..spare.swap_remove(i)
                },
                None => self.image(w, h),
            };
            let of = |s: Slot| -> &wgpu::Buffer {
                let img = drawn.get(s.0 as usize).and_then(Option::as_ref);
                &img.unwrap_or(&none).buf
            };
            let [a, b, c] = pass.inputs;
            self.gpu.banana.run(
                &self.gpu.device,
                enc,
                pass.kernel,
                &pass.params,
                [of(a), of(b), of(c)],
                &out.buf,
            );
            if let Some(d) = drawn.get_mut(pass.out.0 as usize) {
                *d = Some(out);
            }
            for (i, slot) in drawn.iter_mut().enumerate().skip(graph.inputs as usize) {
                let kept = i == graph.out.0 as usize || i == pass.out.0 as usize;
                if !kept
                    && last[i].is_none_or(|l| l <= n)
                    && let Some(img) = slot.take()
                    && !graph.data.iter().any(|d| d.slot.0 as usize == i)
                {
                    spare.push(img);
                }
            }
        }
        drawn
            .get_mut(graph.out.0 as usize)
            .and_then(Option::take)
            .unwrap_or(none)
    }

    /// [`Compositor::own_effect`] for the pack's effects.
    pub(super) fn banana_effect(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
    ) -> bool {
        let Some(values) = plugins.0.values_at(call).map(plain) else {
            return false;
        };
        let at = moment(call);
        let canvas = Canvas {
            size: [img.w, img.h],
            full: [f64::from(call.full_size.0), f64::from(call.full_size.1)],
        };
        let others = self.other_frames(enc, plugins, call, img, &values, at);
        let Some(graph) = bv_banana::effect_graph(
            &call.fx.id,
            accel::Values(&values),
            canvas,
            at,
            others.len(),
        ) else {
            return false;
        };
        // The interface reads the picture the effect gets (an eyedropper).
        if plugins.0.watched(call)
            && let Some(input) = self.download(enc, img)
        {
            plugins.0.note_input(call, Arc::new(input));
        }
        let inputs: Vec<&Img> = std::iter::once(&*img).chain(others.iter()).collect();
        let out = self.banana(enc, &graph, &inputs);
        // The effect's own picture, never another's kept on the card.
        *img = if graph.out.0 == 0 || graph.out.0 >= graph.inputs {
            out
        } else {
            self.copy(enc, &out)
        };
        true
    }

    /// The other frames `call` reads, as it gets them: `img` where they are
    /// this one, or not to be had (as its plug-in does).
    fn other_frames(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        img: &Img,
        values: &[(String, accel::Value)],
        at: accel::Moment,
    ) -> Vec<Img> {
        let times = bv_banana::other_frames(&call.fx.id, accel::Values(values), at);
        if times.is_empty() {
            return Vec::new();
        }
        let size = (img.w, img.h);
        let key = plugins.0.input_key(call, size);
        // The moments the plug-in host takes the pictures at (none: this
        // frame's), and those kept: marked as used before any is put in
        // another's place.
        let wanted: Vec<(f64, Option<Tick>, Option<Img>)> = times
            .iter()
            .map(|&t| {
                if (t - at.frames).abs() < 1e-3 || call.source.is_none() {
                    return (t, None, None);
                }
                let when = call
                    .rate
                    .snap_nearest(bv_media::plugins::time_of(t, call.rate));
                let kept = key.and_then(|k| self.past.get(k, when, size));
                (t, Some(when), kept)
            })
            .collect();
        let mut others = Vec::with_capacity(times.len());
        for (t, when, kept) in wanted {
            let Some(when) = when else {
                others.push(img.clone());
                continue;
            };
            if let Some(kept) = kept {
                others.push(kept);
                continue;
            }
            // Read and drawn here, as the frame itself is…
            if let Some(drawn) = self.input_on_card(enc, plugins, call, when, size) {
                if let Some(k) = key
                    && let Some(kept) = self.past.put(&self.gpu.device, k, when, size)
                {
                    let bytes = u64::from(img.w) * u64::from(img.h) * 16;
                    enc.copy_buffer_to_buffer(&drawn.buf, 0, &kept.buf, 0, bytes);
                }
                others.push(drawn);
                continue;
            }
            // … or by the processor.
            let picture = plugins
                .0
                .input_at(call, t, size)
                .filter(|p| (p.width, p.height) == size);
            let Some(picture) = picture else {
                others.push(img.clone());
                continue;
            };
            let room = key.and_then(|k| self.past.put(&self.gpu.device, k, when, size));
            others.push(match room {
                Some(kept) => {
                    let bytes = bytemuck::cast_slice(&picture.data);
                    self.gpu.queue.write_buffer(&kept.buf, 0, bytes);
                    kept
                }
                None => self.upload(None, &picture),
            });
        }
        // This frame is one of the next ones' past, where there is room for
        // as many as they look back.
        let back = times
            .iter()
            .fold(0.0f64, |b, t| b.max(at.frames - t))
            .ceil();
        let bytes = u64::from(img.w) * u64::from(img.h) * 16;
        if back >= 1.0
            && (back as u64) < Past::room(bytes)
            && let Some(key) = key
        {
            let now = call.rate.snap_nearest(call.time);
            if self.past.get(key, now, size).is_none()
                && let Some(kept) = self.past.put(&self.gpu.device, key, now, size)
            {
                enc.copy_buffer_to_buffer(&img.buf, 0, &kept.buf, 0, bytes);
            }
        }
        others
    }

    /// `call`'s input at `owner` (its owner's time), `size` big, drawn on
    /// the card as the engine's `Temporal` does on the processor: the media
    /// through the effects before `call`'s. `None`: the processor must (ACES's
    /// working space, a size the reader does not give, another maker's
    /// plug-in before it, nothing there).
    fn input_on_card(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        owner: Tick,
        size: (u32, u32),
    ) -> Option<Img> {
        let s = call.source?;
        if s.enter.is_some() || s.working.is_some() || s.info.color_space.is_some() {
            return None;
        }
        let local = (!s.media_time).then_some(owner);
        let media = match local {
            Some(l) => bv_timeline::scene::media_time(s.clip, s.info, s.clip.start + l)?,
            None => owner,
        };
        let wanted = s.framing.map_or(size, |f| f.media_size);
        let ask = Ask {
            time: media,
            min_size: wanted,
            want: Want::Decoded,
            span: None,
            video_levels: false,
            eight_bit: false,
            encoded: true,
        };
        // Readers of their own for each effect: the frame's are further on
        // or back.
        let reader = {
            let mut h = std::hash::DefaultHasher::new();
            (call.owner, call.index, s.info.id, wanted).hash(&mut h);
            format!("{:?}", s.info.source).hash(&mut h);
            h.finish() >> 1
        };
        let mut img = match self.past.read(reader, media) {
            Some(f) => self.decoded(enc, &f, wanted, None, false),
            None => {
                let (pool, used) = self
                    .past
                    .readers
                    .get_or_insert_with(|| (VideoPool::new(), Instant::now()));
                *used = Instant::now();
                match pool.picture(reader, s.info, ask).ok()? {
                    Picture::Decoded(f) => {
                        self.past.keep_read(reader, media, &f);
                        self.decoded(enc, &f, wanted, None, false)
                    }
                    Picture::Linear(p) => self.upload(None, &p),
                }
            }
        };
        if (img.w, img.h) != wanted {
            return None;
        }
        if s.video_gamma {
            self.kernel(
                enc,
                K::ToVideoGamma,
                Params::sized(img.w, img.h),
                &[&img.buf],
                Grid::Pixels(img.w, img.h),
            );
        }
        // The media's effects on its own picture, then the event's on the
        // framed one.
        let framed_at = s
            .framing
            .map_or(s.before.len(), |f| f.framed_at.min(s.before.len()));
        let (scale, full) = match s.framing {
            Some(f) => (
                f.media_fx_scale,
                s.info
                    .video
                    .as_ref()
                    .map_or(call.full_size, |v| (v.width, v.height)),
            ),
            None => (s.fx_scale, call.full_size),
        };
        let then = (media, local);
        let before = &s.before[..framed_at];
        self.chain_then(
            enc,
            plugins,
            (call, &s),
            before,
            (scale, full),
            then,
            &mut img,
        )?;
        if let Some(f) = s.framing {
            let at = s.clip.start + local.unwrap_or(owner);
            let place = f
                .place
                .at(s.clip, at, [f64::from(size.0), f64::from(size.1)]);
            img = self.place_box(enc, &img, size, &place, f.quality);
            let before = &s.before[framed_at..];
            let of = (s.fx_scale, call.full_size);
            self.chain_then(enc, plugins, (call, &s), before, of, then, &mut img)?;
        }
        Some(img)
    }

    /// `img` through `chain`, the effects before `call`'s, at another time
    /// (the media's, and the event's for an event's effect), their sizes
    /// scaled, their pictures so big at full resolution. `None`: one is not
    /// drawn here.
    #[allow(clippy::too_many_arguments)]
    fn chain_then(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        (call, s): (&PluginCall<'_>, &FxSource<'_>),
        chain: &[SceneFx<'_>],
        (scale, full_size): (f32, (u32, u32)),
        (media, local): (Tick, Option<Tick>),
        img: &mut Img,
    ) -> Option<()> {
        for b in chain.iter().filter(|b| b.fx.enabled) {
            match &b.fx.plugin {
                Some(p) if p.id != call.fx.id => {
                    let (time, duration) = match b.owner {
                        FxOwner::Media(_) => (media, s.info.duration),
                        _ => (local.unwrap_or(media), s.clip.duration),
                    };
                    let then = PluginCall {
                        fx: p,
                        owner: b.owner,
                        index: b.index,
                        time,
                        duration,
                        timeline: time + (call.timeline - call.time),
                        rate: call.rate,
                        full_size,
                        source: None,
                    };
                    if !self.own_effect(enc, plugins, &then, img) {
                        return None;
                    }
                }
                Some(_) => {}
                None => self.effect(enc, img, b.fx, scale),
            }
            if s.clips {
                self.kernel(
                    enc,
                    K::ClipRange,
                    Params::sized(img.w, img.h),
                    &[&img.buf],
                    Grid::Pixels(img.w, img.h),
                );
            }
        }
        Some(())
    }

    /// [`Compositor::own_transition`] for the pack's transitions.
    pub(super) fn banana_transition(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        (from, to): (&Img, &Img),
        progress: f32,
    ) -> Option<Img> {
        if (from.w, from.h) != (to.w, to.h) {
            return None;
        }
        let values = plain(plugins.0.values_at(call)?);
        let canvas = Canvas {
            size: [from.w, from.h],
            full: [f64::from(call.full_size.0), f64::from(call.full_size.1)],
        };
        let graph = bv_banana::transition_graph(
            &call.fx.id,
            accel::Values(&values),
            canvas,
            moment(call),
            f64::from(progress),
        )?;
        let out = self.banana(enc, &graph, &[from, to]);
        // A picture of its own: what it comes from may be changed later.
        Some(if graph.out.0 < graph.inputs {
            self.copy(enc, &out)
        } else {
            out
        })
    }

    /// [`Compositor::own_generator`] for the pack's generators.
    pub(super) fn banana_generator(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        m: &MediaLayer<'_>,
    ) -> Option<Img> {
        let MediaSource::Generated(Generator::Plugin(fx)) = &m.info.source else {
            return None;
        };
        let video = m.info.video.as_ref()?;
        if video.reading != bv_timeline::VideoReading::default() {
            return None;
        }
        let full = (video.width.max(1), video.height.max(1));
        let (w, h) = bv_media::plugins::covering(full, m.min_size);
        let values = plain(bv_media::plugins::generator_values(
            fx,
            video.frame_rate,
            m.time,
        )?);
        let at = accel::Moment {
            frames: video.frame_rate.frame_at(m.time) as f64,
            rate: video.frame_rate.as_f64(),
            ruler: 0.0,
            length: 0.0,
        };
        let canvas = Canvas {
            size: [w, h],
            full: [f64::from(full.0), f64::from(full.1)],
        };
        let graph = bv_banana::generator_graph(&fx.id, accel::Values(&values), canvas, at)?;
        Some(self.banana(enc, &graph, &[]))
    }
}

#[cfg(test)]
#[allow(clippy::print_stdout)]
mod tests {
    use super::*;
    use bv_banana::cpu::{Picture, run};
    use bv_banana::fx::{Frame, Kind, V};
    use bv_core::ImageRgbaF32;

    use crate::{Gpu, GpuError};

    fn unit(x: u32) -> f32 {
        let mut h = x;
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb_352d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x846c_a68b);
        h ^= h >> 16;
        (h >> 8) as f32 / 16_777_216.0
    }

    /// A sky with a sun, hills, edges, grain and a see-through part.
    fn picture(w: u32, h: u32, which: u32) -> Picture {
        let mut p = Picture::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let mut c = [0.25 + 0.5 * v, 0.45 + 0.2 * v, 0.85 - 0.4 * v];
                let sx = if which == 0 { 0.3 } else { 0.7 };
                if ((u - sx) * 1.78).hypot(v - 0.3) < 0.06 {
                    c = [1.0, 0.98, 0.9];
                }
                if v > 0.62 + 0.1 * (u * 7.0 + which as f32 * 2.0).sin() {
                    c = [0.28, 0.4, 0.12];
                }
                if (0.45..0.55).contains(&u) && (0.4..0.8).contains(&v) {
                    c = [0.8, 0.58, 0.46];
                }
                let g = unit(x * 7919 + y * 104_729 + which * 31) * 0.06 - 0.03;
                let a = if u > 0.9 { (1.0 - u) * 10.0 } else { 1.0 };
                p.data[(y * w + x) as usize] = [
                    (c[0] + g).max(0.0) * a,
                    (c[1] + g).max(0.0) * a,
                    (c[2] + g).max(0.0) * a,
                    a,
                ];
            }
        }
        p
    }

    fn rows(data: &mut [[f32; 4]], width: usize, f: &(dyn Fn(usize, &mut [[f32; 4]]) + Sync)) {
        let n = std::thread::available_parallelism().map_or(4, |n| n.get());
        let each = (data.len() / width.max(1)).div_ceil(n).max(1);
        std::thread::scope(|s| {
            for (c, chunk) in data.chunks_mut(width * each).enumerate() {
                s.spawn(move || {
                    for (y, row) in chunk.chunks_mut(width).enumerate() {
                        f(c * each + y, row);
                    }
                });
            }
        });
    }

    /// A compositor on the best card (`WGPU_BACKEND=vulkan` picks the
    /// interface), if there is one.
    fn compositor() -> Option<Compositor> {
        match Gpu::open(None) {
            Ok(g) => {
                println!("{:?}", g.adapter());
                Some(Compositor::new(g))
            }
            Err(GpuError::NoAdapter) if std::env::var_os("BV_REQUIRE_GPU").is_none() => {
                println!("no graphics card: skipped");
                None
            }
            Err(e) => panic!("the graphics card did not open: {e}"),
        }
    }

    /// `fx` with settings `v` drawn on the card, and on the processor when
    /// asked.
    fn drawn(
        c: &mut Compositor,
        fx: &bv_banana::fx::Fx,
        v: &V<'_>,
        too: bool,
    ) -> (ImageRgbaF32, Option<Picture>) {
        let (w, h) = (192, 108);
        let mut at = Frame {
            w,
            h,
            scale: 0.4,
            shape: w as f32 / h as f32,
            frame: 31.0,
            rate: 30.0,
            length: 60.0,
            progress: 0.43,
            alone: false,
            others: 0,
        };
        let mut inputs = vec![picture(w, h, 0)];
        if fx.kind == Kind::Transition {
            inputs.push(picture(w, h, 1));
        } else {
            at.others = fx.times(v, at.frame).len();
            inputs.extend((0..at.others).map(|k| picture(w, h, 1 + k as u32)));
        }
        let g = fx.graph(v, at);
        let refs: Vec<&Picture> = inputs.iter().collect();
        let cpu = too.then(|| run(&g, &refs, &rows));
        let mut enc = c
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let sent: Vec<Img> = inputs
            .iter()
            .map(|p| {
                let mut i = ImageRgbaF32::transparent(p.w, p.h);
                i.data.clone_from(&p.data);
                c.upload(None, &Arc::new(i))
            })
            .collect();
        let refs: Vec<&Img> = sent.iter().collect();
        let out = c.banana(&mut enc, &g, &refs);
        let card = c.download(&mut enc, &out).expect("read back");
        c.end_frame();
        (card, cpu)
    }

    /// Every effect of the pack, at its defaults and each preset, draws on
    /// the card as on the processor.
    #[test]
    fn the_pack_draws_alike_on_the_processor_and_on_the_card() {
        let Some(mut c) = compositor() else { return };
        let mut differing = Vec::new();
        let mut worst = (0.0f64, String::new());
        for fx in bv_banana::all() {
            for n in std::iter::once(None).chain((0..fx.presets.len()).map(Some)) {
                let v = V::of(fx, |p| {
                    let values = fx.presets.get(n?)?.values;
                    let v = values.iter().find(|(name, _)| *name == p.name)?.1;
                    Some(std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0)))
                });
                let (card, cpu) = drawn(&mut c, fx, &v, true);
                let cpu = cpu.expect("drawn on the processor");
                let (mut off, mut sum) = (0usize, 0f64);
                for (x, y) in cpu.data.iter().zip(&card.data) {
                    let d = (0..4).map(|k| (x[k] - y[k]).abs()).fold(0f32, f32::max);
                    // A place read a hair to the other side of an edge.
                    if d.is_nan() || d > 0.02 {
                        off += 1;
                    } else {
                        sum += f64::from(d);
                    }
                }
                let total = cpu.data.len();
                let mean = sum / (total - off).max(1) as f64;
                let name = n.map_or("", |n| fx.presets[n].name);
                if mean > worst.0 {
                    worst = (mean, format!("{} {name}", fx.id));
                }
                if off * 100 > total || mean > 4e-4 {
                    differing.push(format!(
                        "{} {name}: {off} pixels, the rest by {mean:.1e}",
                        fx.id
                    ));
                }
            }
        }
        println!("the rest differ by {:.1e} at most ({})", worst.0, worst.1);
        assert!(differing.is_empty(), "{differing:#?}");
    }

    /// Settings at their ends draw numbers on the card too: what is none on
    /// a card spreads as black.
    #[test]
    fn settings_at_their_ends_draw_numbers_on_the_card() {
        use bv_banana::fx::Shape;
        let Some(mut c) = compositor() else { return };
        let mut wrong = Vec::new();
        for fx in bv_banana::all() {
            for high in [false, true] {
                let v = V::of(fx, |p| {
                    Some(match p.shape {
                        Shape::Num { hard, .. } => [hard[usize::from(high)], 0.0, 0.0],
                        Shape::Int { lo, hi } => [f64::from(if high { hi } else { lo }), 0.0, 0.0],
                        Shape::Choice(o) => {
                            [if high { o.len() as f64 - 1.0 } else { 0.0 }, 0.0, 0.0]
                        }
                        Shape::Toggle => [f64::from(u8::from(high)), 0.0, 0.0],
                        Shape::Angle => [if high { 1e5 } else { -1e5 }, 0.0, 0.0],
                        Shape::Point => [if high { 50.0 } else { -50.0 }; 3],
                        Shape::Color => [if high { 8.0 } else { 0.0 }; 3],
                        _ => return None,
                    })
                });
                let (card, _) = drawn(&mut c, fx, &v, false);
                let bad = card
                    .data
                    .iter()
                    .filter(|c| c.iter().any(|v| !v.is_finite()))
                    .count();
                if bad > 0 {
                    let end = if high { "most" } else { "least" };
                    wrong.push(format!("{} at its {end}: {bad} pixels", fx.id));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
