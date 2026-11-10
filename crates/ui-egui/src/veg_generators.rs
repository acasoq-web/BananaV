//! The media generators a `.veg` keeps in its own kind of record (Color
//! Gradient's and Credit Roll's), read as BananaV's own generators of the
//! same names and written back as those records ([`crate::veg_ui`]).

use std::collections::BTreeMap;

use bv_core::{FrameRate, TICKS_PER_SECOND, Tick};
use bv_timeline::veg::{DocEffect, DocKey, NativeKey, ParamValue};
use bv_timeline::{KeyCurve, PluginFx, PluginKey, PluginParam, PluginValue};

/// Color Gradient's class.
pub(crate) const COLOR_GRADIENT_CLASS: &str = "8B5DEABC-53BD-49F6-9803-367478307A59";
/// Credit Roll's class.
pub(crate) const CREDIT_ROLL_CLASS: &str = "02FD3B60-986B-11D4-9F6F-00485481BD31";

/// What fills a dual line between its sides, as Credit Roll's connector
/// list has them.
const CONNECTORS: [&str; 10] = [
    "", ".", "_", "-", "\u{2022}", "\u{2219}", "\u{25AA}", "\u{25AB}", ". ", " . ",
];

/// Credit Roll's styles' parameters' first words: the header's, the single
/// item's, the dual line's.
const CREDIT_STYLES: [&str; 3] = ["title", "item", "columns"];

