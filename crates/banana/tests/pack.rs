//! Every effect of the pack, checked: its description, its presets, what it
//! draws from any picture, its settings (each must matter), its ends, and
//! that a preview of fewer pixels shows what the render will.

#![allow(clippy::print_stdout)]

use std::collections::HashSet;

use bv_banana::cpu::{Picture, run};
use bv_banana::fx::{Frame, Fx, Kind, Param, Shape, V, common};

fn threads(data: &mut [[f32; 4]], width: usize, f: &(dyn Fn(usize, &mut [[f32; 4]]) + Sync)) {
    let n = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows = data.len() / width.max(1);
    let each = rows.div_ceil(n).max(1);
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

fn unit(x: u32) -> f32 {
    let mut h = x;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16_777_216.0
}

/// A picture with a sky, a sun, hills, hard edges, fine texture and — on
/// the right — a see-through part; `which` a different one.
fn picture(w: u32, h: u32, which: u32) -> Picture {
    let mut p = Picture::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let mut c = [0.25 + 0.5 * v, 0.45 + 0.2 * v, 0.85 - 0.4 * v];
            let (sx, sy) = if which == 0 { (0.3, 0.3) } else { (0.7, 0.35) };
            let d = ((u - sx) * 1.78).hypot(v - sy);
            if d < 0.06 {
                c = [1.0, 0.98, 0.9];
            }
            let hill = 0.62 + 0.1 * (u * 7.0 + which as f32 * 2.0).sin();
            if v > hill {
                let t = unit((x * 8 / w.max(8)) * 7 + (y * 8 / h.max(8)) * 131 + which);
                c = [0.18 + 0.2 * t, 0.3 + 0.15 * t, 0.12];
            }
            if (0.45..0.55).contains(&u) && (0.4..0.8).contains(&v) {
                c = [0.85, 0.12, 0.1];
            }
            // Skin, a dark wall and fine grain.
            if (0.6..0.72).contains(&u) && (0.2..0.5).contains(&v) {
                c = [0.8, 0.58, 0.46];
            }
            if u < 0.12 && v > 0.5 {
                c = [0.02, 0.02, 0.03];
            }
            if v > 0.3 {
                let g = unit(x * 7919 + y * 104_729 + which * 31) * 0.06 - 0.03;
                c = c.map(|k| (k + g).max(0.0));
            }
            let a = if u > 0.9 { (1.0 - u) * 10.0 } else { 1.0 };
            p.data[(y * w + x) as usize] = [c[0] * a, c[1] * a, c[2] * a, a];
        }
    }
    p
}

/// A picture of half the pixels, each the mean of four.
fn halved(p: &Picture) -> Picture {
    let mut o = Picture::new(p.w / 2, p.h / 2);
    for y in 0..o.h {
        for x in 0..o.w {
            let at = |i: u32, j: u32| p.data[((y * 2 + j) * p.w + x * 2 + i) as usize];
            o.data[(y * o.w + x) as usize] = std::array::from_fn(|c| {
                (at(0, 0)[c] + at(1, 0)[c] + at(0, 1)[c] + at(1, 1)[c]) * 0.25
            });
        }
    }
    o
}

fn at(w: u32, h: u32, scale: f32) -> Frame {
    Frame {
        w,
        h,
        scale,
        shape: 16.0 / 9.0,
        frame: 31.0,
        rate: 30.0,
        length: 60.0,
        progress: 0.43,
        alone: false,
        others: 0,
    }
}

fn pictures(fx: &Fx, v: &V<'_>, f: &mut Frame) -> Vec<Picture> {
    let mut p = vec![picture(f.w, f.h, 0)];
    if fx.kind == Kind::Transition {
        p.push(picture(f.w, f.h, 1));
    } else {
        for (n, _) in fx.times(v, f.frame).iter().enumerate() {
            p.push(picture(f.w, f.h, 1 + n as u32));
        }
        f.others = p.len() - 1;
    }
    p
}

