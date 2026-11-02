//! BananaV's own effects drawn on the card: the plug-ins' math as kernels
//! (`shaders/own_*.wgsl`), from what `bv_effects::accel` works out of the
//! values their instances have at the frame's time — the picture stays on
//! the card. Others' plug-ins run on the processor.

use std::sync::Arc;

use bv_effects::accel::{self, Plan};
use bv_timeline::scene::{MediaLayer, PluginCall, Plugins};
use bv_timeline::{Generator, MediaSource, PluginValue};

use super::{Compositor, Img};
use crate::kernels::{Grid, K, Params};

impl Compositor {
    /// Draws `call` on the card when it is one of BananaV's own effects
    /// (whether it did).
    pub(super) fn own_effect(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
    ) -> bool {
        self.own_effect_from(enc, plugins, call, img, None)
    }

    /// [`Self::own_effect`], `start` the picture `call`'s chain started
    /// from when the card keeps it: Bézier Masking's Mask FX draws there
    /// only with it.
    pub(super) fn own_effect_from(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        img: &mut Img,
        start: Option<&Img>,
    ) -> bool {
        if !call.fx.id.starts_with(bv_effects::ID_PREFIX) {
            return false;
        }
        let Some(values) = plugins.0.values_at(call) else {
            return false;
        };
        let values: Vec<(String, accel::Value)> = values
            .into_iter()
            .filter_map(|(name, v)| {
                let v = match v {
                    PluginValue::Num(n) => accel::Value::Num(n),
                    PluginValue::Curves(c) => accel::Value::Curves(c),
                    PluginValue::Text(t) => accel::Value::Text(t),
                    PluginValue::Bytes(_) => return None,
                };
                Some((name, v))
            })
            .collect();
        let full = [call.full_size.0 as f64, call.full_size.1 as f64];
        let at = accel::Moment {
            frames: bv_media::plugins::frames(call.time, call.rate),
            rate: call.rate.as_f64(),
            ruler: call.timeline.as_seconds_f64(),
            length: bv_media::plugins::frames(call.duration, call.rate),
        };
        let plan = if accel::reads_other_frames(&call.fx.id, accel::Values(&values)) {
            // The frame's picture as the card has it, the others from the
            // host, as the plug-in gets them.
            let Some(current) = self.download(enc, img) else {
                return false;
            };
            let size = (img.w, img.h);
            let input = |t: f64| plugins.0.input_at(call, t, size);
            let number = |name: &str, t: f64| plugins.0.numbers_at(call, name, t);
            accel::temporal_plan(
                &call.fx.id,
                accel::Values(&values),
                at,
                &current,
                &input,
                &number,
            )
        } else {
            accel::plan(&call.fx.id, accel::Values(&values), full, at)
        };
        let Some(plan) = plan else {
            return false;
        };
        if matches!(plan, Plan::Masks { mask_fx: true, .. }) && start.is_none() {
            return false;
        }
        // A table bigger than the card takes in one piece: the processor.
        let max = self.gpu.device.limits().max_storage_buffer_binding_size;
        let too_big = |lut: &bv_lut::Lut| lut_entries(lut) as u64 * 16 > max;
        if let Plan::Lut { lut: Some(lut), .. } = &plan
            && too_big(lut)
        {
            return false;
        }
        if let Plan::Grade(g) = &plan
            && [&g.camera, &g.look]
                .into_iter()
                .any(|l| matches!(l, Some(accel::GradeLut::Cube { lut, .. }) if too_big(lut)))
        {
            return false;
        }
        // The interface reads the picture the effect gets (an eyedropper).
        if plugins.0.watched(call)
            && let Some(input) = self.download(enc, img)
        {
            plugins.0.note_input(call, Arc::new(input));
        }
        let scale = img.w as f64 / full[0].max(1.0);
        match plan {
            Plan::Masks { .. } => self.masks(enc, &plan, img, scale, start),
            _ => self.own(enc, &plan, img, full, scale),
        }
        true
    }

