//! BananaV's effects as other OFX hosts get them: the `bananav-ofx`
//! binary built, put in a bundle, found and loaded from it.

use std::sync::Arc;

use bv_core::ImageRgbaF32;
use bv_ofx::{Animation, Binary, ClipSource, Context, Instance, RenderRequest, Setup, Value};

mod common;

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

#[test]
fn the_bundle_loads_in_a_host_and_crops() {
    // Built as a vendor ships it, in a folder of its own.
    let dir = std::env::temp_dir().join(format!("bananav-ofx-bundle-{}", std::process::id()));
    common::bundle_in(&dir);
    let bundles = bv_ofx::plugin::find_bundles(&dir);
    assert_eq!(bundles.len(), 1);
    let binaries = bv_ofx::plugin::bundle_binaries(&bundles[0]);
    let bin = Binary::load(&binaries[0]).expect("it loads");
    let (plugins, failures) = bin.plugins().unwrap();
    assert!(failures.is_empty(), "{failures:?}");
    let ids: Vec<_> = plugins.iter().map(|p| p.info.identifier.as_str()).collect();
    assert_eq!(
        ids,
        [
            "org.bananav.Crop",
            "org.bananav.ColorCurves",
            "org.bananav.ColorGrading",
            "org.bananav.ColorCorrector",
            "org.bananav.CookieCutter",
            "org.bananav.LayerDimensionality",
            "org.bananav.Pixelate",
            "org.bananav.LutFilter",
            "org.bananav.WhiteBalance",
            "org.bananav.HslAdjust",
            "org.bananav.PictureInPicture",
            "org.bananav.SaturationAdjust",
            "org.bananav.Border",
            "org.bananav.BlackBarFill",
            "org.bananav.UnsharpMask",
            "org.bananav.AddNoise",
            "org.bananav.Timecode",
            "org.bananav.ColorBalance",
            "org.bananav.ColorCorrectorSecondary",
            "org.bananav.TitlesText",
            "org.bananav.LinearWipe",
            "org.bananav.ClockWipe",
            "org.bananav.Iris",
            "org.bananav.BarnDoor",
            "org.bananav.VenetianBlinds",
            "org.bananav.StarWipe",
            "org.bananav.Push",
            "org.bananav.Slide",
            "org.bananav.Squeeze",
            "org.bananav.Split",
            "org.bananav.Swap",
            "org.bananav.Zoom",
            "org.bananav.Spiral",
            "org.bananav.Flash",
            "org.bananav.Dissolve",
            "org.bananav.CrossEffect",
            "org.bananav.Portals",
            "org.bananav.TinyPlanet",
            "org.bananav.Defocus",
            "org.bananav.LinearBlur",
            "org.bananav.QuickBlur",
            "org.bananav.RadialBlur",
            "org.bananav.SharpenFx",
            "org.bananav.Median",
            "org.bananav.MinMax",
            "org.bananav.ConvolutionKernel",
            "org.bananav.Spherize",
            "org.bananav.Swirl",
            "org.bananav.Threshold",
            "org.bananav.Wave",
            "org.bananav.PinchPunch",
            "org.bananav.OffsetWrap",
            "org.bananav.BlackRestore",
            "org.bananav.BroadcastColors",
            "org.bananav.ChannelBlend",
            "org.bananav.ChromaBlur",
            "org.bananav.MaskGenerator",
            "org.bananav.LensCorrection",
            "org.bananav.RadialPixelate",
            "org.bananav.Deform",
            "org.bananav.LabAdjust",
            "org.bananav.SoftContrast",
            "org.bananav.GradientMap",
            "org.bananav.FillLight",
            "org.bananav.LensFlare",
            "org.bananav.Starburst",
            "org.bananav.Glint",
            "org.bananav.Rays",
            "org.bananav.LightRays",
            "org.bananav.FilmEffects",
            "org.bananav.FilmGrain",
            "org.bananav.Newsprint",
            "org.bananav.TvSimulator",
            "org.bananav.BumpMap",
            "org.bananav.Denoise",
            "org.bananav.Looks",
            "org.bananav.Checkerboard",
            "org.bananav.ColorGradient",
            "org.bananav.NoiseTexture",
            "org.bananav.SolidColor",
            "org.bananav.TestPattern",
            "org.bananav.CreditRoll",
            "org.bananav.Blinds3D",
            "org.bananav.Cascade3D",
            "org.bananav.FlyInOut3D",
            "org.bananav.Shuffle3D",
            "org.bananav.PageLoop",
            "org.bananav.PagePeel",
            "org.bananav.PageRoll",
            "org.bananav.BrightnessContrast",
            "org.bananav.Levels",
            "org.bananav.BlackAndWhite",
            "org.bananav.Sepia",
            "org.bananav.Invert",
            "org.bananav.GaussianBlur",
            "org.bananav.Vignette",
            "org.bananav.Mirror",
            "org.bananav.Glow",
            "org.bananav.ChromaKeyer",
            "org.bananav.GLTransition",
            "org.bananav.DisplacementMap",
            "org.bananav.HeightMap",
            "org.bananav.FlickerControl",
            "org.bananav.MeshWarp",
            "org.bananav.BezierMasking",
            "org.bananav.ColorMatch",
            "org.bananav.VideoStabilization",
            "org.bananav.Stabilize",
            "org.bananav.VideoStabilization360",
            "org.bananav.SceneRotation",
            "org.bananav.DualFishEyeStitching",
            "org.bananav.Stereoscopic3DAdjust",
            "org.bananav.SmartZoom",
            "org.bananav.AiSmoothen",
            "org.bananav.AiSharpen",
            "org.bananav.AiDehaze",
            "org.bananav.SlowMotion",
            "org.bananav.MotionBlur",
            "org.bananav.WarpFlow",
            "org.bananav.GradientWipe"
        ]
    );
    // Half the width kept, from the right: VEGAS's Crop Left Half (its
    // middle at 0.75).
    let setup = Setup {
        frame_rate: 25.0,
        duration: 10.0,
        size: (8.0, 2.0),
        pixel_aspect: 1.0,
        interactive: false,
    };
    let inst = Instance::new(&plugins[0], Context::Filter, &setup).unwrap();
    inst.set_values(&[
        ("xCrop".into(), Animation::constant(Value::Num(vec![0.5]))),
        (
            "location".into(),
            Animation::constant(Value::Num(vec![0.75, 0.5])),
        ),
    ]);
    let white: Arc<dyn ClipSource> =
        Arc::new(Still(Arc::new(ImageRgbaF32::filled(8, 2, [1.0; 4]))));
    let req = RenderRequest {
        time: 0.0,
        width: 8,
        height: 2,
        scale: 1.0,
        draft: false,
        interactive: false,
        sequential: false,
        timeline: None,
    };
    let out = inst.render_with(&[("Source", white)], &[], &req).unwrap();
    assert_eq!(out.pixel(3, 0), [0.0; 4]);
    assert_eq!(out.pixel(4, 0), [1.0; 4]);
    drop(inst);
    // A transition in the host: Linear Wipe halfway, left to right, from
    // red to blue.
    let wipe = plugins
        .iter()
        .find(|p| p.info.identifier == "org.bananav.LinearWipe")
        .expect("the wipe");
    let inst = Instance::new(wipe, Context::Transition, &setup).unwrap();
    let solid = |c: [f32; 4]| -> Arc<dyn ClipSource> {
        Arc::new(Still(Arc::new(ImageRgbaF32::filled(8, 2, c))))
    };
    let half = Animation::constant(Value::Num(vec![0.5]));
    let out = inst
        .render_with(
            &[
                ("SourceFrom", solid([1.0, 0.0, 0.0, 1.0])),
                ("SourceTo", solid([0.0, 0.0, 1.0, 1.0])),
            ],
            &[("Transition".to_string(), half)],
            &req,
        )
        .unwrap();
    assert_eq!(
        out.pixel(1, 0),
        [0.0, 0.0, 1.0, 1.0],
        "the left: the second"
    );
    assert_eq!(
        out.pixel(6, 1),
        [1.0, 0.0, 0.0, 1.0],
        "the right: the first"
    );
    drop(inst);
    // Gradient Wipe, its gradient picture not given: the first picture at
    // the start, the second at the end.
    let gradient = plugins
        .iter()
        .find(|p| p.info.identifier == "org.bananav.GradientWipe")
        .expect("the gradient wipe");
    let inst = Instance::new(gradient, Context::Transition, &setup).unwrap();
    for (at, c) in [(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 1.0, 1.0])] {
        let out = inst
            .render_with(
                &[
                    ("SourceFrom", solid([1.0, 0.0, 0.0, 1.0])),
                    ("SourceTo", solid([0.0, 0.0, 1.0, 1.0])),
                ],
                &[(
                    "Transition".to_string(),
                    Animation::constant(Value::Num(vec![at])),
                )],
                &req,
            )
            .unwrap();
        assert_eq!(out.pixel(1, 0), c);
        assert_eq!(out.pixel(6, 1), c);
    }
    drop(inst);
    let _ = std::fs::remove_dir_all(&dir);
}
