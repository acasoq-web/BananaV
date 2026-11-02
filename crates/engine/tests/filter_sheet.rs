//! BananaV's table-driven video effects at each of their presets, on the
//! sample picture: a contact sheet for looking at them (`target/telas`).
//!
//! `cargo test -p bv-engine --test filter_sheet -- --ignored`

use std::collections::BTreeMap;
use std::path::PathBuf;

use bv_engine::plugins::{PresetValue, own_presets};
use bv_timeline::{Effect, PluginFx, PluginParam, PluginValue};

const IDS: [&str; 48] = [
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
    "org.bananav.LightRays",
    "org.bananav.FilmEffects",
    "org.bananav.FilmGrain",
    "org.bananav.Newsprint",
    "org.bananav.TvSimulator",
    "org.bananav.BumpMap",
    "org.bananav.Denoise",
    "org.bananav.Looks",
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
];

#[test]
#[ignore = "renders a contact sheet for visual review"]
fn filters_contact_sheet() {
    let (w, h) = (160u32, 90u32);
    let per_row = 4usize;
    let mut tiles: Vec<Vec<u8>> = Vec::new();
    for id in IDS {
        for preset in own_presets(id) {
            let params: BTreeMap<String, PluginParam> = preset
                .values
                .iter()
                .filter_map(|(name, v)| match v {
                    PresetValue::Num(n) => Some((
                        name.to_string(),
                        PluginParam {
                            value: PluginValue::Num(n.clone()),
                            keys: Vec::new(),
                        },
                    )),
                    _ => None,
                })
                .collect();
            let fx = PluginFx {
                id: id.to_string(),
                version: [1, 0],
                name: preset.name.to_string(),
                params,
            };
            let rgba = bv_engine::effect_sample(&[Effect::plugin(fx)], w, h);
            assert_eq!(rgba.len(), (w * h * 4) as usize, "{id} {}", preset.name);
            tiles.push(rgba);
        }
    }
    let rows = tiles.len().div_ceil(per_row);
    let width = w as usize * per_row;
    let height = h as usize * rows;
    let mut rgb = vec![0u8; width * height * 3];
    for (t, tile) in tiles.iter().enumerate() {
        let (tx, ty) = (t % per_row, t / per_row);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let s = (y * w as usize + x) * 4;
                let a = u32::from(tile[s + 3]);
                // Over a checkerboard where it is see-through.
                let check = if (x / 8 + y / 8) % 2 == 0 { 90 } else { 140 };
                let d = ((ty * h as usize + y) * width + tx * w as usize + x) * 3;
                for c in 0..3 {
                    rgb[d + c] = ((u32::from(tile[s + c]) * a + check * (255 - a)) / 255) as u8;
                }
            }
        }
    }
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    ppm.extend_from_slice(&rgb);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("efeitos_folha.ppm"), ppm).unwrap();
    assert!(tiles.len() >= IDS.len(), "every effect has presets");
}
