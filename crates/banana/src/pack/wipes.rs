//! Wipes: a pattern says where the other picture comes first.

use crate::fx::{B, Fx, Group, Handle, N, Put};
use crate::generated::k;
use crate::graph::{Kernel, Slot};
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::transition(id, name, Group::Wipes, about)
}

/// What every wipe ends with: its edge, the glow on it and its speed.
fn edge(fx: Fx, soft: f64) -> Fx {
    fx.within("soft", "Edge Softness", soft, 0.0, 100.0)
        .toggle("reverse", "Reverse", false)
        .hint("What came first comes last.")
        .ease(0)
        .section("edgeGlow", "Edge Glow", false)
        .num("glow", "Glow", 0.0, 0.0, 300.0)
        .num("glowWidth", "Glow Width", 4.0, 0.2, 30.0)
        .color("glowColor", "Glow Color", [1.0, 0.8, 0.35])
        .end()
}

/// What a pattern's numbers are.
#[derive(Clone, Copy)]
struct Pattern {
    turn: f32,
    middle: [f32; 2],
    size: f32,
    stretch: f32,
    uneven: f32,
    moves: f32,
    own: [f32; 2],
}

impl Pattern {
    fn of(b: &B<'_>) -> Pattern {
        let or = |name: &str, v: f32| if b.has(name) { b.n(name) } else { v };
        Pattern {
            turn: if b.has("angle") { b.a("angle") } else { 0.0 },
            middle: if b.has("center") {
                b.p("center")
            } else {
                [0.0; 2]
            },
            size: or("size", 10.0) / 100.0,
            stretch: or("stretch", 100.0) / 100.0,
            uneven: or("uneven", 0.0) / 100.0,
            moves: or("moves", 0.0),
            own: [0.0; 2],
        }
    }
}

/// Runs a wipe's kernel.
fn wipe(b: &mut B<'_>, kernel: Kernel, p: Pattern, kind: i32, x: i32, w: i32) -> Slot {
    let ints = [x, kind, i32::from(b.on("reverse")), w];
    let n = numbers(b, p);
    b.run(kernel, &[b.src(), b.to()], n, ints)
}

fn numbers(b: &B<'_>, p: Pattern) -> N {
    let mut n = nums![
        b.t().clamp(0.0, 1.0),
        b.k("soft"),
        b.len("glowWidth"),
        b.color("glowColor"),
        b.k("glow"),
        p.turn,
        p.middle,
        p.size.max(0.002),
        p.stretch,
        p.uneven,
        p.moves,
    ];
    for v in p.own {
        v.put(&mut n);
    }
    n
}

fn plain(b: &mut B<'_>, kernel: Kernel, kind: i32) -> Slot {
    let p = Pattern::of(b);
    wipe(b, kernel, p, kind, 0, 0)
}

/// A noise wipe.
fn noisy(b: &mut B<'_>, kind: i32) -> Slot {
    let mut p = Pattern::of(b);
    p.own = [b.k("line"), 0.0];
    let layers = if b.has("detail") { b.i("detail") } else { 4 };
    wipe(b, k::WIPE_NOISE, p, kind, layers, 0)
}

fn noise_settings(fx: Fx, size: f64, line: f64, detail: i32) -> Fx {
    let mut fx = fx.num("size", "Size", size, 1.0, 100.0);
    // Blobs have one layer, grains none.
    if detail > 1 {
        fx = fx.int("detail", "Detail", detail, 1, 7);
    }
    fx = fx
        .pct("line", "Along a Line", line)
        .hint("From all over at once (0) to a line that crosses the frame (100).")
        .angle("angle", "Angle", 0.0);
    if detail > 0 {
        fx = fx.num("moves", "Flow", 1.0, -10.0, 10.0);
    }
    fx.seed().handle(Handle::Arrow("", "angle"))
}