fn draw(fx: &Fx, v: &V<'_>, mut f: Frame) -> Picture {
    let p = pictures(fx, v, &mut f);
    let g = fx.graph(v, f);
    let refs: Vec<&Picture> = p.iter().collect();
    run(&g, &refs, &threads)
}

fn defaults(fx: &Fx) -> V<'_> {
    V::of(fx, |_| None)
}

fn with<'a>(fx: &'a Fx, changes: &[(&str, [f64; 3])]) -> V<'a> {
    V::of(fx, |p| {
        changes.iter().find(|(n, _)| *n == p.name).map(|(_, v)| *v)
    })
}

fn preset_value(values: &[(&str, &[f64])], p: &Param) -> Option<[f64; 3]> {
    let v = values.iter().find(|(n, _)| *n == p.name)?.1;
    Some(std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0)))
}

/// How far apart two pictures are: the mean of the channels' differences.
fn apart(a: &Picture, b: &Picture) -> f32 {
    let n = (a.data.len() * 4) as f32;
    a.data
        .iter()
        .zip(&b.data)
        .flat_map(|(x, y)| (0..4).map(move |c| (x[c] - y[c]).abs().min(4.0)))
        .sum::<f32>()
        / n
}

fn finite(p: &Picture) -> bool {
    p.data.iter().all(|c| c.iter().all(|v| v.is_finite()))
}

#[test]
fn the_effects_are_described_well() {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for fx in bv_banana::all() {
        assert!(ids.insert(fx.id), "{} twice", fx.id);
        assert!(names.insert(fx.name), "{} twice", fx.name);
        assert!(
            fx.id.chars().all(|c| c.is_ascii_alphanumeric()),
            "{}",
            fx.id
        );
        assert!(fx.about.ends_with('.'), "{}: {}", fx.id, fx.about);
        assert!(!fx.presets.is_empty(), "{} has no presets", fx.id);
        let mut settings = HashSet::new();
        for p in &fx.params {
            if p.shape == Shape::End {
                continue;
            }
            assert!(settings.insert(p.name), "{}: {} twice", fx.id, p.name);
            assert!(!p.label.is_empty(), "{}: {}", fx.id, p.name);
            match p.shape {
                Shape::Num { hard, show } => {
                    let d = p.default[0];
                    assert!(
                        hard[0] <= show[0] && show[1] <= hard[1] && show[0] < show[1],
                        "{}: {}",
                        fx.id,
                        p.name
                    );
                    assert!(
                        (show[0]..=show[1]).contains(&d),
                        "{}: {} starts at {d}, its slider shows {show:?}",
                        fx.id,
                        p.name
                    );
                }
                Shape::Int { lo, hi } => {
                    let d = p.default[0] as i32;
                    assert!((lo..=hi).contains(&d), "{}: {}", fx.id, p.name);
                }
                Shape::Choice(o) => {
                    assert!((p.default[0] as usize) < o.len(), "{}: {}", fx.id, p.name);
                }
                _ => {}
            }
        }
        let mut presets = HashSet::new();
        for p in &fx.presets {
            assert!(presets.insert(p.name), "{}: {} twice", fx.id, p.name);
            for (name, v) in p.values {
                let param = fx
                    .params
                    .iter()
                    .find(|x| x.name == *name)
                    .unwrap_or_else(|| panic!("{}: {}: no setting {name}", fx.id, p.name));
                assert_eq!(v.len(), param.len(), "{}: {}: {name}", fx.id, p.name);
                match param.shape {
                    Shape::Num { hard, .. } => assert!(
                        (hard[0]..=hard[1]).contains(&v[0]),
                        "{}: {}: {name}",
                        fx.id,
                        p.name
                    ),
                    Shape::Int { lo, hi } => assert!(
                        (f64::from(lo)..=f64::from(hi)).contains(&v[0]),
                        "{}: {}: {name}",
                        fx.id,
                        p.name
                    ),
                    Shape::Choice(o) => {
                        assert!((v[0] as usize) < o.len(), "{}: {}: {name}", fx.id, p.name)
                    }
                    _ => {}
                }
            }
        }
        for h in &fx.handles {
            use bv_banana::fx::Handle;
            let names: Vec<&str> = match *h {
                Handle::Point(a) => vec![a],
                Handle::Line(a, b) | Handle::Ring(a, b) | Handle::Path(a, b) => vec![a, b],
                Handle::Arrow("", b) => vec![b],
                Handle::Arrow(a, b) => vec![a, b],
                Handle::Quad(q) => q.to_vec(),
            };
            for n in names {
                assert!(
                    fx.params.iter().any(|p| p.name == n),
                    "{}: a handle on {n}",
                    fx.id
                );
            }
        }
    }
    assert!(ids.len() > 250, "{}", ids.len());
}

