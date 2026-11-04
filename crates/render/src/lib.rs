//! Composição de vídeo em CPU.
//!
//! É o caminho de referência: o compositor em GPU (wgpu) deve produzir os
//! mesmos quadros, e os testes comparam os dois. Tudo acontece em RGB linear
//! BT.709, float 32, alfa pré-multiplicado.
//!
//! Para cada trilha de vídeo (de baixo para cima): o clipe visível no
//! instante passa pelos efeitos do clipe, é encaixado com o pan/crop, recebe
//! fades e — se estiver sobre o clipe anterior da mesma trilha — a transição
//! de entrada (crossfade automático). A trilha então recebe os próprios
//! efeitos e é composta com o nível dela sobre o que está embaixo.

pub mod effects;
mod flow;
mod layer;
mod par;
mod space;
pub mod transition;

use std::sync::Arc;

use bv_core::{ImageRgbaF32, Tick};
use bv_media::{Ask, Picture, Want};
use bv_timeline::scene::{EventLayer, FrameScene, Framing, MediaLayer, TrackContent};
use bv_timeline::{OutputRotation, Project, RenderQuality};

pub use bv_color::convert::set_threads;
pub use bv_media::VideoPool;
pub use bv_timeline::scene::{Rect, fit_rect, fit_rect_par};
pub use layer::place_par;
pub use layer::{place, place_box, place_with};

/// Cor mostrada no lugar de mídia que não pôde ser lida ("mídia offline").
pub const OFFLINE: [f32; 4] = [0.25, 0.0, 0.02, 1.0];

/// Operador "over" de Porter-Duff, alfa pré-multiplicado.
#[inline]
pub fn over(src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    let k = 1.0 - src[3];
    [
        src[0] + dst[0] * k,
        src[1] + dst[1] * k,
        src[2] + dst[2] * k,
        src[3] + dst[3] * k,
    ]
}

/// Compõe o quadro do instante `t` da timeline em `width` × `height`.
///
/// O projeto pode ser renderizado em resolução menor que a dele (prévia em
/// meia ou um quarto de resolução): as camadas são encaixadas no tamanho pedido
/// e os parâmetros em pixels dos efeitos são escalados.
pub fn render_frame(
    project: &Project,
    t: Tick,
    width: u32,
    height: u32,
    pool: &mut VideoPool,
) -> ImageRgbaF32 {
    render_frame_on(project, t, width, height, pool, [0.0, 0.0, 0.0, 1.0])
}

/// [`render_frame`] over `background` (transparent for files that keep the
/// alpha channel).
pub fn render_frame_on(
    project: &Project,
    t: Tick,
    width: u32,
    height: u32,
    pool: &mut VideoPool,
    background: [f32; 4],
) -> ImageRgbaF32 {
    let scene = bv_timeline::scene::frame_scene(project, t, width, height);
    render_scene(&scene, pool, background)
}

/// Draws the frame `scene` describes, over `background`.
pub fn render_scene(
    scene: &FrameScene<'_>,
    pool: &mut VideoPool,
    background: [f32; 4],
) -> ImageRgbaF32 {
    let (width, height) = (scene.width, scene.height);
    let background = match &scene.color {
        Some(p) => {
            let c = p.enter([background[0], background[1], background[2]]);
            [c[0], c[1], c[2], background[3]]
        }
        None => background,
    };
    let mut out = ImageRgbaF32::filled(width, height, background);
    if scene.muted {
        if let Some(p) = &scene.color {
            leave_through_view(&mut out, p);
        }
        return out;
    }
    pool.prefetch_scene(scene, Want::Linear);
    let units = bv_timeline::scene::composite_units(&scene.tracks);
    out = draw_units(scene, &units, out, pool);
    // Tools › Video › Video Output FX: applied to the finished frame.
    effects::apply_chain(scene, &mut out, None, scene.output_fx, scene.scale);
    // The video bus's Fade to Color, over everything.
    if let Some((c, v)) = scene.output_fade {
        let w = width as usize;
        par::for_each_pixel(&mut out.data, w, |_, _, px| {
            let a = px[3];
            *px = [
                px[0] * (1.0 - v) + c[0] * v * a,
                px[1] * (1.0 - v) + c[1] * v * a,
                px[2] * (1.0 - v) + c[2] * v * a,
                a,
            ];
        });
    }
    // Back to light (under ACES the view takes the values as they are);
    // the 8-bit formats cannot hold values outside 0–1.
    if scene.video_gamma && scene.color.is_none() {
        let clip = scene.clips;
        let w = width as usize;
        par::for_each_pixel(&mut out.data, w, |_, _, px| {
            *px = from_video_gamma(*px, clip);
        });
    }
    if let Some(p) = &scene.color {
        leave_through_view(&mut out, p);
    }
    out
}

/// The finished frame through ACES's look and view: each straight color
/// to the display's code values, then taken as video's (BT.709's curve)
/// light, as the preview and the encoders take frames — they show and
/// write those code values.
pub fn leave_through_view(img: &mut ImageRgbaF32, p: &bv_timeline::ColorPipeline) {
    let w = img.width as usize;
    par::for_each_pixel(&mut img.data, w, |_, _, px| {
        let a = px[3];
        if a <= 0.0 {
            *px = [0.0, 0.0, 0.0, a];
            return;
        }
        let v = p
            .view
            .apply([px[0], px[1], px[2]].map(|c| f64::from(c / a)));
        let l = v.map(|c| bv_color::transfer::bt709_to_linear(c as f32) * a);
        *px = [l[0], l[1], l[2], a];
    });
}

/// The finished frame as it leaves the project: [`render_frame`] turned by
/// the output rotation (File › Properties › Video). `width` × `height` is
/// the output size, already turned (see `ProjectSettings::output_size`).
pub fn render_output(
    project: &Project,
    t: Tick,
    width: u32,
    height: u32,
    pool: &mut VideoPool,
) -> ImageRgbaF32 {
    render_output_on(project, t, width, height, pool, [0.0, 0.0, 0.0, 1.0])
}

/// [`render_output`] over `background`.
pub fn render_output_on(
    project: &Project,
    t: Tick,
    width: u32,
    height: u32,
    pool: &mut VideoPool,
    background: [f32; 4],
) -> ImageRgbaF32 {
    render_output_as(project, t, width, height, pool, background, None)
}

/// [`render_output_on`] with the pictures scaled at `quality` (`None`: as
/// the project says) — the Video Preview's own quality.
pub fn render_output_as(
    project: &Project,
    t: Tick,
    width: u32,
    height: u32,
    pool: &mut VideoPool,
    background: [f32; 4],
    quality: Option<RenderQuality>,
) -> ImageRgbaF32 {
    render_output_with(project, t, (width, height), pool, background, quality, None)
}

/// [`render_output_as`] with plug-in effects drawn by `plugins`.
pub fn render_output_with(
    project: &Project,
    t: Tick,
    (width, height): (u32, u32),
    pool: &mut VideoPool,
    background: [f32; 4],
    quality: Option<RenderQuality>,
    plugins: Option<bv_timeline::scene::Plugins<'_>>,
) -> ImageRgbaF32 {
    render_output_held(
        project,
        t,
        None,
        (width, height),
        pool,
        background,
        quality,
        plugins,
    )
}

/// [`render_output_with`] for a sub-frame of the video bus's motion blur
/// (`held`: the frame's time, which the tracks bypassing it show).
#[allow(clippy::too_many_arguments)]
pub fn render_output_held(
    project: &Project,
    t: Tick,
    held: Option<Tick>,
    (width, height): (u32, u32),
    pool: &mut VideoPool,
    background: [f32; 4],
    quality: Option<RenderQuality>,
    plugins: Option<bv_timeline::scene::Plugins<'_>>,
) -> ImageRgbaF32 {
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
    if let Some(q) = quality {
        scene.set_quality(q);
    }
    scene.plugins = plugins;
    rotate(&render_scene(&scene, pool, background), rotation)
}

/// `img` turned by `rotation`.
pub fn rotate(img: &ImageRgbaF32, rotation: OutputRotation) -> ImageRgbaF32 {
    let (sw, sh) = (img.width as usize, img.height as usize);
    let (w, h) = match rotation {
        OutputRotation::None => return img.clone(),
        OutputRotation::Inverted180 => (sw, sh),
        OutputRotation::Clockwise90 | OutputRotation::Counterclockwise90 => (sh, sw),
    };
    let mut out = ImageRgbaF32::transparent(w as u32, h as u32);
    par::for_each_pixel(&mut out.data, w, |x, y, px| {
        let (sx, sy) = match rotation {
            OutputRotation::Clockwise90 => (y, sh - 1 - x),
            OutputRotation::Counterclockwise90 => (sw - 1 - y, x),
            _ => (sw - 1 - x, sh - 1 - y),
        };
        *px = img.data[sy * sw + sx];
    });
    out
}

/// `img` reduced to `w`×`h` as the compositors reduce media pictures for
/// their effects: halved while it is twice as big or more, then scaled to
/// the size when it is not that yet.
pub fn reduce_to(img: &ImageRgbaF32, w: u32, h: u32) -> ImageRgbaF32 {
    let mut out = img.clone();
    while out.width >= w.max(1) * 2 && out.height >= h.max(1) * 2 && out.width > 1 {
        out = layer::half_size(&out);
    }
    if (out.width, out.height) == (w, h) {
        out
    } else {
        place(&out, w, h, &Default::default())
    }
}

