//! The graphics card's frames against the processor's (`bv-render`, the
//! reference): the same scenes — every transition, effect, framing,
//! quality, compositing gamma, rotation, offline media — drawn by both,
//! and every Y′CbCr layout converted by both. Skipped (passes) on a
//! computer without a usable graphics card.

#![allow(clippy::print_stdout)]

use std::sync::Arc;

use bv_core::{
    ColorInfo, ColorPrimaries, ColorRange, ImageRgbaF32, MatrixCoefficients, PixelFormat, Plane,
    PlaneData, Tick, TransferCharacteristic, VideoFrame,
};
use bv_gpu::{Compositor, Gpu, Output, Rendered};
use bv_render::VideoPool;
use bv_timeline::fx::grade;
use bv_timeline::{
    Effect, EffectKind, FadeCurve, Generator, MediaInfo, MediaSource, Project, ProjectSettings,
    RenderQuality, TrackKind, Transform, Transition,
};

fn gpu() -> Option<Compositor> {
    match Gpu::open(None) {
        Ok(g) => {
            println!("graphics card: {:?}", g.adapter());
            g.warm_up().expect("every kernel compiles");
            Some(Compositor::new(g))
        }
        Err(e) => {
            println!("no graphics card ({e}): skipped");
            None
        }
    }
}

/// The largest difference between two images, any channel.
fn max_diff(a: &ImageRgbaF32, b: &ImageRgbaF32) -> f32 {
    assert_eq!((a.width, a.height), (b.width, b.height), "sizes");
    a.data
        .iter()
        .zip(&b.data)
        .flat_map(|(x, y)| (0..4).map(move |c| (x[c] - y[c]).abs()))
        .fold(0.0, f32::max)
}

fn gpu_frame(c: &mut Compositor, p: &Project, t: Tick, w: u32, h: u32) -> ImageRgbaF32 {
    let rotation = p.settings.rotation;
    let (sw, sh) = if rotation.is_quarter_turn() {
        (h, w)
    } else {
        (w, h)
    };
    let scene = bv_timeline::scene::frame_scene(p, t, sw, sh);
    let mut pool = VideoPool::new();
    match c
        .render(
            &scene,
            rotation,
            &mut pool,
            [0.0, 0.0, 0.0, 1.0],
            Output::Linear,
        )
        .expect("the card draws")
    {
        Rendered::Linear(img) => img,
        other => unreachable!("{other:?}"),
    }
}

/// Draws `p` at `t` both ways and checks they agree.
fn check(c: &mut Compositor, name: &str, p: &Project, t: Tick, tolerance: f32) {
    let (w, h) = (192, 108);
    let cpu = bv_render::render_output(p, t, w, h, &mut VideoPool::new());
    let gpu = gpu_frame(c, p, t, w, h);
    let d = max_diff(&cpu, &gpu);
    println!("{name:40} {d:.2e}");
    assert!(d <= tolerance, "{name}: differs by {d}");
}

/// 32-bit floating point (full range), compositing in light (the other
/// formats have their own scenes).
fn linear() -> ProjectSettings {
    ProjectSettings {
        pixel_format: bv_timeline::PixelFormat::Float32Full,
        ..ProjectSettings::default()
    }
}

fn media(p: &mut Project, g: Generator) -> bv_timeline::MediaId {
    let info = MediaInfo::generated(g, &p.settings);
    p.add_media(info)
}

fn bars_and_gradient() -> (Project, bv_timeline::ClipId, bv_timeline::ClipId) {
    let mut p = Project::new(linear());
    let v = p.tracks[0].id;
    let bars = media(&mut p, Generator::ColorBars);
    let grad = media(
        &mut p,
        Generator::Gradient {
            from: [0.9, 0.2, 0.1, 1.0],
            to: [0.1, 0.3, 0.8, 1.0],
            angle_deg: 30.0,
        },
    );
    let a = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
    let b = p
        .insert_media(grad, Tick::from_seconds(3), Some(v), None)
        .unwrap()[0];
    (p, a, b)
}

const TOL: f32 = 2e-3;

