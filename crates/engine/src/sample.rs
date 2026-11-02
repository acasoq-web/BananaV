//! BananaV's own sample picture, on which the Video FX window shows what
//! each effect and preset does: an evening sky with the sun, hills, a lake
//! and trees — colors, light and edges for every effect to change. The
//! Transitions window shows plug-ins' transitions from it to its night.

use std::sync::{Arc, OnceLock};

use bv_core::{FrameRate, ImageRgbaF32, Tick};
use bv_timeline::scene::{FxOwner, PluginCall, PluginEffects};
use bv_timeline::{ClipId, Effect, PluginFx};

use crate::plugins::{OfxEffects, Purpose};

/// A frame the Video Preview or the Trimmer showed (8-bit, BT.709's
/// values) as an effect gets its picture: video's values when the project
/// composites on them (`video_gamma`), else light; premultiplied.
pub fn frame_picture(f: &crate::PreviewFrame, video_gamma: bool) -> ImageRgbaF32 {
    let data = f
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let v = |c: u8| {
                let x = f32::from(c) / 255.0;
                if video_gamma {
                    x
                } else {
                    bv_color::transfer::bt709_to_linear(x)
                }
            };
            let a = f32::from(p[3]) / 255.0;
            [v(p[0]) * a, v(p[1]) * a, v(p[2]) * a, a]
        })
        .collect();
    ImageRgbaF32 {
        width: f.width,
        height: f.height,
        data,
    }
}

/// The first frame of the picture (or video) file `path` as an effect
/// gets its picture (see [`frame_picture`]), reduced while it is twice
/// 1920 × 1080 or more.
pub fn file_picture(path: &str, video_gamma: bool) -> bv_media::Result<ImageRgbaF32> {
    let info = bv_media::probe_file(path)?;
    let mut pool = bv_media::VideoPool::new();
    let img = pool.frame_scaled(&info, Tick::ZERO, 1920, 1080)?;
    let mut img = (*img).clone();
    bv_render::enter_chain(&mut img, video_gamma, None);
    Ok(img)
}

/// The picture, `width`×`height`, in linear light.
pub fn sample_picture(width: u32, height: u32) -> ImageRgbaF32 {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let aspect = w / h;
    let mut img = ImageRgbaF32::filled(width, height, [0.0, 0.0, 0.0, 1.0]);
    for y in 0..height {
        for x in 0..width {
            let u = (x as f32 + 0.5) / w;
            let v = (y as f32 + 0.5) / h;
            let c = scene(u, v, aspect).map(bv_color::transfer::bt709_to_linear);
            *img.pixel_mut(x, y) = [c[0], c[1], c[2], 1.0];
        }
    }
    img
}

/// `effects` on the sample picture, `width`×`height`, as 8-bit RGBA ready
/// to show (straight colors: what an effect cuts away is see-through). Sizes in the effects (a blur's radius, the blocks of pixelate)
/// are for the project's frames: here they shrink with the picture, as on
/// a 1080-line frame.
pub fn effect_sample(effects: &[Effect], width: u32, height: u32) -> Vec<u8> {
    crate::plugins::register_own();
    if effects.iter().any(|e| e.plugin.is_some()) {
        // Plug-ins run on a thread with their stack.
        let effects = effects.to_vec();
        return on_plugin_stack(move || draw_effects(&effects, width, height)).unwrap_or_default();
    }
    draw_effects(effects, width, height)
}

/// Runs `f` on a thread with the stack plug-ins want; `None` when no
/// thread could be made.
fn on_plugin_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::Builder::new()
        .name("bv-sample".into())
        .stack_size(bv_ofx::PLUGIN_STACK)
        .spawn(f)
        .ok()?
        .join()
        .ok()
}

fn draw_effects(effects: &[Effect], width: u32, height: u32) -> Vec<u8> {
    let mut img = run_effects(sample_picture(width, height), effects);
    // Straight colors and their alpha: what an effect makes see-through
    // shows the window's squares under it.
    for px in &mut img.data {
        if px[3] > 0.0 && px[3] < 1.0 {
            for c in 0..3 {
                px[c] /= px[3];
            }
        }
    }
    bv_color::linear_to_display_rgba8(&img, bv_core::TransferCharacteristic::Bt709)
}