/// What the interface shows names no other program or company.
#[test]
fn the_texts_name_no_other_program() {
    let refused = [
        "vegas", "sapphire", "boris", "mocha", "magix", "sony", "adobe",
    ];
    for fx in bv_banana::all() {
        let mut texts = vec![fx.name, fx.about];
        for p in &fx.params {
            texts.extend([p.label, p.hint]);
            if let Shape::Choice(o) = p.shape {
                texts.extend(o);
            }
        }
        texts.extend(fx.presets.iter().map(|p| p.name));
        for t in texts {
            let low = t.to_lowercase();
            assert!(!refused.iter().any(|r| low.contains(r)), "{}: {t}", fx.id);
        }
    }
}

#[test]
fn every_preset_draws_numbers_on_any_picture() {
    for fx in bv_banana::all() {
        for n in std::iter::once(None).chain((0..fx.presets.len()).map(Some)) {
            let v = V::of(fx, |p| {
                n.and_then(|n| preset_value(fx.presets[n].values, p))
            });
            let o = draw(fx, &v, at(96, 54, 0.05));
            assert!(finite(&o), "{} {n:?}", fx.id);
            assert_eq!((o.w, o.h), (96, 54), "{}", fx.id);
        }
        // Odd and tiny pictures, upright ones.
        let v = defaults(fx);
        for (w, h) in [(1, 1), (2, 3), (7, 1), (33, 60)] {
            let mut f = at(w, h, 0.05);
            f.shape = w as f32 / h as f32;
            let o = draw(fx, &v, f);
            assert!(finite(&o), "{} at {w} × {h}", fx.id);
            assert_eq!((o.w, o.h), (w, h), "{}", fx.id);
        }
    }
}

/// Settings at their ends, and values no setting has, never break a frame.
#[test]
fn settings_at_their_ends_draw_numbers() {
    for fx in bv_banana::all() {
        for high in [false, true] {
            let v = V::of(fx, |p| {
                Some(match p.shape {
                    Shape::Num { hard, .. } => [hard[usize::from(high)], 0.0, 0.0],
                    Shape::Int { lo, hi } => [f64::from(if high { hi } else { lo }), 0.0, 0.0],
                    Shape::Choice(o) => [if high { o.len() as f64 - 1.0 } else { 0.0 }, 0.0, 0.0],
                    Shape::Toggle => [f64::from(u8::from(high)), 0.0, 0.0],
                    Shape::Angle => [if high { 1e5 } else { -1e5 }, 0.0, 0.0],
                    Shape::Point => [if high { 50.0 } else { -50.0 }; 3],
                    Shape::Color => [if high { 8.0 } else { 0.0 }; 3],
                    _ => return None,
                })
            });
            let o = draw(fx, &v, at(32, 18, 0.02));
            assert!(
                finite(&o),
                "{} at its {}",
                fx.id,
                if high { "most" } else { "least" }
            );
        }
        // What is no number: the defaults.
        let v = V::of(fx, |_| Some([f64::NAN; 3]));
        let d = defaults(fx);
        let f = at(24, 14, 0.02);
        assert_eq!(draw(fx, &v, f), draw(fx, &d, f), "{}", fx.id);
    }
}