#[test]
fn track_motion_with_shadow_and_glow() {
    let Some(mut c) = gpu() else { return };
    let s = Tick::from_seconds;
    let (mut p, bars_clip, grad) = bars_and_gradient();
    let _ = (bars_clip, grad);
    let top = p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .unwrap()
        .id;
    let tilted = bv_timeline::Transform {
        scale: 0.6,
        stretch: 0.8,
        x: 0.15,
        y: -0.1,
        rotation_deg: 12.0,
        ..bv_timeline::Transform::default()
    };
    let new = (bv_timeline::KeyCurve::Smooth, 0.0);
    p.set_track_motion_at(top, false, Tick::ZERO, tilted, new)
        .unwrap();
    check(&mut c, "track motion", &p, s(1), TOL);
    p.set_track_halo_on(top, false, true, true).unwrap();
    p.set_track_halo_on(top, false, false, true).unwrap();
    let glow = bv_timeline::HaloLook {
        color: [0.2, 0.6, 1.0, 1.0],
        ..bv_timeline::HaloLook::GLOW
    };
    p.set_track_halo_at(top, false, false, Tick::ZERO, glow)
        .unwrap();
    check(
        &mut c,
        "track motion with a shadow and a glow",
        &p,
        s(1),
        TOL,
    );
    // Keyframed: halfway between two boxes.
    p.set_track_motion_at(top, false, s(2), bv_timeline::Transform::default(), new)
        .unwrap();
    check(&mut c, "track motion keyframed", &p, s(1), TOL);
}

#[test]
fn adjustment_events_and_tracks() {
    let Some(mut c) = gpu() else { return };
    let s = Tick::from_seconds;
    let (mut p, bars_clip, grad) = bars_and_gradient();
    let _ = (bars_clip, grad);
    // An adjustment event blurring and inverting the bars and the
    // gradient below it, at 60 %.
    let top = p.insert_track_at(bv_timeline::TrackKind::Video, 0);
    let adj = p.add_media(MediaInfo::generated(Generator::Adjustment, &p.settings));
    let id = p.insert_media(adj, Tick::ZERO, Some(top), None).unwrap()[0];
    p.set_clip_duration(id, s(8)).unwrap();
    let event = p.clip_mut(id).unwrap();
    event.effects.push(Effect::new(EffectKind::Invert));
    event
        .effects
        .push(effect(EffectKind::GaussianBlur, &[(0, 6.0)]));
    event.level = 0.6;
    check(&mut c, "adjustment event", &p, s(1), TOL);
    check(&mut c, "adjustment event over a crossfade", &p, s(4), TOL);
    // An adjustment track over all of it, and with the event's track as its
    // only child.
    let adjust = p.insert_adjustment_track_at(0);
    let t = p.track_by_id_mut(adjust).unwrap();
    t.effects
        .push(effect(EffectKind::BrightnessContrast, &[(0, 0.3)]));
    t.level = 0.7;
    check(&mut c, "adjustment track", &p, s(1), TOL);
    p.make_compositing_child(top, false).unwrap();
    check(&mut c, "adjustment track with a child", &p, s(1), TOL);
    // An event of its own over what is below it (a gradient at half its
    // level), through its effects with the rest.
    let grad = p.clip(grad).unwrap().media;
    let own = p
        .insert_media(grad, Tick::ZERO, Some(adjust), None)
        .unwrap()[0];
    p.clip_mut(own).unwrap().level = 0.5;
    check(&mut c, "adjustment track with an event", &p, s(1), TOL);
}

#[test]
fn compositing_modes_and_groups() {
    let Some(mut c) = gpu() else { return };
    // The gradient, half transparent, over the bars on a track below.
    let mut base = Project::new(linear());
    let top = base.tracks[0].id;
    let below = base.add_track(bv_timeline::TrackKind::Video);
    let grad = media(
        &mut base,
        Generator::Gradient {
            from: [0.9, 0.2, 0.1, 0.6],
            to: [0.1, 0.3, 0.8, 1.0],
            angle_deg: 30.0,
        },
    );
    let bars = media(&mut base, Generator::ColorBars);
    base.insert_media(grad, Tick::ZERO, Some(top), None)
        .unwrap();
    base.insert_media(bars, Tick::ZERO, Some(below), None)
        .unwrap();
    for mode in bv_timeline::CompositeMode::ALL {
        let mut p = base.clone();
        p.track_by_id_mut(top).unwrap().composite_mode = mode;
        p.track_by_id_mut(top).unwrap().level = 0.8;
        check(&mut c, &format!("{mode:?}"), &p, Tick::ZERO, TOL);
        // As a compositing parent of the bars, the group over black by
        // Screen.
        p.make_compositing_child(below, false).unwrap();
        p.track_by_id_mut(top).unwrap().parent_mode = bv_timeline::CompositeMode::Screen;
        check(&mut c, &format!("{mode:?} parent"), &p, Tick::ZERO, TOL);
    }
}

