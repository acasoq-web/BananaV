//! Add Noise through BananaV's OpenFX host, as any host runs it: described
//! as VEGAS's (its settings and presets) and pictures through it.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ClipSource, Context, Instance, Plugin, RenderRequest, Setup, Value,
};

const NOISE: &str = "org.bananav.AddNoise";

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
        .find(|p| p.info.identifier == NOISE)
        .expect(NOISE)
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

fn instance(values: &[(&str, f64)]) -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (160.0, 90.0),
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

fn render(inst: &Instance, src: &Arc<ImageRgbaF32>, time: f64) -> ImageRgbaF32 {
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(src)));
    let req = RenderRequest {
        time,
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

fn gray(v: f32) -> Arc<ImageRgbaF32> {
    Arc::new(ImageRgbaF32::filled(160, 90, [v, v, v, 1.0]))
}

/// The mean and standard deviation of channel `c`.
fn spread(img: &ImageRgbaF32, c: usize) -> (f64, f64) {
    let n = img.data.len() as f64;
    let mean = img.data.iter().map(|p| p[c] as f64).sum::<f64>() / n;
    let var = img
        .data
        .iter()
        .map(|p| (p[c] as f64 - mean).powi(2))
        .sum::<f64>()
        / n;
    (mean, var.sqrt())
}

#[test]
fn add_noise_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Add Noise");
    assert_eq!(info.grouping, "BananaV/Creative");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Noise level", "Monochromatic", "Gaussian noise", "Animate"]
    );
    let defaults: Vec<_> = filter.params.iter().map(|p| p.default.num(0)).collect();
    assert_eq!(defaults, [0.5, 1.0, 0.0, 1.0]);
    // Each animates, the checkboxes too (VEGAS shows a clock by each).
    assert!(filter.params.iter().all(|p| p.animates));
    let names: Vec<_> = bv_effects::presets(NOISE).iter().map(|p| p.name).collect();
    assert_eq!(names, ["Medium", "Grainy", "Extreme", "Color", "Screen"]);
}

#[test]
fn the_default_noise_is_gray_and_as_wide_as_vegass() {
    let out = render(&instance(&[]), &gray(0.5), 0.0);
    // Even over 2/3 × 0.5 = 1/3: a standard deviation of 1/3 / √12.
    let (mean, sd) = spread(&out, 0);
    assert!((mean - 0.5).abs() < 0.005, "{mean}");
    assert!((sd - 1.0 / 3.0 / 12f64.sqrt()).abs() < 0.003, "{sd}");
    assert!(
        out.data
            .iter()
            .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 1.0)
    );
    assert!(
        out.data
            .iter()
            .all(|p| (p[0] - 0.5).abs() <= 1.0 / 6.0 + 1e-6)
    );
}

#[test]
fn colored_noise_differs_in_each_channel() {
    let out = render(&instance(&[("monochromatic", 0.0)]), &gray(0.5), 0.0);
    let differ = out
        .data
        .iter()
        .filter(|p| p[0] != p[1] && p[1] != p[2])
        .count();
    assert!(differ > out.data.len() * 9 / 10, "{differ}");
}

#[test]
fn gaussian_noise_spreads_as_a_bell_curve() {
    let out = render(
        &instance(&[("level", 0.1), ("gaussian", 1.0)]),
        &gray(0.5),
        0.0,
    );
    // A standard deviation of 2/3 × 0.1 × (1 - 0.1 / 4).
    let width = 2.0 / 3.0 * 0.1 * 0.975;
    let (_, sd) = spread(&out, 0);
    assert!((sd - width).abs() < 0.003, "{sd}");
    // Some far out: past two standard deviations.
    let far = out
        .data
        .iter()
        .filter(|p| (p[0] - 0.5).abs() as f64 > 2.0 * width)
        .count();
    assert!(far > out.data.len() / 40, "{far}");
}

#[test]
fn past_black_it_folds_back_instead_of_stopping() {
    // On black, VEGAS's strongest noise is specks of every shade, not of
    // black: what goes below 0 comes back up.
    let out = render(
        &instance(&[("level", 1.0), ("gaussian", 1.0)]),
        &gray(0.0),
        0.0,
    );
    let black = out.data.iter().filter(|p| p[0] == 0.0).count();
    assert!(black < out.data.len() / 100, "{black}");
    let (mean, _) = spread(&out, 0);
    assert!(mean > 0.3, "{mean}");
}

#[test]
fn animate_draws_a_new_noise_every_frame() {
    let src = gray(0.5);
    let moving = instance(&[]);
    let (a, b) = (render(&moving, &src, 3.0), render(&moving, &src, 4.0));
    assert_ne!(a.data, b.data);
    assert_eq!(a.data, render(&moving, &src, 3.0).data);
    let still = instance(&[("animate", 0.0)]);
    assert_eq!(
        render(&still, &src, 3.0).data,
        render(&still, &src, 4.0).data
    );
}
