//! Unsharp Mask through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's (its settings, their sliders over 0–1 with Amount
//! going past them, its presets) and pictures through it.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ClipSource, Context, Instance, Plugin, RenderRequest, Setup, Value,
};

const UNSHARP: &str = "org.bananav.UnsharpMask";

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
        .find(|p| p.info.identifier == UNSHARP)
        .expect(UNSHARP)
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

fn instance(w: u32, h: u32, values: &[(&str, f64)]) -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (w as f64, h as f64),
        pixel_aspect: 1.0,
        interactive: false,
    };
    let inst = Instance::new(plugin(), Context::Filter, &setup).expect("an instance");
    let values: Vec<_> = values
        .iter()
        .map(|(n, v)| (n.to_string(), Animation::constant(Value::Num(vec![*v]))))
        .collect();
    inst.set_values(&values);
    inst
}

fn render(inst: &Instance, src: &Arc<ImageRgbaF32>) -> ImageRgbaF32 {
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(src)));
    let req = RenderRequest {
        time: 0.0,
        width: src.width,
        height: src.height,
        scale: 1.0,
        draft: false,
        interactive: false,
        sequential: false,
        timeline: None,
    };
    inst.render_with(&[("Source", source)], &[], &req)
        .expect("it renders")
}

/// A gray picture `w` × `h`, its columns from `x` on `right`, the others
/// `left`.
fn step(w: u32, h: u32, x: u32, left: f32, right: f32) -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(w, h);
    for (i, px) in img.data.iter_mut().enumerate() {
        let v = if (i as u32 % w) < x { left } else { right };
        *px = [v, v, v, 1.0];
    }
    Arc::new(img)
}

fn gray(img: &ImageRgbaF32, x: u32, y: u32) -> f32 {
    img.data[(y * img.width + x) as usize][0]
}

#[test]
fn unsharp_mask_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Unsharp Mask");
    assert_eq!(info.grouping, "BananaV/Blur");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Amount", "Radius", "Threshold"]);
    for p in &filter.params {
        // Sliders over 0–1, at 0 at first; Amount typed past them.
        assert_eq!(
            (p.display_min[0], p.display_max[0]),
            (0.0, 1.0),
            "{}",
            p.name
        );
        assert_eq!(p.default.num(0), 0.0, "{}", p.name);
    }
    let amount = &filter.params[0];
    assert!(amount.min[0] <= -3.0 && amount.max[0] >= 4.0);
    let names: Vec<_> = bv_effects::presets(UNSHARP)
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Light",
            "Medium",
            "Strong",
            "Extreme",
            "Negative",
            "Outline Edges"
        ]
    );
}

#[test]
fn an_edge_is_sharpened_and_flat_parts_kept() {
    let src = step(64, 8, 32, 0.3, 0.7);
    let inst = instance(64, 8, &[("amount", 1.0), ("radius", 0.1)]);
    let out = render(&inst, &src);
    // Either side of the edge pulled apart, overshooting.
    assert!(gray(&out, 32, 4) > 0.75, "{}", gray(&out, 32, 4));
    assert!(gray(&out, 31, 4) < 0.25, "{}", gray(&out, 31, 4));
    // Farther than the blur reaches, and at the frame's ends (the picture
    // held there is as flat), nothing changes but VEGAS's filter's few
    // ten-thousandths on flat parts.
    for x in [0, 10, 20, 44, 54, 63] {
        let want = if x < 32 { 0.3 } else { 0.7 };
        assert!(
            (gray(&out, x, 4) - want).abs() < 1e-3,
            "{x}: {}",
            gray(&out, x, 4)
        );
    }
}

#[test]
fn changes_under_the_threshold_are_left_out() {
    // A faint step and a strong one: VEGAS's Extreme sharpens the strong
    // one only.
    let mut img = ImageRgbaF32::transparent(96, 4);
    for (i, px) in img.data.iter_mut().enumerate() {
        let x = i as u32 % 96;
        let v = match x {
            0..32 => 0.5,
            32..64 => 0.505,
            _ => 0.8,
        };
        *px = [v, v, v, 1.0];
    }
    let src = Arc::new(img);
    let inst = instance(
        96,
        4,
        &[("amount", 4.0), ("radius", 0.025), ("threshold", 0.1)],
    );
    let out = render(&inst, &src);
    // Away from the strong step (its blur reaches 3 pixels).
    for x in 0..60 {
        let want = if x < 32 { 0.5 } else { 0.505 };
        assert!(
            (gray(&out, x, 2) - want).abs() < 1e-6,
            "{x}: {}",
            gray(&out, x, 2)
        );
    }
    assert!(gray(&out, 64, 2) > 0.9, "{}", gray(&out, 64, 2));
    assert!(gray(&out, 63, 2) < 0.4, "{}", gray(&out, 63, 2));
}

#[test]
fn negative_turns_the_picture_around_its_blur() {
    // VEGAS's Negative (Amount -2, Radius 1): a light square and a dark
    // one on gray come out the other way round.
    let mut img = ImageRgbaF32::transparent(64, 36);
    for (i, px) in img.data.iter_mut().enumerate() {
        let (x, y) = (i as u32 % 64, i as u32 / 64);
        let v = if (10..16).contains(&x) && (10..16).contains(&y) {
            0.9
        } else if (40..46).contains(&x) && (20..26).contains(&y) {
            0.0
        } else {
            0.4
        };
        *px = [v, v, v, 1.0];
    }
    let src = Arc::new(img);
    let inst = instance(64, 36, &[("amount", -2.0), ("radius", 1.0)]);
    let out = render(&inst, &src);
    let (light, dark) = (gray(&out, 12, 12), gray(&out, 42, 22));
    assert!(dark > light + 0.5, "{light} {dark}");
}