#[test]
fn track_effects_after_compositing() {
    let Some(mut c) = gpu() else { return };
    // The gradient a compositing parent of the bars: its Invert before
    // compositing, its blur after, on the group; the bars' own Sepia after
    // theirs.
    let mut p = Project::new(linear());
    let top = p.tracks[0].id;
    let below = p.add_track(bv_timeline::TrackKind::Video);
    let grad = media(
        &mut p,
        Generator::Gradient {
            from: [0.9, 0.2, 0.1, 0.6],
            to: [0.1, 0.3, 0.8, 1.0],
            angle_deg: 30.0,
        },
    );
    let bars = media(&mut p, Generator::ColorBars);
    p.insert_media(grad, Tick::ZERO, Some(top), None).unwrap();
    p.insert_media(bars, Tick::ZERO, Some(below), None).unwrap();
    p.make_compositing_child(below, false).unwrap();
    let t = p.track_by_id_mut(top).unwrap();
    t.level = 0.8;
    t.effects.push(Effect::new(EffectKind::Invert));
    t.effects
        .push(effect(EffectKind::GaussianBlur, &[(0, 6.0)]));
    t.composite_at = Some(1);
    let b = p.track_by_id_mut(below).unwrap();
    b.effects.push(Effect::new(EffectKind::Sepia));
    b.composite_at = Some(0);
    check(
        &mut c,
        "track effects after compositing",
        &p,
        Tick::ZERO,
        TOL,
    );
}