#[test]
fn transitions_begin_and_end_with_their_pictures() {
    for fx in bv_banana::all()
        .iter()
        .filter(|f| f.kind == Kind::Transition)
    {
        let v = defaults(fx);
        let mut f = at(64, 36, 0.04);
        let (a, b) = (picture(64, 36, 0), picture(64, 36, 1));
        f.progress = 0.0;
        assert_eq!(draw(fx, &v, f), a, "{}", fx.id);
        f.progress = 1.0;
        assert_eq!(draw(fx, &v, f), b, "{}", fx.id);
        // And near them, near: nothing jumps.
        for (n, name) in std::iter::once("defaults")
            .chain(fx.presets.iter().map(|p| p.name))
            .enumerate()
        {
            let v = V::of(fx, |p| {
                (n > 0)
                    .then(|| preset_value(fx.presets[n - 1].values, p))
                    .flatten()
            });
            f.progress = 0.004;
            let d0 = apart(&draw(fx, &v, f), &a);
            f.progress = 0.996;
            let d1 = apart(&draw(fx, &v, f), &b);
            assert!(d0 < 0.06 && d1 < 0.06, "{} ({name}): {d0} {d1}", fx.id);
        }
    }
}

#[test]
fn no_mix_leaves_the_picture_and_the_mask_holds_the_effect() {
    let (w, h) = (64, 36);
    for fx in bv_banana::all()
        .iter()
        .filter(|f| f.kind != Kind::Transition)
    {
        let v = with(fx, &[(common::MIX, [0.0; 3])]);
        assert!(
            fx.kind == Kind::Generator || fx.is_identity(&v),
            "{}",
            fx.id
        );
        let o = draw(fx, &v, at(w, h, 0.04));
        assert!(apart(&o, &picture(w, h, 0)) < 1e-6, "{}", fx.id);
        // A small mask: its outside is the picture's own.
        let v = with(
            fx,
            &[
                (common::MASK, [2.0, 0.0, 0.0]),
                (common::MASK_WIDTH, [0.2, 0.0, 0.0]),
                (common::MASK_HEIGHT, [0.2, 0.0, 0.0]),
                (common::MASK_FEATHER, [0.0; 3]),
            ],
        );
        let o = draw(fx, &v, at(w, h, 0.04));
        let src = picture(w, h, 0);
        for (x, y) in [(2, 2), (60, 2), (2, 33), (60, 33)] {
            let i = (y * w + x) as usize;
            assert_eq!(o.data[i], src.data[i], "{} at {x}, {y}", fx.id);
        }
    }
}

