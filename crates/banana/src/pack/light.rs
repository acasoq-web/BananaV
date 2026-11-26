//! Light.

use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::filter(id, name, Group::Light, about)
}

const WHITE: [f64; 3] = [1.0, 1.0, 1.0];

/// What every glow has: where it starts, how wide and how bright.
fn glow_settings(fx: Fx, size: f64, brightness: f64, color: [f64; 3]) -> Fx {
    fx.num("brightness", "Brightness", brightness, 0.0, 300.0)
        .color("color", "Color", color)
        .num("size", "Size", size, 0.0, 30.0)
        .within("stretch", "Stretch", 0.0, -100.0, 100.0)
        .hint("Wider (above 0) or taller (below 0).")
        .pct("threshold", "Threshold", 60.0)
        .pct("soft", "Softness", 25.0)
        .within("saturation", "Saturation", 100.0, 0.0, 300.0)
}

fn glow_end(fx: Fx) -> Fx {
    fx.pct("keep", "Keep Picture", 100.0)
        .hint("How much of the picture stays under the glow: 0 leaves the glow alone.")
        .blend(1)
}

/// A blur that is not round: wider or taller by `stretch`.
fn stretched(b: &mut B<'_>, s: Slot, size: f32, stretch: f32, edges: i32) -> Slot {
    let k = 2f32.powf(stretch * 2.0);
    b.blur_xy(s, size * k, size / k, edges)
}

/// The bright parts as the settings say, blurred: a narrow blur and a wide
/// one, so that the light has a core and falls off far.
fn halo(b: &mut B<'_>) -> Slot {
    let hot = b.bright(
        b.src(),
        b.k("threshold"),
        b.k("soft") * 0.5,
        b.k("saturation"),
        1.0,
    );
    halo_of(b, hot, b.len("size"), b.k("stretch"))
}

fn halo_of(b: &mut B<'_>, hot: Slot, size: f32, stretch: f32) -> Slot {
    let near = stretched(b, hot, size * 0.35, stretch, 0);
    let mid = stretched(b, near, size * 0.9, stretch, 0);
    let far = stretched(b, mid, size * 2.2, stretch, 0);
    b.run(k::SUM3, &[near, mid, far], nums![0.42, 0.36, 0.34], [0; 4])
}