#[test]
fn frame_blend() {
    let Some(mut c) = gpu() else { return };
    // Two pictures, red and blue, a 24 fps sequence in a 60 fps project:
    // the project's second frame 0.4 of the way from the first picture
    // to the second, blended.
    let dir = std::env::temp_dir().join(format!("bv-blend-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, rgb) in [[255u8, 0, 0], [0, 0, 255]].iter().enumerate() {
        bmp(&dir.join(format!("f{i:04}.bmp")), 16, 9, *rgb);
    }
    let first = dir.join("f0000.bmp").to_string_lossy().to_string();
    let rate = bv_core::FrameRate::new(24, 1);
    let info = bv_media::probe_sequence(&first, 2, rate).unwrap();
    let mut p = Project::new(ProjectSettings {
        frame_rate: bv_core::FrameRate::new(60, 1),
        resample: bv_timeline::ResampleMode::FrameBlend,
        ..linear()
    });
    let v = p.tracks[0].id;
    let m = p.add_media(info);
    p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    let t = bv_core::FrameRate::new(60, 1).frame_start(1);
    check(&mut c, "frame blend", &p, t, TOL);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deinterlacing() {
    let Some(mut c) = gpu() else { return };
    // Even lines red, odd lines blue, the upper field first.
    let dir = std::env::temp_dir().join(format!("bv-fields-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fields.bmp");
    striped_bmp(&file, 16, 10);
    let mut info = bv_media::probe_file(&file.to_string_lossy()).unwrap();
    info.video.as_mut().unwrap().field_order = bv_core::FieldOrder::UpperFirst;
    let mut p = Project::new(linear());
    let v = p.tracks[0].id;
    let m = p.add_media(info);
    p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    for method in [
        bv_timeline::DeinterlaceMethod::BlendFields,
        bv_timeline::DeinterlaceMethod::Interpolate,
        bv_timeline::DeinterlaceMethod::SmartAdaptive,
    ] {
        p.settings.deinterlace = method;
        check(
            &mut c,
            &format!("deinterlace {method:?}"),
            &p,
            Tick::ZERO,
            TOL,
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `w` × `h` picture whose even lines (from the top) are red and odd ones
/// blue, as a 24-bit BMP file (its rows from the bottom).
fn striped_bmp(path: &std::path::Path, w: u32, h: u32) {
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
    for r in 0..h {
        let y = h - 1 - r;
        let bgr = if y.is_multiple_of(2) {
            [0, 0, 255]
        } else {
            [255, 0, 0]
        };
        for _ in 0..w {
            b.extend_from_slice(&bgr);
        }
        b.resize(b.len() + (row - w * 3) as usize, 0);
    }
    std::fs::write(path, b).unwrap();
}

/// A `w` × `h` picture of one color, as a 24-bit BMP file.
fn bmp(path: &std::path::Path, w: u32, h: u32, rgb: [u8; 3]) {
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
    for _ in 0..h {
        for _ in 0..w {
            b.extend_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        }
        b.resize(b.len() + (row - w * 3) as usize, 0);
    }
    std::fs::write(path, b).unwrap();
}

#[test]
fn transitions_fades_and_levels() {
    let Some(mut c) = gpu() else { return };
    let s = Tick::from_seconds;
    for kind in Transition::ALL {
        let (mut p, _, b) = bars_and_gradient();
        p.clip_mut(b).unwrap().transition = kind;
        // The crossfade runs from 3 to 5 s.
        check(
            &mut c,
            &format!("crossfade {kind:?}"),
            &p,
            s(3) + Tick(s(1).0 * 3 / 5),
            TOL,
        );
    }
    let (mut p, a, b) = bars_and_gradient();
    p.set_fades(a, Some(s(1)), None).unwrap();
    p.set_fades(b, None, Some(s(2))).unwrap();
    p.clip_mut(b).unwrap().fade_out_curve = FadeCurve::Smooth;
    p.set_clip_level(a, 0.7).unwrap();
    check(&mut c, "fade in, level", &p, Tick(s(1).0 / 2), TOL);
    check(&mut c, "fade out", &p, s(7), TOL);
    // Two tracks, the top one half transparent.
    let v2 = p.add_track(TrackKind::Video);
    let checker = media(&mut p, Generator::Checkerboard { size: 24 });
    p.insert_media(checker, Tick::ZERO, Some(v2), None).unwrap();
    p.track_by_id_mut(v2).unwrap().level = 0.5;
    check(&mut c, "two tracks", &p, s(2), TOL);
    // The top track faded halfway to its top color, at 60% composite level.
    use bv_timeline::EnvelopeKind;
    p.toggle_envelope(v2, EnvelopeKind::FadeToColor).unwrap();
    p.set_envelope_point(v2, EnvelopeKind::FadeToColor, Tick::ZERO, 0.5)
        .unwrap();
    p.track_by_id_mut(v2).unwrap().fade_colors = Some([[255, 160, 40, 255], [0, 0, 0, 255]]);
    p.toggle_envelope(v2, EnvelopeKind::CompositeLevel).unwrap();
    p.set_envelope_point(v2, EnvelopeKind::CompositeLevel, Tick::ZERO, 0.6)
        .unwrap();
    check(&mut c, "fade to color, composite level", &p, s(2), TOL);
    // Toward the bottom color, composited in video gamma.
    p.set_envelope_point(v2, EnvelopeKind::FadeToColor, Tick::ZERO, -0.8)
        .unwrap();
    p.settings.compositing_gamma = bv_timeline::CompositingGamma::Video;
    check(
        &mut c,
        "fade to the bottom color, video gamma",
        &p,
        s(2),
        TOL,
    );
}

/// Pan/Crop masks: a feathered oval and a rectangle cut out of it, on
/// framed bars, cutting the picture or keeping an effect inside them, and
/// animated.
#[test]
fn pan_crop_masks() {
    use bv_timeline::mask::{MaskFeather, MaskMode, MaskPath, PanCropMask};
    let Some(mut c) = gpu() else { return };
    let s = Tick::from_seconds;
    let (mut p, bars, _) = bars_and_gradient();
    let info = p.media(p.clip(bars).unwrap().media).unwrap().video.unwrap();
    let (w, h) = (info.width as f32, info.height as f32);
    let mut oval = MaskPath::oval([0.1 * w, 0.1 * h], [0.8 * w, 0.9 * h]);
    oval.feather = MaskFeather::Both;
    oval.feather_pct = 4.0;
    let mut hole = MaskPath::rectangle([0.4 * w, 0.4 * h], [0.6 * w, 0.6 * h]);
    hole.mode = MaskMode::Negative;
    hole.opacity = 0.7;
    let mut mask = PanCropMask {
        enabled: true,
        ..PanCropMask::default()
    };
    mask.add_path(oval);
    mask.add_path(hole);
    let event = p.clip_mut(bars).unwrap();
    event.transform = Transform {
        scale: 0.8,
        rotation_deg: 10.0,
        x: 0.05,
        ..Transform::default()
    };
    event.mask = Some(mask.clone());
    check(&mut c, "pan/crop masks cutting", &p, s(1), TOL);
    let event = p.clip_mut(bars).unwrap();
    event.effects.push(Effect::new(EffectKind::Invert));
    event.mask.as_mut().unwrap().apply_to_fx = true;
    check(&mut c, "pan/crop masks keeping an effect", &p, s(1), TOL);
    // Animated: the oval moved halfway at 1 s.
    let m = p.clip_mut(bars).unwrap().mask.as_mut().unwrap();
    m.add_key(Tick::ZERO);
    m.map_path_at(s(2), 0, |q| [q[0] + 0.2 * w, q[1]]);
    check(&mut c, "pan/crop masks animated", &p, s(1), TOL);
}

#[test]
fn framing_and_qualities() {
    let Some(mut c) = gpu() else { return };
    let tfs = [
        ("identity", Transform::default()),
        (
            "reduced, moved",
            Transform {
                scale: 0.3,
                x: 0.2,
                y: -0.1,
                ..Transform::default()
            },
        ),
        (
            "rotated, cropped",
            Transform {
                scale: 0.8,
                rotation_deg: 30.0,
                crop_left: 0.1,
                crop_top: 0.05,
                crop_right: 0.2,
                ..Transform::default()
            },
        ),
        (
            "enlarged, flipped",
            Transform {
                scale: 3.0,
                flip_h: true,
                flip_v: true,
                stretch: 0.8,
                ..Transform::default()
            },
        ),
    ];
    for quality in RenderQuality::ALL {
        for (name, tf) in &tfs {
            let mut p = Project::new(linear());
            p.settings.render_quality = quality;
            let v = p.tracks[0].id;
            let bars = media(&mut p, Generator::Checkerboard { size: 7 });
            let id = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
            p.clip_mut(id).unwrap().transform = *tf;
            check(&mut c, &format!("{quality:?} {name}"), &p, Tick::ZERO, TOL);
        }
    }
    // A picture of another shape (letterboxed) and of another size.
    let mut p = Project::new(linear());
    let v = p.tracks[0].id;
    let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
    let mut tall = info.video.unwrap();
    tall.width = 640;
    tall.height = 480;
    info.video = Some(tall);
    let m = p.add_media(info);
    p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    check(&mut c, "4:3 in 16:9", &p, Tick::ZERO, TOL);
    // Wide pixels (HDV's 1440 × 1080, 4 : 3 pixels), with effects, in a
    // project of square pixels and in one of NTSC DV ones.
    let mut p = Project::new(linear());
    let v = p.tracks[0].id;
    let mut info = MediaInfo::generated(Generator::Checkerboard { size: 9 }, &p.settings);
    let mut wide = info.video.unwrap();
    wide.width = 1440;
    wide.pixel_aspect = bv_timeline::PixelAspect::new(4, 3);
    info.video = Some(wide);
    let m = p.add_media(info);
    let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
    check(&mut c, "HDV pixels", &p, Tick::ZERO, TOL);
    p.clip_mut(id)
        .unwrap()
        .effects
        .push(Effect::new(EffectKind::Invert));
    p.settings.pixel_aspect = bv_timeline::PixelAspect::new(10, 11);
    check(
        &mut c,
        "HDV pixels in a DV project, effect",
        &p,
        Tick::ZERO,
        TOL,
    );
    // Reduce Interlace Flicker, under a moved picture's edges.
    let mut p = Project::new(linear());
    let v = p.tracks[0].id;
    let bars = media(&mut p, Generator::Checkerboard { size: 3 });
    let id = p.insert_media(bars, Tick::ZERO, Some(v), None).unwrap()[0];
    let clip = p.clip_mut(id).unwrap();
    clip.reduce_flicker = true;
    clip.transform = Transform {
        scale: 0.7,
        y: 0.1,
        ..Transform::default()
    };
    check(&mut c, "reduce interlace flicker", &p, Tick::ZERO, TOL);
}

fn effect(kind: EffectKind, params: &[(usize, f32)]) -> Effect {
    let mut e = Effect::new(kind);
    for &(i, v) in params {
        e.set_param(i, v);
    }
    e
}

#[test]
fn every_effect() {
    let Some(mut c) = gpu() else { return };
    let effects = [
        effect(EffectKind::BrightnessContrast, &[(0, 0.1), (1, 0.3)]),
        effect(
            EffectKind::ColorGrade,
            &[
                (grade::EXPOSURE, 0.5),
                (grade::CONTRAST, 0.2),
                (grade::SATURATION, 1.3),
                (grade::TEMPERATURE, 0.3),
                (grade::TINT, -0.2),
                (grade::LIFT, 0.05),
                (grade::GAMMA + 1, 0.1),
                (grade::GAIN + 2, -0.1),
            ],
        ),
        effect(EffectKind::Levels, &[(0, 0.1), (1, 0.9), (2, 1.2)]),
        effect(EffectKind::BlackAndWhite, &[(0, 0.8)]),
        effect(EffectKind::Sepia, &[(0, 0.7)]),
        effect(EffectKind::Invert, &[(0, 1.0)]),
        effect(EffectKind::GaussianBlur, &[(0, 12.0)]),
        effect(EffectKind::Sharpen, &[(0, 1.0), (1, 3.0)]),
        effect(EffectKind::Vignette, &[]),
        effect(EffectKind::Mirror, &[(0, 1.0), (1, 1.0)]),
        effect(EffectKind::Pixelate, &[(0, 30.0)]),
        effect(EffectKind::ChromaKey, &[]),
        effect(EffectKind::Glow, &[(0, 0.4), (1, 0.8), (2, 10.0)]),
    ];
    for e in &effects {
        for place in ["event", "track", "output"] {
            let (mut p, a, _) = bars_and_gradient();
            match place {
                "event" => p.clip_mut(a).unwrap().effects.push(e.clone()),
                "track" => p.tracks[0].effects.push(e.clone()),
                _ => p.video_output_fx.push(e.clone()),
            }
            check(
                &mut c,
                &format!("{:?} on the {place}", e.kind),
                &p,
                Tick::from_seconds(1),
                TOL,
            );
        }
    }
}

#[test]
fn gamma_rotation_offline_and_muted() {
    use bv_timeline::{CompositingGamma, OutputRotation, PixelFormat as Px};
    let Some(mut c) = gpu() else { return };
    let s = Tick::from_seconds;
    for (name, fmt, gamma) in [
        ("8-bit", Px::Full8Bit, CompositingGamma::Linear),
        (
            "32-bit, video gamma",
            Px::Float32Full,
            CompositingGamma::Video,
        ),
    ] {
        let (mut p, a, _) = bars_and_gradient();
        p.settings.pixel_format = fmt;
        p.settings.compositing_gamma = gamma;
        p.set_clip_level(a, 0.6).unwrap();
        check(&mut c, name, &p, s(4), TOL);
    }
    for r in [
        OutputRotation::Clockwise90,
        OutputRotation::Counterclockwise90,
        OutputRotation::Inverted180,
    ] {
        let (mut p, _, _) = bars_and_gradient();
        p.settings.rotation = r;
        let (w, h) = (108, 192);
        let cpu = bv_render::render_output(&p, s(4), w, h, &mut VideoPool::new());
        let gpu = gpu_frame(&mut c, &p, s(4), w, h);
        let d = max_diff(&cpu, &gpu);
        println!("{:40} {d:.2e}", format!("rotation {r:?}"));
        assert!(d <= TOL, "{r:?}: {d}");
    }
    let mut p = Project::new(linear());
    let v = p.tracks[0].id;
    let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
    info.source = MediaSource::File {
        path: "missing.mp4".into(),
    };
    let m = p.add_media(info);
    p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    check(&mut c, "offline media", &p, Tick::ZERO, 0.0);
    p.mute_all_video = true;
    check(&mut c, "mute all video", &p, Tick::ZERO, 0.0);
    // The video bus's Fade to Color over everything, a third of the way
    // to its bottom color.
    let (mut p, _, _) = bars_and_gradient();
    p.toggle_video_bus_envelope(
        bv_timeline::EnvelopeKind::FadeToColor,
        bv_timeline::KeyCurve::Linear,
    )
    .unwrap();
    p.set_envelope_point(
        bv_timeline::EnvelopeOwner::VideoBus,
        bv_timeline::EnvelopeKind::FadeToColor,
        Tick::ZERO,
        -0.33,
    )
    .unwrap();
    check(&mut c, "video bus fade to color", &p, s(4), TOL);
}

/// ACES (File › Properties › Video): every display's every view, both
/// working spaces and the look, over a crossfade with light pushed past
/// white and strong colors (where the output transform clips), a fade to
/// color, offline media and muted video.
#[test]
fn aces_color_management() {
    use bv_timeline::{
        AcesDisplay, AcesLook, AcesSpace, AcesView, ColorManagement, EnvelopeKind,
        PixelFormat as Px, ViewTransform,
    };
    let Some(mut c) = gpu() else { return };
    let s = Tick::from_seconds;
    let aces = |p: &mut Project, space, display, view, look| {
        p.settings.pixel_format = Px::Float32Full;
        p.settings.color = ColorManagement {
            space,
            view: Some(ViewTransform { display, view }),
            look,
            ..ColorManagement::default()
        };
    };
    for space in AcesSpace::ALL {
        for display in AcesDisplay::ALL {
            for view in AcesView::ALL {
                let (mut p, a, _) = bars_and_gradient();
                aces(&mut p, space, display, view, AcesLook::None);
                // Two stops brighter and more saturated: past white.
                p.clip_mut(a).unwrap().effects.push(effect(
                    EffectKind::ColorGrade,
                    &[(grade::EXPOSURE, 2.0), (grade::SATURATION, 1.8)],
                ));
                check(
                    &mut c,
                    &format!("ACES {space:?} {display:?} {view:?}"),
                    &p,
                    s(4),
                    TOL,
                );
            }
        }
    }
    let (mut p, a, _) = bars_and_gradient();
    aces(
        &mut p,
        AcesSpace::AcesCg,
        AcesDisplay::Rec709,
        AcesView::Aces,
        AcesLook::BlueLightFix,
    );
    p.clip_mut(a)
        .unwrap()
        .effects
        .push(effect(EffectKind::ColorGrade, &[(grade::EXPOSURE, 3.0)]));
    check(&mut c, "ACES Blue Light Artifact Fix", &p, s(4), TOL);
    p.settings.color.look = AcesLook::GamutCompression;
    check(
        &mut c,
        "ACES 1.3 Reference Gamut Compression",
        &p,
        s(4),
        TOL,
    );
    let v = p.tracks[0].id;
    p.toggle_envelope(v, EnvelopeKind::FadeToColor).unwrap();
    p.set_envelope_point(v, EnvelopeKind::FadeToColor, Tick::ZERO, 0.4)
        .unwrap();
    p.track_by_id_mut(v).unwrap().fade_colors = Some([[40, 120, 255, 255], [0, 0, 0, 255]]);
    check(&mut c, "ACES fade to color", &p, s(4), TOL);
    // The 2.222 compositing gamma: the encoded values taken for ACES's
    // light, the look left out.
    p.settings.compositing_gamma = bv_timeline::CompositingGamma::Video;
    check(&mut c, "ACES 2.222 compositing gamma", &p, s(4), TOL);
    // Media in color spaces of their own (Media Properties) whose steps
    // the card draws: their file values through the conversion.
    p.settings.compositing_gamma = bv_timeline::CompositingGamma::Linear;
    for space in [
        bv_timeline::ColorSpace::AcesCg,
        bv_timeline::ColorSpace::LinearSGamut3Cine,
    ] {
        p.media.iter_mut().for_each(|m| m.color_space = Some(space));
        check(&mut c, &format!("ACES media in {space:?}"), &p, s(4), TOL);
    }
    p.media.iter_mut().for_each(|m| m.color_space = None);
    let mut p = Project::new(linear());
    aces(
        &mut p,
        AcesSpace::Aces2065_1,
        AcesDisplay::Srgb,
        AcesView::Aces,
        AcesLook::None,
    );
    let v = p.tracks[0].id;
    let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
    info.source = MediaSource::File {
        path: "missing.mp4".into(),
    };
    let m = p.add_media(info);
    p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
    check(&mut c, "ACES offline media", &p, Tick::ZERO, TOL);
    p.mute_all_video = true;
    check(&mut c, "ACES mute all video", &p, Tick::ZERO, TOL);
}

/// A frame of every Y′CbCr layout, filled with a varied pattern.
fn frame(format: PixelFormat, color: ColorInfo, w: u32, h: u32) -> VideoFrame {
    let mut f = VideoFrame::black(w, h, format, color);
    let depth = format.bit_depth();
    let max = (1u32 << depth) - 1;
    for (n, plane) in f.planes.iter_mut().enumerate() {
        let (pw, ph) = (plane.width, plane.height);
        let value = |x: usize, y: usize| -> u32 {
            let v = (x * 37 + y * 101 + n * 59 + (x * y) % 23) as u32;
            v % (max + 1)
        };
        plane.data = match &plane.data {
            PlaneData::U8(_) => PlaneData::U8(
                (0..ph)
                    .flat_map(|y| (0..pw).map(move |x| (y, x)))
                    .map(|(y, x)| value(x, y) as u8)
                    .collect(),
            ),
            PlaneData::U16(_) => PlaneData::U16(
                (0..ph)
                    .flat_map(|y| (0..pw).map(move |x| (y, x)))
                    .map(|(y, x)| value(x, y) as u16)
                    .collect(),
            ),
        };
        let _: &Plane = plane;
    }
    f
}

#[test]
fn every_ycbcr_layout() {
    let Some(mut c) = gpu() else { return };
    let colors = [
        ColorInfo::default(),
        ColorInfo {
            primaries: ColorPrimaries::Bt601_525,
            transfer: TransferCharacteristic::Bt709,
            matrix: MatrixCoefficients::Bt601,
            range: ColorRange::Full,
        },
        ColorInfo {
            primaries: ColorPrimaries::Bt2020,
            transfer: TransferCharacteristic::Pq,
            matrix: MatrixCoefficients::Bt2020Ncl,
            range: ColorRange::Limited,
        },
        ColorInfo {
            primaries: ColorPrimaries::Bt709,
            transfer: TransferCharacteristic::Srgb,
            matrix: MatrixCoefficients::Identity,
            range: ColorRange::Full,
        },
    ];
    for format in [
        PixelFormat::Yuv420P8,
        PixelFormat::Yuv422P8,
        PixelFormat::Yuv444P8,
        PixelFormat::Yuv420P10,
        PixelFormat::Yuv422P10,
        PixelFormat::Yuv444P10,
    ] {
        for color in colors {
            // Odd sizes: the last chroma column and row are half covered.
            let f = frame(format, color, 99, 61);
            for (min, levels) in [
                ((99, 61), false),
                ((40, 25), false),
                ((20, 12), false),
                ((9, 5), false),
                // The video levels formats: studio RGB.
                ((99, 61), true),
            ] {
                let factor = bv_media::reduction(f.width, f.height, min.0, min.1);
                let cpu = bv_color::frame_to_linear_levels(&f, factor, levels);
                let gpu = c.linear_levels(&f, min, levels).expect("the card converts");
                let d = max_diff(&cpu, &gpu);
                // PQ's light goes up to 100 (10,000 cd/m²).
                let tol = if color.transfer == TransferCharacteristic::Pq {
                    0.05
                } else {
                    1e-4
                };
                assert!(
                    d <= tol,
                    "{format:?} {color:?} reduced {factor}×: differs by {d}"
                );
            }
        }
    }
    println!("every layout agrees");
}

/// A few scenes and layouts on every card of the computer, not only the
/// best one.
#[test]
fn every_card() {
    for a in bv_gpu::adapters() {
        let gpu = Gpu::open(Some(&a.name)).expect("the card opens");
        gpu.warm_up().expect("every kernel compiles");
        let mut c = Compositor::new(gpu);
        let (mut p, a_clip, b) = bars_and_gradient();
        p.clip_mut(b).unwrap().transition = Transition::Iris;
        p.clip_mut(a_clip)
            .unwrap()
            .effects
            .push(effect(EffectKind::GaussianBlur, &[(0, 8.0)]));
        p.settings.render_quality = RenderQuality::Best;
        check(
            &mut c,
            &format!("{} ({})", a.name, a.backend),
            &p,
            Tick::from_seconds(4),
            TOL,
        );
        let f = frame(PixelFormat::Yuv420P10, ColorInfo::default(), 99, 61);
        let cpu = bv_color::frame_to_linear_scaled(&f, 2);
        let d = max_diff(&cpu, &c.linear(&f, (40, 25)).unwrap());
        assert!(d <= 1e-4, "{}: Y′CbCr differs by {d}", a.name);
    }
}

#[test]
fn screen_bytes() {
    let Some(mut c) = gpu() else { return };
    let (p, _, _) = bars_and_gradient();
    let (w, h) = (192, 108);
    let t = Tick::from_seconds(4);
    let cpu = bv_color::linear_to_display_rgba8(
        &bv_render::render_output(&p, t, w, h, &mut VideoPool::new()),
        TransferCharacteristic::Bt709,
    );
    let scene = bv_timeline::scene::frame_scene(&p, t, w, h);
    let Rendered::Display { rgba, .. } = c
        .render(
            &scene,
            p.settings.rotation,
            &mut VideoPool::new(),
            [0.0, 0.0, 0.0, 1.0],
            Output::Display(TransferCharacteristic::Bt709),
        )
        .unwrap()
    else {
        unreachable!()
    };
    let worst = cpu
        .iter()
        .zip(&rgba)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    println!("screen bytes differ by {worst} at most");
    assert!(worst <= 1, "{worst}");
    let _ = Arc::new(());
}

#[test]
fn encoder_bytes() {
    // The frames packed for the video encoders (NV12, P010): the card's as
    // the processor's, also at a width whose rows are not whole words.
    let Some(mut c) = gpu() else { return };
    let (p, _, _) = bars_and_gradient();
    let t = Tick::from_seconds(4);
    for (w, h) in [(192, 108), (194, 110)] {
        let img = bv_render::render_output(&p, t, w, h, &mut VideoPool::new());
        let scene = bv_timeline::scene::frame_scene(&p, t, w, h);
        for ten_bit in [false, true] {
            let cpu = bv_color::linear_to_420(&img, ten_bit);
            let Rendered::Yuv420 { data, .. } = c
                .render(
                    &scene,
                    p.settings.rotation,
                    &mut VideoPool::new(),
                    [0.0, 0.0, 0.0, 1.0],
                    Output::Yuv420 {
                        ten_bit,
                        video_levels: false,
                    },
                )
                .unwrap()
            else {
                unreachable!()
            };
            assert_eq!(data.len(), cpu.len(), "{w}x{h} ten_bit {ten_bit}");
            let worst = if ten_bit {
                let word = |b: &[u8], i: usize| u16::from_le_bytes([b[2 * i], b[2 * i + 1]]) >> 6;
                (0..cpu.len() / 2)
                    .map(|i| word(&cpu, i).abs_diff(word(&data, i)))
                    .max()
                    .unwrap()
            } else {
                cpu.iter()
                    .zip(&data)
                    .map(|(a, b)| a.abs_diff(*b) as u16)
                    .max()
                    .unwrap()
            };
            println!("{w}x{h} encoder bytes (ten_bit {ten_bit}) differ by {worst} at most");
            assert!(worst <= 1, "{w}x{h} ten_bit {ten_bit}: {worst}");
        }
    }
}
