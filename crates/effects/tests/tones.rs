//! Black and White, Invert and Sepia through BananaV's OpenFX host, as any
//! host runs them: described as VEGAS's, their presets in VEGAS's order,
//! and a color through each.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

fn plugin(id: &str) -> &'static Arc<Plugin> {
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
        .find(|p| p.info.identifier == id)
        .unwrap_or_else(|| panic!("no {id}"))
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

/// `id` with `values` over its defaults, rendering one opaque pixel of
/// `color`.
fn through(id: &str, values: &[(&str, Vec<f64>)], color: [f32; 3]) -> [f32; 4] {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (1.0, 1.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    let inst = Instance::new(plugin(id), Context::Filter, &setup).expect("an instance");
    let values: Vec<(String, Animation)> = values
        .iter()
        .map(|(n, v)| (n.to_string(), Animation::constant(Value::Num(v.clone()))))
        .collect();
    inst.set_values(&values);
    let names: Vec<String> = values.into_iter().map(|(n, _)| n).collect();
    inst.changed(&names, ChangeReason::User, 0.0);
    let mut img = ImageRgbaF32::transparent(1, 1);
    *img.pixel_mut(0, 0) = [color[0], color[1], color[2], 1.0];
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::new(img)));
    let req = RenderRequest {
        time: 0.0,
        width: 1,
        height: 1,
        scale: 1.0,
        draft: false,
        interactive: true,
        sequential: false,
        timeline: None,
    };
    inst.render_with(&[("Source", source)], &[], &req)
        .expect("it renders")
        .pixel(0, 0)
}

fn bytes(p: [f32; 4]) -> [u8; 3] {
    [p[0], p[1], p[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn labels(id: &str) -> Vec<String> {
    let filter = plugin(id).info.context(Context::Filter).unwrap();
    assert!(
        filter.params.iter().all(|p| p.animates),
        "{id}: all have clocks"
    );
    filter.params.iter().map(|p| p.label.clone()).collect()
}

fn preset_names(id: &str) -> Vec<&'static str> {
    bv_effects::presets(id)
        .into_iter()
        .map(|p| p.name)
        .collect()
}

#[test]
fn black_and_white_takes_each_color_to_its_average() {
    let id = "org.bananav.BlackAndWhite";
    assert_eq!(labels(id), ["Blend amount"]);
    assert_eq!(
        preset_names(id),
        [
            "25% Black and White",
            "50% Black and White",
            "75% Black and White",
            "100% Black and White"
        ]
    );
    // VEGAS's card at 1: the plain average.
    let red = [1.0, 0.0, 0.0];
    assert_eq!(bytes(through(id, &[], red)), [85, 85, 85]);
    assert_eq!(
        bytes(through(id, &[("blendAmount", vec![0.0])], red)),
        [255, 0, 0]
    );
}

#[test]
fn invert_mixes_each_channel_with_its_opposite() {
    let id = "org.bananav.Invert";
    assert_eq!(labels(id), ["Blend amount"]);
    assert_eq!(
        preset_names(id),
        ["60% Inversion", "75% Inversion", "100% Inversion"]
    );
    let c = [0.0, 100.0 / 255.0, 1.0];
    assert_eq!(bytes(through(id, &[], c)), [255, 155, 0]);
    // VEGAS's 60% Inversion on the card.
    assert_eq!(
        bytes(through(id, &[("amount", vec![0.6])], c)),
        [153, 133, 102]
    );
}

#[test]
fn sepia_takes_the_color_away_then_tints() {
    let id = "org.bananav.Sepia";
    assert_eq!(
        labels(id),
        ["Color", "Blending strength", "Blending falloff"]
    );
    assert_eq!(
        preset_names(id),
        [
            "Neutral",
            "Warm",
            "Cold",
            "Old Photographs",
            "Old Newspaper",
            "Currency"
        ]
    );
    // At its defaults nothing changes.
    let red = [1.0, 0.0, 0.0];
    assert_eq!(bytes(through(id, &[], red)), [255, 0, 0]);
    // VEGAS's Old Newspaper and Neutral on the card's red.
    let newspaper = [
        ("color", vec![166.0 / 255.0, 63.0 / 255.0, 0.0]),
        ("blendingStrength", vec![0.48]),
        ("blendingFalloff", vec![0.0]),
    ];
    assert_eq!(bytes(through(id, &newspaper, red)), [140, 62, 48]);
    let neutral = [
        ("blendingStrength", vec![0.72]),
        ("blendingFalloff", vec![0.63]),
    ];
    assert_eq!(bytes(through(id, &neutral, red)), [115, 115, 54]);
}