/// Every setting changes what is drawn, with some value of the others.
#[test]
fn every_setting_matters() {
    let mut dead = Vec::new();
    for fx in bv_banana::all() {
        // The states tried: the defaults, the presets, and everything that
        // is off at first turned on.
        let mut states: Vec<Vec<(&str, [f64; 3])>> = vec![Vec::new()];
        for p in &fx.presets {
            states.push(
                p.values
                    .iter()
                    .map(|(n, v)| {
                        (
                            *n,
                            std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0)),
                        )
                    })
                    .collect(),
            );
        }
        for p in &fx.params {
            if let Shape::Choice(o) = p.shape {
                states.extend((0..o.len()).map(|n| vec![(p.name, [n as f64, 0.0, 0.0])]));
            }
        }
        let lively: Vec<(&str, [f64; 3])> = fx
            .params
            .iter()
            .filter_map(|p| match p.shape {
                Shape::Num { show, .. } if p.default[0] == 0.0 => {
                    Some((p.name, [show[1] * 0.4, 0.0, 0.0]))
                }
                Shape::Angle if p.default[0] == 0.0 => Some((p.name, [25.0, 0.0, 0.0])),
                _ => None,
            })
            .collect();
        for c in fx.params.iter() {
            if let Shape::Choice(o) = c.shape {
                for n in 0..o.len() {
                    let mut s = lively.clone();
                    s.push((c.name, [n as f64, 0.0, 0.0]));
                    states.push(s);
                }
            }
        }
        states.push(lively);
        // Places elsewhere, the rest as it is.
        states.push(
            fx.params
                .iter()
                .enumerate()
                .filter(|(_, p)| p.shape == Shape::Point)
                .map(|(i, p)| {
                    (
                        p.name,
                        [0.35 + 0.1 * (i % 4) as f64, 0.6 - 0.1 * (i % 3) as f64, 0.0],
                    )
                })
                .collect(),
        );
        // And everything anywhere on its slider.
        for n in 0..14u32 {
            states.push(
                fx.params
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        !p.is_empty() && !p.name.starts_with("mask") && p.name != common::MIX
                    })
                    .map(|(i, p)| {
                        let r = |k: u32| f64::from(unit(n * 7919 + i as u32 * 131 + k + 1));
                        let v = match p.shape {
                            Shape::Num { show, .. } => {
                                [show[0] + (show[1] - show[0]) * r(0), 0.0, 0.0]
                            }
                            Shape::Int { lo, hi } => [
                                (f64::from(lo) + f64::from(hi - lo) * r(0)).round(),
                                0.0,
                                0.0,
                            ],
                            Shape::Choice(o) => [(o.len() as f64 * r(0)).floor(), 0.0, 0.0],
                            Shape::Toggle => [r(0).round(), 0.0, 0.0],
                            Shape::Angle => [r(0) * 360.0, 0.0, 0.0],
                            _ => [r(0), r(1), r(2)],
                        };
                        (p.name, v)
                    })
                    .collect(),
            );
        }
        for p in &fx.params {
            // The mask's are checked elsewhere; the tracker's are the host's.
            let name = p.name;
            if p.is_empty() || p.hidden || name.starts_with("mask") || name == common::MIX {
                continue;
            }
            let other: [f64; 3] = match p.shape {
                Shape::Num { show, .. } => {
                    let d = p.default[0];
                    let far = if d - show[0] > show[1] - d {
                        show[0]
                    } else {
                        show[1]
                    };
                    [d + (far - d) * 0.618, 0.0, 0.0]
                }
                Shape::Int { lo, hi } => {
                    let d = p.default[0] as i32;
                    [
                        f64::from(if d < hi { (d + 3).min(hi) } else { lo }),
                        0.0,
                        0.0,
                    ]
                }
                Shape::Choice(o) => [((p.default[0] as usize + 1) % o.len()) as f64, 0.0, 0.0],
                Shape::Toggle => [1.0 - p.default[0], 0.0, 0.0],
                Shape::Angle => [p.default[0] + 50.0, 0.0, 0.0],
                Shape::Point => [p.default[0] + 0.23, p.default[1] - 0.17, 0.0],
                Shape::Color => p.default.map(|c| if c > 0.5 { c - 0.45 } else { c + 0.45 }),
                _ => continue,
            };
            let mut matters = false;
            // Small pictures first; a large one for what is a pixel or two.
            let sizes = [(128, 72), (640, 400)];
            let tries = sizes.iter().flat_map(|z| {
                // Few of the large ones: they take long.
                let n = if z.0 > 200 { 10 } else { states.len() };
                states.iter().rev().take(n).map(move |s| (*z, s))
            });
            'found: for ((w, h), s) in tries {
                let frames: &[f64] = if h > 100 {
                    &[31.0]
                } else {
                    &[31.0, 44.0, 3.0, 25.0, 36.0, 17.0]
                };
                for &frame in frames {
                    let mut f = at(w, h, 0.5);
                    f.frame = frame;
                    f.progress = frame / 60.0;
                    let a = draw(fx, &with(fx, s), f);
                    let mut c: Vec<(&str, [f64; 3])> =
                        s.iter().copied().filter(|(n, _)| *n != name).collect();
                    c.push((name, other));
                    let b = draw(fx, &with(fx, &c), f);
                    if apart(&a, &b) > 1e-6 {
                        matters = true;
                        break 'found;
                    }
                    // Other frames' times may change though the test's
                    // pictures do not.
                    let (va, vb) = (with(fx, s), with(fx, &c));
                    if fx.times(&va, frame) != fx.times(&vb, frame) {
                        matters = true;
                        break 'found;
                    }
                }
            }
            if !matters {
                dead.push(format!("{}: {name}", fx.id));
            }
        }
    }
    assert!(dead.is_empty(), "settings that change nothing: {dead:#?}");
}