/// The enabled ones of `effects` on `img`, in order: BananaV's own worked
/// out here, a plug-in drawing itself at its defaults' time. Sizes in the
/// effects are for 1080-line frames: they shrink with the picture.
fn run_effects(mut img: ImageRgbaF32, effects: &[Effect]) -> ImageRgbaF32 {
    let (width, height) = (img.width, img.height);
    let scale = height.max(1) as f32 / 1080.0;
    for (i, e) in effects.iter().enumerate().filter(|(_, e)| e.enabled) {
        match &e.plugin {
            // A plug-in's draws itself, at its defaults' time.
            Some(fx) => {
                let call = PluginCall {
                    fx,
                    owner: FxOwner::Output,
                    index: i,
                    time: Tick::ZERO,
                    duration: Tick::from_seconds(10),
                    timeline: Tick::ZERO,
                    rate: FrameRate::FPS_30,
                    full_size: (width, height),
                    source: None,
                };
                if let Some(out) = samples().apply(&call, Arc::new(img.clone())) {
                    img = out;
                }
            }
            None => bv_render::effects::apply(&mut img, e, scale),
        }
    }
    img
}

/// The enabled ones of `effects` as a `.cube` 3D table `size` points a
/// side (2–129), titled `title`: what the chain makes of each color of
/// the table's lattice — the Plug-In Chooser's Save LUT. A table's colors
/// are encoded ones (video's values); a project that composites on light
/// (`video_gamma` false) hands its chains light, so there the lattice goes
/// in, and comes out, through BT.709's transfer. What changes a picture
/// other than by its colors (a blur, a mask, a vignette) comes out as the
/// lattice's neighbors and places make it: nothing a table can hold.
/// `None` when the chain changed the picture's size.
pub fn effect_chain_cube(
    effects: &[Effect],
    size: usize,
    video_gamma: bool,
    title: &str,
) -> Option<String> {
    crate::plugins::register_own();
    let size = size.clamp(2, 129);
    let title = title.replace('"', "'");
    if effects.iter().any(|e| e.enabled && e.plugin.is_some()) {
        // Plug-ins run on a thread with their stack.
        let effects = effects.to_vec();
        return on_plugin_stack(move || chain_cube(&effects, size, video_gamma, &title)).flatten();
    }
    chain_cube(effects, size, video_gamma, &title)
}

fn chain_cube(effects: &[Effect], size: usize, video_gamma: bool, title: &str) -> Option<String> {
    let n = size as u32;
    let step = 1.0 / (size - 1) as f32;
    // The lattice: red along each row, green down a block, blue from block
    // to block — read in rows, red changes fastest, as the format has it.
    let mut img = ImageRgbaF32::filled(n, n * n, [0.0, 0.0, 0.0, 1.0]);
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let [r_v, g_v, b_v] = [r, g, b].map(|k| {
                    let v = k as f32 * step;
                    if video_gamma {
                        v
                    } else {
                        bv_color::transfer::bt709_to_linear(v)
                    }
                });
                *img.pixel_mut(r, g + b * n) = [r_v, g_v, b_v, 1.0];
            }
        }
    }
    let img = run_effects(img, effects);
    if img.width != n || img.height != n * n {
        return None;
    }
    let mut out = String::with_capacity(size * size * size * 30 + 100);
    out.push_str(&format!(
        "TITLE \"{title}\"\nLUT_3D_SIZE {size}\nDOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n"
    ));
    for px in &img.data {
        // Straight colors, encoded.
        let a = px[3];
        let c = |i: usize| {
            let v = if a > 0.0 && a < 1.0 { px[i] / a } else { px[i] };
            if video_gamma {
                v
            } else {
                bv_color::transfer::linear_to_bt709(v.max(0.0))
            }
        };
        out.push_str(&format!("{:.6} {:.6} {:.6}\n", c(0), c(1), c(2)));
    }
    Some(out)
}

/// The sample picture's night: mirrored, in the blue of the evening's end
/// (what a transition's sample goes to).
pub fn night_picture(width: u32, height: u32) -> ImageRgbaF32 {
    let day = sample_picture(width, height);
    let mut img = day.clone();
    for y in 0..height {
        for x in 0..width {
            let c = day.pixel(width - 1 - x, y);
            *img.pixel_mut(x, y) = [c[0] * 0.22, c[1] * 0.35, c[2] * 0.8 + 0.01, 1.0];
        }
    }
    img
}

/// A transition plug-in going from the sample picture to its night,
/// `progress` (0–1) of the way, `width`×`height`, as 8-bit RGBA ready to
/// show; `None` when the plug-in cannot draw it.
pub fn transition_sample(fx: &PluginFx, progress: f32, width: u32, height: u32) -> Option<Vec<u8>> {
    let fx = fx.clone();
    on_plugin_stack(move || draw_transition(&fx, progress, width, height)).flatten()
}