/// The file's 100 ns steps in a second.
const UNITS: i64 = 10_000_000;

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(d: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn f64_at(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// A keyframe's curve to the next, from the file's number.
fn key_curve(n: u32) -> KeyCurve {
    match n {
        1 => KeyCurve::Hold,
        2 => KeyCurve::Slow,
        3 => KeyCurve::Fast,
        4 => KeyCurve::Smooth,
        5 => KeyCurve::Sharp,
        _ => KeyCurve::Linear,
    }
}

/// A Color Gradient record: its type, angle (radians), center (from the
/// top left) and color points (each one's distance along the angle's
/// axis, as that program's page shows it, and its color, straight RGBA).
#[derive(Clone, Debug, PartialEq)]
struct Gradient {
    kind: u32,
    angle: f64,
    center: [f64; 2],
    points: Vec<(f64, [f32; 4])>,
}

impl Gradient {
    /// The 336 bytes of a record: the points used counting the center, the
    /// type, the angle, ten colors (blue, green, red, alpha), ten x and ten
    /// y (parts of the frame from its top left).
    fn read(d: &[u8]) -> Option<Gradient> {
        let n = (u32_at(d, 0)? as usize).clamp(1, 10);
        let kind = u32_at(d, 4)?.min(2);
        let angle = f64_at(d, 8)?;
        let color = |i: usize| -> Option<[f32; 4]> {
            let at = 0x10 + 16 * i;
            Some([
                f32_at(d, at + 8)?,
                f32_at(d, at + 4)?,
                f32_at(d, at)?,
                f32_at(d, at + 12)?,
            ])
        };
        let x = |i: usize| f64_at(d, 0xB0 + 8 * i);
        let y = |i: usize| f64_at(d, 0x100 + 8 * i);
        let center = [x(0)?, y(0)?];
        let (cos, sin) = (angle.cos(), angle.sin());
        // Each point's distance from the center, on the side of the axis
        // it lies (a linear gradient's; the others' have none).
        let points = (1..n)
            .map(|i| {
                let (dx, dy) = (x(i)? - center[0], y(i)? - center[1]);
                let mut r = (dx * dx + dy * dy).sqrt();
                let side = if cos.abs() > sin.abs() {
                    cos * dx
                } else {
                    sin * dy
                };
                if kind == 0 && side < 0.0 {
                    r = -r;
                }
                Some((r, color(i)?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Gradient {
            kind,
            angle,
            center,
            points,
        })
    }

    /// Its settings as BananaV's Color Gradient's, `n` points (the last
    /// repeated where it has fewer).
    fn values(&self, n: usize) -> Vec<(String, Vec<f64>)> {
        let mut v = vec![
            ("gradientType".to_string(), vec![f64::from(self.kind)]),
            (
                "aspectRatioAngle".to_string(),
                vec![self.angle.to_degrees()],
            ),
            (
                "center".to_string(),
                vec![self.center[0], 1.0 - self.center[1]],
            ),
            ("controlPoints".to_string(), vec![n.max(1) as f64]),
        ];
        let last = self.points.last().copied().unwrap_or((0.0, [0.0; 4]));
        for i in 0..n.min(9) {
            let (r, c) = self.points.get(i).copied().unwrap_or(last);
            v.push((format!("distance{}", i + 1), vec![r]));
            v.push((format!("color{}", i + 1), c.map(f64::from).to_vec()));
        }
        v
    }
}

/// BananaV's Color Gradient's settings from a `.veg`'s record and its
/// keyframes (when they change it over time): every point count the
/// largest any keyframe has, as that program's.
pub(crate) fn gradient_in(
    data: &[u8],
    keys: &[NativeKey],
    rate: FrameRate,
) -> Option<BTreeMap<String, PluginParam>> {
    let main = Gradient::read(data)?;
    let keyed: Vec<(i64, u32, Gradient)> = keys
        .iter()
        .filter_map(|k| Some((k.time, k.curve, Gradient::read(&k.data)?)))
        .collect();
    let changes = keyed.len() > 1 && keyed.iter().any(|(_, _, g)| *g != keyed[0].2);
    let n = if changes {
        keyed
            .iter()
            .map(|(_, _, g)| g.points.len())
            .max()
            .unwrap_or(1)
    } else {
        main.points.len()
    }
    .clamp(1, 9);
    let mut params: BTreeMap<String, PluginParam> = main
        .values(n)
        .into_iter()
        .map(|(name, v)| {
            (
                name,
                PluginParam {
                    value: PluginValue::Num(v),
                    keys: Vec::new(),
                },
            )
        })
        .collect();
    if changes {
        let frame = |units: i64| {
            let t = Tick((units as i128 * TICKS_PER_SECOND as i128 / UNITS as i128) as i64);
            // On the media's frames.
            rate.frame_start(rate.frame_at(t))
        };
        for (time, curve, g) in &keyed {
            for (name, v) in g.values(n) {
                if name == "controlPoints" {
                    continue;
                }
                if let Some(p) = params.get_mut(&name) {
                    let curve = if name == "gradientType" {
                        KeyCurve::Hold
                    } else {
                        key_curve(*curve)
                    };
                    p.keys
                        .push(PluginKey::new(frame(*time), PluginValue::Num(v), curve));
                }
            }
        }
    }
    Some(params)
}

/// A string of 16-bit units, up to its end.
fn wide(d: &[u8], at: usize, units: usize) -> Option<String> {
    let raw = d.get(at..at + 2 * units)?;
    let u: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .take_while(|&c| c != 0)
        .collect();
    Some(String::from_utf16_lossy(&u))
}

/// A snapping from the record's alignment (0 left, 1 right, 2 center) as
/// BananaV's (0 left, 1 center, 2 right).
fn snapping(alignment: u32) -> f64 {
    match alignment {
        1 => 2.0,
        2 => 1.0,
        _ => 0.0,
    }
}

/// BananaV's Credit Roll's settings from a `.veg`'s record: the 712 bytes
/// of its settings, then its credits (UTF-16, their length before them).
/// Its keyframes hold their settings until the next (VEGAS's way): the
/// record is the roll.
pub(crate) fn credit_in(data: &[u8]) -> Option<BTreeMap<String, PluginParam>> {
    let d = data;
    if u32_at(d, 0)? != 1 {
        return None;
    }
    let mut v: Vec<(String, PluginValue)> = Vec::new();
    let color = |at: usize| -> Option<Vec<f64>> {
        let [b, g, r, a] = [0, 4, 8, 12].map(|k| f32_at(d, at + k).map(f64::from));
        Some(vec![r?, g?, b?, a?])
    };
    {
        let mut num = |name: &str, x: Vec<f64>| v.push((name.to_string(), PluginValue::Num(x)));
        num("effect", vec![f64::from(u32_at(d, 0x08)?.min(1))]);
        num("left", vec![f64_at(d, 0x18)?]);
        num("right", vec![f64_at(d, 0x20)?]);
        num("top", vec![f64_at(d, 0x28)?]);
        num("in", vec![f64::from(u32_at(d, 0x30)?.min(15))]);
        num("out", vec![f64::from(u32_at(d, 0x34)?.min(15))]);
        num("display", vec![f64::from(u32_at(d, 0x38)?.min(3))]);
        let connector = wide(d, 0x3C, 12)?;
        let connector = CONNECTORS.iter().position(|c| *c == connector).unwrap_or(0);
        num("connector", vec![connector as f64]);
        num("background", color(0x54)?);
        num("direction", vec![f64::from(u32_at(d, 0x64)?.min(1))]);
        num("centerWidth", vec![f64_at(d, 0x68)?]);
        // The header's style, the single item's and the dual line's (its
        // left side's, its right side's snapping).
        for (style, at) in CREDIT_STYLES.iter().zip([0x70usize, 0xE8, 0x160]) {
            let flags = u32_at(d, at + 0x44)?;
            num(
                &format!("{style}Size"),
                vec![f64::from(u32_at(d, at + 0x40)? as i32)],
            );
            num(&format!("{style}Bold"), vec![f64::from(flags & 1)]);
            num(&format!("{style}Italic"), vec![f64::from((flags >> 1) & 1)]);
            num(&format!("{style}Color"), color(at + 0x48)?);
            if *style != "columns" {
                num(
                    &format!("{style}Snapping"),
                    vec![snapping(u32_at(d, at + 0x58)?)],
                );
            }
            num(
                &format!("{style}Kerning"),
                vec![f64::from(u32_at(d, at + 0x5C)? != 0)],
            );
            num(&format!("{style}SpaceAbove"), vec![f64_at(d, at + 0x60)?]);
            num(&format!("{style}SpaceBelow"), vec![f64_at(d, at + 0x68)?]);
            // Its tracking: 1 the font's own spacing.
            num(
                &format!("{style}Tracking"),
                vec![f64_at(d, at + 0x70)? - 1.0],
            );
        }
        num("columnsLeftSnapping", vec![snapping(u32_at(d, 0x1B8)?)]);
        num("columnsRightSnapping", vec![snapping(u32_at(d, 0x2A8)?)]);
    }
    for (style, at) in CREDIT_STYLES.iter().zip([0x70usize, 0xE8, 0x160]) {
        v.push((format!("{style}Font"), PluginValue::Text(wide(d, at, 32)?)));
    }
    // The credits: lines ending in CR LF.
    let bytes = u32_at(d, 0x2C8)? as usize;
    let text = wide(d, 0x2D0, bytes / 2).unwrap_or_default();
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
    v.push(("text".to_string(), PluginValue::Text(text)));
    Some(
        v.into_iter()
            .map(|(name, value)| {
                (
                    name,
                    PluginParam {
                        value,
                        keys: Vec::new(),
                    },
                )
            })
            .collect(),
    )
}

/// Titles & Text's sizes: points, from the RTF's half points.
const TITLES_POINTS: f64 = 0.5;

/// The name BananaV's Titles & Text keeps a `.veg`'s setting under (its
/// numbers as they are: the same units).
fn titles_param(name: &str) -> Option<&'static str> {
    Some(match name {
        "TextColor" => "textColor",
        "Scale" => "scale",
        "Location" => "location",
        "Alignment" => "anchor",
        "FitBackgroundColor" => "fitBackgroundColor",
        "Background" => "background",
        "Tracking" => "tracking",
        "LineSpacing" => "lineSpacing",
        "OutlineWidth" => "outlineWidth",
        "OutlineColor" => "outlineColor",
        "ShadowEnable" => "shadowEnable",
        "ShadowColor" => "shadowColor",
        "ShadowOffsetX" => "shadowOffsetX",
        "ShadowOffsetY" => "shadowOffsetY",
        "ShadowBlur" => "shadowBlur",
        _ => return None,
    })
}

/// BananaV's Titles & Text's settings from a `.veg`'s: its text (rich
/// text there) as the text, its font, size, bold and italic, and the runs
/// of the others; the rest by name, with their keyframes.
pub(crate) fn titles_in(e: &DocEffect, rate: FrameRate) -> BTreeMap<String, PluginParam> {
    let mut params = BTreeMap::new();
    let mut put = |name: &str, value: PluginValue| {
        params.insert(
            name.to_string(),
            PluginParam {
                value,
                keys: Vec::new(),
            },
        );
    };
    let num = |v: Vec<f64>| PluginValue::Num(v);
    for (name, value) in &e.params {
        if name == "Text" {
            let ParamValue::Text(rtf) = value else {
                continue;
            };
            let rich = crate::veg_rtf::read(rtf);
            let Some(first) = rich.spans.first() else {
                put("text", PluginValue::Text(String::new()));
                continue;
            };
            let text: String = rich.spans.iter().map(|s| s.text.as_str()).collect();
            let size = |hp: u32| f64::from(hp) * TITLES_POINTS;
            // The runs that differ from the first's style.
            let mut runs = String::new();
            let mut at = 0usize;
            for s in &rich.spans {
                let n = s.text.chars().count();
                let mut set = Vec::new();
                if s.font != first.font {
                    set.push(format!("font={}", s.font));
                }
                if s.half_points != first.half_points {
                    set.push(format!("size={}", size(s.half_points)));
                }
                if s.bold != first.bold {
                    set.push(format!("bold={}", u8::from(s.bold)));
                }
                if s.italic != first.italic {
                    set.push(format!("italic={}", u8::from(s.italic)));
                }
                if !set.is_empty() && n > 0 {
                    runs.push_str(&format!("{} {} {}\n", at, at + n, set.join(";")));
                }
                at += n;
            }
            put("text", PluginValue::Text(text));
            put("styles", PluginValue::Text(runs));
            put("font", PluginValue::Text(first.font.clone()));
            put("size", num(vec![size(first.half_points)]));
            put("bold", num(vec![f64::from(u8::from(first.bold))]));
            put("italic", num(vec![f64::from(u8::from(first.italic))]));
            put("justification", num(vec![f64::from(rich.justify)]));
            continue;
        }
        if name == "AnimationName" {
            if let ParamValue::Text(t) = value {
                put("animationName", PluginValue::Text(t.clone()));
            }
            continue;
        }
        let Some(ours) = titles_param(name) else {
            continue;
        };
        let mut v = match value {
            ParamValue::Number(x) => vec![*x],
            ParamValue::Integer(x) => vec![*x as f64],
            ParamValue::Numbers(v) => v.clone(),
            ParamValue::Text(_) => continue,
        };
        // A color without its alpha has it whole.
        if (ours.ends_with("Color") || ours == "background") && v.len() == 3 {
            v.push(1.0);
        }
        put(ours, num(v));
    }
    // Their keyframes.
    for (name, keys) in &e.keys {
        let Some(ours) = titles_param(name) else {
            continue;
        };
        let color = ours.ends_with("Color") || ours == "background";
        let Some(p) = params.get_mut(ours) else {
            continue;
        };
        p.keys = keys
            .iter()
            .map(|key| {
                let mut v = key.value.clone();
                if color && v.len() == 3 {
                    v.push(1.0);
                }
                let at = Tick((key.frame * TICKS_PER_SECOND as f64 / rate.as_f64()).round() as i64);
                PluginKey::new(
                    at,
                    PluginValue::Num(v),
                    bv_timeline::veg::interpolation_curve(key.interpolation),
                )
            })
            .collect();
    }
    params
}

// Writing them.

/// A key curve's number among a native plug-in's keyframes.
fn curve_number(c: KeyCurve) -> u32 {
    match c {
        KeyCurve::Hold => 1,
        KeyCurve::Slow => 2,
        KeyCurve::Fast => 3,
        KeyCurve::Smooth => 4,
        KeyCurve::Sharp => 5,
        _ => 0,
    }
}

/// A parameter's numbers at `t`: its value, or between its keyframes
/// (straight across, held where a keyframe holds).
fn nums_at(p: &PluginParam, t: Tick) -> Vec<f64> {
    let num = |v: &PluginValue| match v {
        PluginValue::Num(n) => n.clone(),
        _ => Vec::new(),
    };
    if p.keys.is_empty() {
        return num(&p.value);
    }
    let after = p.keys.iter().position(|k| k.at > t);
    match after {
        Some(0) => num(&p.keys[0].value),
        None => num(&p.keys[p.keys.len() - 1].value),
        Some(i) => {
            let (a, b) = (&p.keys[i - 1], &p.keys[i]);
            let (va, vb) = (num(&a.value), num(&b.value));
            if a.curve == KeyCurve::Hold || va.len() != vb.len() {
                return va;
            }
            let u = (t - a.at).0 as f64 / (b.at - a.at).0.max(1) as f64;
            va.iter().zip(&vb).map(|(x, y)| x + (y - x) * u).collect()
        }
    }
}

/// A generator's settings at `t` (its plug-in's defaults where it has
/// none): the numbers of parameter `name`.
fn setting(fx: &PluginFx, name: &str, t: Tick, default: &[f64]) -> Vec<f64> {
    match fx.params.get(name) {
        Some(p) => {
            let v = nums_at(p, t);
            if v.is_empty() { default.to_vec() } else { v }
        }
        None => default.to_vec(),
    }
}

/// A text setting (none: `default`).
fn text_setting(fx: &PluginFx, name: &str, default: &str) -> String {
    match fx.params.get(name).map(|p| &p.value) {
        Some(PluginValue::Text(t)) => t.clone(),
        _ => default.to_string(),
    }
}

/// When a generator's settings change: its parameters' keyframes, in
/// order, each with the curve of the first that has one there.
fn key_times(fx: &PluginFx) -> Vec<(Tick, KeyCurve)> {
    let mut v: Vec<(Tick, KeyCurve)> = Vec::new();
    for p in fx.params.values() {
        for k in &p.keys {
            if !v.iter().any(|(t, _)| *t == k.at) {
                v.push((k.at, k.curve));
            }
        }
    }
    v.sort_by_key(|(t, _)| *t);
    v
}

/// The file's 100 ns steps of `t`.
fn units_of(t: Tick) -> i64 {
    (t.0 as i128 * UNITS as i128 / TICKS_PER_SECOND as i128) as i64
}

/// A native generator's record at its keyframes (one at its start when it
/// does not change), the record itself the first's.
fn native_doc(
    class: &str,
    preset: &str,
    fx: &PluginFx,
    record: &dyn Fn(Tick) -> Vec<u8>,
) -> DocEffect {
    let times = key_times(fx);
    let keys: Vec<NativeKey> = if times.is_empty() {
        vec![NativeKey {
            time: 0,
            curve: 0,
            flags: 0,
            data: record(Tick::ZERO),
        }]
    } else {
        times
            .iter()
            .map(|&(t, c)| NativeKey {
                time: units_of(t),
                curve: curve_number(c),
                flags: 0,
                data: record(t),
            })
            .collect()
    };
    DocEffect {
        plugin: String::new(),
        preset: preset.to_string(),
        generator: true,
        native: Some(class.to_string()),
        native_data: keys.last().map(|k| k.data.clone()).unwrap_or_default(),
        native_keys: keys,
        ..Default::default()
    }
}

/// Color Gradient's record at `t`: the points on the angle's axis.
fn gradient_record(fx: &PluginFx, t: Tick) -> Vec<u8> {
    let one = |name: &str, d: f64| setting(fx, name, t, &[d]).first().copied().unwrap_or(d);
    let kind = one("gradientType", 0.0).round().clamp(0.0, 2.0) as u32;
    let points = one("controlPoints", 2.0).round().clamp(1.0, 9.0) as usize;
    let angle = one("aspectRatioAngle", 0.0).to_radians();
    let c = setting(fx, "center", t, &[0.5, 0.5]);
    let center = [
        c.first().copied().unwrap_or(0.5),
        1.0 - c.get(1).copied().unwrap_or(0.5),
    ];
    let mut d = vec![0u8; 336];
    let put = |d: &mut Vec<u8>, at: usize, b: &[u8]| d[at..at + b.len()].copy_from_slice(b);
    put(&mut d, 0, &((points + 1) as u32).to_le_bytes());
    put(&mut d, 4, &kind.to_le_bytes());
    put(&mut d, 8, &angle.to_le_bytes());
    put(&mut d, 0xB0, &center[0].to_le_bytes());
    put(&mut d, 0x100, &center[1].to_le_bytes());
    let (cos, sin) = (angle.cos(), angle.sin());
    for i in 1..=points {
        let r = one(&format!("distance{i}"), 0.0);
        let r = if kind == 0 { r } else { r.abs() };
        let col = setting(fx, &format!("color{i}"), t, &[1.0, 1.0, 1.0, 1.0]);
        let ch = |k: usize| col.get(k).copied().unwrap_or(1.0) as f32;
        for (k, v) in [ch(2), ch(1), ch(0), ch(3)].iter().enumerate() {
            put(&mut d, 0x10 + 16 * i + 4 * k, &v.to_le_bytes());
        }
        put(&mut d, 0xB0 + 8 * i, &(cos * r + center[0]).to_le_bytes());
        put(&mut d, 0x100 + 8 * i, &(sin * r + center[1]).to_le_bytes());
    }
    d
}

/// Credit Roll's record: its settings (712 bytes), then its credits.
fn credit_record(fx: &PluginFx) -> Vec<u8> {
    let t = Tick::ZERO;
    let one = |name: &str, d: f64| setting(fx, name, t, &[d]).first().copied().unwrap_or(d);
    let mut d = vec![0u8; 0x2C8];
    let put = |d: &mut Vec<u8>, at: usize, b: &[u8]| d[at..at + b.len()].copy_from_slice(b);
    let wide = |d: &mut Vec<u8>, at: usize, s: &str, units: usize| {
        for (k, u) in s.encode_utf16().take(units - 1).enumerate() {
            d[at + 2 * k..at + 2 * k + 2].copy_from_slice(&u.to_le_bytes());
        }
    };
    let bgra = |d: &mut Vec<u8>, at: usize, c: &[f64]| {
        let ch = |k: usize| c.get(k).copied().unwrap_or(1.0) as f32;
        for (k, v) in [ch(2), ch(1), ch(0), ch(3)].iter().enumerate() {
            d[at + 4 * k..at + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
    };
    let whole = |name: &str, d: f64, max: f64| one(name, d).round().clamp(0.0, max) as u32;
    put(&mut d, 0x00, &1u32.to_le_bytes());
    put(&mut d, 0x04, &0x2C8u32.to_le_bytes());
    put(&mut d, 0x08, &whole("effect", 0.0, 1.0).to_le_bytes());
    put(&mut d, 0x10, &0.5f64.to_le_bytes());
    put(&mut d, 0x18, &one("left", 0.1).to_le_bytes());
    put(&mut d, 0x20, &one("right", 0.9).to_le_bytes());
    put(&mut d, 0x28, &one("top", 0.1).to_le_bytes());
    put(&mut d, 0x30, &whole("in", 0.0, 15.0).to_le_bytes());
    put(&mut d, 0x34, &whole("out", 0.0, 15.0).to_le_bytes());
    put(&mut d, 0x38, &whole("display", 0.0, 3.0).to_le_bytes());
    wide(
        &mut d,
        0x3C,
        CONNECTORS[whole("connector", 0.0, 9.0) as usize],
        12,
    );
    bgra(
        &mut d,
        0x54,
        &setting(fx, "background", t, &[0.0, 0.0, 0.0, 0.0]),
    );
    put(&mut d, 0x64, &whole("direction", 0.0, 1.0).to_le_bytes());
    put(&mut d, 0x68, &one("centerWidth", 0.05).to_le_bytes());
    // A snapping (0 left, 1 center, 2 right) as the record's alignment.
    let alignment = |snap: f64| match snap.round() as i32 {
        1 => 2u32,
        2 => 1,
        _ => 0,
    };
    // The header's, the single item's, the dual line's left side, its
    // connector and its right side.
    for (s, (style, snap)) in [
        ("title", one("titleSnapping", 1.0)),
        ("item", one("itemSnapping", 1.0)),
        ("columns", one("columnsLeftSnapping", 1.0)),
        ("columns", 1.0),
        ("columns", one("columnsRightSnapping", 1.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let at = 0x70 + 0x78 * s;
        wide(
            &mut d,
            at,
            &text_setting(fx, &format!("{style}Font"), "Arial"),
            32,
        );
        let size = one(&format!("{style}Size"), 20.0).round() as i32;
        put(&mut d, at + 0x40, &size.to_le_bytes());
        let flags = u32::from(one(&format!("{style}Bold"), 0.0) >= 0.5)
            | (u32::from(one(&format!("{style}Italic"), 0.0) >= 0.5) << 1);
        put(&mut d, at + 0x44, &flags.to_le_bytes());
        bgra(
            &mut d,
            at + 0x48,
            &setting(fx, &format!("{style}Color"), t, &[1.0; 4]),
        );
        put(&mut d, at + 0x58, &alignment(snap).to_le_bytes());
        let kerning = u32::from(one(&format!("{style}Kerning"), 0.0) >= 0.5);
        put(&mut d, at + 0x5C, &kerning.to_le_bytes());
        put(
            &mut d,
            at + 0x60,
            &one(&format!("{style}SpaceAbove"), 0.0).to_le_bytes(),
        );
        put(
            &mut d,
            at + 0x68,
            &one(&format!("{style}SpaceBelow"), 0.05).to_le_bytes(),
        );
        let tracking = one(&format!("{style}Tracking"), 0.0) + 1.0;
        put(&mut d, at + 0x70, &tracking.to_le_bytes());
    }
    // The credits: a line each, each ending in CR LF.
    let text = text_setting(fx, "text", "");
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut wide_text: Vec<u16> = Vec::new();
    for l in &lines {
        wide_text.extend(l.encode_utf16());
        wide_text.extend([13, 10]);
    }
    wide_text.push(0);
    d.extend_from_slice(&((wide_text.len() * 2) as u32).to_le_bytes());
    d.extend_from_slice(&(lines.len() as u32).to_le_bytes());
    for u in wide_text {
        d.extend_from_slice(&u.to_le_bytes());
    }
    d
}

/// A parameter's keyframes as the file keeps an OpenFX plug-in's: frames
/// of `rate`, every number (a color's alpha too), handles for the numbers
/// with a point.
fn ofx_keys(p: &PluginParam, ty: bv_ofx::ParamType, rate: FrameRate) -> Vec<DocKey> {
    use bv_ofx::ParamType as T;
    let frames = |t: Tick| t.0 as f64 * rate.as_f64() / TICKS_PER_SECOND as f64;
    let handled = matches!(ty, T::Double | T::Double2D | T::Double3D);
    let whole = matches!(ty, T::Integer | T::Boolean | T::Choice);
    p.keys
        .iter()
        .filter_map(|k| {
            let PluginValue::Num(v) = &k.value else {
                return None;
            };
            let frame = frames(k.at);
            Some(DocKey {
                frame,
                interpolation: if whole { 0 } else { interpolation(k.curve) },
                value: v.clone(),
                handles: if handled {
                    v.iter()
                        .map(|x| [frame - 0.1, *x, frame + 0.1, *x])
                        .collect()
                } else {
                    Vec::new()
                },
            })
        })
        .collect()
}

/// A curve's number among an OpenFX parameter's interpolations.
fn interpolation(c: KeyCurve) -> i32 {
    match c {
        KeyCurve::Linear => 1,
        KeyCurve::Fast => 2,
        KeyCurve::Slow => 3,
        KeyCurve::Smooth => 4,
        KeyCurve::Sharp => 5,
        KeyCurve::Hold => 6,
        KeyCurve::Bezier => 9,
        KeyCurve::BezierSplit => 10,
    }
}

/// `name` with its first letter upper case (the file's names).
fn upper_first(name: &str) -> String {
    let mut n = name.to_string();
    if let Some(first) = n.get(..1) {
        let upper = first.to_ascii_uppercase();
        n.replace_range(..1, &upper);
    }
    n
}

/// An OpenFX generator of BananaV's as the file keeps its twin: every
/// parameter in the order it is defined (its groups as zeros), by the
/// twin's names, with its keyframes.
fn ofx_out(fx: &PluginFx, rate: FrameRate) -> Option<DocEffect> {
    use bv_ofx::ParamType as T;
    let known = bv_ofx::registry().known();
    let info = known.iter().find(|i| i.identifier == fx.id)?;
    let described = info
        .context(bv_ofx::Context::Generator)
        .or_else(|| info.described.first())?;
    let mut params = Vec::new();
    let mut keys = Vec::new();
    for p in &described.params {
        let nums = || match fx.params.get(&p.name).map(|x| &x.value) {
            Some(PluginValue::Num(v)) => v.clone(),
            _ => match &p.default {
                bv_ofx::Value::Num(v) => v.clone(),
                _ => Vec::new(),
            },
        };
        let first = || nums().first().copied().unwrap_or(0.0);
        let value = match p.ty {
            T::Group => ParamValue::Integer(0),
            T::Integer | T::Boolean | T::Choice => ParamValue::Integer(first().round() as i64),
            T::Double => ParamValue::Number(first()),
            T::Double2D | T::Double3D | T::Rgb | T::Rgba | T::Integer2D | T::Integer3D => {
                ParamValue::Numbers(nums())
            }
            _ => continue,
        };
        let name = upper_first(&p.name);
        if let Some(param) = fx.params.get(&p.name)
            && !param.keys.is_empty()
        {
            keys.push((name.clone(), ofx_keys(param, p.ty, rate)));
        }
        params.push((name, value));
    }
    Some(DocEffect {
        plugin: format!(
            "{}{}",
            crate::veg_ui::VEG_PLUGINS,
            crate::veg_ui::plain(&info.label)
        ),
        preset: String::new(),
        params,
        keys,
        generator: true,
        ..Default::default()
    })
}

/// A run of Titles & Text's text in a style of its own: its characters,
/// and the font, size, bold and italic it sets.
type StyleRun = (
    usize,
    usize,
    Option<String>,
    Option<f64>,
    Option<bool>,
    Option<bool>,
);

/// Titles & Text's runs as its styles parameter holds them: a line each,
/// `start end name=value;…`.
fn runs_of(text: &str) -> Vec<StyleRun> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            let start: usize = parts.next()?.parse().ok()?;
            let end: usize = parts.next()?.parse().ok()?;
            let mut run: StyleRun = (start, end, None, None, None, None);
            let on = |v: &str| v.parse::<f64>().ok().map(|b| b >= 0.5);
            for pair in parts.next().unwrap_or("").split(';') {
                let Some((k, v)) = pair.split_once('=') else {
                    continue;
                };
                match k.trim() {
                    "font" => run.2 = Some(v.trim().to_string()),
                    "size" => run.3 = v.trim().parse().ok(),
                    "bold" => run.4 = on(v.trim()),
                    "italic" => run.5 = on(v.trim()),
                    _ => {}
                }
            }
            (end > start).then_some(run)
        })
        .collect()
}

/// Titles & Text as the file keeps its twin: its text as rich text (the
/// runs of other fonts, sizes, bold and italic in it), the rest by the
/// twin's names and units, in its order.
fn titles_out(fx: &PluginFx, rate: FrameRate) -> Option<DocEffect> {
    let t = Tick::ZERO;
    let one = |name: &str, d: f64| setting(fx, name, t, &[d]).first().copied().unwrap_or(d);
    let text = text_setting(fx, "text", "Sample Text");
    let base = (
        text_setting(fx, "font", "Verdana"),
        one("size", 48.0),
        one("bold", 0.0) >= 0.5,
        one("italic", 0.0) >= 0.5,
    );
    let runs = runs_of(&text_setting(fx, "styles", ""));
    // The text in spans of one style each.
    let mut spans: Vec<crate::veg_rtf::Span> = Vec::new();
    for (i, c) in text.chars().filter(|c| *c != '\r').enumerate() {
        let mut st = base.clone();
        for r in runs.iter().filter(|r| r.0 <= i && i < r.1) {
            if let Some(f) = &r.2 {
                st.0 = f.clone();
            }
            if let Some(v) = r.3 {
                st.1 = v;
            }
            if let Some(b) = r.4 {
                st.2 = b;
            }
            if let Some(b) = r.5 {
                st.3 = b;
            }
        }
        let half_points = (st.1 / TITLES_POINTS).round().max(1.0) as u32;
        match spans.last_mut() {
            Some(s)
                if s.font == st.0
                    && s.half_points == half_points
                    && s.bold == st.2
                    && s.italic == st.3 =>
            {
                s.text.push(c);
            }
            _ => spans.push(crate::veg_rtf::Span {
                text: c.to_string(),
                font: st.0.clone(),
                half_points,
                bold: st.2,
                italic: st.3,
            }),
        }
    }
    let rtf = crate::veg_rtf::write(&spans, one("justification", 0.0).round() as i32);
    let mut params: Vec<(String, ParamValue)> = vec![
        ("Text".into(), ParamValue::Text(rtf)),
        (
            "TextColor".into(),
            ParamValue::Numbers(setting(fx, "textColor", t, &[1.0; 4])),
        ),
        (
            "AnimationName".into(),
            ParamValue::Text(text_setting(fx, "animationName", "_None")),
        ),
    ];
    let mut keys = Vec::new();
    if let Some(p) = fx.params.get("textColor")
        && !p.keys.is_empty()
    {
        keys.push((
            "TextColor".to_string(),
            ofx_keys(p, bv_ofx::ParamType::Rgba, rate),
        ));
    }
    for vegas in [
        "Scale",
        "Location",
        "Alignment",
        "AdvancedGroup",
        "FitBackgroundColor",
        "Background",
        "Tracking",
        "LineSpacing",
        "OutlineGroup",
        "OutlineWidth",
        "OutlineColor",
        "ShadowGroup",
        "ShadowEnable",
        "ShadowColor",
        "ShadowOffsetX",
        "ShadowOffsetY",
        "ShadowBlur",
    ] {
        let Some(ours) = titles_param(vegas) else {
            params.push((vegas.into(), ParamValue::Integer(0)));
            continue;
        };
        let defaults: &[f64] = match ours {
            "scale" | "lineSpacing" => &[1.0],
            "location" => &[0.5, 0.5],
            "anchor" => &[4.0],
            "background" => &[0.0; 4],
            "outlineColor" => &[1.0; 4],
            "shadowColor" => &[0.0, 0.0, 0.0, 1.0],
            "shadowOffsetX" | "shadowOffsetY" => &[0.2],
            "shadowBlur" => &[0.4],
            _ => &[0.0],
        };
        let v = setting(fx, ours, t, defaults);
        let value = match ours {
            "anchor" | "fitBackgroundColor" | "shadowEnable" => {
                ParamValue::Integer(v.first().copied().unwrap_or(0.0).round() as i64)
            }
            _ if v.len() == 1 => ParamValue::Number(v[0]),
            _ => ParamValue::Numbers(v),
        };
        if let Some(p) = fx.params.get(ours)
            && !p.keys.is_empty()
        {
            let ty = match &value {
                ParamValue::Integer(_) => bv_ofx::ParamType::Integer,
                ParamValue::Numbers(n) if n.len() == 2 => bv_ofx::ParamType::Double2D,
                ParamValue::Numbers(_) => bv_ofx::ParamType::Rgba,
                _ => bv_ofx::ParamType::Double,
            };
            keys.push((vegas.to_string(), ofx_keys(p, ty, rate)));
        }
        params.push((vegas.into(), value));
    }
    Some(DocEffect {
        plugin: format!("{}titlesandtext", crate::veg_ui::VEG_PLUGINS),
        preset: String::new(),
        params,
        keys,
        generator: true,
        ..Default::default()
    })
}

/// One of BananaV's generators as the file keeps its twin (`None`: one it
/// has no twin for).
pub(crate) fn generator_out(fx: &PluginFx, rate: FrameRate) -> Option<DocEffect> {
    match fx.id.as_str() {
        "org.bananav.ColorGradient" => Some(native_doc(COLOR_GRADIENT_CLASS, "", fx, &|t| {
            gradient_record(fx, t)
        })),
        "org.bananav.CreditRoll" => {
            let record = credit_record(fx);
            Some(native_doc(CREDIT_ROLL_CLASS, "", fx, &|_| record.clone()))
        }
        "org.bananav.TitlesText" => titles_out(fx, rate),
        "org.bananav.SolidColor"
        | "org.bananav.Checkerboard"
        | "org.bananav.TestPattern"
        | "org.bananav.NoiseTexture" => ofx_out(fx, rate),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record: white to black across, as a new media's.
    fn white_to_black() -> Vec<u8> {
        let mut d = vec![0u8; 336];
        d[0..4].copy_from_slice(&3u32.to_le_bytes());
        for (i, c) in [(1usize, [1f32; 4]), (2, [0.0, 0.0, 0.0, 1.0])] {
            for (k, v) in c.iter().enumerate() {
                let at = 0x10 + 16 * i + 4 * k;
                d[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        for (i, (x, y)) in [(0.5f64, 0.5f64), (0.0, 0.5), (1.0, 0.5)]
            .into_iter()
            .enumerate()
        {
            d[0xB0 + 8 * i..0xB8 + 8 * i].copy_from_slice(&x.to_le_bytes());
            d[0x100 + 8 * i..0x108 + 8 * i].copy_from_slice(&y.to_le_bytes());
        }
        d
    }

    #[test]
    fn a_record_becomes_points_on_the_axis() {
        let p = gradient_in(&white_to_black(), &[], FrameRate::new(30, 1)).expect("read");
        let num = |k: &str| match &p[k].value {
            PluginValue::Num(v) => v.clone(),
            _ => Vec::new(),
        };
        assert_eq!(num("controlPoints"), [2.0]);
        // The first point half the frame to the left, the second to the
        // right.
        assert_eq!(num("distance1"), [-0.5]);
        assert_eq!(num("distance2"), [0.5]);
        assert_eq!(num("color1"), [1.0; 4]);
        assert_eq!(num("center"), [0.5, 0.5]);
    }
}