/// A preview of half the pixels shows what the whole picture does.
#[test]
fn a_smaller_preview_shows_the_same() {
    let (w, h) = (960, 520);
    let mut off = Vec::new();
    for fx in bv_banana::all() {
        let v = defaults(fx);
        let mut f = at(w, h, 0.5);
        let whole = pictures(fx, &v, &mut f);
        let small: Vec<Picture> = whole.iter().map(halved).collect();
        let refs: Vec<&Picture> = whole.iter().collect();
        let full = run(&fx.graph(&v, f), &refs, &threads);
        (f.w, f.h, f.scale) = (w / 2, h / 2, 0.25);
        let refs: Vec<&Picture> = small.iter().collect();
        let half = run(&fx.graph(&v, f), &refs, &threads);
        // Both brought to 24 × 13 cells.
        let cells = |p: &Picture| -> Vec<[f32; 4]> {
            let (cw, ch) = (p.w / 24, p.h / 13);
            let mut out = Vec::new();
            for j in 0..13 {
                for i in 0..24 {
                    let mut sum = [0.0f32; 4];
                    for y in j * ch..(j + 1) * ch {
                        for x in i * cw..(i + 1) * cw {
                            let c = p.data[(y * p.w + x) as usize];
                            for k in 0..4 {
                                sum[k] += c[k].clamp(-1.0, 3.0);
                            }
                        }
                    }
                    out.push(sum.map(|v| v / (cw * ch) as f32));
                }
            }
            out
        };
        let (a, b) = (cells(&full), cells(&half));
        let d: f32 = a
            .iter()
            .zip(&b)
            .flat_map(|(x, y)| (0..4).map(move |c| (x[c] - y[c]).abs()))
            .sum::<f32>()
            / (a.len() * 4) as f32;
        if d > 0.02 {
            off.push(format!("{}: {d:.3}", fx.id));
        }
    }
    assert!(off.is_empty(), "previews that differ: {off:#?}");
}

/// Frames a moment apart are alike: nothing that moves flashes by accident.
#[test]
fn motion_is_smooth() {
    // Meant to jump from frame to frame.
    let jumpy = [
        "DigitalGlitch",
        "FilmGrain",
        "SensorNoise",
        "OldFilm",
        "BadReception",
        "Videotape",
        "FrostedGlass",
        "Lightning",
        "LightningStorm",
        "ElectricEdges",
        "Flashbulbs",
        "LampFlicker",
        "CameraShake",
        "TileScramble",
        "Compression",
        "DataMosh",
        "MuzzleFlash",
        "LaserBeam",
    ];
    for fx in bv_banana::all() {
        if fx.kind == Kind::Transition || fx.is_temporal() || jumpy.contains(&fx.id) {
            continue;
        }
        let v = defaults(fx);
        let mut f = at(96, 54, 0.09);
        f.frame = 40.0;
        let a = draw(fx, &v, f);
        f.frame = 40.25;
        let b = draw(fx, &v, f);
        let d = apart(&a, &b);
        assert!(d < 0.02, "{}: {d}", fx.id);
        if !fx.moves {
            assert_eq!(a, b, "{} moves", fx.id);
        }
    }
}