/// How a media picture enters ACES's working space.
#[derive(Clone, Copy, Debug)]
pub enum Enter<'a> {
    /// Taken as ACES2065-1 (VEGAS's Default): its light, or with the 2.222
    /// gamma its encoded values, through the pipeline's `input`.
    Matrix(&'a [[f32; 3]; 3]),
    /// Its file's values (encoded again: a camera's log as it was
    /// recorded) in the color space Media Properties gives, through the
    /// conversion from it (`ColorPipeline::media_input`).
    Space(&'a bv_ocio::Processor),
}

impl<'a> Enter<'a> {
    /// How `m`'s picture enters `p`'s working space.
    pub fn of(
        p: &'a bv_timeline::ColorPipeline,
        space: Option<&'a bv_ocio::Processor>,
    ) -> Enter<'a> {
        space.map_or(Enter::Matrix(&p.input), Enter::Space)
    }
}

/// A media picture as it enters an effect chain: in video gamma when the
/// project composites on the encoded values (`video_gamma`), else as it is;
/// then under ACES in its working space, as `enter` says.
pub fn enter_chain(img: &mut ImageRgbaF32, video_gamma: bool, enter: Option<Enter<'_>>) {
    let w = img.width as usize;
    if video_gamma || matches!(enter, Some(Enter::Space(_))) {
        par::for_each_pixel(&mut img.data, w, |_, _, px| *px = to_video_gamma(*px));
    }
    match enter {
        // On the premultiplied colors: a matrix keeps them so.
        Some(Enter::Matrix(m)) => par::for_each_pixel(&mut img.data, w, |_, _, px| {
            let c = [0, 1, 2].map(|i| m[i][0] * px[0] + m[i][1] * px[1] + m[i][2] * px[2]);
            *px = [c[0], c[1], c[2], px[3]];
        }),
        // Curves work on the straight colors.
        Some(Enter::Space(q)) => par::for_each_pixel(&mut img.data, w, |_, _, px| {
            let a = px[3];
            if a <= 0.0 {
                *px = [0.0, 0.0, 0.0, a];
                return;
            }
            let c = q.apply_f32([px[0] / a, px[1] / a, px[2] / a]);
            *px = [c[0] * a, c[1] * a, c[2] * a, a];
        }),
        None => {}
    }
}

/// Light → video gamma (BT.709 encoding), for compositing on the encoded
/// values. Works on the straight color of premultiplied pixels.
fn to_video_gamma(px: [f32; 4]) -> [f32; 4] {
    map_straight(px, bv_color::transfer::linear_to_bt709)
}

fn from_video_gamma(px: [f32; 4], clip: bool) -> [f32; 4] {
    let px = if clip {
        let a = px[3].clamp(0.0, 1.0);
        [
            px[0].clamp(0.0, a),
            px[1].clamp(0.0, a),
            px[2].clamp(0.0, a),
            a,
        ]
    } else {
        px
    };
    map_straight(px, bv_color::transfer::bt709_to_linear)
}

fn map_straight(px: [f32; 4], f: impl Fn(f32) -> f32) -> [f32; 4] {
    let a = px[3];
    if a <= 0.0 {
        return px;
    }
    // Negative light (out-of-gamut effect results) keeps its sign.
    let g = |c: f32| (c / a).signum() * f((c / a).abs()) * a;
    [g(px[0]), g(px[1]), g(px[2]), a]
}

fn scale_layer(img: &mut ImageRgbaF32, k: f32) {
    if (k - 1.0).abs() < 1e-6 {
        return;
    }
    let w = img.width as usize;
    par::for_each_pixel(&mut img.data, w, |_, _, px| *px = px.map(|c| c * k));
}

/// A track's picture as it composites: its content through its effects
/// (those before its Composite button), toward its fade color, at its
/// level.
fn finished_layer(
    scene: &FrameScene<'_>,
    track: &bv_timeline::scene::TrackScene<'_>,
    pool: &mut VideoPool,
    below: &ImageRgbaF32,
) -> ImageRgbaF32 {
    // An adjustment track's picture is what is below it (its events over
    // it: `adjust_into`), through its effects (its level mixes it in).
    let mut layer = if track.adjustment {
        below.clone()
    } else {
        track_layer(scene, &track.content, pool, below)
    };
    effects::apply_chain(
        scene,
        &mut layer,
        Some(track.track),
        &track.effects[..track.post_from],
        scene.scale,
    );
    let w = layer.width as usize;
    if let Some((c, v)) = track.fade {
        // Toward the fade color, where the track shows.
        par::for_each_pixel(&mut layer.data, w, |_, _, px| {
            let a = px[3];
            *px = [
                px[0] * (1.0 - v) + c[0] * v * a,
                px[1] * (1.0 - v) + c[1] * v * a,
                px[2] * (1.0 - v) + c[2] * v * a,
                a,
            ];
        });
    }
    if let Some(m) = &track.motion {
        layer = moved(&layer, m);
    }
    // A compositor reads the picture as it is: its level mixes the result.
    if !track.adjustment && scene.compositor_call(track, false).is_none() {
        scale_layer(&mut layer, track.level);
    }
    layer
}

/// A picture moved as a Track Motion says, its 2D shadow and glow under
/// it.
fn moved(layer: &ImageRgbaF32, m: &bv_timeline::MotionAt) -> ImageRgbaF32 {
    let (w, h) = (layer.width, layer.height);
    let picture = layer::place_with(layer, w, h, &m.transform, m.quality);
    if m.shadow.is_none() && m.glow.is_none() {
        return picture;
    }
    let mut out = ImageRgbaF32::transparent(w, h);
    for look in [m.shadow, m.glow].into_iter().flatten() {
        let halo = halo(&picture, &m.transform, &look, m.quality);
        over_into(&mut out, &halo);
    }
    over_into(&mut out, &picture);
    out
}

/// A 2D shadow or glow of `picture` (placed by `tf`): its shape moved,
/// scaled about the box's center, blurred and in the look's color.
fn halo(
    picture: &ImageRgbaF32,
    tf: &bv_timeline::Transform,
    look: &bv_timeline::HaloLook,
    quality: bv_timeline::RenderQuality,
) -> ImageRgbaF32 {
    let (w, h) = (picture.width, picture.height);
    let s = look.size.max(0.01);
    let at = bv_timeline::Transform {
        x: tf.x * (1.0 - s) + look.x,
        y: tf.y * (1.0 - s) + look.y,
        scale: s,
        ..bv_timeline::Transform::default()
    };
    let mut shape = layer::place_with(picture, w, h, &at, quality);
    let radius = look.blur.clamp(0.0, 1.0) * bv_timeline::motion::HALO_BLUR * w as f32;
    if radius >= 0.5 {
        shape = effects::blur(&shape, radius);
    }
    let k = look.intensity.clamp(0.0, 1.0) * look.color[3];
    let c = look.color;
    par::for_each_pixel(&mut shape.data, w as usize, |_, _, px| {
        let a = px[3] * k;
        *px = [c[0] * a, c[1] * a, c[2] * a, a];
    });
    shape
}

/// `top` over `under`.
fn over_into(under: &mut ImageRgbaF32, top: &ImageRgbaF32) {
    let w = under.width as usize;
    par::for_each_pixel(&mut under.data, w, |x, y, px| {
        *px = over(top.data[y * w + x], *px);
    });
}

/// Draws `units` (top first) over `under`, the bottom one first, as
/// VEGAS composites tracks: a track through its compositing mode; a
/// compositing parent's children first, over nothing, then the parent
/// through its mode, and that group through its parent composite mode.
/// A group's track's effects after its Composite button then work on the
/// group's picture.
fn draw_units(
    scene: &FrameScene<'_>,
    units: &[bv_timeline::scene::CompositeUnit],
    mut under: ImageRgbaF32,
    pool: &mut VideoPool,
) -> ImageRgbaF32 {
    // 3D tracks next to one another, drawn together when a 2D one (or the
    // end) comes: a 2D track is a barrier between them.
    let mut space: Vec<(ImageRgbaF32, bv_timeline::space::Plane)> = Vec::new();
    let (w, h) = (scene.width as f32, scene.height as f32);
    for u in units.iter().rev() {
        let t = &scene.tracks[u.track];
        if u.children.is_empty() {
            if t.adjustment {
                flush_space(&mut space, &mut under, scene);
                adjust_into(scene, t, pool, &mut under);
                continue;
            }
            let layer = finished_layer(scene, t, pool, &under);
            if let Some(b) = t.space {
                space.push((layer, b.plane(w, h)));
                continue;
            }
            flush_space(&mut space, &mut under, scene);
            composite_into(scene, t, false, &mut under, layer, t.level);
            post_composite(scene, t, &mut under);
        } else {
            let nothing = ImageRgbaF32::transparent(scene.width, scene.height);
            let mut group = draw_units(scene, &u.children, nothing, pool);
            if t.adjustment {
                adjust_into(scene, t, pool, &mut group);
            } else {
                let layer = finished_layer(scene, t, pool, &group);
                composite_into(scene, t, false, &mut group, layer, t.level);
                post_composite(scene, t, &mut group);
            }
            // 3D Parent Motion: the group's picture on its box in space.
            if let Some(b) = t.parent_space {
                space.push((group, b.plane(w, h)));
                continue;
            }
            flush_space(&mut space, &mut under, scene);
            // Parent Motion: the whole group moved.
            if let Some(m) = &t.parent_motion {
                group = moved(&group, m);
            }
            composite_into(scene, t, true, &mut under, group, 1.0);
        }
    }
    flush_space(&mut space, &mut under, scene);
    under
}

