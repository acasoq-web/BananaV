//! BananaV's transitions at a few steps of the way, on the sample
//! pictures: contact sheets for looking at them (`target/telas`), VEGAS's
//! and the GL Transitions' presets.
//!
//! `cargo test -p bv-engine --test transition_sheet -- --ignored`

use std::collections::BTreeMap;
use std::path::PathBuf;

use bv_engine::plugins::{PresetValue, own_presets, register_own};
use bv_timeline::{PluginFx, PluginParam, PluginValue};

const IDS: [&str; 25] = [
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
    "org.bananav.Blinds3D",
    "org.bananav.Cascade3D",
    "org.bananav.FlyInOut3D",
    "org.bananav.Shuffle3D",
    "org.bananav.PageLoop",
    "org.bananav.PagePeel",
    "org.bananav.PageRoll",
    "org.bananav.GradientWipe",
];

#[test]
#[ignore = "renders a contact sheet for visual review"]
fn transitions_contact_sheet() {
    let rows = sheet(&IDS, "transicoes_folha.ppm");
    assert!(rows >= IDS.len(), "every transition has presets");
}

#[test]
#[ignore = "renders a contact sheet for visual review"]
fn gl_transitions_contact_sheet() {
    let rows = sheet(&["org.bananav.GLTransition"], "gl_transicoes_folha.ppm");
    assert_eq!(rows, 66, "a preset for each GL transition, and Custom's");
}

/// Draws every preset of `ids` at a few steps into `file`; returns how
/// many rows (presets) it drew.
fn sheet(ids: &[&str], file: &str) -> usize {
    register_own();
    let (w, h) = (128u32, 72u32);
    let steps = [0.2f32, 0.4, 0.6, 0.8];
    let mut rows: Vec<Vec<u8>> = Vec::new();
    for &id in ids {
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
            // A row: each step side by side.
            let mut row = vec![0u8; (w as usize * steps.len()) * h as usize * 3];
            for (k, &p) in steps.iter().enumerate() {
                let rgba = bv_engine::transition_sample(&fx, p, w, h)
                    .unwrap_or_else(|| panic!("{id} {} draws", preset.name));
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let s = (y * w as usize + x) * 4;
                        let d = (y * w as usize * steps.len() + k * w as usize + x) * 3;
                        row[d..d + 3].copy_from_slice(&rgba[s..s + 3]);
                    }
                }
            }
            rows.push(row);
        }
    }
    let width = w as usize * steps.len();
    let mut ppm = format!("P6\n{} {}\n255\n", width, h as usize * rows.len()).into_bytes();
    for r in &rows {
        ppm.extend_from_slice(r);
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/telas");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), ppm).unwrap();
    rows.len()
}
