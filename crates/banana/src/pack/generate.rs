//! Generated pictures: over the picture as an effect, alone as a media.

use crate::bolt::{self, Bolts, Guide};
use crate::fx::{B, Fx, Handle, N, Put};
use crate::generated::k;
use crate::graph::{Kernel, Slot};
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::generator(id, name, about)
}

/// How a pattern is see-through.
const SEE: &[&str] = &["Opaque", "By Its Light", "By Its Pattern"];

/// What every pattern has: its place, size, turn and speed.
fn place(fx: Fx, size: f64, speed: f64) -> Fx {
    still(fx, size)
        .num("speed", "Speed", speed, -4.0, 4.0)
        .num("phase", "Phase", 0.0, -10.0, 10.0)
        .moving()
}

/// What one that does not move by itself has.
fn still(fx: Fx, size: f64) -> Fx {
    fx.point("center", "Position", [0.5, 0.5])
        .num("size", "Size", size, 1.0, size.max(25.0) * 4.0)
        .angle("turn", "Turn", 0.0)
        .handle(Handle::Point("center"))
}

/// And its colors.
fn dye(fx: Fx, a: [f64; 3], b: [f64; 3]) -> Fx {
    fx.color("c1", "Color 1", a)
        .color("c2", "Color 2", b)
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .within("contrast", "Contrast", 100.0, 0.0, 500.0)
        .choice("see", "Transparency", 0, SEE)
}