    /// Bézier Masking: how much of each pixel stays, from the masks'
    /// outlines (a table: each mask's place in it, its weight and where
    /// its edge softens, then the points), the picture times it — or, with
    /// Mask FX, the effects before it over the chain's input `start` by it.
    fn masks(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plan: &Plan,
        img: &Img,
        scale: f64,
        start: Option<&Img>,
    ) {
        let Plan::Masks {
            shapes,
            any,
            invert,
            mask_fx,
            blend,
        } = plan
        else {
            return;
        };
        let (w, h) = (img.w, img.h);
        let shapes = &shapes[..shapes.len().min(5)];
        let mut table = vec![[0.0f32; 4]; 10];
        let mut first = table.len();
        for (m, shape) in shapes.iter().enumerate() {
            let [lo, hi] = accel::mask_softening(f64::from(shape.feather) * scale, shape.style);
            table[m] = [
                first as f32,
                shape.outline.len() as f32,
                shape.weight,
                lo as f32,
            ];
            table[5 + m][0] = hi as f32;
            first += shape.outline.len();
        }
        for shape in shapes {
            table.extend(shape.outline.iter().map(|p| [p[0], p[1], 0.0, 0.0]));
        }
        let entries = self.buffer(table.len() as u64 * 16);
        self.gpu
            .queue
            .write_buffer(&entries, 0, bytemuck::cast_slice(&table));
        let mut p = Params::sized(w, h).with_f(&[*blend, scale as f32]);
        (p.sw, p.sh) = (table.len() as u32, 1);
        p.i = [*any as i32, 0, 0, 0];
        p.j = [
            shapes.len() as u32,
            *invert as u32,
            *mask_fx as u32,
            start.is_some() as u32,
        ];
        match start.filter(|_| *mask_fx) {
            Some(start) => {
                let amount = self.image(w, h);
                p.j[3] = 1;
                self.kernel(
                    enc,
                    K::BezierMasking,
                    p,
                    &[&entries, &amount.buf],
                    Grid::Pixels(w, h),
                );
                self.kernel(
                    enc,
                    K::BezierMaskFx,
                    Params::sized(w, h),
                    &[&amount.buf, &start.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            None => {
                p.j[3] = 0;
                self.kernel(
                    enc,
                    K::BezierMasking,
                    p,
                    &[&entries, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
        }
    }

    /// The picture one of BananaV's own generators draws for `m`, drawn
    /// here at the size and time the processor would draw it (`None`:
    /// another maker's generator, or one the processor draws).
    pub(super) fn own_generator(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        m: &MediaLayer<'_>,
    ) -> Option<Img> {
        let MediaSource::Generated(Generator::Plugin(fx)) = &m.info.source else {
            return None;
        };
        if !fx.id.starts_with(bv_effects::ID_PREFIX) {
            return None;
        }
        let video = m.info.video.as_ref()?;
        let full = (video.width.max(1), video.height.max(1));
        let (w, h) = bv_media::plugins::covering(full, m.min_size);
        let values: Vec<(String, accel::Value)> =
            bv_media::plugins::generator_values(fx, video.frame_rate, m.time)?
                .into_iter()
                .filter_map(|(name, v)| {
                    let v = match v {
                        PluginValue::Num(n) => accel::Value::Num(n),
                        PluginValue::Curves(c) => accel::Value::Curves(c),
                        PluginValue::Text(t) => accel::Value::Text(t),
                        PluginValue::Bytes(_) => return None,
                    };
                    Some((name, v))
                })
                .collect();
        let Plan::Generate { kernel, f, j } =
            accel::generator_plan(&fx.id, accel::Values(&values), full, (w, h))?
        else {
            return None;
        };
        let img = self.image(w, h);
        let mut p = Params::sized(w, h).with_f(&f);
        p.j = j;
        let k = match kernel {
            accel::GenerateKernel::Checkerboard => K::KitCheckerboard,
            accel::GenerateKernel::TestPattern => K::KitTestPattern,
            accel::GenerateKernel::NoiseTexture => K::KitNoiseTexture,
            accel::GenerateKernel::SolidColor => K::Fill,
        };
        self.kernel(enc, k, p, &[&img.buf], Grid::Pixels(w, h));
        Some(img)
    }

    /// Draws `call` on the card when it is one of BananaV's own
    /// transitions with a kernel (`None`: the plug-in draws it on the
    /// processor), from `from` to `to` at `progress`.
    pub(super) fn own_transition(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        (from, to): (&Img, &Img),
        progress: f32,
    ) -> Option<Img> {
        if !call.fx.id.starts_with(bv_effects::ID_PREFIX) {
            return None;
        }
        let values: Vec<(String, accel::Value)> = plugins
            .0
            .values_at(call)?
            .into_iter()
            .filter_map(|(name, v)| match v {
                PluginValue::Num(n) => Some((name, accel::Value::Num(n))),
                PluginValue::Text(t) => Some((name, accel::Value::Text(t))),
                _ => None,
            })
            .collect();
        if call.fx.id == accel::GL_TRANSITION_ID {
            // The code made a kernel, run on the pictures as the card has
            // them (none: the processor runs it).
            if from.w != to.w || from.h != to.h {
                return None;
            }
            let (w, h) = (from.w, from.h);
            let (wgsl, f) =
                accel::gl_transition_card(accel::Values(&values), f64::from(progress), w, h)?;
            let out = self.image(w, h);
            let p = Params::sized(w, h).with_f(&f);
            let ran = self.gpu.kernels.run_made(
                &self.gpu.device,
                enc,
                (crate::kernels::Sig::Triple, &wgsl, "glt_main"),
                &p,
                &[&from.buf, &to.buf, &out.buf],
                Grid::Pixels(w, h),
            );
            return ran.then_some(out);
        }
        if call.fx.id == accel::WARP_FLOW_ID {
            // The flow worked out on the processor from the pictures as
            // the card has them, the frame between drawn here.
            if from.w != to.w || from.h != to.h {
                return None;
            }
            let a = self.download(enc, from)?;
            let b = self.download(enc, to)?;
            let between =
                accel::warp_flow_card(accel::Values(&values), &a, &b, f64::from(progress));
            let out = self.image(from.w, from.h);
            self.flow_between(enc, (from, to), &between, &out);
            return Some(out);
        }
        let full = [call.full_size.0 as f64, call.full_size.1 as f64];
        let (w, h) = (from.w, from.h);
        let plan = accel::transition_plan(
            &call.fx.id,
            accel::Values(&values),
            full,
            [f64::from(w), f64::from(h)],
            f64::from(progress),
        )?;
        let out = self.image(w, h);
        let mut p = Params::sized(w, h).with_f(&plan.f);
        p.i = plan.i;
        p.j = plan.j;
        let k = transition_kernel(plan.kernel);
        // The pictures as the kernel reads them, and a blur after it.
        let (mut first, mut second) = (None, None);
        let mut after = None;
        match &plan.steps {
            accel::TransitionSteps::None => {}
            accel::TransitionSteps::Light { second: lit, blur } => {
                // The picture lighting, and its light beside it.
                let base = if *lit { to } else { from };
                let light = blur.map(|taps| self.recursive_gauss(enc, base, &taps));
                let l = light.as_ref().unwrap_or(base);
                self.kernel(
                    enc,
                    k,
                    p,
                    &[&base.buf, &l.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                return Some(out);
            }
            accel::TransitionSteps::Cells {
                pictures,
                counts,
                table,
            } => {
                let entries = self.buffer(table.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&entries, 0, bytemuck::cast_slice(table));
                if pictures & 1 != 0 {
                    first = Some(self.cells(enc, from, *counts, &entries));
                }
                if pictures & 2 != 0 {
                    second = Some(self.cells(enc, to, *counts, &entries));
                }
            }
            accel::TransitionSteps::Table { table } => {
                // The two pictures and the kernel's table.
                let steps = self.buffer(table.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&steps, 0, bytemuck::cast_slice(table));
                self.kernel(
                    enc,
                    k,
                    p,
                    &[&from.buf, &to.buf, &steps, &out.buf],
                    Grid::Pixels(w, h),
                );
                return Some(out);
            }
            accel::TransitionSteps::Cards { under, table } => {
                // What is under the cards, then the cards over it.
                match under {
                    1 | 2 => {
                        let src = if *under == 1 { from } else { to };
                        self.kernel(
                            enc,
                            K::CopyInto,
                            Params::sized(w, h),
                            &[&src.buf, &out.buf],
                            Grid::Pixels(w, h),
                        );
                    }
                    _ => self.kernel(
                        enc,
                        K::Fill,
                        Params::sized(w, h),
                        &[&out.buf],
                        Grid::Pixels(w, h),
                    ),
                }
                let cards = self.buffer(table.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&cards, 0, bytemuck::cast_slice(table));
                self.kernel(
                    enc,
                    k,
                    p,
                    &[&from.buf, &to.buf, &cards, &out.buf],
                    Grid::Pixels(w, h),
                );
                return Some(out);
            }
            accel::TransitionSteps::Blur { pictures, taps } => {
                if pictures & 1 != 0 {
                    first = Some(self.recursive_gauss(enc, from, taps));
                }
                if pictures & 2 != 0 {
                    second = Some(self.recursive_gauss(enc, to, taps));
                }
                if pictures & 4 != 0 {
                    after = Some(*taps);
                }
            }
        }
        let a = first.as_ref().unwrap_or(from);
        let b = second.as_ref().unwrap_or(to);
        self.kernel(enc, k, p, &[&a.buf, &b.buf, &out.buf], Grid::Pixels(w, h));
        Some(match after {
            Some(taps) => self.recursive_gauss(enc, &out, &taps),
            None => out,
        })
    }

    /// `img` blurred as VEGAS blurs (`bv_effects`' `recursive_gauss`):
    /// Deriche's filter across, `taps[0]`, then down, `taps[1]`.
    fn recursive_gauss(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        taps: &[[f32; 16]; 2],
    ) -> Img {
        let (w, h) = (img.w, img.h);
        let across = self.image(w, h);
        let p = Params::sized(w, h).with_f(&taps[0]);
        self.kernel(
            enc,
            K::KitGaussRows,
            p,
            &[&img.buf, &across.buf],
            Grid::Lines(h),
        );
        let down = self.image(w, h);
        let p = Params::sized(w, h).with_f(&taps[1]);
        self.kernel(
            enc,
            K::KitGaussColumns,
            p,
            &[&across.buf, &down.buf],
            Grid::Lines(w),
        );
        down
    }

    /// Cross Effect's pixelation of `img`: each pixel its cell's average,
    /// the cells as `table` lays them (`counts` across and down).
    fn cells(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        counts: [u32; 2],
        table: &wgpu::Buffer,
    ) -> Img {
        let (w, h) = (img.w, img.h);
        let [nx, ny] = counts.map(|c| c.max(1));
        let sums = self.image(nx, h);
        let mut p = Params::sized(nx, h);
        (p.sw, p.sh) = (w, h);
        p.i = [nx as i32, ny as i32, 0, 0];
        self.kernel(
            enc,
            K::TrCellRows,
            p,
            &[&img.buf, table, &sums.buf],
            Grid::Pixels(nx, h),
        );
        let cells = self.image(nx, ny);
        let mut p = Params::sized(nx, ny);
        (p.sw, p.sh) = (nx, h);
        p.i = [nx as i32, ny as i32, 0, 0];
        self.kernel(
            enc,
            K::TrCells,
            p,
            &[&sums.buf, table, &cells.buf],
            Grid::Pixels(nx, ny),
        );
        let out = self.image(w, h);
        let mut p = Params::sized(w, h);
        p.i = [nx as i32, ny as i32, 0, 0];
        self.kernel(
            enc,
            K::TrCellFill,
            p,
            &[&cells.buf, table, &out.buf],
            Grid::Pixels(w, h),
        );
        out
    }

    /// Draws a Custom compositing mode's `call` on the card when it is one
    /// of BananaV's own compositors (`None`: the processor draws it):
    /// `under` as `top` guides it, the two swapped by Flip Input Surfaces.
    pub(super) fn own_compositor(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plugins: Plugins<'_>,
        call: &PluginCall<'_>,
        top: &Img,
        under: &Img,
    ) -> Option<Img> {
        if !call.fx.id.starts_with(bv_effects::ID_PREFIX) {
            return None;
        }
        let values: Vec<(String, accel::Value)> = plugins
            .0
            .values_at(call)?
            .into_iter()
            .filter_map(|(name, v)| match v {
                PluginValue::Num(n) => Some((name, accel::Value::Num(n))),
                _ => None,
            })
            .collect();
        let full = [call.full_size.0 as f64, call.full_size.1 as f64];
        let plan = accel::compositor_plan(&call.fx.id, accel::Values(&values), full)?;
        let (picture, map) = if plan.flip {
            (top, under)
        } else {
            (under, top)
        };
        let (w, h) = (picture.w, picture.h);
        let out = self.image(w, h);
        let mut p = Params::sized(w, h).with_f(&plan.f);
        p.j = plan.j;
        let grid = Grid::Pixels(w, h);
        match plan.kernel {
            accel::CompositorKernel::Displacement => {
                // The scales' reach in pixels, as the processor's.
                p = p.with_f(&[plan.f[0] * w as f32, plan.f[1] * h as f32]);
                let bufs = [&picture.buf, &map.buf, &out.buf];
                self.kernel(enc, K::KitDisplace, p, &bufs, grid);
            }
            accel::CompositorKernel::BumpMap => {
                let bufs = [&picture.buf, &map.buf, &out.buf];
                self.kernel(enc, K::KitBumpWith, p, &bufs, grid);
            }
            accel::CompositorKernel::Height => {
                // The map's heights in fixed point, smoothed by a box four
                // times each way as the processor's (integer running sums,
                // the edge values held), then the picture moved along
                // their slopes.
                let n = accel::height_numbers(&plan, w, h)?;
                let mut heights = self.image(w, h);
                let mut q = p;
                q.i[0] = n.bits as i32;
                let bufs = [&map.buf, &map.buf, &heights.buf];
                self.kernel(enc, K::KitHeights, q, &bufs, grid);
                let mut b = Params::sized(w, h);
                b.j = [n.radius, 0, 0, 0];
                for _ in 0..accel::HEIGHT_PASSES {
                    for (k, lines) in [(K::KitBoxRows, h), (K::KitBoxColumns, w)] {
                        let next = self.image(w, h);
                        self.kernel(enc, k, b, &[&heights.buf, &next.buf], Grid::Lines(lines));
                        heights = next;
                    }
                }
                let q = p.with_f(&[n.unit, n.scale, n.depth]);
                let bufs = [&picture.buf, &heights.buf, &out.buf];
                self.kernel(enc, K::KitHeightMap, q, &bufs, grid);
            }
        }
        Some(out)
    }

    /// One of Color Grading's LUTs over `img`.
    fn grade_lut(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        lut: Option<&accel::GradeLut>,
        img: &mut Img,
        full: [f64; 2],
        scale: f64,
    ) {
        match lut {
            Some(accel::GradeLut::Cube {
                lut,
                tetrahedral,
                strength,
            }) => {
                let plan = Plan::Lut {
                    lut: Some(lut.clone()),
                    tetrahedral: *tetrahedral,
                    strength: *strength,
                };
                self.own(enc, &plan, img, full, scale);
            }
            Some(accel::GradeLut::AppleLog { video, strength }) => {
                let (w, h) = (img.w, img.h);
                let mut p = Params::sized(w, h).with_f(&[*strength]);
                p.j[0] = u32::from(*video);
                self.kernel(enc, K::AppleLog, p, &[&img.buf], Grid::Pixels(w, h));
            }
            None => {}
        }
    }

    /// Draws `plan` over `img`, a frame `full` canonical units big at render
    /// scale `scale`.
    fn own(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        plan: &Plan,
        img: &mut Img,
        full: [f64; 2],
        scale: f64,
    ) {
        let (w, h) = (img.w, img.h);
        let s = scale as f32;
        match plan {
            Plan::Crop {
                pivot,
                lo,
                hi,
                turn,
                opacity,
            } => {
                let p = Params::sized(w, h).with_f(&[
                    pivot[0], pivot[1], lo[0], lo[1], hi[0], hi[1], turn[0], turn[1], *opacity, s,
                ]);
                self.kernel(enc, K::CropWindow, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Corrector {
                shifts,
                saturation,
                power,
                gain,
                offset,
            } => {
                let p = Params::sized(w, h).with_f(&[
                    shifts[0][0],
                    shifts[0][1],
                    shifts[1][0],
                    shifts[1][1],
                    shifts[2][0],
                    shifts[2][1],
                    *saturation,
                    *power,
                    *gain,
                    *offset,
                ]);
                self.kernel(enc, K::ColorCorrector, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Cookie(c) => {
                let mut p = Params::sized(w, h).with_f(&[
                    c.color[0],
                    c.color[1],
                    c.color[2],
                    c.feather,
                    c.border,
                    c.size,
                    c.center[0],
                    c.center[1],
                    full[0] as f32,
                    full[1] as f32,
                    s,
                ]);
                p.j = [c.shape, c.quarters, c.hole as u32, 0];
                p.i = [c.repeat[0] as i32, c.repeat[1] as i32, 0, 0];
                self.kernel(enc, K::CookieCutter, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Grade(g) => {
                self.grade_lut(enc, g.camera.as_ref(), img, full, scale);
                let table = self.buffer(g.steps.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&g.steps));
                let mut p = Params::sized(w, h);
                p.sw = g.entries;
                p.sh = 1;
                self.kernel(enc, K::Grade, p, &[&table, &img.buf], Grid::Pixels(w, h));
                self.grade_lut(enc, g.look.as_ref(), img, full, scale);
            }
            Plan::Curves(tables) => {
                // The three tables side by side: entry k's red, green and
                // blue.
                let n = accel::TABLE;
                let mut entries = Vec::with_capacity(n * 4);
                for k in 0..n {
                    let at = |c: usize| tables[c].get(k).copied().unwrap_or(0.0);
                    entries.extend_from_slice(&[at(0), at(1), at(2), 0.0]);
                }
                let table = self.buffer(entries.len() as u64 * 4);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&entries));
                let mut p = Params::sized(w, h);
                p.sw = n as u32;
                p.sh = 1;
                self.kernel(enc, K::Curves, p, &[&table, &img.buf], Grid::Pixels(w, h));
            }
            Plan::Layer(l) => {
                let out = self.layer(enc, l, img, scale);
                *img = out;
            }
            Plan::Pixelate { cells } => self.pixelate(enc, *cells, img),
            Plan::Warp {
                inverse,
                samples,
                opacity,
            } => {
                let out = self.image(w, h);
                let mut f = inverse.to_vec();
                f.push(*opacity);
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [*samples, 0, 0, 0];
                self.kernel(
                    enc,
                    K::PictureInPicture,
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Hsl {
                hue,
                saturation,
                luminance,
            } => {
                if *hue == 0.0 && *saturation == 1.0 && *luminance == 1.0 {
                    return;
                }
                let p = Params::sized(w, h).with_f(&[*hue, *saturation, *luminance]);
                self.kernel(enc, K::HslAdjust, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Fill {
                background,
                background_samples,
                background_blur,
                foreground,
                foreground_samples,
                shadow_sigma,
                shadow,
            } => {
                let blur = accel::fill_background_sections(*background_blur, full, w, h);
                let out = self.fill(
                    enc,
                    img,
                    [
                        (background, *background_samples),
                        (foreground, *foreground_samples),
                    ],
                    (blur, *shadow_sigma * scale),
                    *shadow,
                );
                *img = out;
            }
            Plan::Border {
                kind,
                frame,
                width,
                sigma,
                depth,
                shades,
                color,
            } => {
                if *width <= 0.0 {
                    return;
                }
                let mut f = vec![frame[0], frame[1], *width, *sigma, depth[0], depth[1]];
                for m in shades {
                    f.extend_from_slice(m);
                }
                f.extend_from_slice(color);
                f.push(s);
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [*kind, 0, 0, 0];
                self.kernel(enc, K::Border, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Secondary {
                limits,
                flags,
                turn,
                shift,
                saturation,
                power,
                gain,
                offset,
                alpha,
            } => {
                let mut f: Vec<f32> = limits.concat();
                f.extend_from_slice(turn);
                f.extend_from_slice(shift);
                f.extend_from_slice(&[*saturation, *power, *gain, *offset, *alpha]);
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [*flags, 0, 0, 0];
                self.kernel(enc, K::ColorSecondary, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Balance { shifts, range } => {
                if *shifts == [0.0; 3] {
                    return;
                }
                let mut p = Params::sized(w, h).with_f(shifts);
                p.j = [*range, 0, 0, 0];
                self.kernel(enc, K::ColorBalance, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Saturation {
                amount,
                center,
                spread,
                low,
                high,
            } => {
                if *amount == 0.0 && *low == 0.0 && *high == 0.0 {
                    return;
                }
                let p = Params::sized(w, h).with_f(&[*amount, *center, *spread, *low, *high]);
                self.kernel(enc, K::SaturationAdjust, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Affine {
                matrix: m,
                offset: o,
                equalize: true,
            } => self.equalize(enc, m, o, img),
            Plan::Affine {
                matrix: m,
                offset: o,
                equalize: false,
            } => {
                let identity = (0..3).all(|i| {
                    o[i].abs() < 1e-7
                        && (0..3).all(|j| (m[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-7)
                });
                if identity {
                    return;
                }
                let p = Params::sized(w, h).with_f(&[
                    m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1],
                    m[2][2], o[0], o[1], o[2],
                ]);
                self.kernel(enc, K::ColorMatrix, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Unsharp {
                radius,
                amount,
                threshold,
            } => {
                if *radius == 0.0 {
                    return;
                }
                // The picture blurred as VEGAS blurs, its edges held.
                let taps = accel::unsharp_sections(*radius, full, w, h);
                let blurred = self.recursive_gauss(enc, img, &taps);
                let p = Params::sized(w, h).with_f(&[*amount, *threshold]);
                self.kernel(
                    enc,
                    K::UnsharpMask,
                    p,
                    &[&blurred.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Noise {
                width,
                monochromatic,
                gaussian,
                seed,
            } => {
                if *width <= 0.0 {
                    return;
                }
                let mut p = Params::sized(w, h).with_f(&[*width]);
                p.j = [*monochromatic as u32, *gaussian as u32, *seed, 0];
                self.kernel(enc, K::AddNoise, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Masks { .. } => self.masks(enc, plan, img, scale, None),
            Plan::Frame { picture } => {
                if picture.width == w && picture.height == h {
                    self.gpu
                        .queue
                        .write_buffer(&img.buf, 0, bytemuck::cast_slice(&picture.data));
                }
            }
            Plan::FlowBetween {
                first,
                second,
                between,
            } => {
                if first.width != w || first.height != h || second.width != w || second.height != h
                {
                    return;
                }
                let a = self.image(w, h);
                let b = self.image(w, h);
                self.gpu
                    .queue
                    .write_buffer(&a.buf, 0, bytemuck::cast_slice(&first.data));
                self.gpu
                    .queue
                    .write_buffer(&b.buf, 0, bytemuck::cast_slice(&second.data));
                self.flow_between(enc, (&a, &b), between, img);
            }
            Plan::MotionBlur { mask, kernel, size } => {
                if mask.len() != w as usize * h as usize {
                    return;
                }
                let cells = scalar_cells(kernel);
                let prep = || FftPrep {
                    reflect: true,
                    bloom: None,
                    smooth: false,
                    threshold: None,
                    edges: None,
                };
                let blurred = self.image(w, h);
                self.fft_convolve(enc, img, img, &blurred, &cells, *size, prep(), None);
                let moving = self.image(w, h);
                self.gpu
                    .queue
                    .write_buffer(&moving.buf, 0, bytemuck::cast_slice(mask));
                let spread = self.image(w, h);
                self.fft_convolve(enc, &moving, &moving, &spread, &cells, *size, prep(), None);
                self.kernel(
                    enc,
                    K::KitMotionBlurPick,
                    Params::sized(w, h),
                    &[&blurred.buf, &spread.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Denoise { strengths, center } => {
                if strengths.iter().any(|&h| h > 0.0) {
                    let reach = accel::denoise_reach();
                    let (pw, ph) = (w + 2 * reach, h + 2 * reach);
                    let padded = self.image(pw, ph);
                    let mut p = Params::sized(pw, ph);
                    (p.sw, p.sh) = (w, h);
                    self.kernel(
                        enc,
                        K::KitDenoisePlanes,
                        p,
                        &[&img.buf, &padded.buf],
                        Grid::Pixels(pw, ph),
                    );
                    // The planes as they are, then each one cleaned.
                    let planes = self.image(w, h);
                    let mut p = Params::sized(w, h);
                    (p.sw, p.sh) = (pw, ph);
                    p.i = [-(reach as i32), -(reach as i32), 0, 0];
                    self.kernel(
                        enc,
                        K::CopyInto,
                        p,
                        &[&padded.buf, &planes.buf],
                        Grid::Pixels(w, h),
                    );
                    for (k, strength) in [(0, strengths[0]), (1, strengths[1]), (2, strengths[1])] {
                        if strength <= 0.0 {
                            continue;
                        }
                        let mut p = Params::sized(w, h).with_f(&[strength]);
                        (p.sw, p.sh) = (pw, ph);
                        p.j = [k, 0, 0, 0];
                        self.kernel(
                            enc,
                            K::KitDenoiseNlm,
                            p,
                            &[&padded.buf, &planes.buf],
                            Grid::Pixels(w, h),
                        );
                    }
                    self.kernel(
                        enc,
                        K::KitDenoiseRgb,
                        Params::sized(w, h),
                        &[&planes.buf, &img.buf],
                        Grid::Pixels(w, h),
                    );
                }
                if *center != 0 {
                    let before = self.copy(enc, img);
                    let p = Params::sized(w, h).with_f(&[*center as f32]);
                    self.kernel(
                        enc,
                        K::KitDenoiseSharpen,
                        p,
                        &[&before.buf, &img.buf],
                        Grid::Pixels(w, h),
                    );
                }
            }
            Plan::FlickerAdd {
                amount,
                tones,
                mask,
            } => {
                let mut p = Params::sized(w, h).with_f(&[*amount]);
                p.j = [*tones as u32, u32::from(*mask), 0, 0];
                self.kernel(enc, K::KitFlickerAdd, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::FlickerMatch { table, range, mask } => {
                // The table a level a pixel: its red, green and blue matches.
                let levels = bv_effects::flicker_control::LEVELS;
                let (lut, range) = match table {
                    Some(t) if t.len() == 3 * levels => {
                        let px: Vec<[f32; 4]> = (0..levels)
                            .map(|l| [t[l], t[levels + l], t[2 * levels + l], 0.0])
                            .collect();
                        let lut = self.image(levels as u32, 1);
                        self.gpu
                            .queue
                            .write_buffer(&lut.buf, 0, bytemuck::cast_slice(&px));
                        (lut, *range)
                    }
                    _ => (self.image(1, 1), [0, 0]),
                };
                if range[0] >= range[1] && !*mask {
                    return;
                }
                let mut p = Params::sized(w, h);
                (p.sw, p.sh) = (lut.w, lut.h);
                p.i = [range[0], range[1], 0, 0];
                p.j = [u32::from(*mask), 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitFlickerMatch,
                    p,
                    &[&lut.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::FlickerBlend {
                frames,
                center,
                tones,
                mask,
            } => {
                let sized = |p: &Arc<bv_core::video::ImageRgbaF32>| p.width == w && p.height == h;
                if frames
                    .iter()
                    .any(|f| f.picture.as_ref().is_some_and(|p| !sized(p)))
                {
                    return;
                }
                let pictures: Vec<Option<Img>> = frames
                    .iter()
                    .map(|f| {
                        f.picture.as_ref().map(|p| {
                            let b = self.image(w, h);
                            self.gpu
                                .queue
                                .write_buffer(&b.buf, 0, bytemuck::cast_slice(&p.data));
                            b
                        })
                    })
                    .collect();
                // The frame keeping its alpha, apart, read beside each.
                let own = match pictures.get(*center) {
                    Some(Some(p)) => self.copy(enc, &p.clone()),
                    Some(None) => self.copy(enc, img),
                    None => self.image(1, 1),
                };
                let sums = self.transparent(enc, w, 2 * h);
                for (k, f) in frames.iter().enumerate() {
                    let frame = pictures[k].as_ref().unwrap_or(img);
                    let mut fl = [0.0f32; 10];
                    fl[0] = f.weight;
                    if let Some(m) = f.map {
                        fl[1..].copy_from_slice(&m);
                    }
                    let mut p = Params::sized(w, h).with_f(&fl);
                    let mapped = f.map.is_some() && *center < frames.len();
                    p.j = [*tones as u32, u32::from(k == *center), u32::from(mapped), 0];
                    self.kernel(
                        enc,
                        K::KitFlickerGather,
                        p,
                        &[&frame.buf, &own.buf, &sums.buf],
                        Grid::Pixels(w, h),
                    );
                }
                let mut p = Params::sized(w, h);
                (p.sw, p.sh) = (w, 2 * h);
                p.j = [*tones as u32, u32::from(*mask), 0, 0];
                if frames.is_empty() {
                    // No frame: nothing, Type's sums as they are.
                    p.j[0] = 3;
                }
                self.kernel(
                    enc,
                    K::KitFlickerFinish,
                    p,
                    &[&sums.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Same | Plan::Generate { .. } => {}
            Plan::ChromaKey { f, j, blur } => {
                let mask = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(f);
                p.j = *j;
                self.kernel(
                    enc,
                    K::KitKeyMask,
                    p,
                    &[&img.buf, &mask.buf],
                    Grid::Pixels(w, h),
                );
                let sigma = accel::mask_sigma(*blur, f64::from(w.max(h)));
                let mask = if sigma > 0.6 {
                    self.boxes(enc, mask, [sigma as f32; 2])
                } else {
                    mask
                };
                self.kernel(
                    enc,
                    K::KitKeyApply,
                    p,
                    &[&mask.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::LinearBlur { amount, angle } => {
                let Some((across, slope, radius, [u1, w1, u2, w2])) =
                    accel::line_shear(*amount, *angle, (w, h), full)
                else {
                    return;
                };
                let (lw, lh) = ((u2 - u1).max(1) as u32, (w2 - w1).max(1) as u32);
                let lines = self.image(lw, lh);
                let mut p = Params::sized(lw, lh).with_f(&[slope as f32]);
                (p.sw, p.sh) = (w, h);
                p.i = [u1, w1, u2, w2];
                p.j = [across as u32, 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitShear,
                    p,
                    &[&img.buf, &lines.buf],
                    Grid::Pixels(lw, lh),
                );
                // Along the rows by the radius, across them by 0 (a little).
                let taps = [accel::blur_sections(radius), accel::blur_sections(0.0)];
                let from = self.recursive_gauss(enc, &lines, &taps);
                let mut p = Params::sized(w, h).with_f(&[slope as f32]);
                (p.sw, p.sh) = (lw, lh);
                p.i = [u1, w1, u2, w2];
                p.j = [across as u32, 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitUnshear,
                    p,
                    &[&from.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Gaussian { ranges, channels } => {
                let taps = accel::gaussian_sections(*ranges, full, w, h);
                let blurred = self.recursive_gauss(enc, img, &taps);
                if *channels != [true; 4] {
                    let mut p = Params::sized(w, h);
                    p.j = channels.map(u32::from);
                    self.kernel(
                        enc,
                        K::KitGaussPick,
                        p,
                        &[&img.buf, &blurred.buf],
                        Grid::Pixels(w, h),
                    );
                }
                *img = blurred;
            }
            Plan::FillLight {
                table,
                reach,
                amount,
            } => {
                // Each pixel's light from the table, blurred, then added.
                let entries = self.buffer(table.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&entries, 0, bytemuck::cast_slice(table));
                let light = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitFillMap,
                    Params::sized(w, h),
                    &[&img.buf, &entries, &light.buf],
                    Grid::Pixels(w, h),
                );
                let light = match accel::fill_light_sections(*reach, full, w, h) {
                    Some(taps) => self.recursive_gauss(enc, &light, &taps),
                    None => light,
                };
                let p = Params::sized(w, h).with_f(&[*amount]);
                self.kernel(
                    enc,
                    K::KitFillApply,
                    p,
                    &[&light.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Sharpen { gain } => {
                let taps = accel::sharpen_taps();
                let blurred = self.convolved(enc, img, &taps, &taps);
                let p = Params::sized(w, h).with_f(&[*gain]);
                self.kernel(
                    enc,
                    K::KitSharpen,
                    p,
                    &[&img.buf, &blurred.buf],
                    Grid::Pixels(w, h),
                );
                *img = blurred;
            }
            Plan::Glow {
                reach,
                intensity,
                suppression,
                color,
            } => {
                let bright = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitGlowBright,
                    Params::sized(w, h),
                    &[&img.buf, &bright.buf],
                    Grid::Pixels(w, h),
                );
                let taps = accel::blur_sections(reach * f64::from(w.max(h)));
                let blurred = self.recursive_gauss(enc, &bright, &[taps; 2]);
                let p = Params::sized(w, h).with_f(&[
                    *intensity,
                    *suppression,
                    color[0],
                    color[1],
                    color[2],
                ]);
                self.kernel(
                    enc,
                    K::KitGlowApply,
                    p,
                    &[&blurred.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Vignette { numbers, invert } => {
                let mut f = numbers.clone();
                f.extend(accel::vignette_scale(full, w, h));
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [*invert as u32, 0, 0, 0];
                self.kernel(enc, K::KitVignette, p, &[&img.buf], Grid::Pixels(w, h));
            }
            Plan::Mirror {
                angle,
                center,
                beyond,
            } => {
                let (numbers, j) = accel::mirror_numbers(*angle, *center, full, w, h);
                let f: Vec<f32> = numbers.into_iter().chain(*beyond).collect();
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [j[0], j[1], 0, 0];
                self.kernel(
                    enc,
                    K::KitMirror,
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Ranked { range, round, rank } => {
                let (table, fill) = accel::ranked_table(*range, *round, f64::from(w.max(h)));
                let entries = self.buffer(table.len().max(1) as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&entries, 0, bytemuck::cast_slice(&table));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h);
                p.i = [accel::ranked_rank(*rank), 0, 0, 0];
                p.j = [fill, table.len() as u32 - fill, 0, 0];
                self.kernel(
                    enc,
                    K::KitMedian,
                    p,
                    &[&img.buf, &entries, &out.buf],
                    Grid::Groups(h),
                );
                *img = out;
            }
            Plan::RadialBlur {
                strength,
                proportional,
                center,
            } => {
                let [part, q, fixed] = accel::radial_steps(*strength, f64::from(w.min(h)));
                // The middle's pixel, as the processor rounds it (rows up).
                let fx = center[0] * f64::from(w) - 0.5;
                let fy = (1.0 - center[1]) * f64::from(h) - 0.5;
                let cx = ((fx + 0.5).floor() as i64).clamp(0, i64::from(w) - 1) as i32;
                let cy = ((fy + 0.5).floor() as i64).clamp(0, i64::from(h) - 1) as i32;
                let out = self.copy(enc, img);
                let mut p = Params::sized(w, h).with_f(&[part as f32, q as f32, fixed as f32]);
                p.i = [cx, cy, 0, 0];
                p.j = [*proportional as u32, 0, 0, 0];
                // One group of the card's threads, a diagonal at a time.
                self.kernel(
                    enc,
                    K::KitRadialBlur,
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(16, 16),
                );
                *img = out;
            }
            Plan::RadialPixelate {
                center,
                strength,
                radial,
            } => {
                let [bx, by] = accel::pixelate_blocks(*strength, *radial, (w, h));
                let f = [center[0] as f32, center[1] as f32];
                let polar = self.image(w, h);
                let p = Params::sized(w, h).with_f(&f);
                self.kernel(
                    enc,
                    K::KitPolar,
                    p,
                    &[&img.buf, &polar.buf],
                    Grid::Pixels(w, h),
                );
                let means = self.image(bx, by);
                let mut q = Params::sized(bx, by);
                (q.sw, q.sh) = (w, h);
                self.kernel(
                    enc,
                    K::KitPolarMeans,
                    q,
                    &[&polar.buf, &means.buf],
                    Grid::Pixels(bx, by),
                );
                let mut b = Params::sized(w, h).with_f(&f);
                (b.sw, b.sh) = (bx, by);
                self.kernel(
                    enc,
                    K::KitPolarBack,
                    b,
                    &[&means.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::ChromaBlur { pixels } => {
                let uv = self.image(w, h);
                let p = Params::sized(w, h);
                self.kernel(
                    enc,
                    K::KitChromaUv,
                    p,
                    &[&img.buf, &uv.buf],
                    Grid::Pixels(w, h),
                );
                let sigma = pixels.map(|v| ((v + 1.5).floor() * scale) as f32);
                let uv = self.boxes(enc, uv, sigma);
                self.kernel(
                    enc,
                    K::KitChromaApply,
                    p,
                    &[&uv.buf, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Wave { numbers } => {
                // The moves per row and column, worked out once at the
                // picture's size, each in two exact parts.
                let (rows, columns) = accel::wave_offsets(numbers, w, h);
                let parts = |v: i32| {
                    [
                        v.div_euclid(4096) as f32,
                        v.rem_euclid(4096) as f32,
                        0.0,
                        0.0,
                    ]
                };
                let entries: Vec<[f32; 4]> =
                    rows.iter().chain(&columns).map(|&v| parts(v)).collect();
                let table = self.buffer(entries.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&entries));
                let out = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitWave,
                    Params::sized(w, h),
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::FilmGrain {
                numbers,
                frame,
                canonical,
            } => {
                // The grain tables and each column's and row's places in
                // them, worked out at the picture's size.
                let (entries, [amount, chroma]) =
                    accel::film_grain_table(*numbers, *frame, *canonical, w, h);
                let table = self.buffer(entries.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&entries));
                let out = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitFilmGrain,
                    Params::sized(w, h).with_f(&[amount, chroma]),
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::GradientMap { table, amount, add } => {
                let t = self.buffer(table.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&t, 0, bytemuck::cast_slice(table));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&[*amount]);
                p.j = [u32::from(*add), 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitGradientMap,
                    p,
                    &[&img.buf, &t, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Glint { numbers } => {
                if let Some(c) = accel::glint_card(numbers, w, h) {
                    let lights = self.image(w, h);
                    let mut p = Params::sized(w, h).with_f(&c.lights);
                    p.j = c.flags;
                    self.kernel(
                        enc,
                        K::KitGlintLights,
                        p,
                        &[&img.buf, &lights.buf],
                        Grid::Pixels(w, h),
                    );
                    let (size, cells) = c
                        .kernel
                        .clone()
                        .unwrap_or_else(|| (1, std::sync::Arc::new(vec![[0.0; 4]])));
                    let out = self.image(w, h);
                    let prep = FftPrep {
                        reflect: false,
                        bloom: None,
                        smooth: false,
                        threshold: None,
                        edges: None,
                    };
                    let finish = Some(FftAdd {
                        strength: 1.0,
                        alone: c.alone,
                        mask: Some(c.mask),
                    });
                    self.fft_convolve(enc, &lights, img, &out, &cells, size, prep, finish);
                    *img = out;
                }
            }
            Plan::Rays { f, j } => {
                let lights = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(f);
                p.j = *j;
                self.kernel(
                    enc,
                    K::KitRaysLights,
                    p,
                    &[&img.buf, &lights.buf],
                    Grid::Pixels(w, h),
                );
                let out = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitRays,
                    p,
                    &[&img.buf, &lights.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::SoftContrast {
                tone,
                diffusion,
                vignette,
            } => {
                let taps = |r: [f64; 2]| r.map(|v| accel::blur_sections(v * f64::from(w)));
                let toned = self.image(w, h);
                enc.copy_buffer_to_buffer(&img.buf, 0, &toned.buf, 0, img.buf.size());
                self.kernel(
                    enc,
                    K::KitSoftTone,
                    Params::sized(w, h).with_f(tone),
                    &[&toned.buf],
                    Grid::Pixels(w, h),
                );
                let mut picture = toned;
                if let Some((amount, glow)) = diffusion {
                    let blurred = self.recursive_gauss(enc, &picture, &taps([*glow; 2]));
                    let out = self.image(w, h);
                    self.kernel(
                        enc,
                        K::KitSoftDiffuse,
                        Params::sized(w, h).with_f(&[*amount]),
                        &[&picture.buf, &blurred.buf, &out.buf],
                        Grid::Pixels(w, h),
                    );
                    picture = out;
                }
                if let Some((numbers, blur)) = vignette {
                    let blurred = blur.map(|r| self.recursive_gauss(enc, &picture, &taps(r)));
                    let out = self.image(w, h);
                    let under = blurred.as_ref().map_or(&picture.buf, |b| &b.buf);
                    self.kernel(
                        enc,
                        K::KitSoftVignette,
                        Params::sized(w, h).with_f(numbers),
                        &[&picture.buf, under, &out.buf],
                        Grid::Pixels(w, h),
                    );
                    picture = out;
                }
                *img = picture;
            }
            Plan::Starburst { numbers, canonical } => {
                if let Some(c) = accel::starburst_card(*numbers, *canonical, w, h) {
                    let out = self.image(w, h);
                    let prep = FftPrep {
                        reflect: false,
                        bloom: c.bloom,
                        smooth: c.smooth,
                        threshold: Some(c.threshold),
                        edges: c.edges,
                    };
                    let finish = Some(FftAdd {
                        strength: c.strength,
                        alone: c.alone,
                        mask: None,
                    });
                    let cells = scalar_cells(&c.star);
                    self.fft_convolve(enc, img, img, &out, &cells, c.size, prep, finish);
                    *img = out;
                }
            }
            Plan::Defocus { numbers, canonical } => {
                if let Some((kernel, size, bloom, smooth)) =
                    accel::defocus_card(*numbers, *canonical, w, h)
                {
                    let out = self.image(w, h);
                    let prep = FftPrep {
                        reflect: false,
                        bloom,
                        smooth,
                        threshold: None,
                        edges: None,
                    };
                    let cells = scalar_cells(&kernel);
                    self.fft_convolve(enc, img, img, &out, &cells, size, prep, None);
                    *img = out;
                }
            }
            Plan::LensFlare { numbers, canonical } => {
                // The elements at the picture's size.
                let (spots, count, blend) = accel::lens_flare_card(*numbers, *canonical, w, h);
                let table = self.buffer(spots.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&spots));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&[blend]);
                p.i = [count, 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitLensFlare,
                    p,
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Newsprint {
                dot,
                kind,
                canonical,
            } => {
                // The rims' table and the averaged picture (worked out
                // here), then the dots.
                let (rows, [cols, cells], i, f) =
                    accel::newsprint_card(*dot, *kind, *canonical, w, h);
                let table = self.buffer(rows.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&rows));
                let none = self.buffer(16);
                let mut p = Params::sized(w, h).with_f(&f);
                p.i = i;
                self.kernel(
                    enc,
                    K::KitNewsprintCells,
                    p,
                    &[&img.buf, &none, &table],
                    Grid::Pixels(cols, cells),
                );
                let out = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitNewsprint,
                    p,
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::TvSimulator {
                numbers,
                time,
                canonical,
            } => {
                // Each row's move and the numbers at the picture's size.
                let (rows, i, j, f) = accel::tv_simulator_card(*numbers, *time, *canonical, w, h);
                let table = self.buffer(rows.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&rows));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&f);
                p.i = i;
                p.j = j;
                self.kernel(
                    enc,
                    K::KitTvSimulator,
                    p,
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::FilmEffects { numbers, time } => {
                // The frame's marks and numbers at the picture's size.
                let (marks, i, j, f) = accel::film_effects_card(*numbers, *time, w, h);
                let table = self.buffer(marks.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&marks));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&f);
                p.i = i;
                p.j = j;
                self.kernel(
                    enc,
                    K::KitFilmEffects,
                    p,
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::LightRays { numbers, time } => {
                // The bound's rows and the rays' numbers at the picture's
                // size; the rays drawn from the light by one group of the
                // card's threads, then tinted.
                let (rows, i, j, f) = accel::light_rays_card(*numbers, *time, w, h);
                let table = self.buffer(rows.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&rows));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&f);
                p.i = i;
                p.j = j;
                let bufs = [&img.buf, &table, &out.buf];
                self.kernel(enc, K::KitLightRays, p, &bufs, Grid::Pixels(16, 16));
                self.kernel(enc, K::KitLightRaysFinish, p, &bufs, Grid::Pixels(w, h));
                *img = out;
            }
            Plan::Lens { swirl, settings } => {
                // The columns' and rows' offsets over the area, worked out
                // once at the picture's size.
                let (center, reach, entries) = accel::lens(settings, w, h);
                let table = self.buffer(entries.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&entries));
                let out = self.image(w, h);
                let (kernel, amount) = if *swirl {
                    (K::KitSwirl, settings[4] * 15.0)
                } else {
                    (K::KitPinchPunch, settings[4])
                };
                let mut p =
                    Params::sized(w, h).with_f(&[reach[0] as f32, reach[1] as f32, amount as f32]);
                p.i = [center[0], center[1], 0, 0];
                self.kernel(
                    enc,
                    kernel,
                    p,
                    &[&img.buf, &table, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Deform { settings } => {
                // The shape worked out once at the picture's size, the
                // background after its numbers.
                let (numbers, bits, bounds) = accel::deform(settings, w, h);
                let mut f = numbers.to_vec();
                f.extend(settings[2..6].iter().map(|&c| c as f32));
                let mut p = Params::sized(w, h).with_f(&f);
                p.i = bounds;
                p.j = [bits, 0, 0, 0];
                let out = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitDeform,
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::MeshWarp {
                points,
                cells,
                target,
            } => {
                let (table, size, ratio) = accel::mesh_warp_table(points, *cells, *target, w, h);
                let tb = self.buffer(table.len().max(1) as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&tb, 0, bytemuck::cast_slice(&table));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&ratio);
                (p.sw, p.sh) = (size[0], size[1]);
                p.j = [u32::from(*target), 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitMeshWarp,
                    p,
                    &[&img.buf, &tb, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::LensCorrection { settings, ptlens } => {
                let f = accel::lens_correction_numbers(settings, *ptlens, w, h);
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [u32::from(*ptlens), 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitLensCorrection,
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::SphereTurn { matrix, stereo } => {
                let table = accel::sphere_table(*stereo, w, h);
                let tb = self.buffer(table.len().max(1) as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&tb, 0, bytemuck::cast_slice(&table));
                let out = self.image(w, h);
                let p = Params::sized(w, h).with_f(matrix);
                self.kernel(
                    enc,
                    K::KitSphereTurn,
                    p,
                    &[&img.buf, &tb, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Stitch { numbers } => {
                if w >= 2 && h >= 2 {
                    let table = accel::stitch_table(w, h);
                    let tb = self.buffer(table.len() as u64 * 16);
                    self.gpu
                        .queue
                        .write_buffer(&tb, 0, bytemuck::cast_slice(&table));
                    let out = self.image(w, h);
                    let p = Params::sized(w, h).with_f(numbers);
                    self.kernel(
                        enc,
                        K::KitStitch,
                        p,
                        &[&img.buf, &tb, &out.buf],
                        Grid::Pixels(w, h),
                    );
                    *img = out;
                }
            }
            Plan::Guided { numbers, low } => {
                if w >= 2 && h >= 2 {
                    let ([lw, lh], m) = accel::guided_layout(numbers, *low, w, h);
                    let (pw, ph) = ((w + 2 * m) as f32, (h + 2 * m) as f32);
                    let mut p = Params::sized(lw, lh).with_f(&[numbers[1], numbers[2], pw, ph]);
                    (p.sw, p.sh) = (w, h);
                    p.i = [m as i32, 0, 0, 0];
                    let small = self.image(lw, lh);
                    self.kernel(
                        enc,
                        K::KitGuidedLow,
                        p,
                        &[&img.buf, &small.buf],
                        Grid::Pixels(lw, lh),
                    );
                    let mut q = Params::sized(lw, lh);
                    q.j = [low[2], 0, 0, 0];
                    let worked = self.image(lw, lh);
                    if low[2] == 2 {
                        // Dehaze: the dark channel, the air light, the small result.
                        let dark = self.image(lw, lh);
                        self.kernel(
                            enc,
                            K::KitDehazeDark,
                            q,
                            &[&small.buf, &dark.buf],
                            Grid::Pixels(lw, lh),
                        );
                        let air = self.image(1, 1);
                        self.kernel(
                            enc,
                            K::KitDehazeAir,
                            q,
                            &[&small.buf, &dark.buf, &air.buf],
                            Grid::Groups(1),
                        );
                        self.kernel(
                            enc,
                            K::KitDehazeOp,
                            q,
                            &[&small.buf, &air.buf, &worked.buf],
                            Grid::Pixels(lw, lh),
                        );
                    } else {
                        self.kernel(
                            enc,
                            K::KitGuidedOp,
                            q,
                            &[&small.buf, &worked.buf],
                            Grid::Pixels(lw, lh),
                        );
                    }
                    let coef = self.image(2 * lw, lh);
                    let q = Params::sized(lw, lh);
                    self.kernel(
                        enc,
                        K::KitGuidedCoef,
                        q,
                        &[&small.buf, &worked.buf, &coef.buf],
                        Grid::Pixels(lw, lh),
                    );
                    let mean = self.image(2 * lw, lh);
                    self.kernel(
                        enc,
                        K::KitGuidedMean,
                        q,
                        &[&coef.buf, &mean.buf],
                        Grid::Pixels(lw, lh),
                    );
                    let steps = [(lw - 1) as f32 / (pw - 1.0), (lh - 1) as f32 / (ph - 1.0)];
                    let mut p = Params::sized(w, h)
                        .with_f(&[numbers[1], numbers[2], steps[0], steps[1], numbers[3]]);
                    (p.sw, p.sh) = (lw, lh);
                    p.i = [m as i32, 0, 0, 0];
                    let out = self.image(w, h);
                    self.kernel(
                        enc,
                        K::KitGuidedApply,
                        p,
                        &[&img.buf, &mean.buf, &out.buf],
                        Grid::Pixels(w, h),
                    );
                    *img = out;
                }
            }
            Plan::SmartZoom { numbers } => {
                let table = accel::smart_zoom_table(numbers, w, h);
                let tb = self.buffer(table.len().max(1) as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&tb, 0, bytemuck::cast_slice(&table));
                let p = Params::sized(w, h);
                let across = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitLanczosRows,
                    p,
                    &[&img.buf, &tb, &across.buf],
                    Grid::Pixels(w, h),
                );
                let out = self.image(w, h);
                self.kernel(
                    enc,
                    K::KitLanczosColumns,
                    p,
                    &[&across.buf, &tb, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
                if numbers[3] > 0.0 {
                    let out = self.image(w, h);
                    let p = Params::sized(w, h).with_f(&[numbers[3], numbers[4]]);
                    self.kernel(
                        enc,
                        K::KitSmartEnhance,
                        p,
                        &[&img.buf, &out.buf],
                        Grid::Pixels(w, h),
                    );
                    *img = out;
                }
            }
            Plan::Stereo { numbers, flips } => {
                let (affine, table, band, share) = accel::stereo_card(numbers, *flips, w, h);
                let tb = self.buffer(table.len().max(1) as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&tb, 0, bytemuck::cast_slice(&table));
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&[share]);
                p.i = [band[0], band[1], band[2], 0];
                p.j = [u32::from(affine), 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitStereoWarp,
                    p,
                    &[&img.buf, &tb, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Stabilize { map, fill, reach } => {
                let f = accel::stabilize_numbers(map, w, h);
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&f);
                p.j = [*fill, 0, 0, 0];
                self.kernel(
                    enc,
                    K::KitStabilize,
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                // The pan's blur along the rows.
                let r = (f64::from(*reach) * f64::from(w)).round() as i32;
                if r > 0 {
                    let blurred = self.image(w, h);
                    let mut p = Params::sized(w, h);
                    p.i = [r, 0, 0, 0];
                    self.kernel(
                        enc,
                        K::KitStabilizeBlur,
                        p,
                        &[&out.buf, &blurred.buf],
                        Grid::Pixels(w, h),
                    );
                    *img = blurred;
                } else {
                    *img = out;
                }
            }
            Plan::Remap { kernel, f, j } => {
                let out = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(f);
                p.j = *j;
                self.kernel(
                    enc,
                    remap_kernel(*kernel),
                    p,
                    &[&img.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
                *img = out;
            }
            Plan::Pixel { kernel, f, j } => {
                let mut p = Params::sized(w, h).with_f(f);
                p.j = *j;
                self.kernel(
                    enc,
                    pixel_kernel(*kernel),
                    p,
                    &[&img.buf],
                    Grid::Pixels(w, h),
                );
            }
            Plan::Timecode(caption) => {
                let Some(o) = caption.overlay(w, h) else {
                    return;
                };
                let pixels = self.buffer(o.rgba.len() as u64 * 16);
                self.gpu
                    .queue
                    .write_buffer(&pixels, 0, bytemuck::cast_slice(&o.rgba));
                let mut p = Params::sized(w, h);
                (p.sw, p.sh) = (o.w, o.h);
                p.i = [o.x, o.y, 0, 0];
                self.kernel(enc, K::Overlay, p, &[&pixels, &img.buf], Grid::Pixels(w, h));
            }
            Plan::Lut {
                lut,
                tetrahedral,
                strength,
            } => {
                let Some(lut) = lut else { return };
                if *strength <= 0.0 {
                    return;
                }
                // The shaper's entries, then the lattice's.
                let mut entries: Vec<f32> = Vec::with_capacity(lut_entries(lut) * 4);
                let none = (Vec::new(), [0.0; 3], [1.0; 3]);
                let (shaper, s_lo, s_hi) = lut
                    .shaper
                    .as_ref()
                    .map_or(none.clone(), |t| (t.entries.clone(), t.min, t.max));
                let (cube, c_lo, c_hi) = lut
                    .cube
                    .as_ref()
                    .map_or(none, |t| (t.entries.clone(), t.min, t.max));
                for e in shaper.iter().chain(&cube) {
                    entries.extend_from_slice(&[e[0], e[1], e[2], 0.0]);
                }
                if entries.is_empty() {
                    return;
                }
                let table = self.buffer(entries.len() as u64 * 4);
                self.gpu
                    .queue
                    .write_buffer(&table, 0, bytemuck::cast_slice(&entries));
                let mut p = Params::sized(w, h).with_f(&[
                    s_lo[0], s_lo[1], s_lo[2], s_hi[0], s_hi[1], s_hi[2], c_lo[0], c_lo[1],
                    c_lo[2], c_hi[0], c_hi[1], c_hi[2], *strength,
                ]);
                p.j = [
                    shaper.len() as u32,
                    lut.cube.as_ref().map_or(0, |t| t.size as u32),
                    *tetrahedral as u32,
                    0,
                ];
                p.sw = (entries.len() / 4) as u32;
                p.sh = 1;
                self.kernel(
                    enc,
                    K::LutFilter,
                    p,
                    &[&table, &img.buf],
                    Grid::Pixels(w, h),
                );
            }
        }
    }

    /// `img` through the taps `kx` across and `ky` up and down (odd
    /// counts, centered), its edge pixels held, as the processor's spatial
    /// effects convolve (`spatial::convolve`); a new picture.
    fn convolved(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        kx: &[f32],
        ky: &[f32],
    ) -> Img {
        let (w, h) = (img.w, img.h);
        let table =
            |taps: &[f32]| -> Vec<[f32; 4]> { taps.iter().map(|&t| [t, 0.0, 0.0, 0.0]).collect() };
        let mut done: Option<Img> = None;
        for (k, taps) in [(K::KitConvH, kx), (K::KitConvV, ky)] {
            if taps.len() <= 1 {
                continue;
            }
            let entries = table(taps);
            let buf = self.buffer(entries.len() as u64 * 16);
            self.gpu
                .queue
                .write_buffer(&buf, 0, bytemuck::cast_slice(&entries));
            let to = self.image(w, h);
            let mut p = Params::sized(w, h);
            p.j = [taps.len() as u32, 0, 0, 0];
            let from = done.as_ref().unwrap_or(img);
            self.kernel(enc, k, p, &[&from.buf, &buf, &to.buf], Grid::Pixels(w, h));
            done = Some(to);
        }
        // A picture of its own even when nothing blurs.
        match done {
            Some(img) => img,
            None => self.copy(enc, img),
        }
    }

    /// `img` blurred as the processor's table effects blur a plane
    /// (`kit::gauss_plane`): three boxes across standing for a Gaussian of
    /// `sigma[0]` pixels, then three down for `sigma[1]`, the edges held.
    fn boxes(&mut self, enc: &mut wgpu::CommandEncoder, img: Img, sigma: [f32; 2]) -> Img {
        let (w, h) = (img.w, img.h);
        let mut from = img;
        for (k, s) in [K::BoxH, K::BoxV].into_iter().zip(sigma) {
            for size in accel::gauss_boxes(s) {
                let r = (size - 1) / 2;
                let to = self.image(w, h);
                let mut p = Params::sized(w, h).with_f(&[1.0 / (2 * r + 1) as f32]);
                p.j = [r as u32, 0, 0, 0];
                let grid = if k == K::BoxH {
                    Grid::Lines(h)
                } else {
                    Grid::Lines(w)
                };
                self.kernel(enc, k, p, &[&from.buf, &to.buf], grid);
                from = to;
            }
        }
        from
    }

    /// Pixelate: the average of each row over each cell across, then of
    /// each cell, then each pixel its cell's (a step skipped where the
    /// cells are the pixels).
    /// White Balance with Equalize Histogram: each row's lowest and
    /// highest white-balanced colors, the picture's from them, then every
    /// pixel stretched by them.
    fn equalize(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        m: &[[f32; 3]; 3],
        o: &[f32; 3],
        img: &Img,
    ) {
        let (w, h) = (img.w, img.h);
        let f = [
            m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2], o[0],
            o[1], o[2],
        ];
        let rows = self.image(2, h);
        let mut p = Params::sized(2, h).with_f(&f);
        (p.sw, p.sh) = (w, h);
        self.kernel(
            enc,
            K::EqualizeRows,
            p,
            &[&img.buf, &rows.buf],
            Grid::Pixels(2, h),
        );
        let total = self.image(2, 1);
        let mut p = Params::sized(2, 1);
        (p.sw, p.sh) = (2, h);
        self.kernel(
            enc,
            K::EqualizeTotal,
            p,
            &[&rows.buf, &total.buf],
            Grid::Pixels(2, 1),
        );
        let mut p = Params::sized(w, h).with_f(&f);
        (p.sw, p.sh) = (2, 1);
        self.kernel(
            enc,
            K::EqualizeApply,
            p,
            &[&total.buf, &img.buf],
            Grid::Pixels(w, h),
        );
    }

    fn pixelate(&mut self, enc: &mut wgpu::CommandEncoder, cells: [u32; 2], img: &Img) {
        let (w, h) = (img.w, img.h);
        let (n, m) = (cells[0].clamp(1, w), cells[1].clamp(1, h));
        if (n, m) == (w, h) {
            return;
        }
        let steps = [accel::pixelate_step(n, w), accel::pixelate_step(m, h), 0, 0];
        let rows = (n < w).then(|| {
            let rows = self.image(n, h);
            let mut p = Params::sized(n, h);
            (p.sw, p.sh) = (w, h);
            p.i = steps;
            self.kernel(
                enc,
                K::PixelRows,
                p,
                &[&img.buf, &rows.buf],
                Grid::Pixels(n, h),
            );
            rows
        });
        let rows_buf = rows.as_ref().map_or(&img.buf, |r| &r.buf);
        let grid = (m < h).then(|| {
            let grid = self.image(n, m);
            let mut p = Params::sized(n, m);
            (p.sw, p.sh) = (n, h);
            p.i = steps;
            self.kernel(
                enc,
                K::PixelCells,
                p,
                &[rows_buf, &grid.buf],
                Grid::Pixels(n, m),
            );
            grid
        });
        let cells_buf = match (&grid, &rows) {
            (Some(g), _) => &g.buf,
            (None, Some(r)) => &r.buf,
            (None, None) => return,
        };
        let mut p = Params::sized(w, h);
        (p.sw, p.sh) = (n, m);
        p.i = steps;
        self.kernel(
            enc,
            K::PixelFill,
            p,
            &[cells_buf, &img.buf],
            Grid::Pixels(w, h),
        );
    }

    /// Layer Dimensionality: the picture's alpha blurred four ways in a
    /// field around it, then its surface and what is under it.
    fn layer(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        l: &accel::Layer,
        img: &Img,
        scale: f64,
    ) -> Img {
        let (w, h) = (img.w, img.h);
        let radii = l.sigmas.map(|sigma| accel::box_radius(sigma * scale));
        // Around the picture, as far as the blurs reach (three boxes).
        let margin = radii.iter().max().copied().unwrap_or(0) as u32 * 3 + 2;
        let (fw, fh) = (w + 2 * margin, h + 2 * margin);
        let a = self.image(fw, fh);
        let b = self.image(fw, fh);
        let mut p = Params::sized(fw, fh);
        p.sw = w;
        p.sh = h;
        p.i = [margin as i32, 0, 0, 0];
        self.kernel(
            enc,
            K::FieldAlpha,
            p,
            &[&img.buf, &a.buf],
            Grid::Pixels(fw, fh),
        );
        if radii.iter().any(|&r| r > 0) {
            let mut p = Params::sized(fw, fh).with_f(&radii.map(|r| 1.0 / (2 * r + 1) as f32));
            p.i = radii.map(|r| r as i32);
            for _ in 0..3 {
                self.kernel(enc, K::FieldBoxH, p, &[&a.buf, &b.buf], Grid::Lines(fh));
                self.kernel(enc, K::FieldBoxV, p, &[&b.buf, &a.buf], Grid::Lines(fw));
            }
        }
        // The shadow's move in pixels, as the plug-in rounds it.
        let off = l.offset.map(|v| (v * scale).round() as i32);
        let steep = (l.sigmas[3] * scale).max(1.0) as f32 * l.steep;
        let g = l.glow_color;
        let mut p = Params::sized(w, h).with_f(&[
            l.shadow_opacity,
            g[0],
            g[1],
            g[2],
            l.relief[0],
            l.relief[1],
            l.light[0],
            l.light[1],
            l.light[2],
            l.ambience,
            l.intensity,
            l.shininess,
            steep,
            l.opacity,
            l.shine,
            l.ground,
        ]);
        p.sw = fw;
        p.sh = fh;
        p.i = [margin as i32, off[0], off[1], 0];
        p.j = [l.shadow, l.glow, l.embossed as u32, 0];
        let out = self.image(w, h);
        self.kernel(
            enc,
            K::LayerSurface,
            p,
            &[&img.buf, &a.buf, &out.buf],
            Grid::Pixels(w, h),
        );
        self.kernel(
            enc,
            K::LayerUnder,
            p,
            &[&img.buf, &a.buf, &out.buf],
            Grid::Pixels(w, h),
        );
        out
    }
}

impl Compositor {
    /// Black Bar Fill: the background (`layers[0]`: the picture through its
    /// inverse map, `samples` a pixel) blurred as VEGAS blurs by
    /// `background_blur`'s numbers (none: as it is), its edges held as the
    /// plug-in's; the foreground (`layers[1]`) over it, with its alpha
    /// blurred by `shadow_sigma` pixels, black at `shadow`, between.
    fn fill(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        layers: [(&[f32; 9], u32); 2],
        (background_blur, shadow_sigma): (Option<[[f32; 16]; 2]>, f64),
        shadow: f32,
    ) -> Img {
        let (w, h) = (img.w, img.h);
        let [bg, fg] = layers.map(|(inverse, samples)| {
            let out = self.image(w, h);
            let mut f = inverse.to_vec();
            f.push(1.0);
            let mut p = Params::sized(w, h).with_f(&f);
            p.j = [samples, 0, 0, 0];
            self.kernel(
                enc,
                K::PictureInPicture,
                p,
                &[&img.buf, &out.buf],
                Grid::Pixels(w, h),
            );
            out
        });
        // The background blurred as VEGAS blurs, its edges held.
        let bg = match background_blur {
            Some(taps) => self.recursive_gauss(enc, &bg, &taps),
            None => bg,
        };
        // The shadow: the foreground's alpha blurred, nothing beyond the
        // frame.
        let rs = accel::box_radius(shadow_sigma);
        let margin = (rs * 3 + 2) as u32;
        let (fw, fh) = (w + 2 * margin, h + 2 * margin);
        let a = self.image(fw, fh);
        let mut p = Params::sized(fw, fh);
        (p.sw, p.sh) = (w, h);
        p.i = [margin as i32, 0, 0, 0];
        self.kernel(
            enc,
            K::FieldAlpha,
            p,
            &[&fg.buf, &a.buf],
            Grid::Pixels(fw, fh),
        );
        if rs > 0 && shadow > 0.0 {
            let b = self.image(fw, fh);
            let mut p = Params::sized(fw, fh).with_f(&[1.0 / (2 * rs + 1) as f32; 4]);
            p.i = [rs as i32; 4];
            for _ in 0..3 {
                self.kernel(enc, K::FieldBoxH, p, &[&a.buf, &b.buf], Grid::Lines(fh));
                self.kernel(enc, K::FieldBoxV, p, &[&b.buf, &a.buf], Grid::Lines(fw));
            }
        }
        let mut p = Params::sized(w, h).with_f(&[shadow]);
        (p.sw, p.sh) = (fw, fh);
        p.i = [margin as i32, 0, 0, 0];
        self.kernel(
            enc,
            K::FillCombine,
            p,
            &[&fg.buf, &a.buf, &bg.buf],
            Grid::Pixels(w, h),
        );
        bg
    }
}

/// A transition's kernel.
/// The kernel of a [`Plan::Pixel`].
fn pixel_kernel(k: accel::PixelKernel) -> K {
    use accel::PixelKernel as P;
    match k {
        P::Threshold => K::KitThreshold,
        P::BlackRestore => K::KitBlackRestore,
        P::ChannelBlend => K::KitChannelBlend,
        P::MaskGenerator => K::KitMaskGenerator,
        P::BroadcastColors => K::KitBroadcastColors,
        P::LabAdjust => K::KitLabAdjust,
        P::ColorMatch => K::KitColorMatch,
    }
}

/// The kernel of a [`Plan::Remap`].
fn remap_kernel(k: accel::RemapKernel) -> K {
    use accel::RemapKernel as R;
    match k {
        R::Spherize => K::KitSpherize,
        R::OffsetWrap => K::KitOffsetWrap,
        R::TinyPlanet => K::KitTinyPlanet,
        R::Convolution => K::KitConvolution,
        R::QuickBlur => K::KitQuickBlur,
        R::BumpMap => K::KitBumpMap,
    }
}

fn transition_kernel(k: accel::TransitionKernel) -> K {
    use accel::TransitionKernel as T;
    match k {
        T::LinearWipe => K::TrLinearWipe,
        T::ClockWipe => K::TrClockWipe,
        T::Iris => K::TrIris,
        T::VenetianBlinds => K::TrVenetianBlinds,
        T::StarWipe => K::TrStarWipe,
        T::Moves => K::TrMoves,
        T::GradientWipe => K::TrGradientWipe,
        T::Flash => K::TrFlash,
        T::Dissolve => K::TrDissolve,
        T::CrossEffect => K::TrCrossEffect,
        T::Pages => K::TrPages,
        T::Cards => K::TrCards,
    }
}

/// How many colors a look-up table has, its shaper's and its lattice's.
fn lut_entries(lut: &bv_lut::Lut) -> usize {
    lut.shaper.as_ref().map_or(0, |t| t.entries.len())
        + lut.cube.as_ref().map_or(0, |t| t.entries.len())
}

impl Compositor {
    /// The frame `between.t` of the way from `first` to `second` along the
    /// flow (`kit_flow_between`), written to `out`.
    fn flow_between(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        (first, second): (&Img, &Img),
        between: &accel::FlowBetween,
        out: &Img,
    ) {
        let (w, h) = (first.w, first.h);
        let table = self.buffer(between.table.len().max(1) as u64 * 16);
        self.gpu
            .queue
            .write_buffer(&table, 0, bytemuck::cast_slice(&between.table));
        let mut p = Params::sized(w, h).with_f(&[between.t, between.k]);
        p.i = [between.cells[0] as i32, between.cells[1] as i32, 0, 0];
        p.j = [u32::from(between.nearest), 0, 0, 0];
        self.kernel(
            enc,
            K::KitFlowBetween,
            p,
            &[&first.buf, &second.buf, &table, &out.buf],
            Grid::Pixels(w, h),
        );
    }

    /// `img` convolved with a kernel through the frequency domain into
    /// `out`, as `bv-effects`' `fft` does it on the processor (Defocus's
    /// iris, Starburst's star, Glint's streaks): the kernel's `size` ×
    /// `size` cells (two complex numbers each, rows up) and the picture on
    /// grids of powers of two — the picture through `prep` first —,
    /// transformed by Stockham's steps (`fft_pass`), multiplied and taken
    /// back, held within 0–1; with `add`, Strength of it added to `base`
    /// (none with Effect only), through the mask.
    #[allow(clippy::too_many_arguments)]
    fn fft_convolve(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        base: &Img,
        out: &Img,
        kernel: &[[f32; 4]],
        size: u32,
        prep: FftPrep,
        add: Option<FftAdd>,
    ) {
        let (w, h) = (img.w, img.h);
        let reach = size / 2;
        let (gw, gh) = accel::fft_grid(w as usize, h as usize, reach as usize);
        let (gw, gh) = (gw as u32, gh as u32);
        let bytes = u64::from(gw) * u64::from(gh) * 16;
        let bufs = [self.buffer(bytes), self.buffer(bytes), self.buffer(bytes)];
        // The kernel's cells, rows from the top as the card's pictures.
        let n = size as usize;
        let mut table = vec![[0.0f32; 4]; n * n];
        for (ky, row) in kernel.chunks(n).enumerate() {
            for (kx, &v) in row.iter().enumerate() {
                table[(n - 1 - ky) * n + kx] = v;
            }
        }
        let weights = self.buffer(table.len() as u64 * 16);
        self.gpu
            .queue
            .write_buffer(&weights, 0, bytemuck::cast_slice(&table));
        let mut p = Params::sized(gw, gh);
        p.sw = size;
        p.sh = size;
        p.i = [reach as i32, 0, 0, 0];
        self.kernel(
            enc,
            K::FftKernel,
            p,
            &[&weights, &bufs[0]],
            Grid::Pixels(gw, gh),
        );
        let spectrum = self.fft_passes(enc, &bufs, [0, 1], [gw, gh], -1.0);
        let [a, b] = match spectrum {
            0 => [1, 2],
            1 => [0, 2],
            _ => [0, 1],
        };
        let [t, k] = prep.bloom.unwrap_or([0.0; 2]);
        let e = prep.edges.unwrap_or([0.0, 0.0, 0.0, 0.0, 1.0]);
        let mut p = Params::sized(gw, gh).with_f(&[
            t,
            k,
            prep.threshold.unwrap_or(0.0),
            e[0],
            e[1],
            e[2],
            e[3],
            e[4],
        ]);
        p.sw = w;
        p.sh = h;
        p.i = [reach as i32, i32::from(prep.reflect), 0, 0];
        p.j = [
            u32::from(prep.bloom.is_some()),
            u32::from(prep.smooth),
            u32::from(prep.threshold.is_some()),
            u32::from(prep.edges.is_some()),
        ];
        self.kernel(
            enc,
            K::FftImage,
            p,
            &[&img.buf, &bufs[a]],
            Grid::Pixels(gw, gh),
        );
        let picture = self.fft_passes(enc, &bufs, [a, b], [gw, gh], -1.0);
        let product = a + b - picture;
        self.kernel(
            enc,
            K::FftMultiply,
            Params::sized(gw, gh),
            &[&bufs[picture], &bufs[spectrum], &bufs[product]],
            Grid::Pixels(gw, gh),
        );
        let back = self.fft_passes(enc, &bufs, [product, picture], [gw, gh], 1.0);
        let scale = 1.0 / (gw as f32 * gh as f32);
        match add {
            None => {
                let mut p = Params::sized(w, h).with_f(&[scale]);
                p.sw = gw;
                p.sh = gh;
                p.i = [reach as i32, 0, 0, 0];
                self.kernel(
                    enc,
                    K::FftCrop,
                    p,
                    &[&bufs[back], &out.buf],
                    Grid::Pixels(w, h),
                );
            }
            Some(a) => {
                let (mode, numbers) = a.mask.unwrap_or((0, [0.0; 13]));
                let mut f = vec![scale, a.strength];
                f.extend(numbers);
                let mut p = Params::sized(w, h).with_f(&f);
                p.sw = gw;
                p.sh = gh;
                p.i = [reach as i32, 0, 0, 0];
                p.j = [u32::from(a.alone), mode, 0, 0];
                self.kernel(
                    enc,
                    K::FftAdd,
                    p,
                    &[&bufs[back], &base.buf, &out.buf],
                    Grid::Pixels(w, h),
                );
            }
        }
    }

    /// A grid `size[0]` × `size[1]` transformed along its rows, then its
    /// columns (`sign` −1 forward, 1 back), back and forth between
    /// `bufs[from]` and `bufs[to]`; which of them it ends in.
    fn fft_passes(
        &self,
        enc: &mut wgpu::CommandEncoder,
        bufs: &[wgpu::Buffer; 3],
        [from, to]: [usize; 2],
        [gw, gh]: [u32; 2],
        sign: f32,
    ) -> usize {
        let (mut s, mut d) = (from, to);
        for (axis, n) in [(0u32, gw), (1u32, gh)] {
            let mut half = 1u32;
            while half < n {
                let mut p = Params::sized(gw, gh).with_f(&[sign]);
                p.i = [half as i32, 0, 0, 0];
                p.j = [axis, 0, 0, 0];
                let grid = if axis == 0 {
                    Grid::Pixels(gw / 2, gh)
                } else {
                    Grid::Pixels(gw, gh / 2)
                };
                self.kernel(enc, K::FftPass, p, &[&bufs[s], &bufs[d]], grid);
                std::mem::swap(&mut s, &mut d);
                half *= 2;
            }
        }
        s
    }
}

/// What a picture goes through before the frequency domain
/// (`Compositor::fft_convolve`): the bloom's threshold and slope, the
/// binomial 3 × 3, the lights' threshold and Starburst's soft edges.
struct FftPrep {
    bloom: Option<[f32; 2]>,
    smooth: bool,
    threshold: Option<f32>,
    edges: Option<[f32; 5]>,
    /// The picture reflected past its edges (held otherwise).
    reflect: bool,
}

/// What the frequency domain's result does at the end
/// (`Compositor::fft_convolve`): Strength of it added to the picture, or
/// alone (Effect only), through the mask (its mode and numbers: `vm_mask`).
struct FftAdd {
    strength: f32,
    alone: bool,
    mask: Option<(u32, [f32; 13])>,
}

/// A kernel of plain weights as cells of two complex numbers: the same
/// weight for both.
fn scalar_cells(kernel: &[f32]) -> Vec<[f32; 4]> {
    kernel.iter().map(|&v| [v, 0.0, v, 0.0]).collect()
}