pub fn list() -> Vec<Fx> {
    vec![
        edge(
            fx(
                "LineWipe",
                "Line Wipe",
                "A line crosses the frame, straight or wavering.",
            )
            .choice("kind", "From", 0, &["One Side", "The Middle", "A Corner"])
            .angle("angle", "Angle", 0.0)
            .num("uneven", "Waver", 0.0, 0.0, 100.0)
            .num("size", "Waver Size", 20.0, 2.0, 100.0)
            .num("moves", "Waver Moves", 2.0, -10.0, 10.0)
            .seed(),
            10.0,
        )
        .handle(Handle::Arrow("", "angle"))
        .preset("Left to Right", &[])
        .preset("Diagonal", &[("angle", &[-35.0]), ("soft", &[30.0])])
        .preset("Open the Middle", &[("kind", &[1.0])])
        .preset("Hand Drawn", &[("uneven", &[40.0]), ("soft", &[2.0])])
        .preset(
            "Scanner",
            &[
                ("soft", &[1.0]),
                ("glow", &[200.0]),
                ("glowColor", &[0.3, 1.0, 0.6]),
            ],
        )
        .draw(|b| {
            let kind = b.i("kind");
            plain(b, k::WIPE_LINE, kind)
        }),
        shape(
            "CircleWipe",
            "Circle Wipe",
            "A circle grows from a place.",
            false,
        )
        .preset("Iris In", &[])
        .preset("Iris Out", &[("reverse", &[1.0])])
        .preset("Oval", &[("stretch", &[200.0])])
        .draw(|b| plain(b, k::WIPE_SHAPE, 0)),
        shape("BoxWipe", "Box Wipe", "A box grows from a place.", false)
            .preset("Wide Box", &[("stretch", &[177.0])])
            .preset("Turning Box", &[("moves", &[1.5])])
            .draw(|b| plain(b, k::WIPE_SHAPE, 1)),
        shape(
            "DiamondWipe",
            "Diamond Wipe",
            "A diamond grows from a place.",
            false,
        )
        .preset("Diamond", &[])
        .draw(|b| plain(b, k::WIPE_SHAPE, 2)),
        shape(
            "StarWipe",
            "Star Shape Wipe",
            "A star grows from a place.",
            true,
        )
        .preset("Five Points", &[])
        .preset("Spinning", &[("moves", &[3.0])])
        .preset("Burst", &[("points", &[16.0]), ("depth", &[70.0])])
        .draw(|b| {
            let mut p = Pattern::of(b);
            p.own = [b.n("points"), b.k("depth")];
            wipe(b, k::WIPE_SHAPE, p, 3, 0, 0)
        }),
        shape(
            "HeartWipe",
            "Heart Wipe",
            "A heart grows from a place.",
            false,
        )
        .preset("Heart", &[])
        .preset("Closing Heart", &[("reverse", &[1.0])])
        .draw(|b| plain(b, k::WIPE_SHAPE, 4)),
        edge(
            fx(
                "PolygonWipe",
                "Polygon Wipe",
                "A polygon grows from a place.",
            )
            .point("center", "Center", [0.5, 0.5])
            .int("points", "Sides", 6, 3, 16)
            .angle("angle", "Angle", 0.0)
            .within("stretch", "Width", 100.0, 20.0, 500.0)
            .num("moves", "Turn", 0.0, -12.0, 12.0),
            6.0,
        )
        .handle(Handle::Point("center"))
        .preset("Hexagon", &[])
        .preset("Triangle", &[("points", &[3.0])])
        .draw(|b| {
            let mut p = Pattern::of(b);
            p.own = [b.n("points"), 0.0];
            wipe(b, k::WIPE_SHAPE, p, 5, 0, 0)
        }),
        edge(
            fx(
                "ClockWipe",
                "Clock Hands Wipe",
                "Hands sweep round a place.",
            )
            .point("center", "Center", [0.5, 0.5])
            .int("hands", "Hands", 1, 1, 12)
            .angle("angle", "Angle", 0.0),
            3.0,
        )
        .handle(Handle::Point("center"))
        .handle(Handle::Arrow("center", "angle"))
        .preset("Clock", &[])
        .preset("Two Hands", &[("hands", &[2.0])])
        .preset("Propeller", &[("hands", &[4.0])])
        .draw(|b| {
            let p = Pattern::of(b);
            let hands = b.i("hands");
            wipe(b, k::WIPE_CLOCK, p, hands, 0, 0)
        }),
        edge(
            fx("FanWipe", "Fan Wipe", "Fans open round a place.")
                .point("center", "Center", [0.5, 0.5])
                .int("hands", "Fans", 1, 1, 12)
                .angle("angle", "Angle", 0.0),
            3.0,
        )
        .handle(Handle::Point("center"))
        .handle(Handle::Arrow("center", "angle"))
        .preset("Fan", &[("center", &[0.5, 0.0])])
        .preset("Bow Tie", &[("hands", &[2.0])])
        .preset("Pinwheel", &[("hands", &[6.0])])
        .draw(|b| {
            let p = Pattern::of(b);
            let hands = b.i("hands");
            wipe(b, k::WIPE_CLOCK, p, hands, 1, 0)
        }),
        edge(
            fx("SpiralWipe", "Spiral Wipe", "A spiral winds in or out.")
                .point("center", "Center", [0.5, 0.5])
                .num("turns", "Turns", 5.0, 1.0, 20.0)
                .angle("angle", "Angle", 0.0),
            4.0,
        )
        .handle(Handle::Point("center"))
        .preset("Out", &[])
        .preset("In", &[("reverse", &[1.0])])
        .draw(|b| {
            let mut p = Pattern::of(b);
            p.own = [b.n("turns"), 0.0];
            wipe(b, k::WIPE_CLOCK, p, 1, 0, 1)
        }),
        bars(
            "BlindsWipe",
            "Blinds Wipe",
            "Blinds open everywhere at once.",
            10.0,
        )
        .preset("Upright", &[])
        .preset("Level", &[("angle", &[90.0])])
        .draw(|b| plain(b, k::WIPE_BARS, 0)),
        bars(
            "RollingBlinds",
            "Rolling Blinds",
            "Blinds open one after another across the frame.",
            10.0,
        )
        .pct("uneven", "Delay", 60.0)
        .preset("Rolling", &[])
        .draw(|b| plain(b, k::WIPE_BARS, 2)),
        bars(
            "CheckerWipe",
            "Checkerboard Wipe",
            "Two sets of squares, one after the other.",
            14.0,
        )
        .preset("Checks", &[])
        .preset("Diagonal", &[("angle", &[45.0])])
        .draw(|b| plain(b, k::WIPE_BARS, 1)),
        bars(
            "WeaveWipe",
            "Weave Wipe",
            "Threads weave the other picture in.",
            10.0,
        )
        .preset("Cloth", &[])
        .draw(|b| plain(b, k::WIPE_BARS, 3)),
        bars(
            "BarsWipe",
            "Bars Wipe",
            "Bars fall, each at its own moment.",
            6.0,
        )
        .seed()
        .preset("Equalizer", &[])
        .preset("Sideways", &[("angle", &[90.0])])
        .draw(|b| plain(b, k::WIPE_BARS, 4)),
        edge(
            fx(
                "TileWipe",
                "Tile Wipe",
                "Tiles change, at random or in order.",
            )
            .choice(
                "kind",
                "Order",
                0,
                &["Random", "Along a Line", "From the Middle", "Spiral"],
            )
            .num("size", "Size", 12.0, 1.0, 60.0)
            .within("stretch", "Width", 100.0, 20.0, 500.0)
            .angle("angle", "Angle", 0.0)
            .point("center", "Center", [0.5, 0.5])
            .pct("uneven", "Unevenness", 40.0)
            .pct("grow", "Grow", 35.0)
            .hint("How much each tile grows rather than changes at once.")
            .seed(),
            6.0,
        )
        .preset("Random", &[])
        .preset("Wave of Tiles", &[("kind", &[1.0]), ("angle", &[-30.0])])
        .preset("From the Middle", &[("kind", &[2.0]), ("size", &[8.0])])
        .preset(
            "Pixels",
            &[("size", &[2.5]), ("grow", &[0.0]), ("soft", &[2.0])],
        )
        .draw(|b| {
            let mut p = Pattern::of(b);
            p.own = [b.k("grow"), 0.0];
            let kind = b.i("kind");
            wipe(b, k::WIPE_TILES, p, kind, 0, 0)
        }),
        edge(
            fx("DotWipe", "Dot Wipe", "Dots grow until they meet.")
                .choice(
                    "kind",
                    "Dots",
                    1,
                    &["Rows", "Staggered", "Scattered", "Outward"],
                )
                .num("size", "Size", 8.0, 1.0, 40.0)
                .angle("angle", "Angle", 0.0)
                .point("center", "Center", [0.5, 0.5])
                .pct("wave", "Wave", 50.0)
                .hint("From all at once (0) to a wave across the frame (100).")
                .seed(),
            4.0,
        )
        .preset("Comic", &[])
        .preset("All at Once", &[("wave", &[0.0])])
        .preset("Outward", &[("kind", &[3.0]), ("wave", &[70.0])])
        .draw(|b| {
            let mut p = Pattern::of(b);
            p.own = [b.k("wave"), 0.0];
            let kind = b.i("kind");
            wipe(b, k::WIPE_DOTS, p, kind, 0, 0)
        }),
        edge(
            fx("RingWipe", "Ring Wipe", "Rings open round a place.")
                .choice("kind", "Rings", 0, &["Rings", "Ripple"])
                .point("center", "Center", [0.5, 0.5])
                .num("size", "Ring Spacing", 12.0, 1.0, 60.0)
                .pct("uneven", "From the Middle", 40.0)
                .num("moves", "Flow", 0.0, -4.0, 4.0),
            8.0,
        )
        .handle(Handle::Point("center"))
        .preset("Rings", &[])
        .preset("Ripple", &[("kind", &[1.0]), ("soft", &[30.0])])
        .draw(|b| {
            let kind = b.i("kind");
            plain(b, k::WIPE_RINGS, kind)
        }),
        edge(
            noise_settings(
                fx(
                    "CloudWipe",
                    "Cloud Wipe",
                    "Clouds part and the other picture is behind.",
                ),
                45.0,
                0.0,
                6,
            ),
            25.0,
        )
        .preset("Clouds", &[])
        .preset("Rolling Fog", &[("line", &[60.0]), ("soft", &[50.0])])
        .draw(|b| noisy(b, 0)),
        edge(
            noise_settings(
                fx("BlobWipe", "Blob Wipe", "Soft blobs grow and join."),
                28.0,
                0.0,
                1,
            ),
            8.0,
        )
        .preset("Blobs", &[])
        .preset(
            "Goo",
            &[("soft", &[1.0]), ("line", &[55.0]), ("size", &[18.0])],
        )
        .draw(|b| noisy(b, 1)),
        edge(
            noise_settings(
                fx(
                    "SandWipe",
                    "Sand Wipe",
                    "Grain by grain, as sand blown across.",
                ),
                2.0,
                70.0,
                0,
            ),
            12.0,
        )
        .preset("Blown Sand", &[])
        .preset("All Over", &[("line", &[0.0])])
        .draw(|b| noisy(b, 2)),
        edge(
            noise_settings(
                fx(
                    "LiquidWipe",
                    "Liquid Wipe",
                    "A liquid folds the other picture in.",
                ),
                40.0,
                20.0,
                4,
            ),
            12.0,
        )
        .preset("Ink", &[("soft", &[4.0])])
        .preset("Milk", &[("soft", &[35.0])])
        .draw(|b| noisy(b, 3)),
        edge(
            noise_settings(
                fx(
                    "RidgeWipe",
                    "Ridge Wipe",
                    "Along ridges and veins, as ice cracking.",
                ),
                35.0,
                25.0,
                5,
            ),
            8.0,
        )
        .preset(
            "Frost",
            &[("glow", &[80.0]), ("glowColor", &[0.7, 0.9, 1.0])],
        )
        .preset("Veins", &[])
        .draw(|b| noisy(b, 4)),
        edge(
            noise_settings(
                fx(
                    "FlameWipe",
                    "Flame Wipe",
                    "Tongues of flame rise through the picture.",
                ),
                22.0,
                75.0,
                5,
            ),
            6.0,
        )
        .preset(
            "Flames",
            &[
                ("angle", &[-90.0]),
                ("glow", &[220.0]),
                ("glowColor", &[1.0, 0.45, 0.08]),
                ("glowWidth", &[6.0]),
                ("moves", &[3.0]),
            ],
        )
        .preset(
            "Blue Fire",
            &[
                ("angle", &[-90.0]),
                ("glow", &[220.0]),
                ("glowColor", &[0.2, 0.5, 1.0]),
                ("glowWidth", &[6.0]),
                ("moves", &[3.0]),
            ],
        )
        .draw(|b| noisy(b, 5)),
        cells(
            "BubbleWipe",
            "Bubble Wipe",
            "Bubbles grow until they fill the frame.",
            16.0,
        )
        .preset("Bubbles", &[])
        .draw(|b| plain(b, k::WIPE_CELLS, 0)),
        cells("CellWipe", "Cell Wipe", "Cells change one by one.", 12.0)
            .preset("Cells", &[])
            .preset("Pointillism", &[("size", &[4.0]), ("soft", &[3.0])])
            .draw(|b| plain(b, k::WIPE_CELLS, 2)),
        cells(
            "WallWipe",
            "Cell Wall Wipe",
            "The walls between cells widen.",
            16.0,
        )
        .preset("Walls", &[])
        .preset(
            "Glowing Walls",
            &[("glow", &[150.0]), ("glowColor", &[0.4, 1.0, 0.8])],
        )
        .draw(|b| plain(b, k::WIPE_CELLS, 1)),
        cells(
            "CrackWipe",
            "Crack Wipe",
            "Cracks spread from a place and the pieces go.",
            14.0,
        )
        .point("center", "Center", [0.5, 0.5])
        .handle(Handle::Point("center"))
        .preset("Shatter", &[("soft", &[2.0])])
        .draw(|b| plain(b, k::WIPE_CELLS, 3)),
        edge(
            fx(
                "PlasmaWipe",
                "Plasma Wipe",
                "Waves meeting decide where the other picture comes.",
            )
            .num("size", "Size", 20.0, 2.0, 100.0)
            .angle("angle", "Angle", 0.0)
            .num("moves", "Flow", 3.0, -10.0, 10.0),
            20.0,
        )
        .preset("Plasma", &[])
        .draw(|b| plain(b, k::WIPE_WAVES, 0)),
        edge(
            fx(
                "MoireWipe",
                "Moiré Wipe",
                "Two sets of rings crossing decide where the other picture comes.",
            )
            .point("center", "Center", [0.5, 0.5])
            .num("size", "Ring Spacing", 14.0, 2.0, 100.0),
            20.0,
        )
        .handle(Handle::Point("center"))
        .preset("Moiré", &[])
        .draw(|b| plain(b, k::WIPE_WAVES, 1)),
        edge(
            fx(
                "LightWipe",
                "Light Wipe",
                "By the pictures' own light: bright parts first, or dark.",
            )
            .choice("kind", "Light Of", 0, &["The First", "The Second", "Both"])
            .num("smooth", "Smoothness", 0.6, 0.0, 10.0),
            25.0,
        )
        .preset("Bright First", &[])
        .preset("Dark First", &[("reverse", &[1.0])])
        .preset("Glowing", &[("glow", &[120.0]), ("smooth", &[1.5])])
        .draw(|b| {
            let kind = b.i("kind");
            let s = match kind {
                1 => b.to(),
                2 => b.mix(b.src(), b.to(), 0.5),
                _ => b.src(),
            };
            let m = b.blur(s, b.len("smooth"), 1);
            let p = Pattern::of(b);
            let ints = [1, kind, i32::from(b.on("reverse")), 0];
            let n = numbers(b, p);
            b.run(k::WIPE_LUMA, &[b.src(), b.to(), m], n, ints)
        }),
    ]
}

fn shape(id: &'static str, name: &'static str, about: &'static str, star: bool) -> Fx {
    let mut f = fx(id, name, about)
        .point("center", "Center", [0.5, 0.5])
        .angle("angle", "Angle", 0.0)
        .within("stretch", "Width", 100.0, 20.0, 500.0)
        .num("moves", "Turn", 0.0, -12.0, 12.0)
        .hint("How far it turns as it grows.");
    if star {
        f = f
            .int("points", "Points", 5, 3, 24)
            .within("depth", "Depth", 45.0, 10.0, 95.0);
    }
    edge(f, 6.0).handle(Handle::Point("center"))
}

fn bars(id: &'static str, name: &'static str, about: &'static str, size: f64) -> Fx {
    edge(
        fx(id, name, about)
            .num("size", "Size", size, 1.0, 60.0)
            .angle("angle", "Angle", 0.0),
        4.0,
    )
    .handle(Handle::Arrow("", "angle"))
}

fn cells(id: &'static str, name: &'static str, about: &'static str, size: f64) -> Fx {
    edge(
        fx(id, name, about)
            .num("size", "Size", size, 1.0, 60.0)
            .num("moves", "Drift", 0.0, -10.0, 10.0)
            .seed(),
        6.0,
    )
}