/// An adjustment track over `under` (in place): its events over it, then
/// its effects on all of that, mixed in by its level (their strength).
fn adjust_into(
    scene: &FrameScene<'_>,
    t: &bv_timeline::scene::TrackScene<'_>,
    pool: &mut VideoPool,
    under: &mut ImageRgbaF32,
) {
    if !matches!(t.content, TrackContent::Nothing) {
        let own = track_layer(scene, &t.content, pool, under);
        over_into(under, &own);
    }
    if t.level > 0.0 {
        let layer = finished_layer(scene, t, pool, under);
        mix_into(under, &layer, t.level);
    }
}

/// The 3D tracks gathered, drawn together and laid over `under` by their
/// alpha.
fn flush_space(
    space: &mut Vec<(ImageRgbaF32, bv_timeline::space::Plane)>,
    under: &mut ImageRgbaF32,
    scene: &FrameScene<'_>,
) {
    if space.is_empty() {
        return;
    }
    let picture = space::draw(space, scene.width, scene.height);
    space.clear();
    blend_into(under, &picture, bv_timeline::CompositeMode::SourceAlpha);
}

/// A track's effects after its Composite button
/// (`TrackScene::post_from`), on its group's picture once the track
/// composited into it: VEGAS's post-compositing track effects.
fn post_composite(
    scene: &FrameScene<'_>,
    t: &bv_timeline::scene::TrackScene<'_>,
    group: &mut ImageRgbaF32,
) {
    effects::apply_chain_from(
        scene,
        group,
        Some(t.track),
        t.effects,
        t.post_from,
        scene.scale,
    );
}

/// `top` combined with `under` by track `t`'s Compositing Mode (`parent`:
/// its Parent Composite Mode): a Custom one's compositor draws `under` as
/// `top` guides it, `level` of the way; the others blend (`top` already at
/// its level). A compositor that cannot draw leaves Source Alpha.
fn composite_into(
    scene: &FrameScene<'_>,
    t: &bv_timeline::scene::TrackScene<'_>,
    parent: bool,
    under: &mut ImageRgbaF32,
    mut top: ImageRgbaF32,
    level: f32,
) {
    let mode = if parent { t.parent_mode } else { t.mode };
    if let Some(call) = scene.compositor_call(t, parent) {
        if let Some(plugins) = scene.plugins
            && let Some(out) =
                plugins
                    .0
                    .composite(&call, Arc::new(under.clone()), Arc::new(top.clone()))
        {
            mix_into(under, &out, level);
            return;
        }
        // Not drawn: Source Alpha, at its level.
        if !parent {
            scale_layer(&mut top, level);
        }
    }
    blend_into(under, &top, mode);
}

/// `under` moved `k` of the way to `top` (an adjustment track's strength).
fn mix_into(under: &mut ImageRgbaF32, top: &ImageRgbaF32, k: f32) {
    let w = under.width as usize;
    par::for_each_pixel(&mut under.data, w, |x, y, px| {
        let t = top.data[y * w + x];
        *px = std::array::from_fn(|c| px[c] + (t[c] - px[c]) * k);
    });
}

/// `top` combined with `under` as `mode` says.
fn blend_into(under: &mut ImageRgbaF32, top: &ImageRgbaF32, mode: bv_timeline::CompositeMode) {
    let w = under.width as usize;
    par::for_each_pixel(&mut under.data, w, |x, y, px| {
        *px = mode.blend(top.data[y * w + x], *px);
    });
}

/// The track's picture: its event, or two events in a transition.
fn track_layer(
    scene: &FrameScene<'_>,
    content: &TrackContent<'_>,
    pool: &mut VideoPool,
    below: &ImageRgbaF32,
) -> ImageRgbaF32 {
    match content {
        TrackContent::Nothing => ImageRgbaF32::transparent(scene.width, scene.height),
        TrackContent::Event(e) => event_layer(scene, e, pool, below),
        TrackContent::Transition {
            kind,
            plugin,
            from,
            to,
            progress,
        } => {
            let img_b = event_layer(scene, to, pool, below);
            let img_a = match from {
                Some(a) => event_layer(scene, a, pool, below),
                None => ImageRgbaF32::transparent(scene.width, scene.height),
            };
            match (plugin, scene.plugins) {
                (Some(call), Some(plugins)) => {
                    let (a, b) = (Arc::new(img_a), Arc::new(img_b));
                    plugins
                        .0
                        .transition(call, Arc::clone(&a), Arc::clone(&b), *progress)
                        .unwrap_or_else(|| transition::apply(*kind, &a, &b, *progress))
                }
                _ => transition::apply(*kind, &img_a, &img_b, *progress),
            }
        }
    }
}

/// An event's picture placed in the frame, with its level and fade.
fn event_layer(
    scene: &FrameScene<'_>,
    e: &EventLayer<'_>,
    pool: &mut VideoPool,
    below: &ImageRgbaF32,
) -> ImageRgbaF32 {
    let mut img = match &e.source {
        Some(m) if m.info.is_adjustment() => adjusted(scene, m, below),
        Some(m) => media_layer(scene, e.clip.0, m, pool),
        None => return ImageRgbaF32::transparent(scene.width, scene.height),
    };
    scale_layer(&mut img, e.level);
    if let Some(f) = e.fade {
        scale_layer(&mut img, f);
    }
    img
}

/// An adjustment event's picture: what is below it, through its effects
/// (the frame-sized picture its event effects get).
fn adjusted(scene: &FrameScene<'_>, m: &MediaLayer<'_>, below: &ImageRgbaF32) -> ImageRgbaF32 {
    let mut img = below.clone();
    let framing = Framing {
        framed_at: 0,
        media_size: (img.width, img.height),
        media_fx_scale: 1.0,
        place: m.place,
        quality: m.quality,
    };
    for (k, e) in m.effects.iter().enumerate() {
        match scene.media_call(m, k, scene.scale, Some(framing)) {
            Some(call) => effects::plugin(scene, &call, &mut img),
            None => effects::apply(&mut img, e.fx, scene.scale),
        }
        effects::stored(scene.clips, &mut img);
    }
    img
}

/// The media's picture placed in the frame, with its effects; the offline
/// color where it cannot be read.
fn media_layer(
    scene: &FrameScene<'_>,
    owner: u64,
    m: &MediaLayer<'_>,
    pool: &mut VideoPool,
) -> ImageRgbaF32 {
    // 8-bit formats and the 2.222 compositing gamma work on the encoded
    // values: the media enters the composite encoded; under ACES, in its
    // working space (from its Media Properties' color space, if it has one).
    let space = scene
        .color
        .as_ref()
        .zip(m.info.color_space)
        .and_then(|(p, s)| p.media_input(s));
    let enter = scene
        .color
        .as_deref()
        .map(|p| Enter::of(p, space.as_deref()));
    let read = |pool: &mut VideoPool, owner: u64, ask: Ask| {
        let src = match pool.picture(owner, m.info, ask) {
            Ok(Picture::Linear(src)) => src,
            Ok(Picture::Decoded(f)) => {
                Arc::new(bv_color::frame_to_linear_levels(&f, 1, m.video_levels))
            }
            Err(_) => return None,
        };
        let how = m.deinterlaced(src.height);
        Some(if scene.video_gamma || enter.is_some() || how.is_some() {
            let mut img = (*src).clone();
            enter_chain(&mut img, scene.video_gamma, enter);
            // Interlaced media: whole frames, as the Deinterlace method
            // says.
            if let Some(how) = how {
                layer::deinterlace(&mut img, how);
            }
            Arc::new(img)
        } else {
            src
        })
    };
    let Some(mut src) = read(pool, owner, Ask::of(m, Want::Linear)) else {
        return offline(scene, m);
    };
    // Frame Blend (the Resample mode): the media's next frame mixed in, as
    // far as the instant shown is on the way to it; Optical Flow: the
    // frame between made along what moves.
    if let Some((ask, k)) = Ask::blend(m, Want::Linear)
        && let Some(next) = read(pool, bv_media::blend_owner(owner), ask)
        && (next.width, next.height) == (src.width, src.height)
    {
        let mut img = (*src).clone();
        match m.blend {
            Some((at, _)) if m.flow => img.data = flow::frame_between(owner, at, &src, &next, k),
            _ => mix_into(&mut img, &next, k),
        }
        src = Arc::new(img);
    }
    // The media's effects work on its own picture; then the event's
    // Pan/Crop frames it, and the event's effects work on the framed
    // picture (VEGAS's default: Pan/Crop first in the event's chain).
    let framed_at = m.framed_from();
    let (src, media_fx_scale) = if framed_at == 0 {
        (src, 1.0)
    } else {
        let mut src = reduce_for_display(&src, &m.placement);
        // Effect sizes are in the project's pixels: in the source's here.
        let per_line = m.placement.picture_per_pixel(src.width, src.height).1;
        let fx_scale = (per_line as f32 * scene.scale).max(1e-4);
        for (k, e) in m.effects[..framed_at].iter().enumerate() {
            match scene.media_call(m, k, fx_scale, None) {
                Some(call) => effects::plugin(scene, &call, &mut src),
                None => effects::apply(&mut src, e.fx, fx_scale),
            }
            effects::stored(scene.clips, &mut src);
        }
        (Arc::new(src), fx_scale)
    };
    let mut img = layer::place_box(&src, scene.width, scene.height, &m.placement, m.quality);
    // Reduce Interlace Flicker: on the framed picture, before the event's
    // effects.
    if m.clip.reduce_flicker {
        layer::flicker_filter(&mut img);
    }
    // Its Pan/Crop masks, framed as the picture is: they cut what shows of
    // it, or (Apply to FX) keep its effects inside them, the picture
    // without them elsewhere.
    let mask = m.mask_picture(src.width, src.height).map(|c| {
        layer::place_box(
            &c,
            scene.width,
            scene.height,
            &m.placement.hard(),
            m.quality,
        )
    });
    let unaffected = mask
        .as_ref()
        .filter(|_| m.clip.mask.as_ref().is_some_and(|k| k.apply_to_fx))
        .map(|_| img.clone());
    let framing = Framing {
        framed_at,
        media_size: (src.width, src.height),
        media_fx_scale,
        place: m.place,
        quality: m.quality,
    };
    for (k, e) in m.effects.iter().enumerate().skip(framed_at) {
        match scene.media_call(m, k, scene.scale, Some(framing)) {
            Some(call) => effects::plugin(scene, &call, &mut img),
            None => effects::apply(&mut img, e.fx, scene.scale),
        }
        effects::stored(scene.clips, &mut img);
    }
    if let Some(mask) = &mask {
        match unaffected {
            Some(mut plain) => {
                mix_by(&mut plain, &img, mask);
                img = plain;
            }
            None => cut_by(&mut img, mask),
        }
    }
    img
}

