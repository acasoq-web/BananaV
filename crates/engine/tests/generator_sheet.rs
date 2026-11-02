//! BananaV's own generators at each of their presets: a contact sheet for
//! looking at them (`target/telas`). Credit Roll's are shown a third of
//! the way through an event ten seconds long.
//!
//! `cargo test -p bv-engine --test generator_sheet -- --ignored`

use std::collections::BTreeMap;
use std::path::PathBuf;

use bv_core::Tick;
use bv_engine::plugins::{PresetValue, own_presets, register_own};
use bv_timeline::{Generator, MediaInfo, PluginFx, PluginParam, PluginValue, ProjectSettings};

const IDS: [&str; 5] = [
    "org.bananav.Checkerboard",
    "org.bananav.ColorGradient",
    "org.bananav.NoiseTexture",
    "org.bananav.TestPattern",
    "org.bananav.CreditRoll",
];

/// A preset's picture, `w` × `h`, as 8-bit RGB over a checkerboard where
/// it is see-through.
fn tile(id: &str, preset: &bv_engine::plugins::Preset, w: u32, h: u32) -> Vec<u8> {
    let params: BTreeMap<String, PluginParam> = preset
        .values
        .iter()
        .map(|(name, v)| {
            let value = match v {
                PresetValue::Num(n) => PluginValue::Num(n.clone()),
                PresetValue::Text(t) => PluginValue::Text(t.clone()),
                PresetValue::Curves(_) => PluginValue::Num(Vec::new()),
            };
            (
                name.to_string(),
                PluginParam {
                    value,
                    keys: Vec::new(),
                },
            )
        })
        .collect();
    let fx = PluginFx {
        id: id.to_string(),
        version: [1, 0],
        name: preset.name.to_string(),
        params,
    };
    let settings = ProjectSettings {
        width: w,
        height: h,
        ..ProjectSettings::default()
    };
    let info = MediaInfo::generated(Generator::Plugin(Box::new(fx)), &settings);
    let mut reader = bv_media::open_video(&info).expect("the generator opens");
    reader.set_span(Some(Tick::from_seconds(10)));
    let img = reader
        .frame_at(Tick::from_seconds(3))
        .expect("the generator draws");
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = img.pixel(x, y);
            let check = if (x / 8 + y / 8) % 2 == 0 { 0.35 } else { 0.55 };
            for c in &p[..3] {
                // Over the checkerboard, in video's values.
                let v = bv_color::transfer::linear_to_bt709(*c) + check * (1.0 - p[3]);
                rgb.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
    }
    rgb
}

#[test]
#[ignore = "renders a contact sheet for visual review"]
fn generators_contact_sheet() {
    let sheet = std::thread::Builder::new()
        .stack_size(bv_ofx::PLUGIN_STACK)
        .spawn(|| {
            register_own();
            let (w, h) = (192u32, 108u32);
            let per_row = 4usize;
            let mut tiles = Vec::new();
            for id in IDS {
                for preset in own_presets(id) {
                    tiles.push(tile(id, &preset, w, h));
                }
            }
            let rows = tiles.len().div_ceil(per_row);
            let (width, height) = (w as usize * per_row, h as usize * rows);
            let mut rgb = vec![0u8; width * height * 3];
            for (t, tile) in tiles.iter().enumerate() {
                let (tx, ty) = (t % per_row, t / per_row);
                for y in 0..h as usize {
                    let d = ((ty * h as usize + y) * width + tx * w as usize) * 3;
                    let s = y * w as usize * 3;
                    rgb[d..d + w as usize * 3].copy_from_slice(&tile[s..s + w as usize * 3]);
                }
            }
            (width, height, rgb, tiles.len())
        })
        .expect("a thread")
        .join()
        .expect("the sheet");
    let (width, height, rgb, count) = sheet;
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    ppm.extend_from_slice(&rgb);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("geradores_folha.ppm"), ppm).unwrap();
    assert!(count >= IDS.len(), "every generator has presets");
}
