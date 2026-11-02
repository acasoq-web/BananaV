//! Pixelate through BananaV's OpenFX host, as any host runs it: described
//! as VEGAS's, its presets' cells counted on a frame and each cell the
//! average of the pixels it covers, see-through ones weighing less.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ClipSource, Context, Instance, Plugin, RenderRequest, Setup, Value,
};

const PIXELATE: &str = "org.bananav.Pixelate";

fn plugin() -> &'static Arc<Plugin> {
    static PLUGINS: OnceLock<Vec<Arc<Plugin>>> = OnceLock::new();
    PLUGINS
        .get_or_init(|| {
            let bin = Binary::built_in(
                bv_effects::BINARY,
                bv_effects::number_of_plugins,
                bv_effects::get_plugin,
                None,
            );
            let (plugins, failures) = bin.plugins().expect("BananaV's effects load");
            assert!(failures.is_empty(), "{failures:?}");
            plugins
        })
        .iter()
        .find(|p| p.info.identifier == PIXELATE)
        .expect(PIXELATE)
}

/// A still picture as a clip.
struct Still(Arc<ImageRgbaF32>);

impl ClipSource for Still {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        (self.0.width as f64, self.0.height as f64)
    }
}

/// A 160 × 90 frame whose every pixel differs: red across, green down.
fn ramps() -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(160, 90);
    for y in 0..90 {
        for x in 0..160 {
            *img.pixel_mut(x, y) = [x as f32 / 160.0, y as f32 / 90.0, 0.5, 1.0];
        }
    }
    Arc::new(img)
}

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (160.0, 90.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(), Context::Filter, &setup).expect("an instance")
}

fn render(inst: &Instance, src: &Arc<ImageRgbaF32>) -> ImageRgbaF32 {
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(src)));
    let req = RenderRequest {
        time: 0.0,
        width: src.width,
        height: src.height,
        scale: 1.0,
        draft: false,
        interactive: true,
        sequential: false,
        timeline: None,
    };
    inst.render_with(&[("Source", source)], &[], &req)
        .expect("it renders")
}

fn set(inst: &Instance, h: f64, v: f64) {
    inst.set_values(&[
        (
            "horizontal".into(),
            Animation::constant(Value::Num(vec![h])),
        ),
        ("vertical".into(), Animation::constant(Value::Num(vec![v]))),
    ]);
}

fn preset(inst: &Instance, name: &str) {
    let p = bv_effects::presets(PIXELATE)
        .into_iter()
        .find(|p| p.name == name)
        .expect(name);
    for (param, v) in &p.values {
        let bv_effects::PresetValue::Num(v) = v else {
            panic!("{v:?}")
        };
        inst.set_values(&[((*param).into(), Animation::constant(Value::Num(v.clone())))]);
    }
}

/// Where each run of equal pixels starts along row `y` (from the top).
fn runs_across(img: &ImageRgbaF32, y: u32) -> Vec<u32> {
    (0..img.width)
        .filter(|&x| x == 0 || img.pixel(x, y) != img.pixel(x - 1, y))
        .collect()
}

/// Where each run of equal pixels starts down column `x`.
fn runs_down(img: &ImageRgbaF32, x: u32) -> Vec<u32> {
    (0..img.height)
        .filter(|&y| y == 0 || img.pixel(x, y) != img.pixel(x, y - 1))
        .collect()
}

/// The average of `img` over columns `xs` and rows `ys` (from the top).
fn mean(img: &ImageRgbaF32, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> [f32; 4] {
    let mut sum = [0.0f64; 4];
    let n = (xs.len() * ys.len()) as f64;
    for y in ys {
        for x in xs.clone() {
            for (s, v) in sum.iter_mut().zip(img.pixel(x, y)) {
                *s += f64::from(v);
            }
        }
    }
    sum.map(|s| (s / n) as f32)
}

#[test]
fn pixelate_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Pixelate");
    assert_eq!(info.grouping, "BananaV/Creative");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Horizontal pixelization", "Vertical pixelization"]);
    let names: Vec<_> = bv_effects::presets(PIXELATE)
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Small",
            "Medium",
            "Large",
            "Medium Horizontal",
            "Medium Vertical"
        ]
    );
}

#[test]
fn pixelate_makes_cells_of_the_average_color() {
    let inst = instance();
    let src = ramps();
    // (Default): nothing changes.
    assert_eq!(render(&inst, &src).data, src.data);
    // Large: 160 · 0.25 / 2 + 1 = 21 cells across, 90 · 0.25 / 2 + 1 = 12
    // down (whole ones), from edge to edge.
    preset(&inst, "Large");
    let out = render(&inst, &src);
    let across = runs_across(&out, 45);
    let down = runs_down(&out, 80);
    assert_eq!((across.len(), down.len()), (21, 12), "{across:?} {down:?}");
    // VEGAS's cells: across from the left, 160 ⁄ 21 pixels in 16,384ths
    // apart (the first 7 wide); up from the bottom, 7.5 apart, so the top
    // one runs from row 82 up to the frame's end, 8 tall. Each the average
    // of as many whole pixels as the step holds from its first: the top
    // left one 7 × 7, its top row left out.
    assert_eq!((across[1], down[1]), (7, 8));
    let first = mean(&src, 0..7, 1..8);
    let got = out.pixel(3, 3);
    assert!(
        got.iter().zip(first).all(|(a, b)| (a - b).abs() < 1e-6),
        "{got:?} {first:?}"
    );
    // Medium Horizontal: cells across only, every row its own.
    preset(&inst, "Medium Horizontal");
    let out = render(&inst, &src);
    assert_eq!(runs_across(&out, 45).len(), 41);
    assert_eq!(runs_down(&out, 80).len(), 90);
    // Medium Vertical: the other way.
    preset(&inst, "Medium Vertical");
    let out = render(&inst, &src);
    assert_eq!(runs_across(&out, 45).len(), 160);
    assert_eq!(runs_down(&out, 80).len(), 23);
    // At 1, one cell: the frame's average.
    set(&inst, 1.0, 1.0);
    let out = render(&inst, &src);
    let all = mean(&src, 0..160, 0..90);
    for (x, y) in [(0, 0), (159, 89), (80, 45)] {
        let got = out.pixel(x, y);
        assert!(
            got.iter().zip(all).all(|(a, b)| (a - b).abs() < 1e-5),
            "{got:?} {all:?}"
        );
    }
}

#[test]
fn see_through_pixels_weigh_less_in_a_cell() {
    // Opaque red on the left half, nothing on the right: a cell over the
    // edge is half see-through, red where it shows.
    let mut img = ImageRgbaF32::transparent(160, 90);
    for y in 0..90 {
        for x in 0..80 {
            *img.pixel_mut(x, y) = [1.0, 0.0, 0.0, 1.0];
        }
    }
    let src = Arc::new(img);
    let inst = instance();
    // 13 cells across (160 · 0.15 / 2 = 12, plus 1): the seventh, pixels
    // 73 to 85, is over the edge; it averages its first 12.
    set(&inst, 0.85, 0.0);
    let out = render(&inst, &src);
    let middle = out.pixel(80, 10);
    let expected = mean(&src, 73..85, 10..11);
    assert!(
        middle
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{middle:?} {expected:?}"
    );
    assert!(middle[3] > 0.4 && middle[3] < 0.6 && middle[1] == 0.0);
    assert_eq!(out.pixel(10, 10), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(out.pixel(150, 10)[3], 0.0);
}
