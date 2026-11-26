//! Contact sheets of every effect, to look at (left out by default):
//! `BV_BANANA_PICTURE=a.png BV_BANANA_SECOND=b.png cargo test --release -p
//! bv-banana --test sheets -- --ignored --nocapture`. `BV_BANANA_PRESETS=1`
//! draws every preset too, `BV_BANANA_ONLY=Glow,Halation` only those
//! effects (with their presets; transitions at five moments),
//! `BV_BANANA_CELL=240x135` sets the pictures' size (400x225) and
//! `BV_BANANA_OUT` the folder (`target/banana`).
#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use bv_banana::cpu::{Picture, run};
use bv_banana::fx::{Frame, Fx, Kind, Param, V};

/// A sheet's width, as near as whole pictures come.
const SHEET: u32 = 1620;

fn cell() -> (u32, u32) {
    let size = std::env::var("BV_BANANA_CELL").unwrap_or_else(|_| "400x225".into());
    let (w, h) = size.split_once('x').expect("a size");
    (w.parse().expect("a width"), h.parse().expect("a height"))
}

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

/// The picture of variable `var`, box-averaged to a cell's size.
fn load(var: &str, (cw, ch): (u32, u32)) -> Picture {
    let path = std::env::var(var).unwrap_or_else(|_| panic!("{var}: a PNG"));
    let img = bv_png::decode(&std::fs::read(path).expect("the picture")).expect("a PNG");
    let rgba = img.to_rgba8();
    let (w, h) = (img.width, img.height);
    let mut p = Picture::new(cw, ch);
    for y in 0..ch {
        for x in 0..cw {
            let (x0, x1) = (x * w / cw, ((x + 1) * w / cw).max(x * w / cw + 1));
            let (y0, y1) = (y * h / ch, ((y + 1) * h / ch).max(y * h / ch + 1));
            let mut sum = [0.0f32; 4];
            let mut n = 0.0;
            for j in y0..y1.min(h) {
                for i in x0..x1.min(w) {
                    let k = (j * w + i) as usize * 4;
                    for c in 0..4 {
                        sum[c] += f32::from(rgba[k + c]) / 255.0;
                    }
                    n += 1.0;
                }
            }
            p.data[(y * cw + x) as usize] = sum.map(|v| v / n);
        }
    }
    p
}

/// A preset's value for a setting.
fn preset(fx: &Fx, n: Option<usize>, p: &Param) -> Option<[f64; 3]> {
    let v = fx
        .presets
        .get(n?)?
        .values
        .iter()
        .find(|(k, _)| *k == p.name)?
        .1;
    Some(std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0)))
}

/// The frame as it is at another time: moved, so that trails show.
fn moved(p: &Picture, by: f64) -> Picture {
    let dx = (by * f64::from(p.w) / 200.0).round() as i32;
    let mut o = p.clone();
    for y in 0..p.h as i32 {
        for x in 0..p.w as i32 {
            let sx = (x - dx).clamp(0, p.w as i32 - 1);
            o.data[(y * p.w as i32 + x) as usize] = p.data[(y * p.w as i32 + sx) as usize];
        }
    }
    o
}

fn draw(fx: &Fx, n: Option<usize>, progress: f64, a: &Picture, b: &Picture) -> Picture {
    let v = V::of(fx, |p| preset(fx, n, p));
    let mut at = Frame {
        w: a.w,
        h: a.h,
        scale: a.w as f32 / 1920.0,
        shape: a.w as f32 / a.h as f32,
        frame: 72.0,
        rate: 30.0,
        length: 30.0,
        progress,
        alone: false,
        others: 0,
    };
    let mut pics: Vec<Picture> = vec![a.clone()];
    if fx.kind == Kind::Transition {
        pics.push(b.clone());
    } else {
        for t in fx.times(&v, at.frame) {
            pics.push(moved(a, t - at.frame));
        }
        at.others = pics.len() - 1;
    }
    let g = fx.graph(&v, at);
    let refs: Vec<&Picture> = pics.iter().collect();
    run(&g, &refs, &threads)
}

