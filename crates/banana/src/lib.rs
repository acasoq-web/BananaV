//! Banana Split: BananaV's own pack of effects, transitions and
//! generators, written as OpenFX plug-ins that BananaV hosts as it hosts
//! its own (`apps/ofx-bundle` puts them in `BananaV.ofx` for other hosts).
//!
//! An effect ([`fx::Fx`]) is a description — its settings, presets and
//! handles — and a function that turns its settings' values into the steps
//! of a frame ([`graph::Graph`]): kernels run one after another. Each
//! kernel is written once, in the shading language (`kernels/*.wgsl`): the
//! graphics card runs it as it is (`bv-gpu`'s `compositor/banana.rs`), the
//! processor runs the Rust that `gen/gen.rs` writes from it
//! (`generated.rs`, `cargo xtask banana`), so both draw alike.
//!
//! Lengths are percents of the frame's height and places parts of the
//! frame; noise counts the full frame's pixels: a preview of any quality
//! shows what the render will.

pub mod bolt;
pub mod cpu;
pub mod fx;
#[rustfmt::skip]
#[allow(clippy::all, unused)]
mod generated;
pub mod graph;
mod math;
mod ofx;
mod overlay;
mod pack;

use bv_effects::accel::{Moment, Value, Values};
use bv_effects::{Preset, PresetValue};

pub use ofx::{BINARY, GROUPING, ID_PREFIX, by_id, get_plugin, number_of_plugins};
pub use pack::{all, find};

use fx::{Frame, Kind, Param, V, common};
use graph::{Graph, Kernel};

/// The kernels' families: each a shader module, its name and its source
/// (the shared part, its kernels and their entry points).
pub fn families() -> impl Iterator<Item = (&'static str, String)> {
    (0..generated::FAMILIES.len()).filter_map(family)
}

/// The family numbered `n`.
pub fn family(n: usize) -> Option<(&'static str, String)> {
    let (name, source) = generated::FAMILIES.get(n)?;
    let mut text = format!("{}\n{source}\n", generated::PRELUDE);
    for e in generated::ENTRIES.iter().filter(|e| e.family == n) {
        let k = e.name;
        text.push_str(&format!(
            "\n@compute @workgroup_size(16, 16)\nfn main_{k}(@builtin(global_invocation_id) id: vec3<u32>) {{\n    if id.x >= P.w || id.y >= P.h {{\n        return;\n    }}\n    dst[id.y * P.w + id.x] = k_{k}(vec2<i32>(i32(id.x), i32(id.y)));\n}}\n"
        ));
    }
    Some((name, text))
}

/// A kernel's family and entry point.
pub fn kernel(k: Kernel) -> Option<(usize, String)> {
    let e = generated::ENTRIES.get(k.0 as usize)?;
    Some((e.family, format!("main_{}", e.name)))
}

/// Every kernel.
pub fn kernels() -> impl Iterator<Item = Kernel> {
    (0..generated::ENTRIES.len()).map(|n| Kernel(n as u16))
}

/// A kernel's name.
pub fn kernel_name(k: Kernel) -> &'static str {
    generated::ENTRIES.get(k.0 as usize).map_or("", |e| e.name)
}

/// An effect's values as the host has them, by name.
fn given(p: &Param, values: &Values<'_>) -> Option<[f64; 3]> {
    let n = values.0.iter().find(|(name, _)| name == p.name)?;
    let Value::Num(v) = &n.1 else { return None };
    (v.len() >= p.len()).then(|| std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0)))
}

/// A frame's size and shape: `size` pixels drawn of a frame `full` big
/// (canonical units).
#[derive(Clone, Copy, Debug)]
pub struct Canvas {
    pub size: [u32; 2],
    pub full: [f64; 2],
}

impl Canvas {
    fn frame(&self, at: Moment) -> Frame {
        Frame {
            w: self.size[0].max(1),
            h: self.size[1].max(1),
            scale: (f64::from(self.size[1]) / self.full[1].max(1.0)) as f32,
            shape: (self.full[0] / self.full[1].max(1e-6)) as f32,
            frame: at.frames,
            rate: at.rate,
            length: at.length,
            progress: 0.0,
            alone: false,
            others: 0,
        }
    }
}

