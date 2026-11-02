//! Color Curves through BananaV's OpenFX host, as any host runs it:
//! described, rendered with curves and presets, and Calculate reading the
//! picture.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_core::curve::CurvePoint;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, ParamType, Plugin,
    RenderRequest, Setup, Value,
};

fn curves_plugin() -> &'static Arc<Plugin> {
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
        .find(|p| p.info.identifier == "org.bananav.ColorCurves")
        .expect("Color Curves")
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

/// A grey ramp, black at the left, white at the right; the bottom row half
/// transparent.
fn ramp(w: u32) -> Arc<ImageRgbaF32> {
    let mut img = ImageRgbaF32::transparent(w, 2);
    for x in 0..w {
        let v = x as f32 / (w - 1) as f32;
        *img.pixel_mut(x, 0) = [v, v, v, 1.0];
        *img.pixel_mut(x, 1) = [v * 0.5, v * 0.5, v * 0.5, 0.5];
    }
    Arc::new(img)
}

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (256.0, 2.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(curves_plugin(), Context::Filter, &setup).expect("an instance")
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

fn set_curves(inst: &Instance, curves: Vec<Vec<CurvePoint>>) {
    inst.set_values(&[("curves".into(), Animation::constant(Value::Curves(curves)))]);
}

fn curves_of(inst: &Instance) -> Vec<Vec<CurvePoint>> {
    match inst.values().into_iter().find(|(n, _)| n == "curves") {
        Some((_, a)) => match a.value {
            Value::Curves(c) => c,
            other => panic!("{other:?}"),
        },
        None => panic!("no curves"),
    }
}

#[test]
fn color_curves_describes_itself_as_vegas_does() {
    let info = &curves_plugin().info;
    assert_eq!(info.label, "Color Curves");
    assert_eq!(info.grouping, "BananaV/Color");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Curves", "Auto Modes", "Calculate"]);
    let curves = &filter.params[0];
    assert_eq!(curves.ty, ParamType::Parametric);
    assert_eq!(curves.curves, 3);
    assert_eq!(curves.dimension_labels, ["Red", "Green", "Blue"]);
    assert_eq!(curves.curve_colours.len(), 3);
    assert!(curves.animates);
    // Straight curves to begin with.
    assert_eq!(
        curves.default,
        Value::Curves(vec![bv_core::curve::IDENTITY.to_vec(); 3])
    );
    assert_eq!(
        filter.params[1].options,
        [
            "Color Balance",
            "Color Levels",
            "Contrast",
            "Histogram Equalization"
        ]
    );
    assert_eq!(filter.params[1].default, Value::Num(vec![1.0]));
}

#[test]
fn the_channels_go_through_their_curves() {
    let inst = instance();
    let src = ramp(256);
    // Straight: the picture as it was.
    let same = render(&inst, &src);
    assert_eq!(same.data, src.data);
    // Red inverted, green halved, blue as it was.
    set_curves(
        &inst,
        vec![
            vec![CurvePoint::new(0.0, 1.0), CurvePoint::new(1.0, 0.0)],
            vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 0.5)],
            bv_core::curve::IDENTITY.to_vec(),
        ],
    );
    let out = render(&inst, &src);
    let px = out.pixel(64, 0);
    let v = 64.0 / 255.0;
    assert!((px[0] - (1.0 - v)).abs() < 2e-3, "{px:?}");
    assert!((px[1] - v * 0.5).abs() < 2e-3, "{px:?}");
    assert!((px[2] - v).abs() < 2e-3 && px[3] == 1.0, "{px:?}");
    // Half transparent: the colors' curves, the alpha kept (premultiplied).
    let half = out.pixel(64, 1);
    assert!((half[3] - 0.5).abs() < 1e-6);
    assert!((half[0] / half[3] - (1.0 - v)).abs() < 2e-3, "{half:?}");
}

#[test]
fn the_presets_bend_the_picture_as_vegass() {
    let inst = instance();
    let src = ramp(256);
    let preset = |name: &str| {
        let p = bv_effects::presets("org.bananav.ColorCurves")
            .into_iter()
            .find(|p| p.name == name)
            .expect(name);
        match &p.values[0] {
            ("curves", bv_effects::PresetValue::Curves(c)) => c.clone(),
            other => panic!("{other:?}"),
        }
    };
    // Reduce Contrast: black to 40, white to 210 (of 255).
    set_curves(&inst, preset("Reduce Contrast"));
    let out = render(&inst, &src);
    assert!((out.pixel(0, 0)[0] - 40.0 / 255.0).abs() < 2e-3);
    assert!((out.pixel(255, 0)[1] - 210.0 / 255.0).abs() < 2e-3);
    // Blues and Yellows: blue upside down, red and green as they were.
    set_curves(&inst, preset("Blues and Yellows"));
    let out = render(&inst, &src);
    let px = out.pixel(0, 0);
    assert!(px[0] < 2e-3 && (px[2] - 1.0).abs() < 2e-3, "{px:?}");
    // Increase Contrast: the darks black, the brights white.
    set_curves(&inst, preset("Increase Contrast"));
    let out = render(&inst, &src);
    assert!(out.pixel(30, 0)[0] < 1e-3 && out.pixel(230, 0)[0] > 0.999);
}

#[test]
fn calculate_reads_the_picture() {
    let inst = instance();
    // A dull picture: grey from 0.25 to 0.75.
    let mut img = ImageRgbaF32::transparent(256, 2);
    for y in 0..2 {
        for x in 0..256 {
            let v = 0.25 + 0.5 * x as f32 / 255.0;
            *img.pixel_mut(x, y) = [v, v, v, 1.0];
        }
    }
    let img = Arc::new(img);
    inst.connect("Source", Arc::new(Still(Arc::clone(&img))));
    // Color Levels (the default): each channel stretched to black and white.
    let changed = inst.changed(&["calculate".into()], ChangeReason::User, 0.0);
    assert_eq!(changed, ["curves"]);
    let curves = curves_of(&inst);
    for c in &curves {
        let at = |x| bv_core::curve::curve_at(c, x);
        assert!(at(0.25) < 0.02 && at(0.75) > 0.98, "{c:?}");
    }
    // The picture through them reaches black and white.
    let out = render(&inst, &img);
    assert!(out.pixel(0, 0)[0] < 0.02 && out.pixel(255, 0)[0] > 0.98);
}