/// Every text of the pack the interface shows (and translates), one a
/// line: `cargo test -p bv-banana --test pack texts -- --ignored --nocapture`.
#[test]
#[ignore = "prints"]
fn the_packs_texts() {
    let mut texts: Vec<(String, &str)> = Vec::new();
    for fx in bv_banana::all() {
        texts.push((format!("name {}", fx.id), fx.name));
        texts.push((format!("about {}", fx.id), fx.about));
        texts.push(("group".into(), fx.group.name()));
        for p in &fx.params {
            texts.push((format!("label {}", fx.id), p.label));
            texts.push((format!("hint {}", fx.id), p.hint));
            if let Shape::Choice(o) = p.shape {
                texts.extend(
                    o.iter()
                        .map(|t| (format!("choice {} {}", fx.id, p.label), *t)),
                );
            }
        }
        texts.extend(
            fx.presets
                .iter()
                .map(|p| (format!("preset {}", fx.id), p.name)),
        );
    }
    let mut seen = HashSet::new();
    for (what, t) in texts {
        if !t.is_empty() && seen.insert(t) {
            println!("TEXT\t{t}\t{what}");
        }
    }
}

/// `picture` without its see-through part, then `alpha` of it.
fn veiled(w: u32, h: u32, alpha: f32) -> Picture {
    let mut p = picture(w, h, 0);
    for c in &mut p.data {
        let a = c[3].max(1e-6);
        *c = [c[0] / a * alpha, c[1] / a * alpha, c[2] / a * alpha, alpha];
    }
    p
}

fn through(fx: &Fx, v: &V<'_>, f: Frame, p: &Picture) -> Picture {
    run(&fx.graph(v, f), &[p], &threads)
}

/// A filter changes the picture with nothing set (but those for a fault,
/// a transparency or a moment it has not).
#[test]
fn filters_do_something() {
    let quiet = [
        // For flicker, noise, damaged frames and combing.
        "SteadyLight",
        "FrameRepair",
        "Deinterlace",
        // Its shadow falls outside the picture's alpha.
        "DropShadow",
        // Nothing holds them at this moment.
        "FreezeFrame",
        // Their numbers begin where the picture is as it was.
        "ChannelMixer",
        "GammaGain",
        "HueSaturation",
    ];
    let mut idle = Vec::new();
    for fx in bv_banana::all() {
        if fx.kind != Kind::Filter || quiet.contains(&fx.id) {
            continue;
        }
        let v = defaults(fx);
        let mut f = at(960, 540, 0.5);
        let p = pictures(fx, &v, &mut f);
        let refs: Vec<&Picture> = p.iter().collect();
        let o = run(&fx.graph(&v, f), &refs, &threads);
        if apart(&o, &p[0]) < 1e-4 {
            idle.push(fx.id);
        }
    }
    assert!(idle.is_empty(), "filters that do nothing: {idle:#?}");
}

