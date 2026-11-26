//! Color.

use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::filter(id, name, Group::Color, about)
}

/// The numbers of `k::MATRIX`: rows, contrast, saturation, power and gain.
fn matrix(
    b: &mut B<'_>,
    rows: [[f32; 4]; 3],
    contrast: f32,
    saturation: f32,
    power: f32,
    gain: f32,
) -> Slot {
    let n = nums![rows[0], rows[1], rows[2], contrast, saturation, power, gain];
    b.run(k::MATRIX, &[b.src()], n, [0; 4])
}

/// A matrix `amount` of the way from leaving colors as they are.
fn toward(m: [[f32; 3]; 3], amount: f32) -> [[f32; 4]; 3] {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if c == 3 {
                0.0
            } else {
                let same = f32::from(u8::from(r == c));
                same + (m[r][c] - same) * amount
            }
        })
    })
}

/// Duotone and its kin: `names` are the colors' settings.
fn tones(b: &mut B<'_>, names: &[&str]) -> Slot {
    let mut c = [[0.0f32; 3]; 5];
    for (i, n) in names.iter().enumerate() {
        c[i] = b.color(n);
    }
    if b.on("invert") {
        c[..names.len()].reverse();
    }
    // The middle light where it is half gray: a power of the light.
    let mid = (b.k("balance").clamp(0.02, 0.98)).ln() / 0.5f32.ln();
    let n = nums![
        c[0],
        c[1],
        c[2],
        c[3],
        c[4],
        b.k("smooth"),
        1.0 / mid.max(0.05),
        1.0 - b.k("amount"),
        b.k("light"),
    ];
    b.run(k::TONES, &[b.src()], n, [names.len() as i32, 0, 0, 0])
}

fn tone_settings(fx: Fx) -> Fx {
    fx.pct("balance", "Balance", 50.0)
        .hint("The light that takes the middle of the colors.")
        .pct("smooth", "Smoothness", 100.0)
        .hint("From flat areas of color (0) to a smooth blend.")
        .pct("light", "Keep Light", 0.0)
        .hint("How much of the picture's own light stays, whatever the colors' is.")
        .toggle("invert", "Reverse Colors", false)
        .pct("amount", "Amount", 100.0)
}