pub fn list() -> Vec<Fx> {
    vec![
        glow_end(glow_settings(
            fx(
                "Glow",
                "Light Glow",
                "Bright parts bloom softly around them.",
            ),
            3.0,
            90.0,
            WHITE,
        ))
        .preset(
            "Soft Bloom",
            &[
                ("size", &[5.0]),
                ("brightness", &[60.0]),
                ("threshold", &[45.0]),
            ],
        )
        .preset("Golden", &[("color", &[1.0, 0.78, 0.4]), ("size", &[6.0])])
        .preset(
            "Intense",
            &[
                ("brightness", &[220.0]),
                ("threshold", &[50.0]),
                ("size", &[8.0]),
            ],
        )
        .preset("Wide Streak", &[("stretch", &[70.0]), ("size", &[6.0])])
        .preset(
            "Dream",
            &[
                ("threshold", &[20.0]),
                ("soft", &[60.0]),
                ("brightness", &[70.0]),
                ("size", &[9.0]),
            ],
        )
        .draw(|b| {
            let g = halo(b);
            b.run(
                k::GLOW,
                &[b.src(), g],
                nums![b.k("brightness"), b.color("color"), b.k("keep")],
                [b.i("blend"), 0, 0, 0],
            )
        }),
        glow_end(
            glow_settings(
                fx(
                    "SuperGlow",
                    "Super Glow",
                    "A glow in three widths, each with its own strength and color.",
                ),
                2.0,
                100.0,
                WHITE,
            )
            .section("layers", "Layers", true)
            .num("near", "Near", 100.0, 0.0, 300.0)
            .color("nearColor", "Near Color", [1.0, 0.97, 0.9])
            .num("middle", "Middle", 80.0, 0.0, 300.0)
            .color("middleColor", "Middle Color", [1.0, 0.82, 0.62])
            .num("far", "Far", 60.0, 0.0, 300.0)
            .color("farColor", "Far Color", [0.5, 0.68, 1.0])
            .end(),
        )
        .preset("Cinema Bloom", &[])
        .preset(
            "Neon Night",
            &[
                ("nearColor", &[1.0, 0.9, 1.0]),
                ("middleColor", &[1.0, 0.25, 0.8]),
                ("farColor", &[0.25, 0.35, 1.0]),
                ("size", &[3.0]),
            ],
        )
        .preset(
            "Nuclear",
            &[
                ("brightness", &[200.0]),
                ("middleColor", &[0.7, 1.0, 0.3]),
                ("farColor", &[0.15, 0.7, 0.2]),
                ("threshold", &[45.0]),
            ],
        )
        .draw(|b| {
            let hot = b.bright(
                b.src(),
                b.k("threshold"),
                b.k("soft") * 0.5,
                b.k("saturation"),
                1.0,
            );
            let (s, st) = (b.len("size"), b.k("stretch"));
            let near = stretched(b, hot, s * 0.5, st, 0);
            let mid = stretched(b, near, s * 1.7, st, 0);
            let far = stretched(b, mid, s * 5.0, st, 0);
            let g = b.k("brightness");
            let c = b.color("color");
            let tint = |b: &B<'_>, name: &str| -> [f32; 3] {
                let t = b.color(name);
                std::array::from_fn(|i| t[i] * c[i])
            };
            let m = b.i("blend");
            let (n1, n2, n3) = (
                nums![b.k("near") * g * 0.75, tint(b, "nearColor"), b.k("keep")],
                nums![b.k("middle") * g * 0.75, tint(b, "middleColor"), 1.0],
                nums![b.k("far") * g * 0.75, tint(b, "farColor"), 1.0],
            );
            let o = b.run(k::GLOW, &[b.src(), near], n1, [m, 0, 0, 0]);
            let o = b.run(k::GLOW, &[o, mid], n2, [m, 0, 0, 0]);
            b.run(k::GLOW, &[o, far], n3, [m, 0, 0, 0])
        }),
        glow_end(
            glow_settings(
                fx(
                    "AuraGlow",
                    "Aura Glow",
                    "A glow of one color near the light and another far from it.",
                ),
                4.0,
                110.0,
                [1.0, 0.92, 0.65],
            )
            .color("outer", "Outer Color", [0.75, 0.25, 1.0])
            .pct("turn", "Change At", 30.0),
        )
        .preset("Spirit", &[])
        .preset(
            "Fire",
            &[("color", &[1.0, 0.9, 0.5]), ("outer", &[1.0, 0.18, 0.02])],
        )
        .preset(
            "Ice",
            &[("color", &[0.95, 1.0, 1.0]), ("outer", &[0.1, 0.45, 1.0])],
        )
        .draw(|b| {
            let g = halo(b);
            let n = nums![
                b.k("brightness"),
                b.color("color"),
                b.k("keep"),
                b.color("outer"),
                b.k("turn")
            ];
            b.run(k::GLOW_TWO, &[b.src(), g], n, [b.i("blend"), 0, 0, 0])
        }),
        fx(
            "DarkGlow",
            "Dark Glow",
            "Darkness that spreads from the dark parts, deepening silhouettes.",
        )
        .num("amount", "Amount", 70.0, 0.0, 300.0)
        .num("size", "Size", 3.0, 0.0, 30.0)
        .pct("threshold", "Threshold", 70.0)
        .hint("How dark a part must be to spread.")
        .pct("soft", "Softness", 30.0)
        .color("color", "Keep Color", [0.0, 0.0, 0.0])
        .preset("Deep Shadows", &[])
        .preset(
            "Blue Dusk",
            &[("color", &[0.0, 0.15, 0.6]), ("amount", &[110.0])],
        )
        .preset(
            "Nightmare",
            &[
                ("amount", &[180.0]),
                ("size", &[6.0]),
                ("threshold", &[60.0]),
            ],
        )
        .draw(|b| {
            let n = nums![b.k("threshold"), b.k("soft") * 0.5, 0.6, 1.0];
            let dark = b.run(k::BRIGHT, &[b.src()], n, [1, 0, 0, 0]);
            let g = b.blur(dark, b.len("size"), 1);
            b.run(
                k::GLOW_DARK,
                &[b.src(), g],
                nums![b.k("amount"), b.color("color")],
                [0; 4],
            )
        }),
        glow_end(
            glow_settings(
                fx(
                    "DistantGlow",
                    "Distant Glow",
                    "Light scattered wide by the air, over the whole picture.",
                ),
                14.0,
                70.0,
                [0.95, 0.97, 1.0],
            )
            .pct("veil", "Veil", 15.0)
            .hint("A haze of the picture's own light over everything."),
        )
        .preset(
            "Humid Night",
            &[("brightness", &[110.0]), ("veil", &[30.0])],
        )
        .preset(
            "Stadium",
            &[
                ("size", &[20.0]),
                ("brightness", &[150.0]),
                ("threshold", &[75.0]),
            ],
        )
        .draw(|b| {
            let g = halo(b);
            let m = b.i("blend");
            let n = nums![b.k("brightness"), b.color("color"), b.k("keep")];
            let o = b.run(k::GLOW, &[b.src(), g], n, [m, 0, 0, 0]);
            let veil = b.blur(b.src(), b.len("size") * 1.2, 1);
            let n = nums![b.k("veil"), b.color("color"), 1.0];
            b.run(k::GLOW, &[o, veil], n, [1, 0, 0, 0])
        }),
        fx(
            "NeonEdges",
            "Neon Edges",
            "Outlines that glow as neon tubes.",
        )
        .num("brightness", "Brightness", 120.0, 0.0, 400.0)
        .color("color", "Color", [0.2, 0.9, 1.0])
        .pct("own", "Picture's Colors", 0.0)
        .num("strength", "Edge Strength", 180.0, 0.0, 600.0)
        .within("threshold", "Edge Threshold", 4.0, 0.0, 50.0)
        .num("width", "Line Width", 0.2, 0.05, 2.0)
        .num("size", "Glow Size", 2.5, 0.0, 20.0)
        .num("glow", "Glow", 120.0, 0.0, 400.0)
        .pct("keep", "Keep Picture", 25.0)
        .preset("Cyber", &[])
        .preset("Pink Sign", &[("color", &[1.0, 0.2, 0.6])])
        .preset("Own Colors", &[("own", &[100.0]), ("keep", &[0.0])])
        .preset(
            "Over the Picture",
            &[("keep", &[100.0]), ("brightness", &[80.0])],
        )
        .draw(|b| {
            let gap = b.px_y(b.len("width")) * 0.5;
            let n = nums![
                b.k("strength") * 2.0,
                b.color("color"),
                b.k("own"),
                b.k("threshold"),
                gap
            ];
            let e = b.run(k::NEON, &[b.src()], n, [0; 4]);
            let e = b.blur(e, b.len("width") * 0.35, 0);
            let g = halo_of(b, e, b.len("size"), 0.0);
            let o = b.run(
                k::GLOW,
                &[b.src(), e],
                nums![b.k("brightness"), [1.0; 3], b.k("keep")],
                [0; 4],
            );
            b.run(
                k::GLOW,
                &[o, g],
                nums![b.k("glow") * 1.5, [1.0; 3], 1.0],
                [1, 0, 0, 0],
            )
        }),
        glow_end(
            glow_settings(
                fx("SmokeGlow", "Smoke Glow", "A glow full of drifting smoke."),
                6.0,
                130.0,
                [0.85, 0.9, 1.0],
            )
            .section("smoke", "Smoke", true)
            .pct("amount", "Amount", 70.0)
            .num("scale", "Smoke Size", 25.0, 2.0, 100.0)
            .int("detail", "Detail", 5, 1, 7)
            .num("speed", "Change", 0.4, 0.0, 3.0)
            .num("driftX", "Drift Horizontal", 0.0, -60.0, 60.0)
            .num("driftY", "Drift Vertical", 6.0, -60.0, 60.0)
            .seed()
            .end(),
        )
        .moving()
        .preset("Stage Smoke", &[])
        .preset(
            "Embers",
            &[
                ("color", &[1.0, 0.45, 0.12]),
                ("scale", &[12.0]),
                ("driftY", &[20.0]),
                ("amount", &[90.0]),
            ],
        )
        .preset(
            "Ghost",
            &[
                ("color", &[0.5, 1.0, 0.8]),
                ("threshold", &[30.0]),
                ("size", &[9.0]),
            ],
        )
        .draw(|b| {
            let g = halo(b);
            let n = nums![
                b.k("brightness"),
                b.color("color"),
                b.k("keep"),
                b.len("scale"),
                b.k("amount"),
                [-b.len("driftX"), b.len("driftY")],
                b.n("speed"),
            ];
            b.run(
                k::GLOW_SMOKE,
                &[b.src(), g],
                n,
                [b.i("blend"), b.i("detail"), 0, 0],
            )
        }),
        glow_settings(
            fx(
                "TubeGlow",
                "Tube Glow",
                "An old tube camera's: light that blooms with a dark rim round it.",
            ),
            2.0,
            100.0,
            [1.0, 0.96, 0.88],
        )
        .pct("rim", "Dark Rim", 55.0)
        .num("rimSize", "Rim Size", 4.0, 0.5, 20.0)
        .num("trail", "Comet", 0.0, 0.0, 20.0)
        .angle("angle", "Comet Angle", 0.0)
        .preset("Live Show 1970", &[])
        .preset(
            "Moon Broadcast",
            &[
                ("rim", &[80.0]),
                ("brightness", &[140.0]),
                ("trail", &[4.0]),
            ],
        )
        .draw(|b| {
            let mut hot = b.bright(
                b.src(),
                b.k("threshold"),
                b.k("soft") * 0.5,
                b.k("saturation"),
                1.0,
            );
            let t = b.len("trail");
            if t > 0.0 {
                let a = b.a("angle");
                hot = b.smear(hot, [a.cos() * t, a.sin() * t], 0.4, 0);
            }
            let (s, st) = (b.len("size"), b.k("stretch"));
            let near = stretched(b, hot, s, st, 0);
            let far = b.blur(near, b.len("rimSize"), 0);
            let n = nums![b.k("brightness"), b.color("color"), 0.0, b.k("rim") * 3.0];
            b.run(k::GLOW_TUBE, &[b.src(), near, far], n, [1, 0, 0, 0])
        }),
        glow_end(
            glow_settings(
                fx(
                    "RainbowGlow",
                    "Rainbow Glow",
                    "A glow whose colors go round the rainbow.",
                ),
                5.0,
                120.0,
                WHITE,
            )
            .num("times", "Rainbows", 1.0, 0.2, 5.0)
            .num("phase", "Phase", 0.0, -1.0, 1.0)
            .num("speed", "Speed", 0.0, -2.0, 2.0)
            .pct("vivid", "Vividness", 85.0),
        )
        .moving()
        .preset("Prism", &[])
        .preset(
            "Disco",
            &[("times", &[2.0]), ("speed", &[0.5]), ("threshold", &[40.0])],
        )
        .draw(|b| {
            let g = halo(b);
            let n = nums![
                b.k("brightness"),
                b.color("color"),
                b.k("keep"),
                b.n("times"),
                b.n("phase") + b.run_time("speed"),
                b.k("vivid"),
            ];
            b.run(k::GLOW_RAINBOW, &[b.src(), g], n, [b.i("blend"), 0, 0, 0])
        }),
        glow_end(
            glow_settings(
                fx(
                    "RingGlow",
                    "Ring Glow",
                    "Rings of light round the bright parts.",
                ),
                5.0,
                140.0,
                [0.9, 0.95, 1.0],
            )
            .num("rings", "Rings", 5.0, 0.5, 20.0)
            .num("speed", "Speed", 0.3, -3.0, 3.0)
            .within("sharp", "Sharpness", 2.0, 0.5, 10.0)
            .within("spread", "Color Spread", 30.0, 0.0, 100.0),
        )
        .moving()
        .preset(
            "Halo",
            &[("rings", &[2.0]), ("sharp", &[4.0]), ("speed", &[0.0])],
        )
        .preset(
            "Radio Waves",
            &[("rings", &[8.0]), ("sharp", &[3.0]), ("speed", &[1.0])],
        )
        .draw(|b| {
            let g = halo(b);
            let n = nums![
                b.k("brightness"),
                b.color("color"),
                b.k("keep"),
                b.n("rings"),
                b.run_time("speed"),
                b.n("sharp"),
                b.k("spread") * 1.5,
            ];
            b.run(k::GLOW_RINGS, &[b.src(), g], n, [b.i("blend"), 0, 0, 0])
        }),
        fx(
            "Halation",
            "Halation",
            "The red and orange light that film scatters round highlights.",
        )
        .num("amount", "Amount", 55.0, 0.0, 300.0)
        .num("size", "Size", 0.9, 0.1, 10.0)
        .color("color", "Color", [1.0, 0.28, 0.1])
        .pct("threshold", "Threshold", 72.0)
        .pct("soft", "Softness", 25.0)
        .pct("outside", "Only Outside", 85.0)
        .hint("Keeps it off the highlights themselves.")
        .preset("35 mm", &[("size", &[0.6]), ("amount", &[60.0])])
        .preset("Night Film", &[("size", &[1.8]), ("amount", &[150.0])])
        .preset("Green Print", &[("color", &[0.3, 1.0, 0.35])])
        .draw(|b| {
            let hot = b.bright(b.src(), b.k("threshold"), b.k("soft") * 0.5, 1.0, 1.0);
            let a = b.blur(hot, b.len("size"), 0);
            let c = b.blur(a, b.len("size") * 2.2, 0);
            let g = b.run(k::SUM3, &[a, c], nums![0.7, 0.6, 0.0], [0; 4]);
            let n = nums![b.k("amount"), b.color("color"), 0.0, b.k("outside")];
            b.run(k::HALATION, &[b.src(), g, hot], n, [0; 4])
        }),
        glow_end(
            glow_settings(
                fx(
                    "GradualGlow",
                    "Gradual Glow",
                    "A glow that is strong at one place and fades toward another.",
                ),
                5.0,
                120.0,
                WHITE,
            )
            .point("from", "Strong End", [0.5, 0.9])
            .point("to", "Gone", [0.5, 0.2]),
        )
        .handle(Handle::Line("from", "to"))
        .preset("Bright Sky", &[("threshold", &[40.0])])
        .preset(
            "Wet Floor",
            &[
                ("from", &[0.5, 0.05]),
                ("to", &[0.5, 0.6]),
                ("stretch", &[-60.0]),
            ],
        )
        .draw(|b| {
            let g = halo(b);
            let g = b.run(k::LIGHT_FADE, &[g], nums![b.p("from"), b.p("to")], [0; 4]);
            let n = nums![b.k("brightness"), b.color("color"), b.k("keep")];
            b.run(k::GLOW, &[b.src(), g], n, [b.i("blend"), 0, 0, 0])
        }),
        star(
            fx("StarGlint", "Star Glint", "Stars on the bright points."),
            2,
            0.0,
        )
        .preset("Four Points", &[])
        .preset("Six Points", &[("lines", &[3.0]), ("angle", &[30.0])])
        .preset(
            "Diamond",
            &[
                ("lines", &[4.0]),
                ("odd", &[40.0]),
                ("length", &[30.0]),
                ("threshold", &[85.0]),
            ],
        )
        .preset(
            "Filter 1980",
            &[
                ("angle", &[45.0]),
                ("length", &[35.0]),
                ("brightness", &[160.0]),
            ],
        )
        .draw(draw_star),
        star(
            fx(
                "RainbowGlint",
                "Rainbow Glint",
                "Stars whose points run through the rainbow.",
            ),
            3,
            90.0,
        )
        .preset("Grating", &[])
        .preset("Crystal", &[("lines", &[6.0]), ("length", &[14.0])])
        .draw(draw_star),
        star(
            fx(
                "Glare",
                "Glare",
                "Stars, rainbow points and a halo together, as bright light on a lens.",
            ),
            4,
            35.0,
        )
        .section("haloes", "Halo", true)
        .num("halo", "Halo", 60.0, 0.0, 300.0)
        .num("haloSize", "Halo Size", 4.0, 0.2, 20.0)
        .color("haloColor", "Halo Color", [1.0, 0.9, 0.8])
        .end()
        .preset("Sun on Water", &[("threshold", &[80.0])])
        .preset(
            "Headlights",
            &[
                ("lines", &[6.0]),
                ("odd", &[50.0]),
                ("halo", &[120.0]),
                ("haloSize", &[8.0]),
            ],
        )
        .draw(|b| {
            let o = draw_star(b);
            let hot = b.bright(b.src(), b.k("threshold"), b.k("soft") * 0.5, 1.0, 1.0);
            let g = halo_of(b, hot, b.len("haloSize"), 0.0);
            let n = nums![b.k("halo"), b.color("haloColor"), 1.0];
            b.run(k::GLOW, &[o, g], n, [b.i("blend"), 0, 0, 0])
        }),
        fx(
            "AnamorphicStreaks",
            "Anamorphic Streaks",
            "Long lines of light from the bright points, as an anamorphic lens's.",
        )
        .num("brightness", "Brightness", 130.0, 0.0, 400.0)
        .color("color", "Color", [0.4, 0.65, 1.0])
        .num("length", "Reach", 30.0, 1.0, 150.0)
        .num("width", "Width", 0.15, 0.02, 2.0)
        .angle("angle", "Angle", 0.0)
        .pct("threshold", "Threshold", 78.0)
        .pct("soft", "Softness", 20.0)
        .pct("core", "White Core", 25.0)
        .pct("points", "Points Only", 80.0)
        .hint("Wide bright areas give no lines, small lights do.")
        .blend(1)
        .preset("Blue Lines", &[])
        .preset("Amber Lines", &[("color", &[1.0, 0.62, 0.25])])
        .preset(
            "Upright",
            &[("angle", &[90.0]), ("color", &[1.0, 0.95, 0.85])],
        )
        .draw(|b| {
            let hot = b.bright(b.src(), b.k("threshold"), b.k("soft") * 0.5, 0.35, 1.0);
            let hot = points(b, hot, b.k("points"));
            let a = b.a("angle");
            let l = b.len("length");
            let thin = b.blur(hot, b.len("width"), 0);
            // Three lengths one over another: bright near, faint far.
            let dir = |m: f32| [a.cos() * l * m, a.sin() * l * m];
            let s1 = b.smear(thin, dir(0.3), 0.0, 0);
            let s2 = b.smear(s1, dir(0.8), 0.0, 0);
            let s3 = b.smear(s2, dir(1.6), 0.0, 0);
            let g = b.run(k::SUM3, &[s1, s2, s3], nums![1.6, 1.8, 1.8], [0; 4]);
            let c = b.color("color");
            let w = b.k("core");
            let o = b.run(
                k::GLOW,
                &[b.src(), g],
                nums![b.k("brightness"), c, 1.0],
                [b.i("blend"), 0, 0, 0],
            );
            b.run(
                k::GLOW,
                &[o, s1],
                nums![b.k("brightness") * w * 1.5, [1.0; 3], 1.0],
                [b.i("blend"), 0, 0, 0],
            )
        }),
        rays(
            fx(
                "LightRays",
                "Light Shafts",
                "Shafts of light from a place, through what is bright.",
            ),
            [0.5, 0.75],
            62.0,
        )
        .preset("Window", &[])
        .preset(
            "Forest",
            &[
                ("color", &[1.0, 0.93, 0.7]),
                ("far", &[0.75, 0.9, 0.5]),
                ("shimmer", &[60.0]),
                ("threshold", &[55.0]),
            ],
        )
        .preset(
            "Concert",
            &[
                ("color", &[0.7, 0.4, 1.0]),
                ("far", &[0.2, 0.4, 1.0]),
                ("shimmer", &[100.0]),
                ("speed", &[1.2]),
            ],
        )
        .preset(
            "Light Text",
            &[
                ("threshold", &[15.0]),
                ("length", &[60.0]),
                ("brightness", &[180.0]),
            ],
        )
        .draw(|b| {
            let hot = b.bright(
                b.src(),
                b.k("threshold"),
                b.k("soft") * 0.5,
                b.k("saturation"),
                1.0,
            );
            draw_rays(b, hot, 1.0)
        }),
        rays(
            fx(
                "EdgeRays",
                "Edge Rays",
                "Shafts of light from the outlines.",
            ),
            [0.5, 0.65],
            6.0,
        )
        .num("strength", "Edge Strength", 250.0, 0.0, 600.0)
        .preset("Outlined", &[])
        .preset(
            "Power Up",
            &[
                ("color", &[0.5, 0.9, 1.0]),
                ("far", &[0.1, 0.3, 1.0]),
                ("brightness", &[180.0]),
                ("length", &[35.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.k("strength") * 4.0,
                [1.0; 3],
                b.k("saturation"),
                b.k("threshold") * 0.5,
                (b.at.scale * 2.0).max(0.7)
            ];
            let e = b.run(k::NEON, &[b.src()], n, [0; 4]);
            // Soft outlines shed soft shafts.
            let e = b.blur(e, b.k("soft") * 0.012, 0);
            draw_rays(b, e, 0.9)
        }),
        fx(
            "LensFlare",
            "Camera Flare",
            "A light's flare: its star, halo, streak and ghosts.",
        )
        .point("light", "Light", [0.25, 0.75])
        .point("pivot", "Lens Middle", [0.5, 0.5])
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .color("color", "Color", [1.0, 0.93, 0.82])
        .num("size", "Size", 100.0, 10.0, 300.0)
        .section("parts", "Parts", true)
        .num("core", "Core", 100.0, 0.0, 300.0)
        .num("glow", "Glow", 100.0, 0.0, 300.0)
        .num("star", "Star", 100.0, 0.0, 300.0)
        .int("points", "Star Points", 12, 2, 32)
        .num("halo", "Halo Ring", 80.0, 0.0, 300.0)
        .num("streak", "Streak", 50.0, 0.0, 300.0)
        .num("ghosts", "Ghosts", 100.0, 0.0, 300.0)
        .int("ghostCount", "Ghost Count", 9, 0, 16)
        .pct("ghostColor", "Ghost Colors", 65.0)
        .angle("turn", "Turn", 0.0)
        .seed()
        .end()
        .blend(1)
        .handle(Handle::Line("light", "pivot"))
        .preset("Sun", &[])
        .preset(
            "Anamorphic",
            &[
                ("streak", &[250.0]),
                ("star", &[20.0]),
                ("halo", &[20.0]),
                ("color", &[0.75, 0.86, 1.0]),
            ],
        )
        .preset(
            "Cheap Lens",
            &[
                ("ghosts", &[250.0]),
                ("ghostCount", &[14.0]),
                ("ghostColor", &[100.0]),
            ],
        )
        .preset(
            "Clean Star",
            &[
                ("ghosts", &[0.0]),
                ("halo", &[0.0]),
                ("points", &[6.0]),
                ("star", &[200.0]),
            ],
        )
        .preset(
            "Spaceship",
            &[
                ("color", &[0.6, 0.8, 1.0]),
                ("streak", &[300.0]),
                ("ghosts", &[60.0]),
                ("glow", &[160.0]),
            ],
        )
        .draw(|b| {
            let f = flare(b, b.src(), false, 0.0);
            b.run(
                k::GLOW,
                &[b.src(), f],
                nums![b.k("brightness"), [1.0; 3], 1.0],
                [b.i("blend"), 0, 0, 0],
            )
        }),
        fx(
            "AutoFlare",
            "Auto Flare",
            "A lens flare that finds the picture's brightest light and follows it.",
        )
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .color("color", "Color", [1.0, 0.93, 0.82])
        .num("size", "Size", 90.0, 10.0, 300.0)
        .pct("threshold", "Least Light", 70.0)
        .hint("A light dimmer than this gives no flare.")
        .within("focus", "Focus", 60.0, 10.0, 100.0)
        .hint("How much the very brightest counts over the rest.")
        .point("pivot", "Lens Middle", [0.5, 0.5])
        .section("parts", "Parts", true)
        .num("core", "Core", 40.0, 0.0, 300.0)
        .num("glow", "Glow", 100.0, 0.0, 300.0)
        .num("star", "Star", 100.0, 0.0, 300.0)
        .int("points", "Star Points", 12, 2, 32)
        .num("halo", "Halo Ring", 80.0, 0.0, 300.0)
        .num("streak", "Streak", 60.0, 0.0, 300.0)
        .num("ghosts", "Ghosts", 100.0, 0.0, 300.0)
        .int("ghostCount", "Ghost Count", 9, 0, 16)
        .pct("ghostColor", "Ghost Colors", 65.0)
        .angle("turn", "Turn", 0.0)
        .seed()
        .end()
        .blend(1)
        .preset("Follow the Sun", &[])
        .preset(
            "Follow the Lamp",
            &[
                ("streak", &[200.0]),
                ("color", &[0.7, 0.85, 1.0]),
                ("threshold", &[80.0]),
            ],
        )
        .draw(|b| {
            let n = b.halvings(40);
            let small = b.smaller(b.src(), n, 1);
            let [w, h] = b.size(small);
            let power = 4.0 + b.k("focus") * 28.0;
            let find = b.run_sized(
                k::FIND_LIGHT,
                &[small],
                nums![power],
                [w.min(96) as i32, h.min(54) as i32, 0, 0],
                [1, 1],
                0,
            );
            let f = flare(b, find, true, b.k("threshold"));
            b.run(
                k::GLOW,
                &[b.src(), f],
                nums![b.k("brightness"), [1.0; 3], 1.0],
                [b.i("blend"), 0, 0, 0],
            )
        }),
        fx(
            "Spotlight",
            "Spotlight",
            "A lamp's pool of light on the picture, and its beam in the air.",
        )
        .point("lamp", "Lamp", [0.15, 1.1])
        .point("target", "Points At", [0.55, 0.45])
        .num("radius", "Radius", 28.0, 1.0, 120.0)
        .pct("soft", "Softness", 35.0)
        .num("brightness", "Brightness", 120.0, 0.0, 400.0)
        .pct("ambient", "Light Elsewhere", 25.0)
        .color("color", "Color", [1.0, 0.95, 0.85])
        .num("beam", "Beam", 25.0, 0.0, 200.0)
        .pct("dust", "Dust", 40.0)
        .within("squash", "Pool Shape", 100.0, 30.0, 300.0)
        .seed()
        .handle(Handle::Line("lamp", "target"))
        .handle(Handle::Ring("target", "radius"))
        .moving()
        .preset("Stage", &[("ambient", &[8.0]), ("beam", &[60.0])])
        .preset(
            "Interrogation",
            &[
                ("ambient", &[4.0]),
                ("radius", &[20.0]),
                ("soft", &[60.0]),
                ("lamp", &[0.5, 1.2]),
            ],
        )
        .preset(
            "Police",
            &[
                ("color", &[0.75, 0.85, 1.0]),
                ("soft", &[12.0]),
                ("beam", &[90.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.p("lamp"),
                b.p("target"),
                b.len("radius"),
                b.k("soft"),
                b.k("brightness"),
                b.k("ambient"),
                b.color("color"),
                b.k("beam"),
                b.k("squash"),
                b.k("dust"),
            ];
            b.run(k::SPOT, &[b.src()], n, [0; 4])
        }),
        fx(
            "Relight",
            "Relight 3D",
            "Lights the picture from a place, as if its light and dark were relief.",
        )
        .point("lamp", "Lamp", [0.3, 0.7])
        .num("height", "Height", 25.0, 1.0, 100.0)
        .num("reach", "Reach", 60.0, 5.0, 200.0)
        .num("brightness", "Brightness", 140.0, 0.0, 400.0)
        .pct("ambient", "Light Elsewhere", 35.0)
        .color("color", "Color", [1.0, 0.94, 0.82])
        .num("relief", "Relief", 100.0, -400.0, 400.0)
        .num("smooth", "Smoothness", 0.8, 0.05, 6.0)
        .num("shine", "Shine", 20.0, 0.0, 200.0)
        .within("tight", "Shine Tightness", 25.0, 2.0, 120.0)
        .handle(Handle::Point("lamp"))
        .preset(
            "Candle",
            &[
                ("color", &[1.0, 0.7, 0.35]),
                ("reach", &[40.0]),
                ("ambient", &[20.0]),
            ],
        )
        .preset(
            "Wet Surface",
            &[("shine", &[120.0]), ("tight", &[60.0]), ("smooth", &[0.4])],
        )
        .preset("Carved", &[("relief", &[300.0]), ("smooth", &[0.5])])
        .draw(|b| {
            let m = b.blur(b.src(), b.len("smooth"), 1);
            let n = nums![
                b.p("lamp"),
                b.len("height"),
                b.k("relief"),
                b.k("brightness"),
                b.k("ambient"),
                b.k("shine"),
                b.n("tight"),
                b.color("color"),
                b.len("reach"),
            ];
            b.run(k::RELIGHT, &[b.src(), m], n, [0; 4])
        }),
        fx(
            "DropShadow",
            "Drop Shadow",
            "A soft shadow under a picture with transparency.",
        )
        .num("distance", "Distance", 2.0, 0.0, 30.0)
        .angle("angle", "Angle", -45.0)
        .num("size", "Softness", 1.2, 0.0, 15.0)
        .pct("opacity", "Opacity", 70.0)
        .color("color", "Color", [0.0, 0.0, 0.0])
        .toggle("alone", "Shadow Only", false)
        .handle(Handle::Arrow("", "angle"))
        .preset("Near", &[("distance", &[0.8]), ("size", &[0.5])])
        .preset(
            "Floating Dust",
            &[("distance", &[6.0]), ("size", &[5.0]), ("opacity", &[45.0])],
        )
        .preset("Hard", &[("size", &[0.0]), ("opacity", &[100.0])])
        .preset(
            "Glow Round",
            &[
                ("distance", &[0.0]),
                ("size", &[2.5]),
                ("color", &[1.0, 0.85, 0.3]),
                ("opacity", &[100.0]),
            ],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("size"), 0);
            let (d, a) = (b.len("distance"), b.a("angle"));
            let n = nums![[a.cos() * d, a.sin() * d], b.k("opacity"), b.color("color")];
            b.run(
                k::SHADOW,
                &[b.src(), s],
                n,
                [i32::from(b.on("alone")), 0, 0, 0],
            )
        }),
        fx(
            "LightLeak",
            "Light Leak",
            "Light leaking into the camera and over the picture.",
        )
        .num("brightness", "Brightness", 110.0, 0.0, 300.0)
        .choice("kind", "Kind", 0, &["Clouds", "Bands", "Burned Film"])
        .num("size", "Size", 60.0, 5.0, 200.0)
        .angle("angle", "Angle", 20.0)
        .pct("sides", "From the Sides", 70.0)
        .pct("ragged", "Raggedness", 50.0)
        .color("c1", "Color 1", [0.85, 0.06, 0.02])
        .color("c2", "Color 2", [1.0, 0.42, 0.06])
        .color("c3", "Color 3", [1.0, 0.9, 0.5])
        .num("speed", "Speed", 0.6, -4.0, 4.0)
        .num("phase", "Phase", 0.0, -10.0, 10.0)
        .seed()
        .blend(1)
        .moving()
        .preset("Warm Film", &[])
        .preset(
            "Pink Summer",
            &[
                ("c1", &[0.9, 0.1, 0.4]),
                ("c2", &[1.0, 0.45, 0.5]),
                ("c3", &[1.0, 0.85, 0.7]),
            ],
        )
        .preset(
            "Blue Hour",
            &[
                ("c1", &[0.05, 0.1, 0.6]),
                ("c2", &[0.2, 0.5, 1.0]),
                ("c3", &[0.8, 0.95, 1.0]),
            ],
        )
        .preset(
            "Burnt Reel",
            &[
                ("kind", &[2.0]),
                ("size", &[90.0]),
                ("sides", &[0.0]),
                ("speed", &[1.5]),
                ("brightness", &[150.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.len("size"),
                b.a("angle"),
                b.n("phase") + b.run_time("speed"),
                b.k("sides"),
                b.color("c1"),
                b.color("c2"),
                b.color("c3"),
                b.k("ragged"),
            ];
            // Soft light: drawn small.
            let h = b.halvings(240);
            let size = [b.w() >> h, b.h() >> h].map(|n| n.max(8));
            let seed = b.seed_of("seed");
            let l = b.run_sized(k::LEAK, &[b.src()], n, [0, b.i("kind"), 0, 0], size, seed);
            let l = enlarged(b, l);
            let n = nums![b.k("brightness"), [1.0; 3], 1.0];
            b.run(k::GLOW, &[b.src(), l], n, [b.i("blend"), 0, 0, 0])
        }),
        fx(
            "BokehLights",
            "Bokeh Lights",
            "Discs of light out of focus drifting over the picture.",
        )
        .num("brightness", "Brightness", 70.0, 0.0, 300.0)
        .num("size", "Size", 7.0, 0.5, 30.0)
        .pct("count", "How Many", 45.0)
        .int("layers", "Depth", 3, 1, 5)
        .int("blades", "Blades", 6, 0, 10)
        .angle("turn", "Turn", 10.0)
        .color("color", "Color 1", [1.0, 0.75, 0.35])
        .color("color2", "Color 2", [0.4, 0.75, 1.0])
        .pct("rim", "Bright Rims", 40.0)
        .pct("soft", "Softness", 10.0)
        .pct("twinkle", "Twinkle", 30.0)
        .num("speed", "Drift", 3.0, 0.0, 30.0)
        .angle("way", "Drift Angle", 60.0)
        .seed()
        .blend(1)
        .moving()
        .preset(
            "Holidays",
            &[("color", &[1.0, 0.3, 0.2]), ("color2", &[0.3, 1.0, 0.4])],
        )
        .preset(
            "City Night",
            &[
                ("color", &[1.0, 0.7, 0.3]),
                ("color2", &[1.0, 0.95, 0.8]),
                ("blades", &[0.0]),
            ],
        )
        .preset(
            "Dream Dust",
            &[
                ("size", &[3.0]),
                ("count", &[70.0]),
                ("soft", &[60.0]),
                ("color", &[1.0, 0.9, 0.95]),
                ("color2", &[0.85, 0.9, 1.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.len("size"),
                b.k("count"),
                b.time() * b.len("speed") / (b.len("size") * 2.4).max(1e-3),
                b.a("way") + std::f32::consts::PI,
                b.n("blades"),
                b.a("turn"),
                b.color("color"),
                b.color("color2"),
                b.k("rim"),
                b.k("twinkle"),
                b.k("soft"),
            ];
            let l = b.run(k::BOKEH, &[b.src()], n, [0, b.i("layers"), 0, 0]);
            let n = nums![b.k("brightness"), [1.0; 3], 1.0];
            b.run(k::GLOW, &[b.src(), l], n, [b.i("blend"), 0, 0, 0])
        }),
        fx(
            "Flashbulbs",
            "Flashbulbs",
            "A crowd's camera flashes over the picture.",
        )
        .num("brightness", "Brightness", 130.0, 0.0, 400.0)
        .num("size", "Size", 5.0, 0.5, 30.0)
        .num("rate", "Rate", 1.2, 0.05, 5.0)
        .num("length", "Flash Length", 0.15, 0.02, 1.0)
        .color("color", "Color", [1.0, 0.98, 0.94])
        .pct("crowd", "Crowd Height", 60.0)
        .hint("How much of the frame, from the bottom, flashes.")
        .num("star", "Star", 60.0, 0.0, 200.0)
        .num("lift", "Room Light", 12.0, 0.0, 100.0)
        .hint("How much each flash lights the whole picture.")
        .seed()
        .blend(1)
        .moving()
        .preset("Red Carpet", &[("rate", &[1.5]), ("size", &[8.0])])
        .preset(
            "Stadium",
            &[
                ("size", &[1.5]),
                ("rate", &[0.6]),
                ("crowd", &[100.0]),
                ("star", &[30.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.len("size"),
                b.n("rate"),
                b.n("length"),
                b.color("color"),
                b.k("crowd"),
                b.k("star"),
            ];
            let l = b.run(k::FLASHES, &[b.src()], n, [0; 4]);
            let g = b.blur(l, 0.12, 0);
            let n = nums![b.k("brightness"), [1.0; 3], 1.0];
            let o = b.run(k::GLOW, &[b.src(), l], n, [b.i("blend"), 0, 0, 0]);
            let n = nums![b.k("brightness") * b.k("lift") * 4.0, [1.0; 3], 1.0];
            b.run(k::GLOW, &[o, g], n, [1, 0, 0, 0])
        }),
        fx(
            "ElectricEdges",
            "Electric Edges",
            "Discharges crawling along the picture's outlines.",
        )
        .num("strength", "Edge Strength", 250.0, 0.0, 800.0)
        .num("spread", "Edge Width", 0.8, 0.1, 6.0)
        .num("width", "Width", 12.0, 1.0, 60.0)
        .num("glow", "Glow Width", 45.0, 2.0, 150.0)
        .num("stray", "Straying", 1.5, 0.0, 10.0)
        .num("fine", "Fineness", 9.0, 1.0, 40.0)
        .num("rate", "Changes a Second", 12.0, 0.0, 30.0)
        .num("driftX", "Move Horizontal", 0.0, -20.0, 20.0)
        .num("driftY", "Move Vertical", 0.0, -20.0, 20.0)
        .color("core", "Core", [1.0, 1.0, 1.0])
        .color("color", "Glow Color", [0.3, 0.6, 1.0])
        .num("brightness", "Brightness", 140.0, 0.0, 400.0)
        .seed()
        .blend(1)
        .moving()
        .preset("Charged", &[])
        .preset(
            "Fire Outline",
            &[
                ("color", &[1.0, 0.35, 0.05]),
                ("core", &[1.0, 0.9, 0.5]),
                ("driftY", &[6.0]),
                ("fine", &[14.0]),
            ],
        )
        .draw(|b| {
            let n = nums![b.k("strength") * 2.0, b.at.scale * 1.5];
            let e = b.run(k::EDGES, &[b.src()], n, [0; 4]);
            let e = b.blur(e, b.len("spread"), 0);
            let n = nums![
                [0.0f32; 2],
                0.0,
                0.0,
                b.k("brightness"),
                1.0,
                b.color("core"),
                b.color("color"),
                [0.0f32; 3],
                b.time(),
                [b.len("driftX") * -10.0, b.len("driftY") * 10.0],
                b.k("width") * 0.35,
                b.k("glow") * 0.35,
                b.len("stray"),
                b.n("fine"),
                b.n("rate"),
            ];
            let l = b.run(k::ARC_EDGES, &[b.src(), e], n, [1, 1, 0, 2]);
            let n = nums![1.0, [1.0; 3], 1.0];
            b.run(k::GLOW, &[b.src(), l], n, [b.i("blend"), 0, 0, 0])
        }),
    ]
}

/// A small picture back at the frame's size, whatever its own.
pub fn enlarged(b: &mut B<'_>, s: Slot) -> Slot {
    let [w, h] = b.size(s);
    if [w, h] == [b.w(), b.h()] {
        return s;
    }
    let f = nums![b.w() as f32 / w as f32, b.h() as f32 / h as f32];
    b.run(k::UP, &[s], f, [1, 0, 0, 0])
}

fn star(fx: Fx, lines: i32, rainbow: f64) -> Fx {
    fx.num("brightness", "Brightness", 120.0, 0.0, 400.0)
        .color("color", "Color", [1.0, 0.97, 0.92])
        .num("length", "Reach", 20.0, 1.0, 80.0)
        .int("lines", "Lines", lines, 1, 8)
        .angle("angle", "Angle", 0.0)
        .within("odd", "Odd Lines", 100.0, 0.0, 200.0)
        .hint("The length of every other line.")
        .within("rainbow", "Rainbow", rainbow, 0.0, 100.0)
        .pct("threshold", "Threshold", 78.0)
        .pct("soft", "Softness", 18.0)
        .within("fade", "Falloff", 2.4, 0.5, 6.0)
        .pct("points", "Points Only", 85.0)
        .hint("Wide bright areas give no stars, small lights do.")
        .blend(1)
}

/// Light's points alone, `only` of the way: wide bright areas give none.
pub fn points(b: &mut B<'_>, s: Slot, only: f32) -> Slot {
    if only <= 0.0 {
        return s;
    }
    let around = b.blur(s, 0.022, 1);
    let n = nums![only * 1.15, 1.0 + only * 0.8];
    b.run_like(s, k::POINTS, &[s, around], n, [0; 4])
}

fn draw_star(b: &mut B<'_>) -> Slot {
    let hot = b.bright(b.src(), b.k("threshold"), b.k("soft") * 0.5, 1.0, 1.0);
    let n = b.halvings(270);
    let small = b.smaller(hot, n, 0);
    let small = points(b, small, b.k("points"));
    let len = b.len("length") * b.size(small)[1] as f32;
    let numbers = nums![len, b.a("angle"), b.n("fade"), b.k("rainbow"), b.k("odd")];
    let samples = ((len * 0.9) as i32).clamp(12, 56);
    let s = b.run_like(
        small,
        k::STAR,
        &[small],
        numbers,
        [b.i("lines"), samples, 0, 0],
    );
    let s = b.larger(s, n, 0);
    let numbers = nums![b.k("brightness"), b.color("color"), 1.0];
    b.run(k::GLOW, &[b.src(), s], numbers, [b.i("blend"), 0, 0, 0])
}

fn rays(fx: Fx, center: [f64; 2], threshold: f64) -> Fx {
    fx.point("center", "Center", center)
        .num("brightness", "Brightness", 130.0, 0.0, 400.0)
        .within("length", "Reach", 70.0, 0.0, 150.0)
        .within("fade", "Falloff", 1.4, 0.2, 5.0)
        .color("color", "Color", [1.0, 0.95, 0.82])
        .color("far", "Far Color", [1.0, 0.82, 0.55])
        .pct("threshold", "Threshold", threshold)
        .pct("soft", "Softness", 25.0)
        .within("saturation", "Saturation", 60.0, 0.0, 200.0)
        .section("shimmering", "Shimmer", true)
        .num("shimmer", "Amount", 30.0, 0.0, 100.0)
        .num("fine", "Fineness", 12.0, 2.0, 40.0)
        .num("speed", "Speed", 0.4, -3.0, 3.0)
        .seed()
        .end()
        .blend(1)
        .handle(Handle::Point("center"))
        .moving()
}

/// `hot`'s shafts, `gain` times as bright.
fn draw_rays(b: &mut B<'_>, hot: Slot, gain: f32) -> Slot {
    let n = b.halvings(360);
    let small = b.smaller(hot, n, 0);
    let c = b.p("center");
    let len = b.k("length");
    let numbers = nums![
        c,
        len,
        b.n("fade"),
        b.k("shimmer") * 1.2,
        b.n("fine"),
        b.n("speed"),
        b.color("color"),
        b.color("far"),
    ];
    let a = b.run_like(small, k::RAYS, &[small], numbers, [0, 40, 0, 0]);
    // A second, short step takes the first's samples away.
    let short = nums![
        c,
        len / 40.0 * 1.2,
        0.2,
        0.0,
        1.0,
        0.0,
        [1.0f32; 3],
        [1.0f32; 3]
    ];
    let r = b.run_like(small, k::RAYS, &[a], short, [0, 8, 0, 0]);
    let r = b.larger(r, n, 0);
    let numbers = nums![b.k("brightness") * 1.25 * gain, [1.0; 3], 1.0];
    b.run(k::GLOW, &[b.src(), r], numbers, [b.i("blend"), 0, 0, 0])
}

/// A flare's light, its place the settings' or `find`'s.
fn flare(b: &mut B<'_>, find: Slot, found: bool, least: f32) -> Slot {
    let at = if found { [0.0; 2] } else { b.p("light") };
    let n = nums![
        at,
        b.p("pivot"),
        b.k("size"),
        b.color("color"),
        b.k("core"),
        b.k("star"),
        b.k("halo"),
        b.k("streak"),
        b.k("ghosts"),
        b.k("ghostColor"),
        b.a("turn"),
        b.k("glow"),
        least,
    ];
    let ints = [b.i("points"), b.i("ghostCount"), i32::from(found), 0];
    b.run(k::FLARE, &[b.src(), find], n, ints)
}
