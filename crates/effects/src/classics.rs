//! VEGAS's Chroma Keyer with its controls, on [`crate::filters`]' table: a
//! color, thresholds, blur and the mask. It takes the place of BananaV's
//! older built-in one, which stays for the projects that have it. (VEGAS's
//! other everyday effects are plug-ins of their own now, measured on its
//! pictures.)

use crate::filters::Kind;
use crate::kit::{Buf, Grid, Spec, Vals};
use crate::{Preset, PresetValue};

pub(crate) const CHROMA_KEYER: &[Spec] = &[
    Spec::Rgb(
        c"color",
        c"Color",
        [0.094_118, 0.278_431, 0.945_098],
        c"The color made transparent.",
    ),
    Spec::Number(
        c"lowThreshold",
        c"Low threshold",
        0.0,
        0.0,
        1.0,
        c"Colors this near the key or nearer are transparent.",
    ),
    Spec::Number(
        c"highThreshold",
        c"High threshold",
        1.0,
        0.0,
        1.0,
        c"Colors this far from the key or farther are opaque.",
    ),
    Spec::Number(
        c"blurAmount",
        c"Blur amount",
        0.0,
        0.0,
        1.0,
        c"How much the mask's edges are blurred.",
    ),
    Spec::Toggle(
        c"showMask",
        c"Show mask only",
        false,
        c"The mask alone: white where the picture stays.",
    ),
];
/// The frame's width in this picture's pixels.
fn width_px(g: &Grid) -> f64 {
    g.frame[0] * g.scale[0] / g.par
}

/// The picture through effect `kind` (one of this module's) at settings
/// `s`.
pub(crate) fn process(kind: Kind, s: &Vals, g: &Grid, src: &Buf) -> Buf {
    match kind {
        Kind::ChromaKeyer => chroma_keyer(s, g, src),
        _ => src.clone(),
    }
}

/// A color's hue, saturation and lightness (the HSL model, each 0–1).
fn hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let l = (hi + lo) * 0.5;
    let spread = hi - lo;
    if l <= 0.0 || spread <= 0.0 {
        return [0.0, 0.0, l];
    }
    let s = spread / if l > 0.5 { 2.0 - hi - lo } else { hi + lo };
    let (dr, dg, db) = ((hi - r) / spread, (hi - g) / spread, (hi - b) / spread);
    let h = if r == hi {
        if g == lo { 5.0 + db } else { 1.0 - dg }
    } else if g == hi {
        if b == lo { 1.0 + dr } else { 3.0 - db }
    } else if r == lo {
        3.0 + dg
    } else {
        5.0 - dr
    };
    [h / 6.0, s, l]
}