fn sheet(name: &str, cells: &[(String, Picture)], cols: u32) {
    let out = std::env::var("BV_BANANA_OUT").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/banana"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&out).expect("the folder");
    let (w, h) = cell();
    let pad = 4;
    let rows = (cells.len() as u32).div_ceil(cols);
    let (sw, sh) = (cols * (w + pad) + pad, rows * (h + pad) + pad);
    let mut img = vec![40u8; (sw * sh * 4) as usize];
    for p in img.chunks_mut(4) {
        p[3] = 255;
    }
    for (n, (text, p)) in cells.iter().enumerate() {
        let (cx, cy) = (n as u32 % cols, n as u32 / cols);
        let (ox, oy) = (pad + cx * (w + pad), pad + cy * (h + pad));
        for y in 0..h {
            for x in 0..w {
                let c = p.data[(y * w + x) as usize];
                let check = if (x / 10 + y / 10) % 2 == 0 {
                    0.32
                } else {
                    0.4
                };
                let k = (((oy + y) * sw + ox + x) * 4) as usize;
                for i in 0..3 {
                    let v = c[i] + check * (1.0 - c[3].clamp(0.0, 1.0));
                    img[k + i] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        println!("{name} row {} column {}: {text}", cy + 1, cx + 1);
    }
    let png = bv_png::encode_rgba8(sw, sh, &img, 6);
    let path = out.join(format!("{name}.png"));
    std::fs::write(&path, png).expect("the sheet");
    println!("{}", path.display());
}

#[test]
#[ignore = "for looking at"]
fn sheets() {
    let size = cell();
    let a = load("BV_BANANA_PICTURE", size);
    let b = load("BV_BANANA_SECOND", size);
    let cols = (SHEET / (size.0 + 4)).max(1);
    if let Ok(only) = std::env::var("BV_BANANA_ONLY") {
        for name in only.split(',') {
            let (_, fx) = bv_banana::find(name).unwrap_or_else(|| panic!("{name}"));
            let mut cells = Vec::new();
            if fx.kind == Kind::Transition {
                for t in [0.15, 0.35, 0.5, 0.65, 0.85] {
                    cells.push((format!("{} at {t}", fx.name), draw(fx, None, t, &a, &b)));
                }
            }
            cells.push((
                format!("{} (defaults)", fx.name),
                draw(fx, None, 0.4, &a, &b),
            ));
            for (n, p) in fx.presets.iter().enumerate() {
                cells.push((
                    format!("{}: {}", fx.name, p.name),
                    draw(fx, Some(n), 0.4, &a, &b),
                ));
            }
            sheet(&format!("only-{name}"), &cells, cols);
        }
        return;
    }
    let presets = std::env::var_os("BV_BANANA_PRESETS").is_some();
    let mut groups: Vec<(&str, Vec<(String, Picture)>)> = Vec::new();
    for fx in bv_banana::all() {
        let g = fx.group.name();
        let mut cells = vec![(fx.name.to_string(), draw(fx, None, 0.4, &a, &b))];
        if presets {
            for (n, p) in fx.presets.iter().enumerate() {
                if !p.values.is_empty() {
                    let name = format!("{}: {}", fx.name, p.name);
                    cells.push((name, draw(fx, Some(n), 0.4, &a, &b)));
                }
            }
        }
        match groups.iter_mut().find(|(n, _)| *n == g) {
            Some((_, v)) => v.extend(cells),
            None => groups.push((g, cells)),
        }
    }
    let rows = (920 / (size.1 + 4)).max(1);
    for (g, cells) in &groups {
        let name = g.to_lowercase().replace([' ', '&'], "");
        for (n, part) in cells.chunks((cols * rows) as usize).enumerate() {
            sheet(&format!("{name}-{}", n + 1), part, cols);
        }
    }
}

/// Lightning at many sizes, curves and shapes of frame, alone on black:
/// `cargo test --release -p bv-banana --test sheets bolts -- --ignored`.
#[test]
#[ignore = "for looking at"]
fn bolts() {
    let (_, fx) = bv_banana::find("Lightning").expect("the bolt");
    let (w, h) = cell();
    type Change = (&'static str, [f64; 3]);
    let case = |name: &str, changes: &[Change], (cw, ch): (u32, u32)| {
        let v = V::of(fx, |p| {
            changes.iter().find(|(n, _)| *n == p.name).map(|(_, v)| *v)
        });
        let f = Frame {
            w: cw,
            h: ch,
            scale: ch as f32 / 1080.0,
            shape: cw as f32 / ch as f32,
            frame: 72.0,
            rate: 30.0,
            length: 60.0,
            progress: 0.0,
            alone: true,
            others: 0,
        };
        let p = run(&fx.graph(&v, f), &[&Picture::new(1, 1)], &threads);
        // Centered in a cell.
        let mut o = Picture::new(w, h);
        for c in &mut o.data {
            c[3] = 1.0;
        }
        for y in 0..ch.min(h) {
            for x in 0..cw.min(w) {
                let (ox, oy) = ((w - cw.min(w)) / 2 + x, (h - ch.min(h)) / 2 + y);
                let c = p.data[(y * cw + x) as usize];
                o.data[(oy * w + ox) as usize] = [c[0], c[1], c[2], 1.0];
            }
        }
        (name.to_string(), o)
    };
    let ends = |x0: f64, y0: f64, x1: f64, y1: f64| -> Vec<Change> {
        vec![("from", [x0, y0, 0.0]), ("to", [x1, y1, 0.0])]
    };
    let curve = |between: &[(f64, f64)], (x0, y0): (f64, f64), (x1, y1): (f64, f64)| {
        let mut v = ends(x0, y0, x1, y1);
        v.push(("points", [between.len() as f64, 0.0, 0.0]));
        for (n, (x, y)) in between.iter().enumerate() {
            v.push((bv_banana::fx::POINTS[n], [*x, *y, 0.0]));
        }
        v
    };
    let mut straight = curve(
        &[(0.3, 0.85), (0.5, 0.2), (0.7, 0.85)],
        (0.1, 0.2),
        (0.9, 0.2),
    );
    straight.push(("curve", [1.0, 0.0, 0.0]));
    let cells = vec![
        case("defaults", &[], (w, h)),
        case("across", &ends(0.04, 0.5, 0.96, 0.5), (w, h)),
        case("corner to corner", &ends(-0.1, 1.1, 1.1, -0.1), (w, h)),
        case("half", &ends(0.3, 0.5, 0.7, 0.5), (w, h)),
        case("a quarter", &ends(0.4, 0.5, 0.6, 0.5), (w, h)),
        case("short", &ends(0.46, 0.5, 0.54, 0.5), (w, h)),
        case("very short", &ends(0.485, 0.5, 0.515, 0.5), (w, h)),
        case(
            "a curve",
            &curve(&[(0.5, 0.85)], (0.1, 0.2), (0.9, 0.2)),
            (w, h),
        ),
        case(
            "an S",
            &curve(&[(0.35, 0.9), (0.65, 0.1)], (0.1, 0.5), (0.9, 0.5)),
            (w, h),
        ),
        case(
            "a spiral",
            &curve(
                &[
                    (0.5, 0.9),
                    (0.72, 0.5),
                    (0.5, 0.15),
                    (0.33, 0.45),
                    (0.5, 0.65),
                    (0.58, 0.5),
                ],
                (0.22, 0.5),
                (0.5, 0.4),
            ),
            (w, h),
        ),
        case("straight corners", &straight, (w, h)),
        case("branches", &[("branches", [8.0, 0.0, 0.0])], (w, h)),
        case(
            "six bolts",
            &[("bolts", [6.0, 0.0, 0.0]), ("branches", [0.0; 3])],
            (w, h),
        ),
        case("half the way", &[("length", [50.0, 0.0, 0.0])], (w, h)),
        case(
            "wide, with a wide glow",
            &[("width", [2.0, 0.0, 0.0]), ("glow", [12.0, 0.0, 0.0])],
            (w, h),
        ),
        case("most crooked", &[("stray", [80.0, 0.0, 0.0])], (w, h)),
        case("finest", &[("fine", [4.0, 0.0, 0.0])], (w, h)),
        case("a tall frame", &[], (h * 9 / 16, h)),
        case("a square frame", &[], (h, h)),
        case("a tiny preview", &[], (w / 4, h / 4)),
    ];
    sheet("bolts", &cells, (SHEET / (w + 4)).max(1));
}
