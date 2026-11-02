//! LUT Filter through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's, a picked file naming the table, its colors through
//! the table (either way of working out what lies between its points),
//! mixed by Strength, and a file that is not a table changing nothing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

const LUT: &str = "org.bananav.LutFilter";

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
        .find(|p| p.info.identifier == LUT)
        .expect(LUT)
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

/// A 16 × 9 frame whose colors differ: red across, green down, blue a
/// mix; the last row half see-through.
fn picture() -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(16, 9);
    for y in 0..9 {
        for x in 0..16 {
            let c = [x as f32 / 15.0, y as f32 / 8.0, ((x + y) % 7) as f32 / 6.0];
            let a = if y == 8 { 0.5 } else { 1.0 };
            *img.pixel_mut(x, y) = [c[0] * a, c[1] * a, c[2] * a, a];
        }
    }
    Arc::new(img)
}

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (16.0, 9.0),
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

fn set(inst: &Instance, name: &str, v: Value) {
    inst.set_values(&[(name.into(), Animation::constant(v))]);
}

fn value(inst: &Instance, name: &str) -> Value {
    inst.values()
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| a.value)
        .expect(name)
}

/// In a folder of test `test`'s own, a `.cube` lattice of `n`³ colors
/// each `f` of its place, titled `title`.
fn cube_file(
    test: &str,
    name: &str,
    title: Option<&str>,
    n: usize,
    f: impl Fn([f32; 3]) -> [f32; 3],
) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bananav-lut-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = title.map_or(String::new(), |t| format!("TITLE \"{t}\"\n"));
    s += &format!("LUT_3D_SIZE {n}\n");
    let k = (n - 1) as f32;
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let c = f([r as f32 / k, g as f32 / k, b as f32 / k]);
                s += &format!("{} {} {}\n", c[0], c[1], c[2]);
            }
        }
    }
    let path = dir.join(name);
    std::fs::write(&path, s).unwrap();
    path
}

fn text(p: &Path) -> Value {
    Value::Text(p.display().to_string())
}

/// The straight color of a pixel (its alpha divided out).
fn straight(px: [f32; 4]) -> [f32; 3] {
    [px[0] / px[3], px[1] / px[3], px[2] / px[3]]
}

fn near(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
}

#[test]
fn lut_filter_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "LUT Filter");
    assert_eq!(info.grouping, "BananaV/Color");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Name", "File", "Interpolation", "Strength"]);
    assert_eq!(filter.params[1].string_mode, "OfxParamStringIsFilePath");
    assert_eq!(
        filter.params[2].options,
        ["Linear (good)", "Tetrahedral (best)"]
    );
    assert_eq!(filter.params[3].default, Value::Num(vec![1.0]));
}

#[test]
fn a_picked_file_names_the_table() {
    let titled = cube_file("names", "titled.cube", Some("Teal and Orange"), 2, |c| c);
    let untitled = cube_file("names", "Film Print.cube", None, 2, |c| c);
    let inst = instance();
    set(&inst, "file", text(&titled));
    inst.changed(&["file".into()], ChangeReason::User, 0.0);
    assert_eq!(value(&inst, "name"), Value::Text("Teal and Orange".into()));
    set(&inst, "file", text(&untitled));
    inst.changed(&["file".into()], ChangeReason::User, 0.0);
    assert_eq!(value(&inst, "name"), Value::Text("Film Print".into()));
    let _ = std::fs::remove_dir_all(titled.parent().unwrap());
}

#[test]
fn colors_go_through_the_table() {
    let src = picture();
    let inst = instance();
    // No file: nothing changes.
    assert_eq!(render(&inst, &src).data, src.data);
    // A negative, at full strength then half.
    let negative = cube_file("colors", "negative.cube", None, 5, |c| c.map(|v| 1.0 - v));
    set(&inst, "file", text(&negative));
    let out = render(&inst, &src);
    for (x, y) in [(0, 0), (7, 4), (15, 8)] {
        let (s, o) = (straight(src.pixel(x, y)), straight(out.pixel(x, y)));
        assert!(near(o, s.map(|v| 1.0 - v), 1e-5), "{s:?} → {o:?}");
        // Alpha stays.
        assert_eq!(out.pixel(x, y)[3], src.pixel(x, y)[3]);
    }
    set(&inst, "strength", Value::Num(vec![0.5]));
    let out = render(&inst, &src);
    assert!(near(straight(out.pixel(3, 2)), [0.5; 3], 1e-5));
    // A lattice that is not linear: each way as bv-lut works it out.
    let bent = cube_file("colors", "bent.cube", None, 3, |[r, g, b]| {
        [r * g, g * b, b * r]
    });
    let lut = bv_lut::Lut::read(&bent).unwrap();
    set(&inst, "file", text(&bent));
    set(&inst, "strength", Value::Num(vec![1.0]));
    for (i, how) in [
        bv_lut::Interpolation::Linear,
        bv_lut::Interpolation::Tetrahedral,
    ]
    .into_iter()
    .enumerate()
    {
        set(&inst, "interpolation", Value::Num(vec![i as f64]));
        let out = render(&inst, &src);
        let s = straight(src.pixel(11, 3));
        let o = straight(out.pixel(11, 3));
        assert!(near(o, lut.apply(s, how), 1e-5), "{how:?}: {o:?}");
    }
    // Not a table: nothing changes.
    let dir = negative.parent().unwrap();
    let junk = dir.join("junk.cube");
    std::fs::write(&junk, "hello\n").unwrap();
    set(&inst, "file", text(&junk));
    assert_eq!(render(&inst, &src).data, src.data);
    let _ = std::fs::remove_dir_all(dir);
}