pub fn list() -> Vec<Fx> {
    vec![
        fx(
            "ChannelMixer",
            "Channel Mixer",
            "Makes each color channel from the others, with gain and offset.",
        )
        .section("redOut", "Red Output", true)
        .within("rr", "Red", 100.0, -200.0, 200.0)
        .within("rg", "Green", 0.0, -200.0, 200.0)
        .within("rb", "Blue", 0.0, -200.0, 200.0)
        .within("rk", "Offset", 0.0, -100.0, 100.0)
        .section("greenOut", "Green Output", true)
        .within("gr", "Red", 0.0, -200.0, 200.0)
        .within("gg", "Green", 100.0, -200.0, 200.0)
        .within("gb", "Blue", 0.0, -200.0, 200.0)
        .within("gk", "Offset", 0.0, -100.0, 100.0)
        .section("blueOut", "Blue Output", true)
        .within("br", "Red", 0.0, -200.0, 200.0)
        .within("bg", "Green", 0.0, -200.0, 200.0)
        .within("bb", "Blue", 100.0, -200.0, 200.0)
        .within("bk", "Offset", 0.0, -100.0, 100.0)
        .end()
        .preset(
            "Swap Red & Blue",
            &[
                ("rr", &[0.0]),
                ("rb", &[100.0]),
                ("br", &[100.0]),
                ("bb", &[0.0]),
            ],
        )
        .preset(
            "Swap Green & Blue",
            &[
                ("gg", &[0.0]),
                ("gb", &[100.0]),
                ("bg", &[100.0]),
                ("bb", &[0.0]),
            ],
        )
        .preset(
            "Infrared Film",
            &[
                ("rr", &[-20.0]),
                ("rg", &[140.0]),
                ("gg", &[60.0]),
                ("gr", &[40.0]),
                ("br", &[60.0]),
                ("bb", &[40.0]),
            ],
        )
        .preset(
            "Red Only",
            &[
                ("gg", &[0.0]),
                ("gr", &[100.0]),
                ("bb", &[0.0]),
                ("br", &[100.0]),
            ],
        )
        .draw(|b| {
            let row = |r: &str, g: &str, bl: &str, o: &str| [b.k(r), b.k(g), b.k(bl), b.k(o)];
            let rows = [
                row("rr", "rg", "rb", "rk"),
                row("gr", "gg", "gb", "gk"),
                row("br", "bg", "bb", "bk"),
            ];
            matrix(b, rows, 1.0, 1.0, 1.0, 1.0)
        }),
        fx(
            "GammaGain",
            "Gamma & Gain",
            "Gamma, gain and lift, for the whole picture and for each channel.",
        )
        .within("gamma", "Gamma", 1.0, 0.1, 5.0)
        .num("gain", "Gain", 1.0, 0.0, 2.0)
        .within("lift", "Lift", 0.0, -0.5, 0.5)
        .section("channels", "Channels", false)
        .within("gammaR", "Red Gamma", 1.0, 0.1, 5.0)
        .within("gammaG", "Green Gamma", 1.0, 0.1, 5.0)
        .within("gammaB", "Blue Gamma", 1.0, 0.1, 5.0)
        .num("gainR", "Red Gain", 1.0, 0.0, 2.0)
        .num("gainG", "Green Gain", 1.0, 0.0, 2.0)
        .num("gainB", "Blue Gain", 1.0, 0.0, 2.0)
        .within("liftR", "Red Lift", 0.0, -0.5, 0.5)
        .within("liftG", "Green Lift", 0.0, -0.5, 0.5)
        .within("liftB", "Blue Lift", 0.0, -0.5, 0.5)
        .end()
        .preset("Open Shadows", &[("gamma", &[1.35]), ("lift", &[0.02])])
        .preset("Deep & Crisp", &[("gamma", &[0.85]), ("gain", &[1.08])])
        .preset(
            "Late Sun",
            &[("gainR", &[1.12]), ("gammaB", &[0.9]), ("gainB", &[0.88])],
        )
        .preset(
            "Cold Morning",
            &[("gainR", &[0.9]), ("gainB", &[1.12]), ("gammaB", &[1.06])],
        )
        .preset(
            "Faded Print",
            &[("lift", &[0.08]), ("gain", &[0.94]), ("liftB", &[0.02])],
        )
        .draw(|b| {
            let g = b.n("gamma");
            let p = ["gammaR", "gammaG", "gammaB"].map(|n| 1.0 / (g * b.n(n)).max(0.02));
            let gain = ["gainR", "gainG", "gainB"].map(|n| b.n("gain") * b.n(n));
            let lift = ["liftR", "liftG", "liftB"].map(|n| b.n("lift") + b.n(n));
            b.run(k::GAMMA, &[b.src()], nums![p, gain, lift], [0; 4])
        }),
        fx(
            "HueSaturation",
            "Hue & Saturation",
            "Turns the colors, makes them stronger or weaker, and tints the picture.",
        )
        .angle("hue", "Hue", 0.0)
        .num("saturation", "Saturation", 100.0, 0.0, 200.0)
        .within("vibrance", "Vibrance", 0.0, -100.0, 100.0)
        .hint("Saturation for the dull colors most.")
        .num("brightness", "Brightness", 100.0, 0.0, 200.0)
        .within("offset", "Offset", 0.0, -100.0, 100.0)
        .color("tint", "Colorize", [1.0, 0.8, 0.45])
        .pct("tintAmount", "Colorize Amount", 0.0)
        .preset("Vivid", &[("saturation", &[130.0]), ("vibrance", &[35.0])])
        .preset("Quiet", &[("saturation", &[55.0]), ("vibrance", &[-10.0])])
        .preset("Opposite Colors", &[("hue", &[180.0])])
        .preset(
            "Banana",
            &[("tint", &[1.0, 0.84, 0.18]), ("tintAmount", &[85.0])],
        )
        .preset(
            "Cyanotype",
            &[("tint", &[0.3, 0.62, 1.0]), ("tintAmount", &[100.0])],
        )
        .draw(|b| {
            let n = nums![
                b.a("hue"),
                b.k("saturation"),
                b.k("vibrance"),
                b.k("brightness"),
                b.k("offset"),
                b.color("tint"),
                b.k("tintAmount"),
            ];
            b.run(k::HUE, &[b.src()], n, [0; 4])
        }),
        fx(
            "BlackWhite",
            "Black & White Mixer",
            "Black and white by how much of each color makes the gray, as colored filters do.",
        )
        .within("red", "Red", 30.0, -200.0, 200.0)
        .within("green", "Green", 59.0, -200.0, 200.0)
        .within("blue", "Blue", 11.0, -200.0, 200.0)
        .num("brightness", "Brightness", 100.0, 0.0, 200.0)
        .within("contrast", "Contrast", 100.0, 0.0, 300.0)
        .color("tint", "Tint", [1.0, 0.86, 0.66])
        .pct("tintAmount", "Tint Amount", 0.0)
        .preset(
            "Red Filter",
            &[
                ("red", &[130.0]),
                ("green", &[0.0]),
                ("blue", &[-30.0]),
                ("contrast", &[115.0]),
            ],
        )
        .preset(
            "Green Filter",
            &[("red", &[10.0]), ("green", &[100.0]), ("blue", &[-10.0])],
        )
        .preset(
            "Blue Filter",
            &[("red", &[0.0]), ("green", &[10.0]), ("blue", &[90.0])],
        )
        .preset(
            "Infrared",
            &[
                ("red", &[-40.0]),
                ("green", &[190.0]),
                ("blue", &[-50.0]),
                ("contrast", &[120.0]),
            ],
        )
        .preset(
            "Old Photograph",
            &[("contrast", &[90.0]), ("tintAmount", &[80.0])],
        )
        .preset("Noir", &[("contrast", &[190.0]), ("brightness", &[92.0])])
        .draw(|b| {
            let n = nums![
                b.k("red"),
                b.k("green"),
                b.k("blue"),
                b.k("brightness"),
                b.k("contrast"),
                b.color("tint"),
                b.k("tintAmount"),
            ];
            b.run(k::MONO, &[b.src()], n, [0; 4])
        }),
        fx(
            "Negative",
            "Negative",
            "Turns the picture over: its light, its colors, or each channel.",
        )
        .choice(
            "mode",
            "Turn Over",
            0,
            &["Channels", "Light Only", "Colors Only"],
        )
        .toggle("red", "Red", true)
        .toggle("green", "Green", true)
        .toggle("blue", "Blue", true)
        .preset("Light Only", &[("mode", &[1.0])])
        .preset("Colors Only", &[("mode", &[2.0])])
        .preset("Red Negative", &[("green", &[0.0]), ("blue", &[0.0])])
        .draw(|b| {
            let n = nums![b.on("red"), b.on("green"), b.on("blue")];
            b.run(k::NEGATIVE, &[b.src()], n, [b.i("mode"), 0, 0, 0])
        }),
        tone_settings(
            fx("Duotone", "Duotone", "The picture's light in two colors.")
                .color("dark", "Dark", [0.07, 0.05, 0.22])
                .color("bright", "Bright", [1.0, 0.86, 0.35]),
        )
        .preset(
            "Banana & Night",
            &[
                ("dark", &[0.07, 0.05, 0.22]),
                ("bright", &[1.0, 0.86, 0.35]),
            ],
        )
        .preset(
            "Neon",
            &[("dark", &[0.16, 0.0, 0.32]), ("bright", &[0.2, 1.0, 0.95])],
        )
        .preset(
            "Strawberry",
            &[("dark", &[0.3, 0.02, 0.1]), ("bright", &[1.0, 0.86, 0.88])],
        )
        .preset(
            "Mint",
            &[("dark", &[0.01, 0.14, 0.14]), ("bright", &[0.8, 1.0, 0.86])],
        )
        .preset(
            "Riso Print",
            &[
                ("dark", &[0.0, 0.33, 0.75]),
                ("bright", &[1.0, 0.42, 0.36]),
                ("smooth", &[35.0]),
            ],
        )
        .draw(|b| tones(b, &["dark", "bright"])),
        tone_settings(
            fx("Tritone", "Tritone", "The picture's light in three colors.")
                .color("dark", "Dark", [0.04, 0.04, 0.12])
                .color("mid", "Middle", [0.82, 0.3, 0.2])
                .color("bright", "Bright", [1.0, 0.93, 0.75]),
        )
        .preset(
            "Dusk",
            &[
                ("dark", &[0.06, 0.03, 0.18]),
                ("mid", &[0.85, 0.28, 0.35]),
                ("bright", &[1.0, 0.82, 0.5]),
            ],
        )
        .preset(
            "Deep Sea",
            &[
                ("dark", &[0.0, 0.03, 0.1]),
                ("mid", &[0.0, 0.4, 0.5]),
                ("bright", &[0.85, 1.0, 0.94]),
            ],
        )
        .preset(
            "Vaporwave",
            &[
                ("dark", &[0.12, 0.02, 0.3]),
                ("mid", &[1.0, 0.25, 0.7]),
                ("bright", &[0.45, 0.95, 1.0]),
            ],
        )
        .draw(|b| tones(b, &["dark", "mid", "bright"])),
        tone_settings(
            fx(
                "Quadtone",
                "Quadtone",
                "The picture's light in four colors.",
            )
            .color("dark", "Dark", [0.02, 0.03, 0.06])
            .color("midDark", "Dark Middle", [0.12, 0.34, 0.5])
            .color("midBright", "Bright Middle", [0.92, 0.55, 0.25])
            .color("bright", "Bright", [1.0, 0.95, 0.83]),
        )
        .preset("Teal & Amber", &[])
        .preset(
            "Pop Poster",
            &[
                ("dark", &[0.1, 0.02, 0.2]),
                ("midDark", &[0.8, 0.05, 0.4]),
                ("midBright", &[1.0, 0.65, 0.0]),
                ("bright", &[1.0, 0.97, 0.7]),
                ("smooth", &[0.0]),
            ],
        )
        .draw(|b| tones(b, &["dark", "midDark", "midBright", "bright"])),
        tone_settings(
            fx(
                "FiveTones",
                "Five Tones",
                "The picture's light in five colors.",
            )
            .color("c1", "Color 1", [0.02, 0.02, 0.08])
            .color("c2", "Color 2", [0.2, 0.08, 0.42])
            .color("c3", "Color 3", [0.78, 0.2, 0.38])
            .color("c4", "Color 4", [1.0, 0.62, 0.2])
            .color("c5", "Color 5", [1.0, 0.98, 0.8]),
        )
        .preset("Ember", &[])
        .preset(
            "Rainforest",
            &[
                ("c1", &[0.01, 0.05, 0.03]),
                ("c2", &[0.05, 0.25, 0.15]),
                ("c3", &[0.25, 0.5, 0.2]),
                ("c4", &[0.75, 0.75, 0.3]),
                ("c5", &[1.0, 1.0, 0.85]),
            ],
        )
        .preset(
            "Paper Cut",
            &[
                ("c1", &[0.1, 0.1, 0.12]),
                ("c2", &[0.25, 0.3, 0.45]),
                ("c3", &[0.55, 0.6, 0.7]),
                ("c4", &[0.85, 0.8, 0.72]),
                ("c5", &[1.0, 0.98, 0.94]),
                ("smooth", &[0.0]),
            ],
        )
        .draw(|b| tones(b, &["c1", "c2", "c3", "c4", "c5"])),
        fx(
            "HighlightsOnly",
            "Highlights Only",
            "Keeps only the bright parts, on black or on nothing.",
        )
        .pct("threshold", "Threshold", 65.0)
        .pct("soft", "Softness", 12.0)
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .num("saturation", "Saturation", 100.0, 0.0, 200.0)
        .num("blur", "Blur", 0.0, 0.0, 10.0)
        .toggle("clear", "Transparent", false)
        .hint("What is not bright is see-through instead of black.")
        .preset(
            "Only the Lights",
            &[("threshold", &[85.0]), ("brightness", &[150.0])],
        )
        .preset(
            "Wide Highlights",
            &[("threshold", &[45.0]), ("soft", &[25.0])],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("blur"), 1);
            let n = nums![
                b.k("threshold"),
                b.k("soft"),
                b.k("saturation"),
                b.k("brightness")
            ];
            b.run(k::ONLY_BRIGHT, &[s], n, [i32::from(b.on("clear")), 0, 0, 0])
        }),
        fx(
            "ColorLimiter",
            "Color Limiter",
            "Holds light and color within limits, easing off before them.",
        )
        .within("high", "Brightest", 95.0, 10.0, 120.0)
        .within("low", "Darkest", 3.0, -10.0, 60.0)
        .within("color", "Strongest Color", 45.0, 0.0, 100.0)
        .within("knee", "Ease", 8.0, 0.0, 40.0)
        .preset(
            "Broadcast",
            &[("high", &[92.0]), ("low", &[6.0]), ("color", &[36.0])],
        )
        .preset(
            "Soft Colors",
            &[
                ("high", &[100.0]),
                ("low", &[0.0]),
                ("color", &[18.0]),
                ("knee", &[20.0]),
            ],
        )
        .draw(|b| {
            let n = nums![b.k("high"), b.k("low"), b.k("color"), b.k("knee")];
            b.run(k::LIMIT, &[b.src()], n, [0; 4])
        }),
        fx(
            "ExposureZebra",
            "Exposure Zebra",
            "Moving stripes where the picture is too bright, too dark or too colorful.",
        )
        .within("high", "Too Bright", 95.0, 0.0, 150.0)
        .within("low", "Too Dark", 2.0, -10.0, 60.0)
        .toggle("showColor", "Show Strong Colors", false)
        .within("color", "Strongest Color", 45.0, 0.0, 100.0)
        .num("size", "Stripe Size", 2.0, 0.5, 8.0)
        .num("speed", "Speed", 1.5, 0.0, 6.0)
        .moving()
        .preset(
            "Strict",
            &[
                ("high", &[90.0]),
                ("low", &[5.0]),
                ("showColor", &[1.0]),
                ("color", &[36.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.k("high"),
                b.k("low"),
                b.len("size"),
                b.n("speed"),
                b.k("color")
            ];
            b.run(
                k::ZEBRA,
                &[b.src()],
                n,
                [i32::from(b.on("showColor")), 0, 0, 0],
            )
        }),
        fx(
            "Threshold",
            "Two-Tone Threshold",
            "Black or white (or each color on or off) either side of a level.",
        )
        .pct("level", "Level", 50.0)
        .pct("soft", "Softness", 4.0)
        .choice("by", "Split By", 0, &["Brightness", "Each Channel"])
        .pct("keep", "Keep Color", 0.0)
        .toggle("invert", "Invert", false)
        .preset("Stencil", &[("soft", &[1.0])])
        .preset("Eight Colors", &[("by", &[1.0]), ("soft", &[1.0])])
        .preset(
            "Cut Out Color",
            &[("keep", &[100.0]), ("level", &[35.0]), ("soft", &[8.0])],
        )
        .draw(|b| {
            let n = nums![b.k("level"), b.k("soft") * 0.5, b.on("invert"), b.k("keep")];
            b.run(k::THRESHOLD, &[b.src()], n, [b.i("by"), 0, 0, 0])
        }),
        fx(
            "SplitTint",
            "Split Tint",
            "One tint for the dark parts and another for the light ones.",
        )
        .color("dark", "Shadows", [0.1, 0.32, 0.45])
        .pct("darkAmount", "Shadows Amount", 45.0)
        .color("bright", "Highlights", [1.0, 0.75, 0.42])
        .pct("brightAmount", "Highlights Amount", 45.0)
        .pct("balance", "Balance", 50.0)
        .num("saturation", "Saturation", 100.0, 0.0, 200.0)
        .preset("Teal & Orange", &[])
        .preset(
            "Golden Hour",
            &[("dark", &[0.35, 0.16, 0.2]), ("bright", &[1.0, 0.78, 0.4])],
        )
        .preset(
            "Moonlit",
            &[
                ("dark", &[0.05, 0.1, 0.3]),
                ("bright", &[0.65, 0.8, 1.0]),
                ("saturation", &[60.0]),
            ],
        )
        .preset(
            "Toxic",
            &[
                ("dark", &[0.12, 0.25, 0.02]),
                ("bright", &[0.85, 1.0, 0.35]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.color("dark"),
                b.color("bright"),
                b.k("darkAmount"),
                b.k("brightAmount"),
                b.k("balance"),
                b.k("saturation"),
            ];
            b.run(k::SPLIT_TINT, &[b.src()], n, [0; 4])
        }),
        fx(
            "Posterize",
            "Posterize",
            "Flat steps of color, as a printed poster's.",
        )
        .num("steps", "Steps", 5.0, 2.0, 16.0)
        .pct("soft", "Softness", 10.0)
        .choice("by", "Steps Of", 0, &["Each Channel", "Brightness"])
        .num("smooth", "Smooth First", 0.0, 0.0, 4.0)
        .preset(
            "Poster",
            &[("steps", &[4.0]), ("by", &[1.0]), ("smooth", &[0.6])],
        )
        .preset("Old Computer", &[("steps", &[3.0]), ("soft", &[0.0])])
        .draw(|b| {
            let s = b.blur(b.src(), b.len("smooth") * 0.25, 1);
            let n = nums![b.n("steps"), b.k("soft")];
            b.run(k::POSTERIZE, &[s], n, [b.i("by"), 0, 0, 0])
        }),
        fx(
            "Solarize",
            "Solarize",
            "Light that folds over past a level, as film exposed again in the darkroom.",
        )
        .pct("level", "Level", 50.0)
        .num("times", "Folds", 1.0, 0.5, 6.0)
        .pct("amount", "Amount", 100.0)
        .preset("Darkroom", &[("level", &[60.0])])
        .preset("Chrome", &[("level", &[35.0]), ("times", &[2.0])])
        .preset("Liquid Colors", &[("level", &[30.0]), ("times", &[4.0])])
        .draw(|b| {
            let n = nums![b.k("level"), b.n("times"), b.k("amount")];
            b.run(k::SOLARIZE, &[b.src()], n, [0; 4])
        }),
        fx(
            "BleachBypass",
            "Bleach Bypass",
            "Silver left in the film: strong contrast, weak colors.",
        )
        .pct("amount", "Amount", 70.0)
        .num("saturation", "Saturation", 55.0, 0.0, 150.0)
        .within("contrast", "Contrast", 105.0, 50.0, 200.0)
        .preset(
            "War Film",
            &[
                ("amount", &[90.0]),
                ("saturation", &[35.0]),
                ("contrast", &[115.0]),
            ],
        )
        .preset("Gentle", &[("amount", &[40.0]), ("saturation", &[80.0])])
        .draw(|b| {
            let n = nums![b.k("amount"), b.k("saturation"), b.k("contrast")];
            b.run(k::BLEACH, &[b.src()], n, [0; 4])
        }),
        fx(
            "TwoStrip",
            "Two-Strip Film",
            "The colors of film with only two strips: oranges and teals.",
        )
        .pct("amount", "Amount", 100.0)
        .within("contrast", "Contrast", 110.0, 50.0, 200.0)
        .num("saturation", "Saturation", 115.0, 0.0, 200.0)
        .preset(
            "First Color",
            &[("contrast", &[120.0]), ("saturation", &[130.0])],
        )
        .preset(
            "Faded Reel",
            &[("contrast", &[90.0]), ("saturation", &[70.0])],
        )
        .draw(|b| {
            let m = [[0.92, 0.12, -0.04], [0.08, 0.55, 0.37], [0.0, 0.52, 0.42]];
            matrix(
                b,
                toward(m, b.k("amount")),
                b.k("contrast"),
                b.k("saturation"),
                1.0,
                1.0,
            )
        }),
        fx(
            "ThreeStrip",
            "Three-Strip Film",
            "The deep, rich colors of film dyed in three passes.",
        )
        .pct("amount", "Amount", 100.0)
        .within("contrast", "Contrast", 112.0, 50.0, 200.0)
        .num("saturation", "Saturation", 125.0, 0.0, 200.0)
        .preset(
            "Musical",
            &[("contrast", &[118.0]), ("saturation", &[150.0])],
        )
        .preset(
            "Gentle Dyes",
            &[("amount", &[60.0]), ("contrast", &[104.0])],
        )
        .draw(|b| {
            let m = [
                [1.38, -0.24, -0.14],
                [-0.18, 1.34, -0.16],
                [-0.14, -0.26, 1.4],
            ];
            matrix(
                b,
                toward(m, b.k("amount") * 0.75),
                b.k("contrast"),
                b.k("saturation"),
                1.0,
                1.0,
            )
        }),
        fx(
            "FalseColor",
            "False Color",
            "The light as a map of colors: heat, night vision, ice.",
        )
        .choice(
            "map",
            "Map",
            0,
            &["Heat", "Rainbow", "Night Vision", "Ice", "Sunset"],
        )
        .within("low", "From", 0.0, -50.0, 100.0)
        .within("high", "To", 100.0, 0.0, 200.0)
        .num("times", "Repeat", 1.0, 1.0, 6.0)
        .num("smooth", "Smooth First", 0.4, 0.0, 4.0)
        .pct("amount", "Amount", 100.0)
        .preset("Thermal Camera", &[("smooth", &[1.5])])
        .preset("Night Vision", &[("map", &[2.0]), ("high", &[75.0])])
        .preset("Frozen", &[("map", &[3.0])])
        .preset(
            "Contour Rainbow",
            &[("map", &[1.0]), ("times", &[4.0]), ("smooth", &[2.0])],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("smooth") * 0.3, 1);
            let n = nums![b.k("low"), b.k("high"), b.n("times"), b.k("amount")];
            b.run(k::FALSE_COLOR, &[s], n, [b.i("map"), 0, 0, 0])
        }),
        fx(
            "AnimalEyes",
            "Animal Eyes",
            "The world as eyes with fewer colors see it.",
        )
        .choice("eyes", "Eyes", 0, &["Dog", "Bull", "Cat", "Deep Sea"])
        .num("soft", "Softness", 0.4, 0.0, 4.0)
        .pct("amount", "Amount", 100.0)
        .preset("Dog", &[])
        .preset("Cat at Night", &[("eyes", &[2.0]), ("soft", &[0.8])])
        .draw(|b| {
            let m = match b.i("eyes") {
                1 => [[0.57, 0.43, 0.0], [0.56, 0.44, 0.0], [0.0, 0.24, 0.76]],
                2 => [[0.42, 0.45, 0.08], [0.36, 0.52, 0.12], [0.12, 0.34, 0.6]],
                3 => [[0.02, 0.1, 0.06], [0.04, 0.42, 0.34], [0.06, 0.5, 0.75]],
                _ => [[0.6, 0.4, 0.0], [0.55, 0.45, 0.0], [0.0, 0.2, 0.8]],
            };
            let s = b.blur(b.src(), b.len("soft") * 0.25, 1);
            let rows = toward(m, b.k("amount"));
            let n = nums![rows[0], rows[1], rows[2], 1.0, 1.0, 1.0, 1.0];
            b.run(k::MATRIX, &[s], n, [0; 4])
        }),
        fx(
            "ColorFuse",
            "Color Fuse",
            "Brings every color toward a pair of them, with glowing highlights.",
        )
        .color("dark", "Dark Colors", [0.1, 0.3, 0.42])
        .color("bright", "Bright Colors", [1.0, 0.66, 0.36])
        .pct("amount", "Amount", 70.0)
        .pct("glow", "Glow", 25.0)
        .num("glowSize", "Glow Size", 4.0, 0.0, 20.0)
        .preset("Blockbuster", &[])
        .preset(
            "Candy",
            &[
                ("dark", &[0.4, 0.15, 0.6]),
                ("bright", &[1.0, 0.6, 0.75]),
                ("glow", &[45.0]),
            ],
        )
        .preset(
            "Desert",
            &[("dark", &[0.35, 0.18, 0.1]), ("bright", &[1.0, 0.85, 0.55])],
        )
        .draw(|b| {
            let hot = b.bright(b.src(), 0.55, 0.3, 1.1, 1.0);
            let g = b.blur(hot, b.len("glowSize"), 0);
            let n = nums![
                b.color("dark"),
                b.color("bright"),
                b.k("amount"),
                b.k("glow")
            ];
            b.run(k::FUSE, &[b.src(), g], n, [0; 4])
        }),
        fx(
            "PrintFilm",
            "Print Film",
            "How a film print answers to light: rolled highlights, lifted blacks, its tints.",
        )
        .within("contrast", "Contrast", 112.0, 50.0, 200.0)
        .pct("roll", "Highlight Roll-Off", 35.0)
        .within("lift", "Black Lift", 2.0, 0.0, 30.0)
        .num("saturation", "Saturation", 95.0, 0.0, 200.0)
        .color("dark", "Shadows Tint", [0.46, 0.52, 0.56])
        .color("bright", "Highlights Tint", [0.56, 0.52, 0.45])
        .pct("amount", "Amount", 100.0)
        .preset("Warm Print", &[("bright", &[0.62, 0.52, 0.38])])
        .preset(
            "Cool Print",
            &[("dark", &[0.42, 0.5, 0.64]), ("bright", &[0.5, 0.52, 0.54])],
        )
        .preset(
            "Faded Summer",
            &[
                ("lift", &[10.0]),
                ("contrast", &[92.0]),
                ("bright", &[0.64, 0.54, 0.38]),
                ("saturation", &[80.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.k("contrast"),
                b.k("roll"),
                b.k("lift"),
                b.k("saturation"),
                b.color("dark"),
                b.color("bright"),
                b.k("amount"),
            ];
            b.run(k::PRINT, &[b.src()], n, [0; 4])
        }),
        fx(
            "Mist",
            "Mist",
            "Mist that thickens between two places, still or drifting.",
        )
        .point("from", "Clear Point", [0.5, 0.15])
        .point("to", "Thick", [0.5, 0.95])
        .color("color", "Color", [0.78, 0.82, 0.88])
        .pct("amount", "Amount", 75.0)
        .pct("thick", "Thickening", 60.0)
        .section("patches", "Patches", true)
        .pct("patchy", "Patchiness", 35.0)
        .num("size", "Size", 45.0, 5.0, 150.0)
        .num("speed", "Drift", 0.06, -0.5, 0.5)
        .seed()
        .end()
        .handle(Handle::Line("from", "to"))
        .moving()
        .preset("Morning", &[])
        .preset(
            "Dust Storm",
            &[
                ("color", &[0.78, 0.58, 0.36]),
                ("amount", &[92.0]),
                ("from", &[0.1, 0.5]),
                ("to", &[0.8, 0.6]),
                ("speed", &[0.25]),
            ],
        )
        .preset(
            "Night Haze",
            &[("color", &[0.12, 0.16, 0.28]), ("patchy", &[15.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("from"),
                b.p("to"),
                b.color("color"),
                b.k("amount"),
                b.k("thick"),
                b.k("patchy"),
                b.len("size"),
                b.n("speed"),
            ];
            b.run(k::MIST, &[b.src()], n, [0; 4])
        }),
    ]
}