/// Colors change alike however see-through they are, and nothing where
/// there is nothing.
#[test]
fn colors_change_alike_where_see_through() {
    // What lies over the picture as a whole, and what glows by its light.
    let over = ["Mist", "ColorFuse"];
    let mut wrong = Vec::new();
    for fx in bv_banana::all() {
        if fx.group != bv_banana::fx::Group::Color || over.contains(&fx.id) {
            continue;
        }
        for n in std::iter::once(None).chain((0..fx.presets.len()).map(Some)) {
            let v = V::of(fx, |p| {
                n.and_then(|n| preset_value(fx.presets[n].values, p))
            });
            let f = at(96, 54, 0.1);
            let whole = through(fx, &v, f, &veiled(96, 54, 1.0));
            let half = through(fx, &v, f, &veiled(96, 54, 0.5));
            let none = through(fx, &v, f, &Picture::new(96, 54));
            let d = whole
                .data
                .iter()
                .zip(&half.data)
                .flat_map(|(a, b)| (0..4).map(move |c| (a[c] * 0.5 - b[c]).abs()))
                .fold(0.0f32, f32::max);
            let left = none
                .data
                .iter()
                .flat_map(|c| c.iter().map(|v| v.abs()))
                .fold(0.0f32, f32::max);
            if d > 0.01 || left > 1e-4 {
                wrong.push(format!("{} {n:?}: {d:.3}, {left:.3}", fx.id));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A blur one way leaves the other alone, and none moves the picture, at
/// odd sizes too.
#[test]
fn blurs_keep_to_their_way_and_place() {
    let (_, fx) = bv_banana::find("SoftBlur").expect("the blur");
    for (w, h) in [(321u32, 181u32), (320, 180), (257, 143)] {
        let mut f = at(w, h, 1.0 / 3.0);
        f.shape = w as f32 / h as f32;
        let mut rows = Picture::new(w, h);
        for y in 0..h {
            let c = if y % 2 == 0 { 0.9 } else { 0.1 };
            for x in 0..w {
                rows.data[(y * w + x) as usize] = [c, c, c, 1.0];
            }
        }
        let v = with(fx, &[("size", [12.0, 0.0, 0.0]), ("down", [0.0; 3])]);
        let o = through(fx, &v, f, &rows);
        assert!(apart(&o, &rows) < 1e-4, "across, {w} × {h}");
        // A dot's middle stays.
        for size in [0.5, 2.0, 6.0, 15.0] {
            let mut dot = Picture::new(w, h);
            let (cx, cy) = (w / 2, h / 2);
            dot.data[(cy * w + cx) as usize] = [1.0; 4];
            let v = with(fx, &[("size", [size, 0.0, 0.0])]);
            let o = through(fx, &v, f, &dot);
            let (mut sx, mut sy, mut sum) = (0.0f64, 0.0f64, 0.0f64);
            for y in 0..h {
                for x in 0..w {
                    let k = f64::from(o.data[(y * w + x) as usize][3]);
                    sx += f64::from(x) * k;
                    sy += f64::from(y) * k;
                    sum += k;
                }
            }
            let (dx, dy) = (sx / sum - f64::from(cx), sy / sum - f64::from(cy));
            assert!(
                dx.abs() < 0.3 && dy.abs() < 0.3 && (sum - 1.0).abs() < 0.05,
                "{size} at {w} × {h}: moved {dx:.2}, {dy:.2}, {sum:.3} of its light"
            );
        }
    }
}

/// Past the edges: held, mirrored or repeated as the setting says.
#[test]
fn edges_are_as_they_are_named() {
    let (_, fx) = bv_banana::find("Transform").expect("the transform");
    let edges = fx
        .params
        .iter()
        .find(|p| p.name == "edges")
        .expect("its edges");
    let Shape::Choice(names) = edges.shape else {
        panic!("a choice");
    };
    assert_eq!(names[..4], ["Transparent", "Hold", "Mirror", "Repeat"]);
    // Left half dark, right half light, moved half a frame to the right.
    let (w, h) = (160, 90);
    let mut p = Picture::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let c = (x as f32 + 0.5) / w as f32;
            p.data[(y * w + x) as usize] = [c, c, c, 1.0];
        }
    }
    let position = fx
        .params
        .iter()
        .find(|p| p.shape == Shape::Point && p.name == "position")
        .expect("its position");
    let seen: Vec<[f32; 4]> = (0..4)
        .map(|e| {
            let v = with(
                fx,
                &[
                    ("edges", [f64::from(e), 0.0, 0.0]),
                    (
                        "position",
                        [position.default[0] + 0.5, position.default[1], 0.0],
                    ),
                ],
            );
            through(fx, &v, at(w, h, 0.1), &p).data[(45 * w + 40) as usize]
        })
        .collect();
    // Where the picture's −0.25 shows.
    assert!(seen[0][3] < 1e-3, "{seen:?}");
    assert!(seen[1][0] < 0.02, "{seen:?}");
    assert!((seen[2][0] - 0.25).abs() < 0.02, "{seen:?}");
    assert!((seen[3][0] - 0.75).abs() < 0.02, "{seen:?}");
}