fn draw_transition(fx: &PluginFx, progress: f32, width: u32, height: u32) -> Option<Vec<u8>> {
    let call = PluginCall {
        fx,
        // The samples' own transition.
        owner: FxOwner::Transition(ClipId(u64::MAX)),
        index: 0,
        time: Tick::from_seconds_f64(progress as f64),
        duration: Tick::SECOND,
        timeline: Tick::from_seconds_f64(progress as f64),
        rate: FrameRate::FPS_30,
        full_size: (width, height),
        source: None,
    };
    let from = Arc::new(sample_picture(width, height));
    let to = Arc::new(night_picture(width, height));
    let img = samples().transition(&call, from, to, progress)?;
    Some(bv_color::linear_to_display_rgba8(
        &img,
        bv_core::TransferCharacteristic::Bt709,
    ))
}

/// The map a compositor's sample is guided by: ripples spreading from the
/// middle, `phase` (0–1) of a wave along, in gray.
pub fn ripple_map(width: u32, height: u32, phase: f32) -> ImageRgbaF32 {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let mut img = ImageRgbaF32::filled(width, height, [0.5, 0.5, 0.5, 1.0]);
    for y in 0..height {
        for x in 0..width {
            let dx = ((x as f32 + 0.5) - w / 2.0) / h;
            let dy = ((y as f32 + 0.5) - h / 2.0) / h;
            let d = dx.hypot(dy);
            let wave = (std::f32::consts::TAU * (d * 6.0 - phase)).sin() * (-d * 2.0).exp();
            let v = 0.5 + 0.5 * wave;
            *img.pixel_mut(x, y) = [v, v, v, 1.0];
        }
    }
    img
}

/// A compositor plug-in drawing the sample picture as [`ripple_map`] at
/// `phase` guides it, `width`×`height`, as 8-bit RGBA ready to show;
/// `None` when the plug-in cannot draw it.
pub fn compositor_sample(fx: &PluginFx, phase: f32, width: u32, height: u32) -> Option<Vec<u8>> {
    let fx = fx.clone();
    on_plugin_stack(move || {
        crate::plugins::register_own();
        let call = PluginCall {
            fx: &fx,
            // The samples' own compositor.
            owner: FxOwner::Compositor(bv_timeline::TrackId(u64::MAX), false),
            index: 0,
            time: Tick::ZERO,
            duration: Tick::from_seconds(10),
            timeline: Tick::ZERO,
            rate: FrameRate::FPS_30,
            full_size: (width, height),
            source: None,
        };
        let source = Arc::new(sample_picture(width, height));
        let map = Arc::new(ripple_map(width, height, phase));
        let img = samples().composite(&call, source, map)?;
        Some(bv_color::linear_to_display_rgba8(
            &img,
            bv_core::TransferCharacteristic::Bt709,
        ))
    })
    .flatten()
}

/// The plug-in effects of the samples.
fn samples() -> &'static OfxEffects {
    static SAMPLES: OnceLock<Arc<OfxEffects>> = OnceLock::new();
    SAMPLES.get_or_init(|| OfxEffects::new(Purpose::Preview))
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// The sky above the horizon at `v` (0 at the top).
fn sky(v: f32) -> [f32; 3] {
    const TOP: [f32; 3] = [0.10, 0.16, 0.40];
    const MIDDLE: [f32; 3] = [0.50, 0.34, 0.58];
    const LOW: [f32; 3] = [0.99, 0.66, 0.34];
    if v < 0.3 {
        mix(TOP, MIDDLE, v / 0.3)
    } else {
        mix(MIDDLE, LOW, (v - 0.3) / 0.32)
    }
}