/// `img` as far as `mask`'s alpha keeps it.
fn cut_by(img: &mut ImageRgbaF32, mask: &ImageRgbaF32) {
    let w = img.width as usize;
    par::for_each_pixel(&mut img.data, w, |x, y, px| {
        let k = mask.data[y * w + x][3];
        *px = px.map(|c| c * k);
    });
}

/// `under` moved to `top` as far as `mask`'s alpha says.
fn mix_by(under: &mut ImageRgbaF32, top: &ImageRgbaF32, mask: &ImageRgbaF32) {
    let w = under.width as usize;
    par::for_each_pixel(&mut under.data, w, |x, y, px| {
        let (t, k) = (top.data[y * w + x], mask.data[y * w + x][3]);
        *px = std::array::from_fn(|c| px[c] + (t[c] - px[c]) * k);
    });
}

/// The offline color where the media shows.
fn offline(scene: &FrameScene<'_>, m: &MediaLayer<'_>) -> ImageRgbaF32 {
    let (w, h) = m
        .info
        .video
        .map_or((scene.width, scene.height), |v| (v.width, v.height));
    let r = fit_rect_par(w, h, m.par, scene.width, scene.height);
    let mut img = ImageRgbaF32::transparent(scene.width, scene.height);
    let color = offline_color(scene);
    for y in r.y as u32..(r.y + r.h) as u32 {
        for x in r.x as u32..(r.x + r.w) as u32 {
            *img.pixel_mut(x, y) = color;
        }
    }
    img
}

/// [`OFFLINE`] in the scene's compositing space.
pub fn offline_color(scene: &FrameScene<'_>) -> [f32; 4] {
    match &scene.color {
        Some(p) => {
            let c = p.enter([OFFLINE[0], OFFLINE[1], OFFLINE[2]]);
            [c[0], c[1], c[2], OFFLINE[3]]
        }
        None => OFFLINE,
    }
}