fn dye3(fx: Fx, a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Fx {
    fx.color("c1", "Color 1", a)
        .color("c2", "Color 2", b)
        .color("c3", "Color 3", c)
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .within("contrast", "Contrast", 100.0, 0.0, 500.0)
        .choice("see", "Transparency", 0, SEE)
}

/// The numbers f0 to f15 the patterns share.
fn common(b: &B<'_>) -> N {
    let c3 = if b.has("c3") { b.color("c3") } else { [0.0; 3] };
    let t = if b.has("phase") {
        b.n("phase") + b.run_time("speed")
    } else {
        0.0
    };
    nums![
        b.p("center"),
        b.len("size").max(0.002),
        b.a("turn"),
        b.k("brightness"),
        b.k("contrast"),
        b.color("c1"),
        b.color("c2"),
        c3,
        t,
    ]
}

/// A pattern's kernel: its own numbers after the shared ones.
fn pattern(b: &mut B<'_>, kernel: Kernel, own: &[f32], layers: i32, kind: i32) -> Slot {
    let mut n = common(b);
    for v in own {
        v.put(&mut n);
    }
    let colors = if b.has("c3") { 3 } else { 2 };
    let see = if b.has("see") { b.i("see") } else { 0 };
    b.run(kernel, &[b.src()], n, [layers, see, kind, colors])
}

/// Light: the shared numbers with the color where the kernels read it.
fn light(b: &mut B<'_>, kernel: Kernel, own: &[f32], layers: i32) -> Slot {
    let c2 = if b.has("color2") {
        b.color("color2")
    } else {
        [0.0; 3]
    };
    let at = if b.has("center") {
        b.p("center")
    } else {
        [0.0; 2]
    };
    let mut n = nums![
        at,
        b.len("size").max(0.0005),
        0.0,
        b.k("brightness"),
        1.0,
        [0.0f32; 3],
        b.color("color"),
        c2,
        b.n("phase") + b.run_time("speed"),
    ];
    for v in own {
        v.put(&mut n);
    }
    b.run(kernel, &[b.src()], n, [layers, 1, 0, 2])
}

const DARK: [f64; 3] = [0.0, 0.0, 0.0];
const WHITE: [f64; 3] = [1.0, 1.0, 1.0];

pub fn list() -> Vec<Fx> {
    let mut v = skies();
    v.extend(patterns());
    v.extend(shapes());
    v.extend(energy());
    v
}

fn skies() -> Vec<Fx> {
    vec![
        dye(
            place(
                fx("Clouds", "Clouds", "Clouds that drift and billow."),
                45.0,
                1.0,
            )
            .within("billow", "Billows", 60.0, 0.0, 100.0)
            .int("detail", "Detail", 6, 1, 8)
            .num("windX", "Wind Horizontal", 0.1, -2.0, 2.0)
            .num("windY", "Wind Vertical", 0.0, -2.0, 2.0)
            .seed(),
            [0.13, 0.32, 0.62],
            WHITE,
        )
        .preset("Blue Sky", &[])
        .preset(
            "Storm",
            &[
                ("c1", &[0.05, 0.06, 0.08]),
                ("c2", &[0.55, 0.58, 0.64]),
                ("contrast", &[150.0]),
                ("speed", &[2.0]),
            ],
        )
        .preset(
            "Fog Over",
            &[
                ("c1", &[0.0, 0.0, 0.0]),
                ("c2", &[0.85, 0.87, 0.9]),
                ("see", &[2.0]),
                ("size", &[70.0]),
                ("contrast", &[130.0]),
            ],
        )
        .preset(
            "Sunset",
            &[("c1", &[0.35, 0.1, 0.3]), ("c2", &[1.0, 0.66, 0.36])],
        )
        .draw(|b| {
            let own = [-b.n("windX"), b.n("windY"), b.k("billow")];
            pattern(b, k::CLOUDS, &own, b.i("detail"), 0)
        }),
        dye3(
            place(
                fx(
                    "ColorClouds",
                    "Color Clouds",
                    "Clouds of three colors, each going its own way.",
                ),
                45.0,
                0.6,
            )
            .int("detail", "Detail", 5, 1, 8)
            .num("windX", "Wind Horizontal", 0.0, -2.0, 2.0)
            .num("windY", "Wind Vertical", 0.0, -2.0, 2.0)
            .seed(),
            [0.95, 0.2, 0.35],
            [0.15, 0.75, 0.6],
            [0.2, 0.35, 1.0],
        )
        .preset("Aurora Silk", &[])
        .preset(
            "Pastel",
            &[
                ("c1", &[0.9, 0.55, 0.6]),
                ("c2", &[0.5, 0.75, 0.6]),
                ("c3", &[0.5, 0.6, 0.95]),
                ("contrast", &[70.0]),
            ],
        )
        .draw(|b| {
            let own = [-b.n("windX"), b.n("windY")];
            pattern(b, k::CLOUDS_COLOR, &own, b.i("detail"), 0)
        }),
        dye3(
            place(
                fx(
                    "Nebula",
                    "Nebula",
                    "Gas lit from inside, with stars through it.",
                ),
                60.0,
                0.5,
            )
            .int("detail", "Detail", 6, 1, 8)
            .pct("stars", "Stars", 60.0)
            .num("windX", "Drift Horizontal", 0.02, -1.0, 1.0)
            .num("windY", "Drift Vertical", 0.0, -1.0, 1.0)
            .seed(),
            [0.02, 0.01, 0.06],
            [0.35, 0.12, 0.62],
            [1.0, 0.62, 0.45],
        )
        .preset("Rose", &[])
        .preset(
            "Teal Cradle",
            &[("c2", &[0.05, 0.4, 0.48]), ("c3", &[1.0, 0.75, 0.35])],
        )
        .preset(
            "Red Giant",
            &[("c2", &[0.5, 0.06, 0.04]), ("c3", &[1.0, 0.8, 0.3])],
        )
        .draw(|b| {
            let own = [-b.n("windX"), b.n("windY"), b.k("stars")];
            pattern(b, k::NEBULA, &own, b.i("detail"), 0)
        }),
        dye3(
            place(
                fx(
                    "CloudCeiling",
                    "Cloud Ceiling",
                    "A sky of clouds going away to the horizon.",
                ),
                30.0,
                1.0,
            )
            .within("horizon", "Horizon", 30.0, 0.0, 100.0)
            .hint("Where the horizon is, from the frame's bottom.")
            .within("near", "Height", 50.0, 5.0, 200.0)
            .int("detail", "Detail", 6, 1, 8)
            .num("windX", "Wind Horizontal", 0.0, -2.0, 2.0)
            .num("windY", "Wind Toward", 0.3, -2.0, 2.0)
            .seed(),
            [0.12, 0.3, 0.6],
            WHITE,
            [0.75, 0.82, 0.9],
        )
        .preset("Day", &[])
        .preset(
            "Dusk",
            &[
                ("c1", &[0.15, 0.1, 0.3]),
                ("c2", &[0.95, 0.5, 0.35]),
                ("c3", &[1.0, 0.72, 0.4]),
            ],
        )
        .preset(
            "Cloud Sea",
            &[("horizon", &[70.0]), ("c1", &[0.3, 0.4, 0.55])],
        )
        .draw(|b| {
            let own = [
                -b.n("windX"),
                -b.n("windY"),
                0.5 - b.k("horizon"),
                b.k("near"),
            ];
            pattern(b, k::CLOUD_SKY, &own, b.i("detail"), 0)
        }),
        place(
            fx(
                "ColorTrip",
                "Color Trip",
                "Clouds whose colors turn round the rainbow.",
            ),
            45.0,
            0.8,
        )
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .within("contrast", "Contrast", 100.0, 0.0, 300.0)
        .num("times", "Rainbows", 1.6, 0.2, 6.0)
        .pct("vivid", "Vividness", 85.0)
        .int("detail", "Detail", 4, 1, 8)
        .num("windX", "Drift Horizontal", 0.0, -2.0, 2.0)
        .num("windY", "Drift Vertical", 0.0, -2.0, 2.0)
        .seed()
        .preset("Psychedelic", &[])
        .preset(
            "Soap Film",
            &[("times", &[4.0]), ("vivid", &[50.0]), ("size", &[70.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.len("size"),
                b.a("turn"),
                b.k("brightness"),
                b.k("contrast"),
                [0.0f32; 9],
                b.n("phase") + b.run_time("speed"),
                -b.n("windX"),
                b.n("windY"),
                b.n("times"),
                b.k("vivid"),
            ];
            b.run(k::CLOUDS_HUE, &[b.src()], n, [b.i("detail"), 0, 0, 2])
        }),
        dye(
            place(
                fx(
                    "Hurricane",
                    "Hurricane",
                    "A storm's clouds winding round its eye.",
                ),
                35.0,
                0.6,
            )
            .num("wind", "Winding", 3.0, 0.0, 10.0)
            .num("eye", "Eye", 25.0, 0.0, 100.0)
            .int("detail", "Detail", 6, 1, 8)
            .seed(),
            [0.03, 0.07, 0.14],
            [0.95, 0.97, 1.0],
        )
        .preset("From Space", &[])
        .preset(
            "Portal",
            &[
                ("c1", &[0.03, 0.0, 0.1]),
                ("c2", &[0.6, 0.3, 1.0]),
                ("wind", &[7.0]),
                ("eye", &[40.0]),
                ("speed", &[1.5]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.n("wind"), b.k("eye")];
            pattern(b, k::CLOUD_STORM, &own, b.i("detail"), 0)
        }),
        fx("Aurora", "Aurora", "Northern lights swaying in the sky.")
            .point("center", "Position", [0.5, 0.5])
            .num("size", "Size", 40.0, 5.0, 150.0)
            .num("height", "Height", 45.0, 5.0, 120.0)
            .within("base", "Base", 45.0, 0.0, 100.0)
            .hint("Where the curtains hang to, from the frame's bottom.")
            .int("curtains", "Curtains", 3, 1, 6)
            .pct("rays", "Rays", 60.0)
            .num("sway", "Sway", 100.0, 0.0, 300.0)
            .color("c1", "Low Color", [0.15, 1.0, 0.45])
            .color("c2", "Middle", [0.1, 0.7, 0.75])
            .color("c3", "High Color", [0.6, 0.15, 0.85])
            .num("brightness", "Brightness", 110.0, 0.0, 300.0)
            .num("speed", "Speed", 1.0, -4.0, 4.0)
            .num("phase", "Phase", 0.0, -10.0, 10.0)
            .seed()
            .lay(2)
            .moving()
            .preset("Green Night", &[])
            .preset(
                "Magenta",
                &[
                    ("c1", &[1.0, 0.2, 0.55]),
                    ("c2", &[0.65, 0.2, 0.9]),
                    ("c3", &[0.2, 0.2, 0.9]),
                ],
            )
            .preset(
                "Curtains of Fire",
                &[
                    ("c1", &[1.0, 0.7, 0.2]),
                    ("c2", &[1.0, 0.3, 0.05]),
                    ("c3", &[0.6, 0.02, 0.02]),
                    ("rays", &[100.0]),
                ],
            )
            .draw(|b| {
                let n = nums![
                    b.p("center"),
                    b.len("size"),
                    0.0,
                    b.k("brightness"),
                    1.0,
                    b.color("c1"),
                    b.color("c2"),
                    b.color("c3"),
                    b.n("phase") + b.run_time("speed"),
                    0.0,
                    0.0,
                    b.len("height"),
                    b.k("rays"),
                    b.k("sway"),
                    0.5 - b.k("base"),
                ];
                b.run(k::AURORA, &[b.src()], n, [b.i("curtains"), 1, 0, 3])
            }),
        dye(
            place(
                fx(
                    "Caustics",
                    "Caustics",
                    "The dancing light on a pool's floor.",
                ),
                25.0,
                0.6,
            )
            .within("sharp", "Sharpness", 6.0, 1.0, 20.0)
            .seed(),
            [0.0, 0.16, 0.24],
            [0.75, 1.0, 1.0],
        )
        .preset("Pool", &[])
        .preset(
            "Light Over",
            &[
                ("c1", &[0.0, 0.0, 0.0]),
                ("c2", &[0.8, 1.0, 1.0]),
                ("lay", &[2.0]),
            ],
        )
        .preset(
            "Golden Net",
            &[
                ("c1", &[0.1, 0.03, 0.0]),
                ("c2", &[1.0, 0.8, 0.35]),
                ("sharp", &[10.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.n("sharp")];
            pattern(b, k::CAUSTICS, &own, 1, 0)
        }),
        fx(
            "NightSky",
            "Night Sky",
            "Stars that twinkle, and the milky way.",
        )
        .point("center", "Position", [0.5, 0.5])
        .num("size", "Star Size", 100.0, 20.0, 300.0)
        .angle("turn", "Turn", 0.0)
        .pct("count", "How Many", 45.0)
        .pct("twinkle", "Twinkle", 40.0)
        .pct("points", "Star Points", 30.0)
        .pct("way", "Milky Way", 50.0)
        .angle("wayTurn", "Milky Way Angle", 35.0)
        .color("c1", "Sky", [0.01, 0.015, 0.05])
        .color("c2", "Stars", [1.0, 1.0, 1.0])
        .color("c3", "Milky Way Color", [0.55, 0.6, 0.9])
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .choice("see", "Transparency", 0, &["Opaque", "By Its Light"])
        .num("speed", "Turning", 0.0, -20.0, 20.0)
        .num("phase", "Phase", 0.0, -100.0, 100.0)
        .seed()
        .moving()
        .preset("Desert Night", &[])
        .preset("Time-Lapse", &[("speed", &[3.0]), ("twinkle", &[0.0])])
        .preset(
            "Only Stars",
            &[("see", &[1.0]), ("way", &[0.0]), ("lay", &[2.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.k("size"),
                b.a("turn"),
                b.k("brightness"),
                1.0,
                b.color("c1"),
                b.color("c2"),
                b.color("c3"),
                b.n("phase") + b.run_time("speed"),
                0.0,
                0.0,
                b.k("count"),
                b.k("twinkle"),
                b.k("way"),
                b.a("wayTurn"),
                b.k("points"),
            ];
            b.run(k::STARS, &[b.src()], n, [1, b.i("see"), 0, 3])
        }),
        fx(
            "Moon",
            "Moon",
            "A moon with its seas and craters, at any phase.",
        )
        .point("center", "Position", [0.72, 0.68])
        .num("size", "Radius", 14.0, 1.0, 60.0)
        .pct("full", "Phase", 70.0)
        .hint("From a new moon (0) to a full one (100).")
        .angle("light", "Light From", 20.0)
        .angle("turn", "Turn", 0.0)
        .pct("seas", "Seas", 80.0)
        .pct("craters", "Craters", 70.0)
        .pct("earth", "Earthshine", 40.0)
        .num("glow", "Glow", 70.0, 0.0, 300.0)
        .color("color", "Color", [1.0, 0.97, 0.9])
        .color("glowColor", "Glow Color", [0.7, 0.8, 1.0])
        .num("brightness", "Brightness", 100.0, 0.0, 300.0)
        .num("speed", "Rotation", 0.0, -5.0, 5.0)
        .seed()
        .handle(Handle::Point("center"))
        .handle(Handle::Ring("center", "size"))
        .moving()
        .preset("Gibbous", &[])
        .preset("Crescent", &[("full", &[25.0]), ("earth", &[80.0])])
        .preset(
            "Blood Moon",
            &[
                ("full", &[100.0]),
                ("color", &[0.9, 0.32, 0.18]),
                ("glowColor", &[0.8, 0.2, 0.1]),
            ],
        )
        .preset(
            "Harvest",
            &[
                ("full", &[100.0]),
                ("size", &[24.0]),
                ("color", &[1.0, 0.8, 0.45]),
                ("glowColor", &[1.0, 0.6, 0.3]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.len("size"),
                b.a("turn"),
                b.k("brightness"),
                1.0,
                [0.0f32; 3],
                b.color("color"),
                b.color("glowColor"),
                b.run_time("speed"),
                b.a("light"),
                b.k("full"),
                b.k("seas"),
                b.k("craters"),
                b.k("glow"),
                b.k("earth"),
            ];
            b.run(k::MOON, &[b.src()], n, [1, 1, 0, 2])
        }),
        fx("RainSnow", "Rain & Snow", "Rain or snow falling in layers.")
            .choice("what", "What Falls", 0, &["Rain", "Snow"])
            .num("size", "Size", 0.8, 0.1, 6.0)
            .pct("count", "How Many", 55.0)
            .num("speed", "Speed", 1.2, 0.05, 6.0)
            .angle("angle", "Slant", 6.0)
            .num("sway", "Sway", 0.0, 0.0, 200.0)
            .int("layers", "Depth", 5, 1, 7)
            .color("color", "Color", [0.85, 0.9, 1.0])
            .num("brightness", "Brightness", 55.0, 0.0, 200.0)
            .num("phase", "Phase", 0.0, -10.0, 10.0)
            .seed()
            .lay(2)
            .moving()
            .preset("Rain", &[])
            .preset(
                "Storm",
                &[
                    ("angle", &[22.0]),
                    ("count", &[90.0]),
                    ("speed", &[2.4]),
                    ("brightness", &[70.0]),
                ],
            )
            .preset(
                "Snow",
                &[
                    ("what", &[1.0]),
                    ("speed", &[0.18]),
                    ("size", &[1.4]),
                    ("sway", &[70.0]),
                    ("brightness", &[100.0]),
                    ("angle", &[0.0]),
                ],
            )
            .preset(
                "Blizzard",
                &[
                    ("what", &[1.0]),
                    ("speed", &[1.1]),
                    ("size", &[0.9]),
                    ("angle", &[50.0]),
                    ("count", &[95.0]),
                    ("brightness", &[100.0]),
                ],
            )
            .draw(|b| {
                let long = if b.i("what") == 0 { 1.0 } else { 0.0 };
                let n = nums![
                    [0.0f32; 2],
                    b.len("size"),
                    0.0,
                    b.k("brightness"),
                    1.0,
                    [0.0f32; 3],
                    b.color("color"),
                    [0.0f32; 3],
                    b.n("phase") + b.time(),
                    -b.a("angle"),
                    b.n("speed"),
                    b.k("count"),
                    long,
                    b.k("sway"),
                ];
                b.run(k::FALL, &[b.src()], n, [b.i("layers"), 1, 0, 2])
            }),
    ]
}

fn patterns() -> Vec<Fx> {
    vec![
        dye(
            place(
                fx(
                    "Grid",
                    "Grid",
                    "Lines: squares, triangles, hexagons, or a floor in perspective.",
                ),
                10.0,
                0.0,
            )
            .choice(
                "kind",
                "Kind",
                0,
                &["Squares", "Triangles", "Hexagons", "Perspective"],
            )
            .within("width", "Line Width", 6.0, 0.5, 60.0)
            .pct("soft", "Softness", 0.0)
            .int("major", "Heavy Line Every", 0, 0, 20)
            .within("horizon", "Horizon", 55.0, 0.0, 100.0)
            .hint("For Perspective: where it ends, from the frame's bottom."),
            DARK,
            [0.4, 0.85, 1.0],
        )
        .preset(
            "Blueprint",
            &[
                ("c1", &[0.02, 0.1, 0.3]),
                ("c2", &[0.75, 0.88, 1.0]),
                ("major", &[5.0]),
                ("width", &[4.0]),
                ("size", &[6.0]),
            ],
        )
        .preset(
            "Synthwave Floor",
            &[
                ("kind", &[3.0]),
                ("c2", &[1.0, 0.2, 0.85]),
                ("speed", &[1.0]),
                ("width", &[6.0]),
                ("soft", &[12.0]),
            ],
        )
        .preset("Honeycomb", &[("kind", &[2.0]), ("c2", &[1.0, 0.75, 0.2])])
        .preset("Lines Over", &[("see", &[2.0]), ("width", &[2.0])])
        .draw(|b| {
            let own = [
                0.0,
                0.0,
                b.k("width"),
                b.k("soft"),
                b.n("major"),
                0.5 - b.k("horizon"),
            ];
            pattern(b, k::GRID, &own, 1, b.i("kind"))
        }),
        dye(
            still(
                fx(
                    "Grunge",
                    "Grunge",
                    "Stains, scratches and specks of something worn.",
                ),
                45.0,
            )
            .num("stains", "Stains", 80.0, 0.0, 200.0)
            .num("scratches", "Scratches", 50.0, 0.0, 200.0)
            .num("specks", "Specks", 50.0, 0.0, 200.0)
            .int("detail", "Detail", 6, 1, 8)
            .seed(),
            [0.06, 0.055, 0.05],
            [0.72, 0.68, 0.6],
        )
        .lay(0)
        .preset("Old Wall", &[])
        .preset(
            "Dirt Over",
            &[
                ("c1", &[1.0, 1.0, 1.0]),
                ("c2", &[0.2, 0.16, 0.12]),
                ("lay", &[3.0]),
            ],
        )
        .preset(
            "Scratched Metal",
            &[
                ("stains", &[20.0]),
                ("scratches", &[150.0]),
                ("specks", &[10.0]),
                ("c1", &[0.2, 0.21, 0.23]),
                ("c2", &[0.85, 0.88, 0.9]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("stains"), b.k("scratches"), b.k("specks")];
            pattern(b, k::GRUNGE, &own, b.i("detail"), 0)
        }),
        dye(
            place(
                fx("Cells", "Cells", "Cells, stones, bubbles or crystals."),
                14.0,
                0.5,
            )
            .choice(
                "kind",
                "Kind",
                1,
                &["Bubbles", "Walls", "Stones", "Crystals"],
            )
            .pct("uneven", "Unevenness", 90.0)
            .within("walls", "Wall Width", 15.0, 0.0, 100.0)
            .seed(),
            [0.02, 0.1, 0.1],
            [0.5, 1.0, 0.75],
        )
        .preset("Under the Microscope", &[])
        .preset(
            "Cobblestones",
            &[
                ("kind", &[2.0]),
                ("c1", &[0.08, 0.07, 0.07]),
                ("c2", &[0.65, 0.6, 0.55]),
                ("speed", &[0.0]),
                ("walls", &[25.0]),
            ],
        )
        .preset(
            "Crystal",
            &[
                ("kind", &[3.0]),
                ("c1", &[0.1, 0.05, 0.3]),
                ("c2", &[0.9, 0.8, 1.0]),
                ("speed", &[0.0]),
            ],
        )
        .preset(
            "Bubbles",
            &[
                ("kind", &[0.0]),
                ("c1", &[0.0, 0.2, 0.4]),
                ("c2", &[0.9, 1.0, 1.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("uneven"), b.k("walls")];
            pattern(b, k::CELLS, &own, 1, b.i("kind"))
        }),
        dye(
            place(
                fx("Spiral", "Color Spiral", "A spiral of colors that turns."),
                30.0,
                0.4,
            )
            .int("arms", "Arms", 2, 0, 24)
            .within("tight", "Tightness", 2.0, -10.0, 10.0)
            .choice("kind", "Colors", 0, &["Rainbow", "Two Colors"])
            .pct("vivid", "Vividness", 85.0),
            DARK,
            WHITE,
        )
        .preset("Rainbow", &[])
        .preset(
            "Hypnosis",
            &[
                ("kind", &[1.0]),
                ("contrast", &[500.0]),
                ("arms", &[1.0]),
                ("tight", &[4.0]),
            ],
        )
        .preset(
            "Candy",
            &[
                ("kind", &[1.0]),
                ("arms", &[8.0]),
                ("tight", &[1.0]),
                ("c1", &[0.9, 0.1, 0.2]),
                ("contrast", &[400.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.n("arms"), b.n("tight"), b.k("vivid")];
            pattern(b, k::SPIRAL, &own, 1, b.i("kind"))
        }),
        dye3(
            place(
                fx("Flux", "Flux", "A liquid folding into itself."),
                40.0,
                0.6,
            )
            .num("fold", "Folding", 100.0, 0.0, 250.0)
            .int("detail", "Detail", 4, 1, 7)
            .seed(),
            [0.02, 0.05, 0.16],
            [0.1, 0.45, 0.7],
            [1.0, 0.85, 0.5],
        )
        .preset("Marble Ink", &[])
        .preset(
            "Lava",
            &[
                ("c1", &[0.06, 0.0, 0.0]),
                ("c2", &[0.7, 0.08, 0.0]),
                ("c3", &[1.0, 0.85, 0.25]),
                ("contrast", &[140.0]),
            ],
        )
        .preset(
            "Oil",
            &[
                ("c1", &[0.05, 0.02, 0.1]),
                ("c2", &[0.1, 0.5, 0.45]),
                ("c3", &[0.9, 0.4, 0.75]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("fold") * 0.75];
            pattern(b, k::FLUX, &own, b.i("detail"), 0)
        }),
        dye(
            place(
                fx("Silk", "Silk", "Cloth in folds, with its sheen."),
                40.0,
                0.4,
            )
            .num("deep", "Depth", 100.0, 0.0, 300.0)
            .within("lean", "Lean", 20.0, -200.0, 200.0)
            .seed(),
            [0.25, 0.0, 0.06],
            [1.0, 0.45, 0.5],
        )
        .preset("Red Curtain", &[])
        .preset(
            "Pearl",
            &[("c1", &[0.4, 0.4, 0.45]), ("c2", &[1.0, 0.98, 0.95])],
        )
        .preset(
            "Midnight Satin",
            &[("c1", &[0.0, 0.02, 0.1]), ("c2", &[0.35, 0.5, 0.9])],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("deep"), b.k("lean")];
            pattern(b, k::SILK, &own, 1, 0)
        }),
        dye(
            place(
                fx("Contours", "Contours", "Lines of equal height, as a map's."),
                45.0,
                0.4,
            )
            .num("lines", "Lines", 12.0, 1.0, 40.0)
            .within("width", "Line Width", 12.0, 1.0, 100.0)
            .int("detail", "Detail", 3, 1, 6)
            .seed(),
            [0.06, 0.1, 0.12],
            [0.7, 0.95, 0.85],
        )
        .preset(
            "Map",
            &[
                ("c1", &[0.95, 0.92, 0.84]),
                ("c2", &[0.55, 0.35, 0.2]),
                ("speed", &[0.0]),
            ],
        )
        .preset("Hologram", &[])
        .preset(
            "Wood",
            &[
                ("size", &[90.0]),
                ("lines", &[30.0]),
                ("detail", &[2.0]),
                ("speed", &[0.0]),
                ("c1", &[0.55, 0.33, 0.16]),
                ("c2", &[0.28, 0.15, 0.07]),
                ("width", &[40.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.n("lines"), b.k("width")];
            pattern(b, k::CONTOURS, &own, b.i("detail"), 0)
        }),
        dye(
            place(
                fx("Fibers", "Fibers", "Fibers and grain, close up."),
                12.0,
                0.0,
            )
            .pct("string", "Stringiness", 75.0)
            .int("detail", "Detail", 4, 1, 7)
            .seed(),
            [0.2, 0.17, 0.13],
            [0.9, 0.86, 0.76],
        )
        .preset("Paper Pulp", &[])
        .preset(
            "Fur",
            &[
                ("string", &[100.0]),
                ("size", &[8.0]),
                ("c1", &[0.12, 0.06, 0.02]),
                ("c2", &[0.75, 0.5, 0.28]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("string")];
            pattern(b, k::FIBERS, &own, b.i("detail"), 0)
        }),
        dye(
            place(
                fx("Moire", "Moiré", "Two sets of rings or lines crossing."),
                100.0,
                0.4,
            )
            .point("other", "Second Center", [0.56, 0.5])
            .choice("kind", "Kind", 0, &["Rings", "Lines"])
            .num("count", "How Many", 24.0, 1.0, 120.0)
            .within("ratio", "Second's Count", 100.0, 50.0, 200.0)
            .angle("between", "Between", 6.0),
            DARK,
            WHITE,
        )
        .handle(Handle::Point("other"))
        .preset("Op Art", &[("contrast", &[300.0])])
        .preset(
            "Screens",
            &[("kind", &[1.0]), ("count", &[50.0]), ("speed", &[0.0])],
        )
        .draw(|b| {
            let o = b.p("other");
            let own = [
                o[0],
                o[1],
                b.n("count") / b.k("size").max(0.01),
                b.k("ratio"),
                b.a("between"),
            ];
            pattern(b, k::MOIRE, &own, 1, b.i("kind"))
        }),
        dye(
            place(
                fx("Nerves", "Nerves", "A net of glowing nerves that pulses."),
                18.0,
                0.5,
            )
            .within("thick", "Thickness", 12.0, 2.0, 60.0)
            .num("pulse", "Pulse", 40.0, 0.0, 100.0)
            .seed(),
            [0.01, 0.0, 0.04],
            [0.45, 0.75, 1.0],
        )
        .preset("Mind", &[])
        .preset(
            "Infection",
            &[
                ("c2", &[0.7, 1.0, 0.2]),
                ("c1", &[0.02, 0.05, 0.0]),
                ("thick", &[22.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("thick"), b.k("pulse")];
            pattern(b, k::NERVES, &own, 1, 0)
        }),
        dye(
            still(
                fx("Relief", "Relief", "A rough wall lit from one side."),
                18.0,
            )
            .choice("kind", "Kind", 0, &["Stone", "Stucco", "Hammered", "Paper"])
            .angle("light", "Light From", 135.0)
            .within("low", "Light Height", 45.0, 5.0, 200.0)
            .num("deep", "Depth", 100.0, -300.0, 300.0)
            .int("detail", "Detail", 6, 1, 8)
            .seed(),
            [0.05, 0.05, 0.05],
            [0.85, 0.82, 0.78],
        )
        .preset("Stone", &[])
        .preset(
            "Beaten Copper",
            &[
                ("kind", &[2.0]),
                ("c1", &[0.15, 0.04, 0.01]),
                ("c2", &[1.0, 0.6, 0.35]),
                ("size", &[8.0]),
            ],
        )
        .preset(
            "Drawing Paper",
            &[
                ("kind", &[3.0]),
                ("c1", &[0.6, 0.6, 0.58]),
                ("c2", &[1.0, 1.0, 0.98]),
                ("size", &[10.0]),
            ],
        )
        .draw(|b| {
            let own = [b.a("light"), b.k("low"), b.k("deep")];
            pattern(b, k::RELIEF, &own, b.i("detail"), b.i("kind"))
        }),
        dye3(
            place(
                fx("PaintStrokes", "Paint Strokes", "Strokes of thick paint."),
                35.0,
                0.3,
            )
            .num("long", "Stroke Length", 100.0, 0.0, 300.0)
            .num("hair", "Bristles", 60.0, 0.0, 200.0)
            .seed(),
            [0.06, 0.12, 0.3],
            [0.75, 0.35, 0.2],
            [1.0, 0.88, 0.6],
        )
        .preset("Canvas", &[])
        .preset(
            "Sea",
            &[
                ("c1", &[0.0, 0.1, 0.2]),
                ("c2", &[0.05, 0.45, 0.55]),
                ("c3", &[0.95, 1.0, 0.98]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("long"), b.k("hair")];
            pattern(b, k::PAINT, &own, 1, 0)
        }),
        dye3(
            place(
                fx("Plasma", "Plasma", "Waves meeting, in three colors."),
                15.0,
                1.0,
            )
            .num("bends", "Bends", 100.0, 10.0, 300.0),
            [0.1, 0.0, 0.3],
            [0.95, 0.15, 0.45],
            [1.0, 0.85, 0.3],
        )
        .preset("Arcade", &[])
        .preset(
            "Deep",
            &[
                ("c1", &[0.0, 0.02, 0.08]),
                ("c2", &[0.0, 0.3, 0.5]),
                ("c3", &[0.5, 1.0, 0.9]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("bends")];
            pattern(b, k::PLASMA, &own, 1, 0)
        }),
        dye(
            place(
                fx("Veins", "Veins", "Marble's veins, ridges, or cracks."),
                35.0,
                0.0,
            )
            .within("sharp", "Sharpness", 4.0, 0.5, 20.0)
            .int("detail", "Detail", 6, 1, 8)
            .seed(),
            [0.95, 0.94, 0.92],
            [0.25, 0.26, 0.3],
        )
        .preset("White Marble", &[("sharp", &[9.0])])
        .preset(
            "Black Marble",
            &[
                ("c1", &[0.03, 0.03, 0.035]),
                ("c2", &[0.85, 0.75, 0.55]),
                ("sharp", &[12.0]),
            ],
        )
        .preset(
            "Electric",
            &[
                ("c1", &[0.0, 0.0, 0.05]),
                ("c2", &[0.6, 0.8, 1.0]),
                ("sharp", &[14.0]),
                ("speed", &[2.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.n("sharp")];
            pattern(b, k::VEINS, &own, b.i("detail"), 0)
        }),
        dye(
            place(
                fx("Spots", "Spots", "Dots in rows, staggered or scattered."),
                9.0,
                0.0,
            )
            .choice("kind", "Kind", 1, &["Rows", "Staggered", "Scattered"])
            .within("dot", "Dot Size", 60.0, 1.0, 140.0)
            .pct("uneven", "Unevenness", 0.0)
            .pct("soft", "Softness", 0.0)
            .seed(),
            [1.0, 0.84, 0.2],
            [0.45, 0.22, 0.05],
        )
        .preset(
            "Polka",
            &[("c1", &[0.9, 0.2, 0.25]), ("c2", &[1.0, 1.0, 1.0])],
        )
        .preset(
            "Ripe Banana",
            &[
                ("kind", &[2.0]),
                ("uneven", &[80.0]),
                ("soft", &[10.0]),
                ("dot", &[40.0]),
            ],
        )
        .draw(|b| {
            let own = [0.0, 0.0, b.k("dot"), b.k("uneven"), b.k("soft")];
            pattern(b, k::SPOTS, &own, 1, b.i("kind"))
        }),
        dye(
            still(
                fx("Tiles", "Tiles", "Tiles, bricks or a herringbone floor."),
                9.0,
            )
            .choice("kind", "Kind", 0, &["Tiles", "Bricks", "Herringbone"])
            .within("stretch", "Width", 100.0, 20.0, 500.0)
            .within("joint", "Joints", 5.0, 0.0, 40.0)
            .within("bevel", "Bevel", 8.0, 0.0, 50.0)
            .pct("differ", "Shade Differences", 25.0)
            .seed(),
            [0.12, 0.12, 0.13],
            [0.9, 0.92, 0.94],
        )
        .preset("Bathroom", &[])
        .preset(
            "Bricks",
            &[
                ("kind", &[1.0]),
                ("stretch", &[220.0]),
                ("c1", &[0.45, 0.42, 0.38]),
                ("c2", &[0.75, 0.27, 0.16]),
                ("differ", &[60.0]),
            ],
        )
        .preset(
            "Wooden Floor",
            &[
                ("kind", &[2.0]),
                ("c1", &[0.12, 0.06, 0.02]),
                ("c2", &[0.7, 0.45, 0.24]),
                ("differ", &[70.0]),
                ("joint", &[2.0]),
            ],
        )
        .draw(|b| {
            let own = [
                0.0,
                0.0,
                b.k("stretch"),
                b.k("joint"),
                b.k("bevel"),
                b.k("differ"),
            ];
            pattern(b, k::TILING, &own, 1, b.i("kind"))
        }),
        dye(
            still(fx("Weave", "Weave", "Woven cloth."), 3.0)
                .choice("kind", "Kind", 0, &["Plain", "Twill", "Basket"])
                .pct("tight", "Tightness", 60.0)
                .pct("uneven", "Uneven Threads", 35.0)
                .seed(),
            [0.12, 0.08, 0.05],
            [0.88, 0.78, 0.6],
        )
        .preset("Linen", &[])
        .preset(
            "Jeans",
            &[
                ("kind", &[1.0]),
                ("size", &[1.4]),
                ("c1", &[0.03, 0.05, 0.14]),
                ("c2", &[0.45, 0.58, 0.8]),
            ],
        )
        .preset("Basket", &[("kind", &[2.0]), ("size", &[5.0])])
        .draw(|b| {
            let own = [0.0, 0.0, b.k("tight"), b.k("uneven")];
            pattern(b, k::WEAVE, &own, 1, b.i("kind"))
        }),
    ]
}

/// A color with its opacity, premultiplied.
fn clear(b: &B<'_>, color: &str, opacity: &str) -> [f32; 4] {
    let a = b.k(opacity);
    let c = b.color(color);
    [c[0] * a, c[1] * a, c[2] * a, a]
}

fn shapes() -> Vec<Fx> {
    vec![
        gradient(
            fx(
                "LinearGradient",
                "Linear Gradient",
                "A gradient between two places.",
            )
            .point("from", "From", [0.5, 0.0])
            .point("to", "To", [0.5, 1.0]),
            &["Along a Line", "Mirrored"],
        )
        .handle(Handle::Line("from", "to"))
        .preset(
            "Dusk",
            &[("c1", &[0.98, 0.5, 0.3]), ("c2", &[0.12, 0.1, 0.32])],
        )
        .preset(
            "Fade to Black",
            &[
                ("c1", &[0.0, 0.0, 0.0]),
                ("c2", &[0.0, 0.0, 0.0]),
                ("o2", &[0.0]),
            ],
        )
        .preset(
            "Mint",
            &[("c1", &[0.4, 0.95, 0.75]), ("c2", &[0.1, 0.5, 0.85])],
        )
        .draw(|b| {
            let kind = if b.i("kind") == 1 { 4 } else { 0 };
            draw_gradient(b, kind, 2)
        }),
        gradient(
            fx(
                "RadialGradient",
                "Radial Gradient",
                "A gradient round a place: round, a cone or a diamond.",
            )
            .point("from", "Center", [0.5, 0.5])
            .point("to", "Edge Point", [0.5, 0.98]),
            &["Round", "Cone", "Diamond"],
        )
        .handle(Handle::Line("from", "to"))
        .preset(
            "Spot",
            &[("c1", &[1.0, 1.0, 0.95]), ("c2", &[0.04, 0.05, 0.1])],
        )
        .preset(
            "Vignette",
            &[
                ("c1", &[0.0, 0.0, 0.0]),
                ("c2", &[0.0, 0.0, 0.0]),
                ("o1", &[0.0]),
            ],
        )
        .preset("Bullseye", &[("times", &[8.0]), ("smooth", &[0.0])])
        .draw(|b| {
            let kind = b.i("kind") + 1;
            draw_gradient(b, kind, 2)
        }),
        fx(
            "ThreeColorGradient",
            "Three-Color Gradient",
            "A gradient of three colors between two places.",
        )
        .point("from", "From", [0.0, 0.5])
        .point("to", "To", [1.0, 0.5])
        .choice(
            "kind",
            "Kind",
            0,
            &["Along a Line", "Round", "Cone", "Diamond", "Mirrored"],
        )
        .color("c1", "Color 1", [0.1, 0.15, 0.6])
        .pct("o1", "Opacity 1", 100.0)
        .color("c2", "Color 2", [0.95, 0.3, 0.5])
        .pct("o2", "Opacity 2", 100.0)
        .color("c3", "Color 3", [1.0, 0.85, 0.35])
        .pct("o3", "Opacity 3", 100.0)
        .pct("middle", "Middle", 50.0)
        .num("times", "Repeat", 1.0, 1.0, 20.0)
        .pct("smooth", "Smoothness", 0.0)
        .pct("dither", "Dither", 20.0)
        .handle(Handle::Line("from", "to"))
        .preset("Sunrise", &[])
        .preset(
            "Flag",
            &[
                ("c1", &[0.0, 0.6, 0.25]),
                ("c2", &[1.0, 0.85, 0.1]),
                ("c3", &[0.1, 0.2, 0.6]),
            ],
        )
        .draw(|b| {
            let kind = b.i("kind");
            draw_gradient(b, kind, 3)
        }),
        fx(
            "Shape",
            "Shape",
            "A shape: ellipse, rectangle, polygon, star, heart, cross or ring.",
        )
        .choice(
            "kind",
            "Shape",
            0,
            &[
                "Ellipse",
                "Rectangle",
                "Polygon",
                "Star",
                "Heart",
                "Cross",
                "Ring",
            ],
        )
        .point("center", "Position", [0.5, 0.5])
        .num("size", "Size", 25.0, 0.5, 100.0)
        .within("stretch", "Width", 100.0, 5.0, 500.0)
        .angle("turn", "Turn", 0.0)
        .int("points", "Points", 5, 3, 24)
        .pct("depth", "Depth", 45.0)
        .hint("How far a star's notches go in; how wide a cross's arms and a ring are.")
        .pct("round", "Roundness", 0.0)
        .pct("soft", "Softness", 0.0)
        .pct("outline", "Outline", 0.0)
        .color("color", "Color", [1.0, 0.82, 0.18])
        .color("around", "Around", [0.0, 0.0, 0.0])
        .pct("aroundOpacity", "Around Opacity", 0.0)
        .handle(Handle::Point("center"))
        .handle(Handle::Ring("center", "size"))
        .handle(Handle::Arrow("center", "turn"))
        .preset("Circle", &[])
        .preset("Star", &[("kind", &[3.0])])
        .preset("Heart", &[("kind", &[4.0]), ("color", &[0.95, 0.15, 0.25])])
        .preset(
            "Outlined Hexagon",
            &[("kind", &[2.0]), ("points", &[6.0]), ("outline", &[8.0])],
        )
        .preset(
            "Peephole",
            &[
                ("color", &[1.0, 1.0, 1.0]),
                ("lay", &[3.0]),
                ("around", &[0.0, 0.0, 0.0]),
                ("aroundOpacity", &[100.0]),
                ("lay", &[0.0]),
                ("soft", &[4.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.len("size"),
                b.a("turn"),
                0.0,
                0.0,
                b.color("color"),
                b.color("around"),
                b.k("aroundOpacity"),
                [0.0f32; 2],
                0.0,
                b.k("stretch"),
                b.k("round"),
                b.k("soft") * 0.5,
                b.k("outline"),
                b.k("depth"),
            ];
            b.run(k::SHAPE, &[b.src()], n, [b.i("points"), 0, b.i("kind"), 2])
        }),
    ]
}

fn gradient(fx: Fx, kinds: &'static [&'static str]) -> Fx {
    fx.choice("kind", "Kind", 0, kinds)
        .color("c1", "Color 1", [1.0, 0.85, 0.2])
        .pct("o1", "Opacity 1", 100.0)
        .color("c2", "Color 2", [0.35, 0.12, 0.02])
        .pct("o2", "Opacity 2", 100.0)
        .num("times", "Repeat", 1.0, 1.0, 20.0)
        .pct("smooth", "Smoothness", 0.0)
        .pct("dither", "Dither", 20.0)
        .hint("A little noise, so that no bands show.")
}

fn draw_gradient(b: &mut B<'_>, kind: i32, colors: i32) -> Slot {
    let third = if colors == 3 {
        clear(b, "c3", "o3")
    } else {
        [0.0; 4]
    };
    let middle = if colors == 3 { b.k("middle") } else { 0.5 };
    let n = nums![
        b.p("from"),
        b.p("to"),
        [0.0f32; 2],
        clear(b, "c1", "o1"),
        clear(b, "c2", "o2"),
        third,
        middle,
        b.k("smooth"),
        b.k("dither") / 64.0,
        b.n("times"),
    ];
    b.run(k::GRADIENT, &[b.src()], n, [1, 0, kind, colors])
}

fn energy() -> Vec<Fx> {
    vec![
        fx(
            "DustMotes",
            "Dust Motes",
            "Motes of dust floating in the light.",
        )
        .num("size", "Size", 1.4, 0.1, 6.0)
        .pct("count", "How Many", 60.0)
        .num("driftX", "Drift Horizontal", 1.0, -30.0, 30.0)
        .num("driftY", "Drift Vertical", 1.5, -30.0, 30.0)
        .num("blur", "Out of Focus", 100.0, 0.0, 300.0)
        .int("layers", "Depth", 3, 1, 5)
        .color("color", "Color", [1.0, 0.95, 0.85])
        .num("brightness", "Brightness", 120.0, 0.0, 300.0)
        .num("speed", "Speed", 1.0, -4.0, 4.0)
        .num("phase", "Phase", 0.0, -10.0, 10.0)
        .seed()
        .lay(2)
        .moving()
        .preset("Attic", &[])
        .preset(
            "Plankton",
            &[
                ("color", &[0.5, 0.9, 1.0]),
                ("size", &[0.6]),
                ("count", &[80.0]),
                ("driftY", &[-1.0]),
            ],
        )
        .draw(|b| {
            let own = [
                0.0,
                0.0,
                b.k("count"),
                b.len("driftX"),
                -b.len("driftY"),
                b.k("blur"),
            ];
            light(b, k::DUST, &own, b.i("layers"))
        }),
        fx("Sparkles", "Sparkles", "Sparkles that come and go.")
            .num("size", "Size", 6.0, 0.5, 20.0)
            .pct("count", "How Many", 45.0)
            .num("rate", "Rate", 0.8, 0.05, 5.0)
            .num("points", "Points", 100.0, 0.0, 300.0)
            .pct("colors", "Every Color", 0.0)
            .num("driftX", "Drift Horizontal", 0.0, -30.0, 30.0)
            .num("driftY", "Drift Vertical", 0.0, -30.0, 30.0)
            .int("layers", "Layers", 3, 1, 5)
            .color("color", "Color", [1.0, 0.95, 0.8])
            .num("brightness", "Brightness", 120.0, 0.0, 300.0)
            .num("speed", "Speed", 1.0, -4.0, 4.0)
            .num("phase", "Phase", 0.0, -10.0, 10.0)
            .seed()
            .lay(2)
            .moving()
            .preset("Fairy", &[])
            .preset(
                "Glitter",
                &[
                    ("colors", &[100.0]),
                    ("size", &[2.0]),
                    ("count", &[75.0]),
                    ("rate", &[2.0]),
                ],
            )
            .preset(
                "Golden Rain",
                &[
                    ("color", &[1.0, 0.75, 0.2]),
                    ("driftY", &[-12.0]),
                    ("count", &[60.0]),
                ],
            )
            .draw(|b| {
                let own = [
                    0.0,
                    0.0,
                    b.k("count"),
                    b.n("rate"),
                    b.k("points"),
                    b.k("colors"),
                    b.len("driftX"),
                    -b.len("driftY"),
                ];
                light(b, k::SPARKLES, &own, b.i("layers"))
            }),
        fx(
            "LaserBeam",
            "Laser Beam",
            "A beam of light between two places.",
        )
        .point("from", "From", [0.1, 0.25])
        .point("to", "To", [0.9, 0.75])
        .pct("shot", "Length", 100.0)
        .hint("How much of the way it goes: animate it to shoot.")
        .num("width", "Width", 1.0, 0.05, 10.0)
        .num("glow", "Glow Width", 4.0, 0.1, 30.0)
        .pct("thin", "Taper", 0.0)
        .color("core", "Core", [1.0, 1.0, 1.0])
        .color("color", "Glow Color", [1.0, 0.15, 0.1])
        .num("brightness", "Brightness", 120.0, 0.0, 400.0)
        .pct("flicker", "Flicker", 25.0)
        .num("energy", "Energy", 40.0, 0.0, 200.0)
        .num("speed", "Speed", 1.0, -4.0, 4.0)
        .seed()
        .lay(2)
        .handle(Handle::Line("from", "to"))
        .moving()
        .preset("Red Laser", &[])
        .preset(
            "Blaster",
            &[
                ("color", &[0.2, 1.0, 0.25]),
                ("width", &[2.0]),
                ("thin", &[60.0]),
            ],
        )
        .preset(
            "Blue Sword",
            &[
                ("color", &[0.15, 0.4, 1.0]),
                ("width", &[2.6]),
                ("glow", &[5.0]),
                ("energy", &[20.0]),
            ],
        )
        .draw(|b| {
            let t = b.p("to");
            let n = nums![
                b.p("from"),
                0.0,
                0.0,
                b.k("brightness"),
                1.0,
                b.color("core"),
                b.color("color"),
                [0.0f32; 3],
                b.run_time("speed"),
                t,
                b.len("width"),
                b.len("glow"),
                b.k("flicker"),
                b.k("thin"),
                b.k("shot"),
                b.k("energy"),
            ];
            b.run(k::BEAM, &[b.src()], n, [1, 1, 0, 2])
        }),
        fx(
            "Lightning",
            "Lightning",
            "A bolt between two places, with its branches.",
        )
        .path(("from", [0.3, 1.0]), ("to", [0.65, 0.05]))
        .pct("hold", "Keep to the Points", 60.0)
        .hint("How near it passes to the points between: 0 lets it stray from them.")
        .pct("length", "Reach", 100.0)
        .hint("How much of the way it has come: animate it to strike.")
        .int("bolts", "Bolts", 1, 1, 6)
        .int("branches", "Branches", 4, 0, 8)
        .num("branch", "Branch Brightness", 70.0, 0.0, 200.0)
        .num("branchLong", "Branch Length", 100.0, 0.0, 300.0)
        .num("width", "Width", 0.5, 0.05, 5.0)
        .pct("thin", "Taper", 40.0)
        .num("glow", "Glow Width", 3.0, 0.1, 20.0)
        .num("stray", "Straying", 18.0, 0.0, 80.0)
        .hint("How crooked it is, alike at any length.")
        .within("fine", "Fineness", 1.0, 0.5, 4.0)
        .num("rate", "Strikes a Second", 6.0, 0.0, 30.0)
        .num("speed", "Writhing", 0.6, -4.0, 4.0)
        .pct("flicker", "Flicker", 45.0)
        .color("core", "Core", [1.0, 1.0, 1.0])
        .color("color", "Glow Color", [0.45, 0.6, 1.0])
        .num("brightness", "Brightness", 130.0, 0.0, 400.0)
        .seed()
        .lay(2)
        .moving()
        .preset("Storm", &[])
        .preset(
            "Tesla Coil",
            &[
                ("bolts", &[4.0]),
                ("stray", &[30.0]),
                ("color", &[0.7, 0.35, 1.0]),
                ("rate", &[14.0]),
            ],
        )
        .preset(
            "Steady Arc",
            &[
                ("rate", &[0.0]),
                ("speed", &[2.0]),
                ("branches", &[0.0]),
                ("bolts", &[2.0]),
                ("stray", &[8.0]),
            ],
        )
        .preset(
            "Red Rage",
            &[("color", &[1.0, 0.15, 0.1]), ("bolts", &[2.0])],
        )
        .draw(bolt),
        fx(
            "LightningStorm",
            "Lightning Storm",
            "Many bolts falling from the sky, lighting the picture as they strike.",
        )
        .num("rate", "Strikes a Second", 1.2, 0.1, 8.0)
        .num("width", "Width", 0.4, 0.05, 5.0)
        .num("glow", "Glow Width", 4.0, 0.1, 20.0)
        .num("stray", "Straying", 22.0, 0.0, 80.0)
        .int("branches", "Branches", 5, 0, 8)
        .within("top", "From", 100.0, 0.0, 150.0)
        .within("bottom", "To", 8.0, -50.0, 100.0)
        .num("flash", "Flash", 60.0, 0.0, 200.0)
        .hint("How much each strike lights the whole picture.")
        .color("core", "Core", [1.0, 1.0, 1.0])
        .color("color", "Glow Color", [0.55, 0.65, 1.0])
        .num("brightness", "Brightness", 130.0, 0.0, 400.0)
        .seed()
        .lay(2)
        .moving()
        .preset("Horizon", &[])
        .preset(
            "Apocalypse",
            &[
                ("rate", &[4.0]),
                ("color", &[1.0, 0.35, 0.3]),
                ("flash", &[90.0]),
            ],
        )
        .draw(|b| {
            let t = b.time() * b.n("rate");
            let n = t.floor();
            let age = t - n;
            let seed = b
                .seed_of("seed")
                .wrapping_add((n.max(0.0) as u32).wrapping_mul(7919));
            let h = |i: i32| super::distort::hash01(i, seed);
            let shape = b.at.shape;
            let from = [(h(1) - 0.5) * shape * 0.9, 0.5 - b.k("top")];
            let to = [from[0] + (h(2) - 0.5) * shape * 0.5, 0.5 - b.k("bottom")];
            // It comes down, flickers and goes out.
            let reach = (age * 9.0).min(1.0);
            let beat = 0.75 + 0.25 * (age * 55.0).sin();
            let power =
                (-(age - 0.1).max(0.0) * 6.0).exp() * beat * f32::from(u8::from(h(3) < 0.85));
            let look = Bolts {
                bolts: 1,
                branches: b.i("branches"),
                branch: 0.7,
                branch_long: 1.0,
                stray: b.k("stray"),
                fine: 1.0,
                hold: 0.0,
                reach,
                thin: 0.4,
                flicker: 0.0,
                width: b.len("width"),
                glow: b.len("glow"),
                writhe: 0.0,
                strike: 0,
                seed,
            };
            let light = bolt::Light {
                brightness: b.k("brightness") * power,
                core: b.color("core"),
                glow: b.color("color"),
            };
            let guide = Guide::through(&[from, to], false);
            let o = bolt::draw(b, &guide, &look, light);
            let c = b.color("color");
            let f = b.k("flash") * power * 0.35;
            let sky = b.run(
                k::FILL,
                &[b.src()],
                nums![c[0] * f, c[1] * f, c[2] * f, f.min(1.0)],
                [0; 4],
            );
            b.run(k::ADD, &[o, sky], nums![1.0, [1.0f32; 3]], [0; 4])
        }),
        fx(
            "MuzzleFlash",
            "Muzzle Flash",
            "The flash of a gun: its core, flame, jets and smoke.",
        )
        .point("center", "Muzzle", [0.35, 0.5])
        .angle("turn", "Direction", 0.0)
        .num("size", "Size", 18.0, 1.0, 80.0)
        .pct("age", "Age", 10.0)
        .hint("From the shot (0) to its end (100): animate it, or let it fire by itself.")
        .num("rate", "Shots a Second", 0.0, 0.0, 20.0)
        .num("tongues", "Tongues", 5.0, 2.0, 12.0)
        .num("side", "Side Jets", 60.0, 0.0, 200.0)
        .pct("smoke", "Smoke", 30.0)
        .color("core", "Core", [1.0, 0.98, 0.9])
        .color("color", "Flame", [1.0, 0.72, 0.28])
        .color("outer", "Outer Flame", [1.0, 0.28, 0.05])
        .num("brightness", "Brightness", 130.0, 0.0, 400.0)
        .seed()
        .lay(2)
        .handle(Handle::Point("center"))
        .handle(Handle::Arrow("center", "turn"))
        .moving()
        .preset("Pistol", &[("size", &[10.0]), ("side", &[20.0])])
        .preset("Rifle", &[("size", &[22.0]), ("tongues", &[6.0])])
        .preset("Machine Gun", &[("rate", &[9.0])])
        .preset(
            "Cannon",
            &[("size", &[50.0]), ("smoke", &[80.0]), ("side", &[120.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.len("size"),
                b.a("turn"),
                b.k("brightness"),
                1.0,
                b.color("core"),
                b.color("color"),
                b.color("outer"),
                b.time(),
                b.n("tongues"),
                b.k("side"),
                b.k("smoke"),
                b.k("age"),
                b.n("rate"),
            ];
            b.run(k::MUZZLE, &[b.src()], n, [1, 1, 0, 3])
        }),
    ]
}

fn bolt(b: &mut B<'_>) -> Slot {
    let o = Bolts {
        bolts: b.i("bolts"),
        branches: b.i("branches"),
        branch: b.k("branch"),
        branch_long: b.k("branchLong"),
        stray: b.k("stray"),
        fine: b.n("fine"),
        hold: b.k("hold"),
        reach: b.k("length"),
        thin: b.k("thin"),
        flicker: b.k("flicker"),
        width: b.len("width"),
        glow: b.len("glow"),
        writhe: b.run_time("speed"),
        strike: (b.time() * b.n("rate")).floor() as i32,
        seed: b.seed_of("seed"),
    };
    let light = bolt::Light {
        brightness: b.k("brightness"),
        core: b.color("core"),
        glow: b.color("color"),
    };
    let guide = b.guide("from", "to");
    bolt::draw(b, &guide, &o, light)
}