/// VEGAS's Chroma Keyer: each pixel's distance from the key — the RGB
/// distance times √3 and the HSL one (hue twice, the nearer way round,
/// saturation and lightness, over 3), halved — makes the mask through
/// the thresholds (no more than the pixel's alpha), blurred by a Gaussian
/// that falls to 1⁄255 Blur amount × a quarter of the frame's longer side
/// (and 1.5 pixels) away; where the key is mostly blue or green, what
/// stays loses the spill (blue held under green, green under red); the
/// colors are then multiplied by the mask, or the mask shows white.
fn chroma_keyer(s: &Vals, g: &Grid, src: &Buf) -> Buf {
    let Key {
        key,
        hsl: [kh, ks, kl],
        low,
        high,
        scale,
        lead,
    } = Key::of(s);
    let mask: Vec<f32> = src
        .data
        .iter()
        .map(|&c| {
            let rgb = [c[0], c[1], c[2]].map(f64::from);
            let [h, sa, l] = hsl(rgb);
            let dh = (h - kh).abs();
            let dh = if dh > 0.5 { 1.0 - dh } else { dh };
            let rgb_far = (0..3)
                .map(|k| (key[k] - rgb[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            let d = ((rgb_far * 3f64.sqrt() + (2.0 * dh + (sa - ks).abs() + (l - kl).abs()) / 3.0)
                * 0.5) as f32;
            let m = if d < low {
                0.0
            } else if d > high - 1e-6 {
                1.0
            } else {
                ((d - low) * scale).clamp(0.0, 1.0)
            };
            m.min(c[3])
        })
        .collect();
    let sigma = mask_sigma(s.f(c"blurAmount"), width_px(g).max(g.height_px()));
    let mask = if sigma > 0.6 {
        let mut plane = mask;
        crate::kit::gauss_plane(
            &mut plane,
            src.width().max(0) as usize,
            sigma as f32,
            sigma as f32,
        );
        plane
    } else {
        mask
    };
    let show = s.b(c"showMask");
    let data = src
        .data
        .iter()
        .zip(&mask)
        .map(|(&c, &m)| {
            let mut c = c;
            if m > 0.0 {
                if lead == 2 && c[1] < c[2] {
                    c[2] = c[1];
                } else if lead == 1 && c[0] < c[1] {
                    c[1] = c[0];
                }
            }
            if show {
                [m, m, m, m]
            } else {
                [c[0] * m, c[1] * m, c[2] * m, m]
            }
        })
        .collect();
    Buf {
        bounds: src.bounds,
        data,
    }
}

/// Chroma Keyer's key: its color, the color's hue, saturation and
/// lightness, the thresholds and the scale between them, and the channel
/// that leads it (strictly: 2 blue, 1 green, else 0).
struct Key {
    key: [f64; 3],
    hsl: [f64; 3],
    low: f32,
    high: f32,
    scale: f32,
    lead: u32,
}

impl Key {
    fn of(s: &Vals) -> Key {
        let key = s.get_rgb(c"color");
        let low = s.f(c"lowThreshold") as f32;
        let high = s.f(c"highThreshold") as f32;
        Key {
            key,
            hsl: hsl(key),
            low,
            high,
            scale: if (high - low).abs() >= 1e-6 {
                1.0 / (high - low)
            } else {
                0.0
            },
            lead: if key[2] > key[1] && key[2] > key[0] {
                2
            } else if key[1] > key[2] && key[1] > key[0] {
                1
            } else {
                0
            },
        }
    }
}

/// The mask's blur: a Gaussian that is 1⁄255 at Blur amount × a quarter
/// of `longer` pixels (and 1.5) away.
pub(crate) fn mask_sigma(amount: f64, longer: f64) -> f64 {
    let radius = amount.clamp(0.0, 1.0) * longer * 0.25 + 1.5;
    radius / (2.0 * 255f64.ln()).sqrt()
}

/// Chroma Keyer on the card: the mask (`kit_key_mask`), blurred as the
/// processor's, then the picture through it (`kit_key_apply`).
pub(crate) fn key_plan(s: &Vals) -> crate::accel::Plan {
    let k = Key::of(s);
    let f: Vec<f32> = [k.key[0], k.key[1], k.key[2], k.hsl[0], k.hsl[1], k.hsl[2]]
        .map(|v| v as f32)
        .into_iter()
        .chain([k.low, k.high, k.scale])
        .collect();
    crate::accel::Plan::ChromaKey {
        f,
        j: [k.lead, s.b(c"showMask") as u32, 0, 0],
        blur: s.f(c"blurAmount"),
    }
}

/// The factory presets of effect `kind` (one of this module's).
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let p = |name: &'static str, values: Vec<(&'static str, PresetValue)>| Preset { name, values };
    match kind {
        Kind::ChromaKeyer => {
            let screen = |name: &'static str, color: [f64; 3]| {
                p(
                    name,
                    vec![
                        ("color", num(&color)),
                        ("lowThreshold", num(&[0.0])),
                        ("highThreshold", num(&[1.0])),
                        ("blurAmount", num(&[0.0])),
                        ("showMask", num(&[0.0])),
                    ],
                )
            };
            vec![
                screen("Blue Screen", [0.094_118, 0.278_431, 0.945_098]),
                screen("Green Screen", [0.160_784, 0.956_863, 0.094_118]),
                screen("Pure Red Screen", [1.0, 0.0, 0.0]),
                screen("Pure Green Screen", [0.0, 1.0, 0.0]),
                screen("Pure Blue Screen", [0.0, 0.0, 1.0]),
            ]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            origin: [0.0, 0.0],
            frame: [32.0, 18.0],
            scale: [1.0, 1.0],
            par: 1.0,
        }
    }

    fn vals(kind: Kind, values: &[(&str, &[f64])]) -> Vals {
        Vals::of(kind.specs(), &|spec| {
            let key = spec.name().to_str().unwrap_or("");
            values
                .iter()
                .find(|(n, _)| *n == key)
                .map_or_else(|| spec.default(), |(_, v)| v.to_vec())
        })
    }

    fn flat(c: [f32; 4]) -> Buf {
        Buf::solid([0, 0, 32, 18], c)
    }

    #[test]
    fn the_keyer_takes_its_color_away() {
        let g = grid();
        // VEGAS's default key, its blue screen, goes; a far color stays.
        let key = flat([0.094_118, 0.278_431, 0.945_098, 1.0]);
        let out = process(Kind::ChromaKeyer, &vals(Kind::ChromaKeyer, &[]), &g, &key);
        assert!(out.at(2, 2)[3] < 1e-3);
        let other = flat([0.9, 0.8, 0.1, 1.0]);
        let out = process(Kind::ChromaKeyer, &vals(Kind::ChromaKeyer, &[]), &g, &other);
        assert_eq!(out.at(2, 2), [0.9, 0.8, 0.1, 1.0]);
        // Between the thresholds the mask is the distance; pure blue is
        // 0.29 from the key; its blue spill held under green.
        let blue = flat([0.0, 0.0, 1.0, 1.0]);
        let out = process(Kind::ChromaKeyer, &vals(Kind::ChromaKeyer, &[]), &g, &blue).at(2, 2);
        assert!(out[3] > 0.2 && out[3] < 0.4, "{out:?}");
        assert!((out[2] - out[1]).abs() < 1e-6, "{out:?}");
        // The mask alone: white where the picture stays.
        let shown = process(
            Kind::ChromaKeyer,
            &vals(Kind::ChromaKeyer, &[("showMask", &[1.0])]),
            &g,
            &other,
        );
        assert_eq!(shown.at(2, 2), [1.0; 4]);
        assert_eq!(presets(Kind::ChromaKeyer).len(), 5, "VEGAS's five");
        assert_eq!(hsl([0.0, 1.0, 0.0]), [1.0 / 3.0, 1.0, 0.5]);
    }
}