/// Halves the source while it is more than twice the size it shows at, so
/// effects do not work on pixels nobody sees.
fn reduce_for_display(src: &ImageRgbaF32, place: &bv_timeline::pancrop::Placement) -> ImageRgbaF32 {
    let mut img = src.clone();
    while place.reduces(img.width, img.height) && img.width > 1 {
        img = layer::half_size(&img);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A red frame under an adjustment: an event on the track above, or the
    /// track itself.
    fn red_under_adjustment(track: bool) -> (Project, bv_timeline::TrackId) {
        let mut p = Project::new(linear());
        let below = p.tracks[0].id;
        let red = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings,
        ));
        p.insert_media(red, Tick::ZERO, Some(below), None).unwrap();
        let above = if track {
            p.insert_adjustment_track_at(0)
        } else {
            let t = p.insert_track_at(bv_timeline::TrackKind::Video, 0);
            let adj = p.add_media(bv_timeline::MediaInfo::generated(
                bv_timeline::Generator::Adjustment,
                &p.settings,
            ));
            p.insert_media(adj, Tick::ZERO, Some(t), None).unwrap();
            t
        };
        (p, above)
    }

    #[test]
    fn track_motion_moves_the_picture_with_its_shadow_and_glow() {
        use bv_timeline::{HaloLook, KeyCurve, Transform};
        let solid = |p: &mut Project, rgba| {
            p.add_media(bv_timeline::MediaInfo::generated(
                bv_timeline::Generator::SolidColor { rgba },
                &p.settings,
            ))
        };
        let mut p = Project::new(linear());
        let bottom = p.tracks[0].id;
        let white = solid(&mut p, [1.0; 4]);
        p.insert_media(white, Tick::ZERO, Some(bottom), None)
            .unwrap();
        let top = p.insert_track_at(bv_timeline::TrackKind::Video, 0);
        let red = solid(&mut p, [1.0, 0.0, 0.0, 1.0]);
        p.insert_media(red, Tick::ZERO, Some(top), None).unwrap();
        // Half its size, its center moved a quarter of the frame right:
        // 32 to 64 across, 9 to 27 down.
        let half = Transform {
            scale: 0.5,
            x: 0.25,
            ..Transform::default()
        };
        let new = (KeyCurve::Linear, 0.0);
        p.set_track_motion_at(top, false, Tick::ZERO, half, new)
            .unwrap();
        let at = |p: &Project, x, y| {
            render_frame(p, Tick::ZERO, 64, 36, &mut VideoPool::new()).pixel(x, y)
        };
        assert_eq!(at(&p, 48, 18), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(at(&p, 16, 18), [1.0; 4], "the track below shows");
        // A hard black shadow a fifth of the frame lower, a green glow a
        // fifth larger.
        p.set_track_halo_on(top, false, true, true).unwrap();
        let shadow = HaloLook {
            x: 0.0,
            y: -0.2,
            size: 1.0,
            blur: 0.0,
            intensity: 1.0,
            color: [0.0, 0.0, 0.0, 1.0],
        };
        p.set_track_halo_at(top, false, true, Tick::ZERO, shadow)
            .unwrap();
        p.set_track_halo_on(top, false, false, true).unwrap();
        let glow = HaloLook {
            y: 0.0,
            size: 1.2,
            color: [0.0, 1.0, 0.0, 1.0],
            ..shadow
        };
        p.set_track_halo_at(top, false, false, Tick::ZERO, glow)
            .unwrap();
        let c = at(&p, 48, 31);
        assert!(c[0] < 1e-4 && c[1] < 1e-4 && c[3] > 0.999, "shadow: {c:?}");
        let c = at(&p, 48, 8);
        assert!(c[0] < 1e-4 && c[1] > 0.999, "glow: {c:?}");
        assert_eq!(
            at(&p, 48, 18),
            [1.0, 0.0, 0.0, 1.0],
            "the picture over them"
        );
        // Parent Motion moves a whole group: the bottom track a child of
        // the top one, both moved left.
        p.clear_track_motion(top, false).unwrap();
        p.make_compositing_child(bottom, false).unwrap();
        let left = Transform {
            scale: 0.5,
            x: -0.25,
            ..Transform::default()
        };
        p.set_track_motion_at(top, true, Tick::ZERO, left, new)
            .unwrap();
        assert_eq!(at(&p, 16, 18), [1.0, 0.0, 0.0, 1.0]);
        let c = at(&p, 48, 18);
        assert!(
            c[..3].iter().all(|v| *v < 1e-4),
            "nothing where the group was: {c:?}"
        );
    }

    #[test]
    fn adjustment_events_and_tracks_work_on_what_is_below() {
        let invert = bv_timeline::Effect::new(bv_timeline::EffectKind::Invert);
        let at =
            |p: &Project| render_frame(p, Tick::ZERO, 16, 9, &mut VideoPool::new()).pixel(8, 4);
        // An adjustment event inverting: the red below turns cyan; at half
        // its level, half way.
        let (mut p, t) = red_under_adjustment(false);
        assert!(at(&p)[0] > 0.99, "nothing to do yet: red");
        let id = p.track(t).unwrap().clips[0].id;
        p.clip_mut(id).unwrap().effects.push(invert.clone());
        let c = at(&p);
        assert!(c[0] < 1e-4 && c[1] > 0.99 && c[2] > 0.99, "{c:?}");
        p.clip_mut(id).unwrap().level = 0.5;
        let c = at(&p);
        assert!(
            (c[0] - 0.5).abs() < 1e-3 && (c[1] - 0.5).abs() < 1e-3,
            "{c:?}"
        );
        // An adjustment track: its effects on the track below, its level
        // their strength.
        let (mut p, t) = red_under_adjustment(true);
        p.track_by_id_mut(t).unwrap().effects.push(invert);
        let c = at(&p);
        assert!(c[0] < 1e-4 && c[1] > 0.99, "{c:?}");
        p.track_by_id_mut(t).unwrap().level = 0.25;
        let c = at(&p);
        assert!(
            (c[0] - 0.75).abs() < 1e-3 && (c[1] - 0.25).abs() < 1e-3,
            "{c:?}"
        );
        // Its own events go over what is below it, its effects working on
        // all of that: green over the red, inverted, magenta; mixed in by
        // its level, the green showing at none.
        let green = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [0.0, 1.0, 0.0, 1.0],
            },
            &p.settings,
        ));
        p.insert_media(green, Tick::ZERO, Some(t), None).unwrap();
        let c = at(&p);
        assert!(
            (c[0] - 0.25).abs() < 1e-3 && (c[1] - 0.75).abs() < 1e-3 && (c[2] - 0.25).abs() < 1e-3,
            "{c:?}"
        );
        p.track_by_id_mut(t).unwrap().level = 1.0;
        let c = at(&p);
        assert!(c[0] > 0.99 && c[1] < 1e-4 && c[2] > 0.99, "{c:?}");
        p.track_by_id_mut(t).unwrap().level = 0.0;
        let c = at(&p);
        assert!(c[0] < 1e-4 && c[1] > 0.99 && c[2] < 1e-4, "{c:?}");
    }

    #[test]
    fn a_mask_parent_shows_its_children_where_it_is_white() {
        // A solid mask over a red child, Multiply (Mask) on the mask: white
        // shows the red, black hides it (the black background shows), a
        // middle gray lets part of it through.
        let mut p = Project::new(linear());
        let mask_track = p.tracks[0].id;
        let child = p.add_track(bv_timeline::TrackKind::Video);
        let solid = |p: &mut Project, v: f32| {
            p.add_media(bv_timeline::MediaInfo::generated(
                bv_timeline::Generator::SolidColor {
                    rgba: [v, v, v, 1.0],
                },
                &p.settings,
            ))
        };
        let red = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings,
        ));
        p.insert_media(red, Tick::ZERO, Some(child), None).unwrap();
        p.make_compositing_child(child, false).unwrap();
        p.track_by_id_mut(mask_track).unwrap().composite_mode =
            bv_timeline::CompositeMode::MultiplyMask;
        let with_mask = |p: &Project, v: f32| {
            let mut q = p.clone();
            let m = solid(&mut q, v);
            q.insert_media(m, Tick::ZERO, Some(mask_track), None)
                .unwrap();
            render_frame(&q, Tick::ZERO, 16, 9, &mut VideoPool::new()).pixel(8, 4)
        };
        let white = with_mask(&p, 1.0);
        assert!(white[0] > 0.99 && white[1] < 1e-4, "{white:?}");
        let black = with_mask(&p, 0.0);
        assert!(black[0] < 1e-4 && black[3] > 0.99, "{black:?}");
        let gray = with_mask(&p, 0.5);
        assert!(gray[0] > 0.05 && gray[0] < 0.95, "{gray:?}");
        // Source Alpha (the default) on the mask: the white hides the red.
        p.track_by_id_mut(mask_track).unwrap().composite_mode =
            bv_timeline::CompositeMode::SourceAlpha;
        let plain = with_mask(&p, 1.0);
        assert!(plain[1] > 0.99, "{plain:?}");
    }

    #[test]
    fn effects_after_the_composite_button_work_on_the_group() {
        // A blue child under a red parent at a quarter of its level. The
        // parent's Invert before compositing turns its red cyan first;
        // after it, the group's mix of red and blue is inverted.
        let mut p = Project::new(linear());
        let parent = p.tracks[0].id;
        let child = p.add_track(bv_timeline::TrackKind::Video);
        let solid = |p: &mut Project, rgba| {
            p.add_media(bv_timeline::MediaInfo::generated(
                bv_timeline::Generator::SolidColor { rgba },
                &p.settings,
            ))
        };
        let red = solid(&mut p, [1.0, 0.0, 0.0, 1.0]);
        let blue = solid(&mut p, [0.0, 0.0, 1.0, 1.0]);
        p.insert_media(red, Tick::ZERO, Some(parent), None).unwrap();
        p.insert_media(blue, Tick::ZERO, Some(child), None).unwrap();
        p.make_compositing_child(child, false).unwrap();
        let t = p.track_by_id_mut(parent).unwrap();
        t.level = 0.25;
        let invert = bv_timeline::Effect::new(bv_timeline::EffectKind::Invert);
        t.effects.push(invert.clone());
        let at =
            |p: &Project| render_frame(p, Tick::ZERO, 16, 9, &mut VideoPool::new()).pixel(8, 4);
        let near = |c: [f32; 4], want: [f32; 3]| (0..3).all(|k| (c[k] - want[k]).abs() < 1e-3);
        let c = at(&p);
        assert!(near(c, [0.0, 0.25, 1.0]), "cyan over blue: {c:?}");
        // (Invert works on the encoded values: a quarter of red's light
        // and three quarters of blue's come out as about 0.19 and 0.01.)
        p.track_by_id_mut(parent).unwrap().composite_at = Some(0);
        let c = at(&p);
        assert!(
            c[1] > 0.999 && (c[0] - 0.188).abs() < 0.01 && c[2] < 0.02,
            "red and blue inverted: {c:?}"
        );
        // The child's effect after its button: on the group so far (the
        // child over nothing), the parent then over it.
        p.track_by_id_mut(parent).unwrap().effects.clear();
        let t = p.track_by_id_mut(child).unwrap();
        t.effects.push(invert.clone());
        t.composite_at = Some(0);
        let c = at(&p);
        assert!(near(c, [1.0, 0.75, 0.0]), "red over yellow: {c:?}");
        // Outside a group the button does nothing: the top track's Invert
        // before compositing, not on the yellow below it.
        p.make_compositing_parent(child, false).unwrap();
        let t = p.track_by_id_mut(parent).unwrap();
        t.effects.push(invert);
        t.composite_at = Some(0);
        let c = at(&p);
        assert!(near(c, [0.75, 1.0, 0.25]), "cyan over yellow: {c:?}");
    }

    #[test]
    fn frame_blend_mixes_the_next_frame_in() {
        // Red then blue, a 24 fps sequence in a 60 fps project: its second
        // frame is 0.4 of the way from red to blue.
        let dir = std::env::temp_dir().join(format!("bv-blend-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, rgb) in [[255u8, 0, 0], [0, 0, 255]].iter().enumerate() {
            bmp(&dir.join(format!("f{i:04}.bmp")), 16, 9, *rgb);
        }
        let first = dir.join("f0000.bmp").to_string_lossy().to_string();
        let rate = bv_core::FrameRate::new(24, 1);
        let info = bv_media::probe_sequence(&first, 2, rate).unwrap();
        let mut p = Project::new(bv_timeline::ProjectSettings {
            frame_rate: bv_core::FrameRate::new(60, 1),
            ..linear()
        });
        let v = p.tracks[0].id;
        let m = p.add_media(info);
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let t = bv_core::FrameRate::new(60, 1).frame_start(1);
        let at = |p: &Project| render_frame(p, t, 16, 9, &mut VideoPool::new()).pixel(8, 4);
        let c = at(&p);
        assert!(c[0] > 0.999 && c[2] < 1e-3, "no resampling: red {c:?}");
        p.settings.resample = bv_timeline::ResampleMode::FrameBlend;
        let c = at(&p);
        assert!(
            (c[0] - 0.6).abs() < 1e-3 && (c[2] - 0.4).abs() < 1e-3,
            "blended {c:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_track_bypassing_motion_blur_shows_the_frame_s_time() {
        // Red then blue, a frame each: a sub-frame at the second frame's
        // time for the first frame shows blue, but red where the track
        // bypasses motion blur.
        let dir = std::env::temp_dir().join(format!("bv-bypass-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, rgb) in [[255u8, 0, 0], [0, 0, 255]].iter().enumerate() {
            bmp(&dir.join(format!("f{i:04}.bmp")), 16, 9, *rgb);
        }
        let first = dir.join("f0000.bmp").to_string_lossy().to_string();
        let rate = bv_core::FrameRate::new(24, 1);
        let info = bv_media::probe_sequence(&first, 2, rate).unwrap();
        let mut p = Project::new(bv_timeline::ProjectSettings {
            frame_rate: rate,
            ..linear()
        });
        let v = p.tracks[0].id;
        let m = p.add_media(info);
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let (t, held) = (rate.frame_start(1), rate.frame_start(0));
        let at = |p: &Project| {
            render_output_held(
                p,
                t,
                Some(held),
                (16, 9),
                &mut VideoPool::new(),
                [0.0, 0.0, 0.0, 1.0],
                None,
                None,
            )
            .pixel(8, 4)
        };
        let c = at(&p);
        assert!(
            c[2] > 0.999 && c[0] < 1e-3,
            "the sub-frame's own time: blue {c:?}"
        );
        p.track_by_id_mut(v).unwrap().bypass_motion_blur = true;
        let c = at(&p);
        assert!(c[0] > 0.999 && c[2] < 1e-3, "the frame's time: red {c:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn optical_flow_makes_the_frame_between() {
        // A texture moving 6 pixels across, 24 fps in a 60 fps project:
        // 0.4 of the way, Optical Flow shows it 2.4 pixels on (as a still
        // of it does), where Frame Blend mixes the two frames.
        let dir = std::env::temp_dir().join(format!("bv-flow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (256u32, 144u32);
        let texture = |shift: f32| {
            move |x: u32, y: u32| {
                let (u, v) = (x as f32 - shift, y as f32);
                let g = 0.5
                    + 0.2 * (u * 0.19).sin() * (v * 0.23).cos()
                    + 0.15 * ((u + v) * 0.11).sin()
                    + 0.1 * (u * 0.41).cos() * (v * 0.37).sin();
                let g = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
                [g, g, g]
            }
        };
        bmp_of(&dir.join("f0000.bmp"), w, h, texture(0.0));
        bmp_of(&dir.join("f0001.bmp"), w, h, texture(6.0));
        bmp_of(&dir.join("truth.bmp"), w, h, texture(2.4));
        let first = dir.join("f0000.bmp").to_string_lossy().to_string();
        let info = bv_media::probe_sequence(&first, 2, bv_core::FrameRate::new(24, 1)).unwrap();
        let settings = bv_timeline::ProjectSettings {
            frame_rate: bv_core::FrameRate::new(60, 1),
            width: w,
            height: h,
            ..linear()
        };
        let mut p = Project::new(settings);
        let v = p.tracks[0].id;
        let m = p.add_media(info);
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let t = bv_core::FrameRate::new(60, 1).frame_start(1);
        let frame = |p: &Project| render_frame(p, t, w, h, &mut VideoPool::new());
        p.settings.resample = bv_timeline::ResampleMode::FrameBlend;
        let blended = frame(&p);
        p.settings.resample = bv_timeline::ResampleMode::OpticalFlow;
        let flowed = frame(&p);
        let truth_info = bv_media::probe_file(&dir.join("truth.bmp").to_string_lossy()).unwrap();
        let mut q = Project::new(settings);
        let v = q.tracks[0].id;
        let m = q.add_media(truth_info);
        q.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let truth = frame(&q);
        let error = |img: &ImageRgbaF32| {
            let mut sum = 0.0;
            for y in 30..h - 30 {
                for x in 40..w - 40 {
                    sum += (img.pixel(x, y)[1] - truth.pixel(x, y)[1]).abs();
                }
            }
            sum
        };
        assert!(
            error(&flowed) < error(&blended) * 0.3,
            "flow {} blend {}",
            error(&flowed),
            error(&blended)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn interlaced_media_comes_in_whole() {
        // Even lines red, odd lines blue, the upper field first, its own
        // size in a progressive project.
        let dir = std::env::temp_dir().join(format!("bv-fields-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fields.bmp");
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        let row = (4u32 * 3 + 3) & !3;
        b.extend_from_slice(&(54 + row * 6).to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&4i32.to_le_bytes());
        b.extend_from_slice(&6i32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 24]);
        for r in 0..6u32 {
            let bgr = if (5 - r).is_multiple_of(2) {
                [0, 0, 255]
            } else {
                [255, 0, 0]
            };
            for _ in 0..4 {
                b.extend_from_slice(&bgr);
            }
        }
        std::fs::write(&file, b).unwrap();
        let mut info = bv_media::probe_file(&file.to_string_lossy()).unwrap();
        info.video.as_mut().unwrap().field_order = bv_core::FieldOrder::UpperFirst;
        let mut p = Project::new(bv_timeline::ProjectSettings {
            width: 4,
            height: 6,
            ..linear()
        });
        let v = p.tracks[0].id;
        let m = p.add_media(info);
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let line = |p: &Project, y: u32| {
            render_frame(p, Tick::ZERO, 4, 6, &mut VideoPool::new()).pixel(1, y)
        };
        // Interpolated from the upper field: red everywhere.
        p.settings.deinterlace = bv_timeline::DeinterlaceMethod::Interpolate;
        assert!((0..6).all(|y| line(&p, y)[0] > 0.999), "{:?}", line(&p, 1));
        // Blended: the lines between, half each.
        p.settings.deinterlace = bv_timeline::DeinterlaceMethod::BlendFields;
        let c = line(&p, 2);
        assert!(
            (c[0] - 0.5).abs() < 1e-3 && (c[2] - 0.5).abs() < 1e-3,
            "{c:?}"
        );
        // No deinterlacing: the stripes stay.
        p.settings.deinterlace = bv_timeline::DeinterlaceMethod::None;
        assert!(line(&p, 1)[2] > 0.999);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn video_levels_projects_keep_studio_rgb() {
        // The 2.222 gamma's values: 8-bit (full range) stretches a decoded
        // Y′ 16 to black; the video levels formats keep it.
        for (format, gamma) in [
            (bv_timeline::PixelFormat::Full8Bit, true),
            (bv_timeline::PixelFormat::Legacy8BitVideoLevels, true),
            (bv_timeline::PixelFormat::Float32VideoLevels, true),
            (bv_timeline::PixelFormat::Float32Full, false),
        ] {
            let s = bv_timeline::ProjectSettings {
                pixel_format: format,
                ..bv_timeline::ProjectSettings::default()
            };
            assert_eq!(s.composites_in_video_gamma(), gamma, "{format:?}");
        }
        assert!(bv_timeline::PixelFormat::Float32VideoLevels.is_video_levels());
        assert!(!bv_timeline::PixelFormat::Full8Bit.is_video_levels());
    }

    /// A `w` × `h` picture of one color, as a 24-bit BMP file.
    fn bmp(path: &std::path::Path, w: u32, h: u32, rgb: [u8; 3]) {
        bmp_of(path, w, h, |_, _| rgb);
    }

    /// A BMP file whose pixel (`x`, `y`, from the top left) is `color`'s.
    fn bmp_of(path: &std::path::Path, w: u32, h: u32, color: impl Fn(u32, u32) -> [u8; 3]) {
        let row = (w * 3 + 3) & !3;
        let size = 54 + row * h;
        let mut b = Vec::with_capacity(size as usize);
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&size.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 24]);
        // From the bottom row up.
        for r in 0..h {
            for x in 0..w {
                let rgb = color(x, h - 1 - r);
                b.extend_from_slice(&[rgb[2], rgb[1], rgb[0]]);
            }
            b.resize(b.len() + (row - w * 3) as usize, 0);
        }
        std::fs::write(path, b).unwrap();
    }

    /// 32-bit floating point (full range), compositing in light: what these
    /// tests' mixes are worked out in.
    fn linear() -> bv_timeline::ProjectSettings {
        bv_timeline::ProjectSettings {
            pixel_format: bv_timeline::PixelFormat::Float32Full,
            ..bv_timeline::ProjectSettings::default()
        }
    }

    #[test]
    fn wide_pixels_fill_the_frame() {
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let mut info = bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
            &p.settings,
        );
        let mut video = info.video.unwrap();
        video.width = 1440;
        info.video = Some(video);
        let m = p.add_media(info.clone());
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let left =
            |p: &Project| render_frame(p, Tick::ZERO, 32, 18, &mut VideoPool::new()).pixel(0, 9);
        // Square pixels: 1440 × 1080 is 4:3, pillarboxed in 16:9.
        assert!(left(&p)[0] < 1e-6);
        // HDV's 4 : 3 pixels: as wide as the frame.
        p.media[0].video.as_mut().unwrap().pixel_aspect = bv_timeline::PixelAspect::new(4, 3);
        assert!(left(&p)[0] > 0.99);
    }

    /// Pan/Crop masks cut what shows of the event (a positive one keeps
    /// what is inside it, a negative one what is outside), or with Apply to
    /// FX keep its effects inside them, the picture as it is elsewhere.
    #[test]
    fn pan_crop_masks_cut_the_event_or_keep_its_effects_inside() {
        use bv_timeline::mask::{MaskMode, MaskPath, PanCropMask};
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let info = bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
            &p.settings,
        );
        let (w, h) = info
            .video
            .map(|v| (f64::from(v.width), f64::from(v.height)))
            .unwrap();
        let m = p.add_media(info);
        let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let mut mask = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        mask.add_path(MaskPath::rectangle([0.0, 0.0], [w / 2.0, h]));
        p.clip_mut(id).unwrap().mask = Some(mask);
        let px = |p: &Project, x: u32| {
            render_frame(p, Tick::ZERO, 32, 18, &mut VideoPool::new()).pixel(x, 9)
        };
        assert!(px(&p, 4)[0] > 0.99, "inside the positive mask");
        assert!(px(&p, 28)[0] < 1e-6, "outside it");
        let c = p.clip_mut(id).unwrap();
        c.mask.as_mut().unwrap().paths[0].mode = MaskMode::Negative;
        assert!(px(&p, 4)[0] < 1e-6 && px(&p, 28)[0] > 0.99, "negative");
        // Off: nothing masked.
        p.clip_mut(id).unwrap().mask.as_mut().unwrap().enabled = false;
        assert!(px(&p, 4)[0] > 0.99 && px(&p, 28)[0] > 0.99);
        // Apply to FX: the event inverted (white to black) only inside a
        // positive mask on the left half.
        let c = p.clip_mut(id).unwrap();
        c.effects
            .push(bv_timeline::Effect::new(bv_timeline::EffectKind::Invert));
        let k = c.mask.as_mut().unwrap();
        k.enabled = true;
        k.apply_to_fx = true;
        k.paths[0].mode = MaskMode::Positive;
        let (left, right) = (px(&p, 4), px(&p, 28));
        assert!(
            left[0] < 1e-6 && left[3] > 0.99,
            "the effect inside: {left:?}"
        );
        assert!(right[0] > 0.99, "the picture as it is outside: {right:?}");
    }

    #[test]
    fn event_effects_work_on_the_framed_picture_media_effects_on_the_media() {
        // A white picture half as big, moved to the frame's left half by
        // the event's Pan/Crop.
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let m = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::SolidColor {
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
            &p.settings,
        ));
        let clip = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.set_clip_framing(
            clip,
            &bv_timeline::Transform {
                scale: 0.5,
                x: -0.25,
                ..bv_timeline::Transform::default()
            },
        )
        .unwrap();
        let alpha = |p: &Project, x: u32| {
            render_frame_on(p, Tick::ZERO, 32, 18, &mut VideoPool::new(), [0.0; 4]).pixel(x, 9)[3]
        };
        assert!(alpha(&p, 8) > 0.99 && alpha(&p, 24) < 0.01);
        // The event's Mirror turns the whole frame over (VEGAS's default:
        // Pan/Crop first in the event's chain): the picture shows right.
        let mirror = bv_timeline::Effect::new(bv_timeline::EffectKind::Mirror);
        p.clip_mut(clip).unwrap().effects.push(mirror.clone());
        assert!(alpha(&p, 8) < 0.01 && alpha(&p, 24) > 0.99);
        // The media's turns the media over, before the event frames it.
        p.clip_mut(clip).unwrap().effects.clear();
        p.media[0].effects.push(mirror);
        assert!(alpha(&p, 8) > 0.99 && alpha(&p, 24) < 0.01);
    }

    #[test]
    fn a_track_fades_to_its_color() {
        use bv_timeline::EnvelopeKind;
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let bars = p.add_media(bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::ColorBars,
            &p.settings,
        ));
        p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap();
        // The middle of the picture (an 8 × 8 frame would letterbox it).
        let at =
            |p: &Project| render_frame(p, Tick::ZERO, 16, 9, &mut VideoPool::new()).pixel(8, 4);
        let before = at(&p);
        p.toggle_envelope(v, EnvelopeKind::FadeToColor).unwrap();
        p.set_envelope_point(v, EnvelopeKind::FadeToColor, Tick::ZERO, 0.5)
            .unwrap();
        // Halfway to white (the default top color), in light.
        let px = at(&p);
        for c in 0..3 {
            assert!(
                (px[c] - (before[c] + 1.0) / 2.0).abs() < 1e-3,
                "{before:?} {px:?}"
            );
        }
        // Composite level 0: nothing of the track shows.
        p.toggle_envelope(v, EnvelopeKind::CompositeLevel).unwrap();
        p.set_envelope_point(v, EnvelopeKind::CompositeLevel, Tick::ZERO, 0.0)
            .unwrap();
        let px = at(&p);
        assert!(px[0] < 1e-6, "{px:?}");
    }
    use bv_timeline::scene::media_time;
    use bv_timeline::{FadeCurve, MediaId};

    #[test]
    fn output_fx_mute_envelope_and_mute_all() {
        use bv_timeline::{Effect, EffectKind, EnvelopeKind, Generator, MediaInfo};
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let m = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
            &p.settings,
        ));
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let mut pool = VideoPool::default();
        let px =
            |p: &Project, pool: &mut VideoPool| render_frame(p, Tick::ZERO, 32, 18, pool).data[0];
        assert!(px(&p, &mut pool)[0] > 0.9);
        p.video_output_fx.push(Effect::new(EffectKind::Invert));
        assert!(px(&p, &mut pool)[0] < 0.1, "output FX inverts the frame");
        p.video_output_fx.clear();
        p.toggle_envelope(v, EnvelopeKind::Mute).unwrap();
        p.set_envelope_point(v, EnvelopeKind::Mute, Tick::ZERO, 0.0)
            .unwrap();
        assert!(px(&p, &mut pool)[0] < 0.01, "mute envelope hides the track");
        p.toggle_envelope(v, EnvelopeKind::Mute).unwrap();
        p.mute_all_video = true;
        assert!(px(&p, &mut pool)[0] < 0.01);
    }

    /// The video bus: its Fade to Color over everything (after the output
    /// effects), its mute, and Bypass FX and Envelopes.
    #[test]
    fn the_video_bus_fades_mutes_and_bypasses() {
        use bv_timeline::{
            Effect, EffectKind, EnvelopeKind, EnvelopeOwner, Generator, KeyCurve, MediaInfo,
        };
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let m = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 1.0, 1.0, 1.0],
            },
            &p.settings,
        ));
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let mut pool = VideoPool::default();
        let px =
            |p: &Project, pool: &mut VideoPool| render_frame(p, Tick::ZERO, 32, 18, pool).data[0];
        // Halfway to the bottom color, black.
        p.toggle_video_bus_envelope(EnvelopeKind::FadeToColor, KeyCurve::Linear)
            .unwrap();
        p.set_envelope_point(
            EnvelopeOwner::VideoBus,
            EnvelopeKind::FadeToColor,
            Tick::ZERO,
            -0.5,
        )
        .unwrap();
        let half = px(&p, &mut pool)[0];
        assert!((half - 0.5).abs() < 1e-3, "{half}");
        // After the output effects: the inverted black fades to black too.
        p.video_output_fx.push(Effect::new(EffectKind::Invert));
        assert!(px(&p, &mut pool)[0] < 1e-3);
        // Bypassed: neither the effects nor the fade.
        p.video_bus.bypass = true;
        assert!(px(&p, &mut pool)[0] > 0.99);
        p.video_bus.bypass = false;
        // Muted: nothing comes out.
        p.video_output_fx.clear();
        p.video_bus.muted = true;
        assert!(px(&p, &mut pool)[0] < 0.01);
    }
    use bv_timeline::{Effect, EffectKind, Generator, MediaInfo, TrackKind, Transition};

    fn solid(p: &mut Project, rgba: [f32; 4]) -> MediaId {
        let info = MediaInfo::generated(Generator::SolidColor { rgba }, &p.settings);
        p.add_media(info)
    }

    fn px(p: &Project, t: Tick) -> [f32; 4] {
        render_frame(p, t, 8, 4, &mut VideoPool::new()).pixel(4, 2)
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        (0..4).all(|i| (a[i] - b[i]).abs() < 1e-4)
    }

    #[test]
    fn fundo_preto_sem_clipes() {
        let p = Project::new(linear());
        let img = render_frame(&p, Tick::ZERO, 8, 4, &mut VideoPool::new());
        assert!(img.data.iter().all(|px| *px == [0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn trilha_de_cima_cobre_a_de_baixo_e_opacidade_mistura() {
        let mut p = Project::new(linear());
        let v1 = p.tracks[0].id;
        let v2 = p.add_track(TrackKind::Video);
        let red = solid(&mut p, [1.0, 0.0, 0.0, 1.0]);
        let blue = solid(&mut p, [0.0, 0.0, 1.0, 1.0]);
        let top = p.insert_media(red, Tick::ZERO, Some(v1), None).unwrap()[0];
        p.insert_media(blue, Tick::ZERO, Some(v2), None).unwrap();
        assert!(close(px(&p, Tick::ZERO), [1.0, 0.0, 0.0, 1.0]));
        p.set_clip_level(top, 0.25).unwrap();
        assert!(close(px(&p, Tick::ZERO), [0.25, 0.0, 0.75, 1.0]));
        // Nível da trilha multiplica o do clipe.
        p.track_by_id_mut(v1).unwrap().level = 0.0;
        assert!(close(px(&p, Tick::ZERO), [0.0, 0.0, 1.0, 1.0]));
        assert!(close(px(&p, Tick::from_seconds(6)), [0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn crossfade_automatico_na_sobreposicao() {
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let white = solid(&mut p, [1.0, 1.0, 1.0, 1.0]);
        let black = solid(&mut p, [0.0, 0.0, 0.0, 1.0]);
        // Branco de 0 a 5 s, preto de 3 a 8 s: crossfade de 3 a 5 s.
        p.insert_media(white, Tick::ZERO, Some(v), None).unwrap();
        p.insert_media(black, Tick::from_seconds(3), Some(v), None)
            .unwrap();
        assert!(close(px(&p, Tick::from_seconds(2)), [1.0, 1.0, 1.0, 1.0]));
        let mid = px(&p, Tick::from_seconds(4));
        assert!((mid[0] - 0.5).abs() < 1e-3, "{mid:?}");
        assert!(close(px(&p, Tick::from_seconds(6)), [0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn fade_in_e_fade_out() {
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let white = solid(&mut p, [1.0, 1.0, 1.0, 1.0]);
        let c = p.insert_media(white, Tick::ZERO, Some(v), None).unwrap()[0];
        p.set_fades(c, Some(Tick::from_seconds(2)), Some(Tick::from_seconds(2)))
            .unwrap();
        p.clip_mut(c).unwrap().fade_in_curve = FadeCurve::Linear;
        p.clip_mut(c).unwrap().fade_out_curve = FadeCurve::Linear;
        assert!((px(&p, Tick::from_seconds(1))[0] - 0.5).abs() < 1e-3);
        assert!((px(&p, Tick::from_seconds(4))[0] - 0.5).abs() < 1e-3);
        assert!((px(&p, Tick::from_seconds(2))[0] - 1.0).abs() < 1e-3);
        // Fade-in com cortina: metade do quadro já aparece.
        p.clip_mut(c).unwrap().transition = Transition::Wipe(bv_timeline::Direction::Right);
        let img = render_frame(&p, Tick::from_seconds(1), 160, 90, &mut VideoPool::new());
        assert!(img.pixel(20, 45)[0] > 0.9 && img.pixel(140, 45)[0] < 0.1);
    }

    #[test]
    fn efeito_do_clipe_e_aplicado() {
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let red = solid(&mut p, [1.0, 0.0, 0.0, 1.0]);
        let c = p.insert_media(red, Tick::ZERO, Some(v), None).unwrap()[0];
        p.clip_mut(c)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::BlackAndWhite));
        let out = px(&p, Tick::ZERO);
        assert!(
            (out[0] - out[1]).abs() < 1e-5 && (out[0] - 0.2126).abs() < 1e-3,
            "{out:?}"
        );
    }

    #[test]
    fn past_the_media_end() {
        let mut p = Project::new(linear());
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.duration = Tick::from_seconds(2);
        let m = p.add_media(info.clone());
        let v = p.tracks[0].id;
        let c = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.trim_end(c, Tick::from_seconds(5)).unwrap();
        let clip = p.clip(c).unwrap().clone();
        let s = Tick::from_seconds;
        // Loop: 3 s shows the media's 1 s.
        assert_eq!(media_time(&clip, &info, s(3)), Some(s(1)));
        // Hold Last Frame: the last frame.
        let mut held = clip.clone();
        held.looped = false;
        held.hold_last = true;
        let last = s(2) - p.settings.frame_rate.frame_duration();
        assert_eq!(media_time(&held, &info, s(3)), Some(last));
        // Neither: nothing.
        held.hold_last = false;
        assert_eq!(media_time(&held, &info, s(3)), None);
        assert_eq!(media_time(&held, &info, s(1)), Some(s(1)));
    }

    #[test]
    fn solo_video_track_hides_the_others() {
        let mut p = Project::new(linear());
        let v1 = p.tracks[0].id;
        let v2 = p.add_track(TrackKind::Video);
        let red = solid(&mut p, [1.0, 0.0, 0.0, 1.0]);
        let blue = solid(&mut p, [0.0, 0.0, 1.0, 1.0]);
        p.insert_media(red, Tick::ZERO, Some(v1), None).unwrap();
        p.insert_media(blue, Tick::ZERO, Some(v2), None).unwrap();
        assert!(close(px(&p, Tick::ZERO), [1.0, 0.0, 0.0, 1.0]));
        p.set_track_solo(v2, true).unwrap();
        assert!(close(px(&p, Tick::ZERO), [0.0, 0.0, 1.0, 1.0]));
    }

    #[test]
    fn compositing_gamma() {
        use bv_timeline::{CompositingGamma, PixelFormat};
        // White at 50% over black: half the light in linear compositing,
        // half the encoded value (about a quarter of the light) in video
        // gamma.
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let white = solid(&mut p, [1.0, 1.0, 1.0, 1.0]);
        let c = p.insert_media(white, Tick::ZERO, Some(v), None).unwrap()[0];
        p.set_clip_level(c, 0.5).unwrap();
        assert!((px(&p, Tick::ZERO)[0] - 0.5).abs() < 1e-3);
        p.settings.compositing_gamma = CompositingGamma::Video;
        let encoded_half = bv_color::transfer::bt709_to_linear(0.5);
        assert!((px(&p, Tick::ZERO)[0] - encoded_half).abs() < 1e-3);
        // 8-bit always composites in video gamma.
        p.settings.compositing_gamma = CompositingGamma::Linear;
        p.settings.pixel_format = PixelFormat::Full8Bit;
        assert!((px(&p, Tick::ZERO)[0] - encoded_half).abs() < 1e-3);
        // An unblended pixel comes back unchanged.
        p.set_clip_level(c, 1.0).unwrap();
        assert!((px(&p, Tick::ZERO)[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_media_in_a_color_space_enters_from_its_file_values() {
        use bv_color::transfer::linear_to_bt709;
        use bv_timeline::{AcesDisplay, AcesView, ColorSpace, ViewTransform};
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        // S-Log3's 420 of 1023: a camera's 18% grey.
        let x = 420.0 / 1023.0;
        let grey = solid(&mut p, [x, x, x, 1.0]);
        p.insert_media(grey, Tick::ZERO, Some(v), None).unwrap();
        p.settings.color.view = Some(ViewTransform {
            display: AcesDisplay::Srgb,
            view: AcesView::Aces,
        });
        let shown = |p: &Project, g: f32| {
            let c = p.settings.color_pipeline().unwrap();
            c.view.apply(c.enter([g; 3]).map(f64::from))[0] as f32
        };
        let code = |p: &Project| linear_to_bt709(px(p, Tick::ZERO)[0]);
        let set = |p: &mut Project, s| p.media.iter_mut().for_each(|m| m.color_space = s);
        // Default: its light, as ACES2065-1.
        let light = bv_color::transfer::bt709_to_linear(x);
        assert!((code(&p) - shown(&p, light)).abs() < 1e-3);
        // ACES2065-1 chosen: the file's values are its light.
        set(&mut p, Some(ColorSpace::Aces2065_1));
        assert!((code(&p) - shown(&p, x)).abs() < 1e-3);
        // S-Log3: decoded by its curve, 18%.
        set(&mut p, Some(ColorSpace::SLog3SGamut3Cine));
        assert!((code(&p) - shown(&p, 0.18)).abs() < 1e-3);
        // Without ACES the color space does nothing.
        p.settings.color.view = None;
        let plain = px(&p, Tick::ZERO)[0];
        assert!((plain - light).abs() < 1e-4);
    }

    #[test]
    fn a_render_leaves_in_its_templates_color_space() {
        use bv_timeline::{AcesDisplay, AcesView, ColorSpace, ViewTransform};
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let grey = solid(&mut p, [0.5, 0.5, 0.5, 1.0]);
        p.insert_media(grey, Tick::ZERO, Some(v), None).unwrap();
        p.settings.color.view = Some(ViewTransform {
            display: AcesDisplay::Srgb,
            view: AcesView::Aces,
        });
        // ACES2065-1 out of an ACES2065-1 project: the working values as
        // they are (written as code values: BT.709's curve on them).
        p.settings.color.output = Some(ColorSpace::Aces2065_1);
        let out = bv_color::transfer::linear_to_bt709(px(&p, Tick::ZERO)[0]);
        let light = bv_color::transfer::bt709_to_linear(0.5);
        assert!((out - light).abs() < 1e-4, "{out} vs {light}");
    }

    #[test]
    fn aces_with_the_video_gamma_takes_the_encoded_values() {
        use bv_color::transfer::linear_to_bt709;
        use bv_timeline::{AcesDisplay, AcesSpace, AcesView, CompositingGamma, ViewTransform};
        // As VEGAS's view, measured on the calibration card: with the 2.222
        // gamma a picture's encoded values are ACES's light; with 1.000 its
        // light (BT.709's curve) is.
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let gray = solid(&mut p, [0.5, 0.5, 0.5, 1.0]);
        p.insert_media(gray, Tick::ZERO, Some(v), None).unwrap();
        let light = px(&p, Tick::ZERO)[0];
        let encoded = linear_to_bt709(light);
        p.settings.color.space = AcesSpace::AcesCg;
        p.settings.color.view = Some(ViewTransform {
            display: AcesDisplay::Srgb,
            view: AcesView::Aces,
        });
        // The display's code value a gray `g` (taken as ACES2065-1) shows.
        let shown = |p: &Project, g: f32| {
            let c = p.settings.color_pipeline().unwrap();
            c.view.apply(c.enter([g; 3]).map(f64::from))[0] as f32
        };
        let code = |p: &Project| linear_to_bt709(px(p, Tick::ZERO)[0]);
        assert!((code(&p) - shown(&p, light)).abs() < 1e-3);
        p.settings.compositing_gamma = CompositingGamma::Video;
        assert!((code(&p) - shown(&p, encoded)).abs() < 1e-3);
        assert!(shown(&p, encoded) > shown(&p, light) + 0.1);
    }

    #[test]
    fn output_rotation() {
        let mut img = ImageRgbaF32::filled(3, 2, [0.0, 0.0, 0.0, 1.0]);
        *img.pixel_mut(0, 0) = [1.0, 0.0, 0.0, 1.0];
        let cw = rotate(&img, OutputRotation::Clockwise90);
        assert_eq!((cw.width, cw.height), (2, 3));
        assert_eq!(cw.pixel(1, 0)[0], 1.0, "top-left goes to top-right");
        let ccw = rotate(&img, OutputRotation::Counterclockwise90);
        assert_eq!(ccw.pixel(0, 2)[0], 1.0, "top-left goes to bottom-left");
        let inv = rotate(&img, OutputRotation::Inverted180);
        assert_eq!(inv.pixel(2, 1)[0], 1.0);
        // The output keeps the turned size.
        let mut p = Project::new(linear());
        p.settings.rotation = OutputRotation::Clockwise90;
        assert_eq!(p.settings.output_size(), (1080, 1920));
        let out = render_output(&p, Tick::ZERO, 9, 16, &mut VideoPool::new());
        assert_eq!((out.width, out.height), (9, 16));
    }

    #[test]
    fn midia_offline_aparece_marcada() {
        let mut p = Project::new(linear());
        let v = p.tracks[0].id;
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.source = bv_timeline::MediaSource::File {
            path: "nao-existe.mp4".into(),
        };
        let m = p.add_media(info);
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let mut pool = VideoPool::new();
        let img = render_frame(&p, Tick::ZERO, 16, 9, &mut pool);
        assert_eq!(img.pixel(8, 4), OFFLINE);
        assert!(pool.error(m).is_some());
    }
}