/// The times (frames) of the other frames the effect `id` reads.
pub fn other_frames(id: &str, values: Values<'_>, at: Moment) -> Vec<f64> {
    let Some(fx) = by_id(id).filter(|f| f.kind != Kind::Transition) else {
        return Vec::new();
    };
    let v = V::of(fx, |p| given(p, &values));
    fx.times(&v, at.frames)
}

/// The steps the effect `id` draws a frame with; `others` how many other
/// frames it gets.
pub fn effect_graph(
    id: &str,
    values: Values<'_>,
    canvas: Canvas,
    at: Moment,
    others: usize,
) -> Option<Graph> {
    let fx = by_id(id).filter(|f| f.kind != Kind::Transition)?;
    let v = V::of(fx, |p| given(p, &values));
    let mut frame = canvas.frame(at);
    frame.others = others;
    Some(fx.graph(&v, frame))
}

/// The steps the generator `id` draws a frame with, alone.
pub fn generator_graph(id: &str, values: Values<'_>, canvas: Canvas, at: Moment) -> Option<Graph> {
    let fx = by_id(id).filter(|f| f.kind == Kind::Generator)?;
    let v = V::of(fx, |p| given(p, &values));
    let mut frame = canvas.frame(at);
    frame.alone = true;
    Some(fx.graph(&v, frame))
}

/// The steps the transition `id` draws a frame with, at `progress`.
pub fn transition_graph(
    id: &str,
    values: Values<'_>,
    canvas: Canvas,
    at: Moment,
    progress: f64,
) -> Option<Graph> {
    let fx = by_id(id).filter(|f| f.kind == Kind::Transition)?;
    let v = V::of(fx, |p| given(p, &values));
    let mut frame = canvas.frame(at);
    frame.progress = progress;
    Some(fx.graph(&v, frame))
}

/// Whether `id` is one of the pack's.
pub fn is_own(id: &str) -> bool {
    id.starts_with(ID_PREFIX)
}

/// Whether `id` is one of the pack's generators.
pub fn is_generator(id: &str) -> bool {
    by_id(id).is_some_and(|f| f.kind == Kind::Generator)
}

/// The factory presets of the effect `id` (none for others').
pub fn presets(id: &str) -> Vec<Preset> {
    let Some(fx) = by_id(id) else {
        return Vec::new();
    };
    fx.presets
        .iter()
        .map(|p| Preset {
            name: p.name,
            values: p
                .values
                .iter()
                .map(|(n, v)| (*n, PresetValue::Num(v.to_vec())))
                .collect(),
        })
        .collect()
}

/// The box round the mask of the effect `id` with `values`, for Motion
/// Tracking's import menu: its corners (0–1 of the frame from its top
/// left) from the top left one; `None` without a mask.
pub fn tracked_region(id: &str, values: Values<'_>, aspect: f64) -> Option<[[f64; 2]; 4]> {
    let fx = by_id(id).filter(|f| f.kind != Kind::Transition)?;
    let v = V::of(fx, |p| given(p, &values));
    if v.i(common::MASK) <= 0 {
        return None;
    }
    let [cx, cy] = v.xy(common::MASK_CENTER);
    let a = v.f64(common::MASK_ANGLE).to_radians();
    let half = [
        0.5 * v.f64(common::MASK_WIDTH) * aspect,
        0.5 * v.f64(common::MASK_HEIGHT),
    ];
    let corner = |x: f64, y: f64| {
        let (x, y) = (x * half[0], y * half[1]);
        let (rx, ry) = (x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos());
        [cx + rx / aspect.max(1e-6), 1.0 - (cy + ry)]
    };
    Some([
        corner(-1.0, 1.0),
        corner(1.0, 1.0),
        corner(1.0, -1.0),
        corner(-1.0, -1.0),
    ])
}