/// The scene's color (display-referred, 0–1) at `u`, `v` (0–1 across and
/// down); `aspect`: width ÷ height, for round shapes.
fn scene(u: f32, v: f32, aspect: f32) -> [f32; 3] {
    const HORIZON: f32 = 0.66;
    const SUN: (f32, f32, f32) = (0.7, 0.44, 0.1);
    let far = 0.5 + 0.05 * (u * 9.0 + 0.6).sin() + 0.025 * (u * 23.0).sin();
    let near = 0.58 + 0.06 * (u * 5.0 + 2.1).sin() + 0.02 * (u * 31.0 + 0.4).sin();
    // Trees on the near shore: dark triangles.
    for (x, top, half) in [(0.08, 0.44, 0.05), (0.15, 0.5, 0.04), (0.86, 0.47, 0.045)] {
        if v >= top && v <= HORIZON {
            let reach = half * (v - top) / (HORIZON - top);
            if (u - x).abs() * aspect <= reach * aspect.max(1.0) {
                return [0.06, 0.2, 0.1];
            }
        }
    }
    if v < HORIZON {
        if v > near {
            return mix([0.16, 0.34, 0.18], [0.1, 0.24, 0.12], (v - near) / 0.1);
        }
        if v > far {
            return mix([0.32, 0.22, 0.42], [0.22, 0.17, 0.32], (v - far) / 0.12);
        }
        let mut c = sky(v);
        // The sun, and its glow.
        let (dx, dy) = ((u - SUN.0) * aspect, v - SUN.1);
        let d = (dx * dx + dy * dy).sqrt();
        if d < SUN.2 {
            return [1.0, 0.93, 0.66];
        }
        let glow = (1.0 - (d - SUN.2) / 0.25).max(0.0);
        c = mix(c, [1.0, 0.82, 0.5], glow * glow * 0.6);
        return c;
    }
    // The lake: the sky mirrored, darker, with ripples.
    let mirrored = HORIZON - (v - HORIZON) * 1.6;
    let mut c = sky(mirrored.max(0.0));
    let ripple = 0.5 + 0.5 * ((v * 140.0) + (u * 9.0).sin() * 2.0).sin();
    c = mix(c, [0.05, 0.08, 0.18], 0.35 + 0.15 * ripple);
    // The sun's path on the water.
    let lane = (1.0 - ((u - SUN.0) * aspect).abs() / 0.06).max(0.0);
    mix(c, [1.0, 0.86, 0.55], lane * ripple * 0.7)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::EffectKind;

    #[test]
    fn the_sample_has_sky_sun_and_water() {
        let img = sample_picture(160, 96);
        let top = img.pixel(10, 2);
        let sun = img.pixel(112, 42);
        let lake = img.pixel(40, 90);
        assert!(top[2] > top[0], "a blue sky: {top:?}");
        assert!(sun[0] > 0.8 && sun[1] > 0.7, "the sun: {sun:?}");
        assert!(lake[0] < sun[0], "the water darker: {lake:?}");
    }

    #[test]
    fn effects_change_the_sample() {
        let plain = effect_sample(&[], 64, 40);
        assert_eq!(plain.len(), 64 * 40 * 4);
        let bw = effect_sample(&[Effect::new(EffectKind::BlackAndWhite)], 64, 40);
        assert_ne!(plain, bw);
        // Black and white: grey everywhere.
        for px in bw.as_chunks::<4>().0 {
            assert!(
                px[0].abs_diff(px[1]) <= 1 && px[1].abs_diff(px[2]) <= 1,
                "{px:?}"
            );
        }
        let mut off = Effect::new(EffectKind::Invert);
        off.enabled = false;
        assert_eq!(effect_sample(&[off], 64, 40), plain, "a bypassed effect");
    }

    /// A `.cube` text's points, in its order.
    fn points(cube: &str) -> Vec<[f32; 3]> {
        cube.lines()
            .filter(|l| l.starts_with(|c: char| c.is_ascii_digit() || c == '-'))
            .map(|l| {
                let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
                [v[0], v[1], v[2]]
            })
            .collect()
    }

    #[test]
    fn a_chain_becomes_a_table_of_its_colors() {
        // No effects: the lattice itself, red changing fastest.
        let cube = effect_chain_cube(&[], 3, true, "Plain").unwrap();
        assert!(
            cube.starts_with("TITLE \"Plain\"\nLUT_3D_SIZE 3\n"),
            "{cube}"
        );
        let p = points(&cube);
        assert_eq!(p.len(), 27);
        assert_eq!(p[1], [0.5, 0.0, 0.0]);
        assert_eq!(p[3], [0.0, 0.5, 0.0]);
        assert_eq!(p[9], [0.0, 0.0, 0.5]);
        // Invert on video's values: each color's negative.
        let invert = [Effect::new(EffectKind::Invert)];
        let p = points(&effect_chain_cube(&invert, 3, true, "Invert").unwrap());
        assert_eq!(p[0], [1.0, 1.0, 1.0]);
        assert_eq!(p[26], [0.0, 0.0, 0.0]);
        // On light, through the transfer both ways: black and white still
        // swap, the middle is not the encoded half's negative.
        let p = points(&effect_chain_cube(&invert, 3, false, "Invert").unwrap());
        assert!(
            (p[0][0] - 1.0).abs() < 1e-4 && p[26][0].abs() < 1e-4,
            "{p:?}"
        );
        assert!((p[13][0] - 0.5).abs() > 0.01, "{:?}", p[13]);
        // A bypassed effect is left out.
        let mut off = Effect::new(EffectKind::Invert);
        off.enabled = false;
        let p = points(&effect_chain_cube(&[off], 3, true, "Off").unwrap());
        assert_eq!(p[0], [0.0, 0.0, 0.0]);
    }
}
