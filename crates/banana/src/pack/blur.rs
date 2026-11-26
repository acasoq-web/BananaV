//! Blurs, defocuses and sharpening.

use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::filter(id, name, Group::Blur, about)
}

/// Where a lens is in focus: the numbers `k::LENS` reads from f8 on.
#[derive(Clone, Copy)]
struct Focus {
    kind: i32,
    at: [f32; 2],
    turn: f32,
    half: f32,
    over: f32,
}

impl Focus {
    const ALL: Focus = Focus {
        kind: 0,
        at: [0.0; 2],
        turn: 0.0,
        half: 0.0,
        over: 1.0,
    };
}

/// A lens's iris.
#[derive(Clone, Copy)]
struct Iris {
    stretch: [f32; 2],
    turn: f32,
    blades: f32,
    round: f32,
    boost: f32,
    from: f32,
    spread: f32,
}

/// `s` out of focus by a lens: a disc `radius` (frame heights) big.
fn lens(b: &mut B<'_>, s: Slot, radius: f32, iris: Iris, focus: Focus) -> Slot {
    let r = b.px_y(radius);
    if r < 0.4 {
        return s;
    }
    // Worked out small enough that the samples fill the disc.
    let mut half = 0;
    while r / f32::from(1u16 << half) > 15.0 && half < 6 {
        half += 1;
    }
    let small = b.smaller(s, half, 1);
    let rr = r / f32::from(1u16 << half);
    let samples = ((rr * rr * 1.6) as i32).clamp(24, 150);
    let n = nums![
        rr,
        iris.stretch,
        iris.turn,
        iris.blades,
        iris.round,
        iris.boost,
        iris.from,
        focus.at,
        focus.turn,
        focus.half,
        focus.over,
        iris.spread,
    ];
    let out = b.run_like(small, k::LENS, &[small], n, [1, focus.kind, samples, 0]);
    // The grain the samples leave.
    let out = b.blur_px(out, 0.55, 0.55, 1);
    if half == 0 && focus.kind == 0 {
        return out;
    }
    let big = b.larger(out, half, 1);
    if focus.kind == 0 {
        return big;
    }
    let mut n = nums![r];
    for v in [0.0; 7] {
        crate::fx::Put::put(&v, &mut n);
    }
    crate::fx::Put::put(&focus.at, &mut n);
    for v in [focus.turn, focus.half, focus.over] {
        crate::fx::Put::put(&v, &mut n);
    }
    b.run(k::LENS_JOIN, &[s, big], n, [1, focus.kind, 0, 0])
}

fn iris_settings(fx: Fx) -> Fx {
    fx.section("iris", "Iris", true)
        .int("blades", "Blades", 6, 0, 12)
        .hint("The iris's sides: below 3 it is round.")
        .pct("round", "Roundness", 25.0)
        .angle("turn", "Turn", 0.0)
        .within("stretch", "Stretch", 100.0, 25.0, 400.0)
        .hint("How wide the iris is for its height: 50 as an anamorphic lens's.")
        .section("lights", "Highlights", true)
        .num("boost", "Boost", 300.0, 0.0, 1000.0)
        .pct("from", "From", 75.0)
        .end()
}

fn iris(b: &B<'_>, spread: f32) -> Iris {
    let s = b.k("stretch").max(0.01).sqrt();
    Iris {
        stretch: [s, 1.0 / s],
        turn: b.a("turn"),
        blades: b.n("blades"),
        round: b.k("round"),
        boost: b.k("boost"),
        from: b.k("from"),
        spread,
    }
}

pub fn list() -> Vec<Fx> {
    vec![
        fx(
            "SoftBlur",
            "Soft Blur",
            "A smooth Gaussian blur, as wide as you like.",
        )
        .num("size", "Size", 2.0, 0.0, 30.0)
        .within("across", "Horizontal", 100.0, 0.0, 200.0)
        .within("down", "Vertical", 100.0, 0.0, 200.0)
        .choice("edges", "Edges", 1, &["Transparent", "Hold", "Mirror"])
        .num("brightness", "Brightness", 100.0, 0.0, 200.0)
        .preset("Subtle", &[("size", &[0.5])])
        .preset("Background", &[("size", &[8.0]), ("brightness", &[85.0])])
        .preset("Across Only", &[("size", &[6.0]), ("down", &[0.0])])
        .preset("Abstract", &[("size", &[25.0]), ("edges", &[2.0])])
        .draw(|b| {
            let s = b.len("size");
            let o = b.blur_xy(b.src(), s * b.k("across"), s * b.k("down"), b.i("edges"));
            let g = b.k("brightness");
            if g == 1.0 {
                return o;
            }
            b.run(k::SUM3, &[o], nums![g, 0.0, 0.0], [0; 4])
        }),
        fx(
            "ChannelBlur",
            "Channel Blur",
            "Red, green and blue each blurred on its own.",
        )
        .num("red", "Red", 3.0, 0.0, 20.0)
        .num("green", "Green", 0.0, 0.0, 20.0)
        .num("blue", "Blue", 0.0, 0.0, 20.0)
        .choice("edges", "Edges", 1, &["Transparent", "Hold", "Mirror"])
        .preset("Red Haze", &[])
        .preset("Blue Haze", &[("red", &[0.0]), ("blue", &[4.0])])
        .preset(
            "Old Lens",
            &[("red", &[0.5]), ("green", &[0.0]), ("blue", &[0.9])],
        )
        .draw(|b| {
            let e = b.i("edges");
            let r = b.blur(b.src(), b.len("red"), e);
            let g = b.blur(b.src(), b.len("green"), e);
            let bl = b.blur(b.src(), b.len("blue"), e);
            b.run(k::CHANNEL_JOIN, &[r, g, bl], nums![], [0; 4])
        }),
        fx(
            "ColorBlur",
            "Color Blur",
            "Blurs the colors and leaves the detail of the light.",
        )
        .num("color", "Color Blur", 3.0, 0.0, 20.0)
        .num("light", "Light Blur", 0.0, 0.0, 10.0)
        .num("colorGain", "Color", 100.0, 0.0, 200.0)
        .num("lightGain", "Lights", 100.0, 0.0, 200.0)
        .preset("Chroma Clean-Up", &[("color", &[0.8])])
        .preset(
            "Watercolor Bleed",
            &[("color", &[6.0]), ("colorGain", &[130.0])],
        )
        .draw(|b| {
            let l = b.blur(b.src(), b.len("light"), 1);
            let c = b.blur(b.src(), b.len("color"), 1);
            b.run(
                k::CHROMA_JOIN,
                &[l, c],
                nums![b.k("colorGain"), b.k("lightGain")],
                [0; 4],
            )
        }),
        fx(
            "DirectionalBlur",
            "Directional Blur",
            "Draws the picture out along a direction.",
        )
        .num("length", "Reach", 8.0, 0.0, 60.0)
        .angle("angle", "Angle", 0.0)
        .within("shift", "Shift", 0.0, -50.0, 50.0)
        .hint("Draws out more to one side than the other.")
        .edges(2)
        .handle(Handle::Arrow("", "angle"))
        .preset("Fast Pan", &[("length", &[18.0])])
        .preset("Rain", &[("length", &[10.0]), ("angle", &[80.0])])
        .preset("Dragged", &[("length", &[22.0]), ("shift", &[50.0])])
        .draw(|b| {
            let (l, a) = (b.len("length"), b.a("angle"));
            b.smear(
                b.src(),
                [a.cos() * l, a.sin() * l],
                b.k("shift"),
                b.i("edges"),
            )
        }),
        fx(
            "ZoomSpinBlur",
            "Zoom & Spin Blur",
            "The blur of a zoom, a turn or both, round a place you choose.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("zoom", "Zoom", 12.0, -60.0, 60.0)
        .angle("spin", "Spin", 0.0)
        .within("shift", "Shift", 0.0, -50.0, 50.0)
        .within("clear", "Clear Middle", 0.0, 0.0, 80.0)
        .hint("Keeps a circle in the middle sharp.")
        .edges(2)
        .handle(Handle::Point("center"))
        .handle(Handle::Ring("center", "clear"))
        .preset("Rush", &[("zoom", &[30.0]), ("clear", &[12.0])])
        .preset("Spin", &[("zoom", &[0.0]), ("spin", &[14.0])])
        .preset("Whirl", &[("zoom", &[15.0]), ("spin", &[10.0])])
        .preset("Focus Pull", &[("zoom", &[8.0]), ("clear", &[35.0])])
        .draw(|b| {
            let clear = b.len("clear");
            let over = if clear > 0.0 { 0.25 } else { 0.0 };
            b.spin(
                b.src(),
                b.p("center"),
                b.k("zoom"),
                b.a("spin"),
                b.k("shift"),
                b.i("edges"),
                (clear, over),
            )
        }),
        iris_settings(
            fx(
                "LensDefocus",
                "Lens Defocus",
                "A lens out of focus: the highlights become the iris's shape.",
            )
            .num("size", "Size", 2.2, 0.0, 10.0),
        )
        .preset("Portrait", &[("size", &[1.4]), ("round", &[70.0])])
        .preset(
            "Hexagons",
            &[("size", &[3.0]), ("round", &[0.0]), ("boost", &[600.0])],
        )
        .preset(
            "Anamorphic",
            &[("size", &[2.6]), ("stretch", &[50.0]), ("blades", &[0.0])],
        )
        .preset(
            "City Lights",
            &[("size", &[4.5]), ("boost", &[900.0]), ("from", &[65.0])],
        )
        .draw(|b| {
            let i = iris(b, 0.0);
            lens(b, b.src(), b.len("size"), i, Focus::ALL)
        }),
        iris_settings(
            fx(
                "PrismDefocus",
                "Prism Defocus",
                "A lens out of focus whose colors part at the rims.",
            )
            .num("size", "Size", 2.4, 0.0, 10.0)
            .within("spread", "Color Spread", 25.0, -100.0, 100.0),
        )
        .preset(
            "Fringed Lights",
            &[("size", &[3.0]), ("spread", &[40.0]), ("boost", &[600.0])],
        )
        .preset(
            "Cheap Lens",
            &[("size", &[0.8]), ("spread", &[70.0]), ("boost", &[100.0])],
        )
        .draw(|b| {
            let i = iris(b, b.k("spread") * 0.6);
            lens(b, b.src(), b.len("size"), i, Focus::ALL)
        }),
        iris_settings(
            fx(
                "TiltShift",
                "Tilt-Shift",
                "A band (or a circle) in focus and the rest out of it, as a miniature.",
            )
            .choice("shape", "In Focus", 0, &["Band", "Circle", "One Side"])
            .point("center", "Center", [0.5, 0.42])
            .angle("angle", "Angle", 0.0)
            .num("width", "Width", 10.0, 0.0, 60.0)
            .num("falloff", "Falloff", 30.0, 2.0, 100.0)
            .num("size", "Blur", 1.8, 0.0, 8.0)
            .num("saturation", "Saturation", 115.0, 0.0, 200.0),
        )
        .handle(Handle::Point("center"))
        .handle(Handle::Arrow("center", "angle"))
        .handle(Handle::Ring("center", "width"))
        .preset("Miniature", &[("size", &[2.4]), ("saturation", &[135.0])])
        .preset(
            "Portrait",
            &[
                ("shape", &[1.0]),
                ("center", &[0.5, 0.55]),
                ("width", &[26.0]),
                ("saturation", &[100.0]),
            ],
        )
        .preset(
            "Free Lens",
            &[
                ("angle", &[18.0]),
                ("width", &[6.0]),
                ("size", &[3.2]),
                ("saturation", &[100.0]),
            ],
        )
        .draw(|b| {
            let focus = Focus {
                kind: b.i("shape") + 1,
                at: b.p("center"),
                turn: b.a("angle"),
                half: b.len("width"),
                over: b.len("falloff"),
            };
            let i = iris(b, 0.0);
            let o = lens(b, b.src(), b.len("size"), i, focus);
            let s = b.k("saturation");
            if s == 1.0 {
                return o;
            }
            b.run(
                k::HUE,
                &[o],
                nums![0.0, s, 0.0, 1.0, 0.0, [0.0; 3], 0.0],
                [0; 4],
            )
        }),
        fx(
            "SurfaceBlur",
            "Surface Blur",
            "Smooths surfaces and keeps edges crisp.",
        )
        .num("size", "Size", 1.0, 0.0, 5.0)
        .within("edge", "Edge Threshold", 12.0, 1.0, 60.0)
        .num("detail", "Detail", 0.0, 0.0, 100.0)
        .preset("Gentle", &[("size", &[0.5]), ("edge", &[7.0])])
        .preset("Painted", &[("size", &[1.8]), ("edge", &[18.0])])
        .preset(
            "Plastic",
            &[("size", &[1.2]), ("edge", &[14.0]), ("detail", &[60.0])],
        )
        .draw(|b| {
            let o = surface(b, b.src(), b.len("size"), b.k("edge"));
            let d = b.k("detail");
            if d <= 0.0 {
                return o;
            }
            let soft = b.blur(o, 0.002, 1);
            b.run(k::SHARPEN, &[o, soft], nums![d * 2.0, 0.0, 0.3], [0; 4])
        }),
        fx(
            "EdgeBlur",
            "Edge Blur",
            "Blurs only the edges, or everything but them.",
        )
        .num("size", "Blur", 1.2, 0.0, 10.0)
        .num("strength", "Edge Strength", 150.0, 0.0, 500.0)
        .num("spread", "Edge Width", 0.6, 0.0, 5.0)
        .toggle("invert", "Keep Edges", false)
        .preset("Soft Outlines", &[])
        .preset("Only Edges Sharp", &[("invert", &[1.0]), ("size", &[2.5])])
        .draw(|b| {
            let gap = b.at.scale * 1.5;
            let e = b.run(
                k::EDGES,
                &[b.src()],
                nums![b.k("strength") * 3.0, gap],
                [0; 4],
            );
            let e = b.blur(e, b.len("spread") * 0.5, 1);
            let s = b.blur(b.src(), b.len("size"), 1);
            let inv = i32::from(b.on("invert"));
            b.run(k::EDGE_JOIN, &[b.src(), s, e], nums![1.0], [inv, 0, 0, 0])
        }),
        fx(
            "HighlightBlur",
            "Highlight Blur",
            "Blurs the bright parts (or the dark ones) and leaves the rest sharp.",
        )
        .num("size", "Blur", 3.0, 0.0, 20.0)
        .pct("threshold", "Threshold", 70.0)
        .pct("soft", "Softness", 20.0)
        .toggle("dark", "Dark Parts Instead", false)
        .preset("Dreamy Lights", &[])
        .preset(
            "Melting Shadows",
            &[("dark", &[1.0]), ("threshold", &[30.0])],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("size"), 1);
            b.run(
                k::LIGHT_JOIN,
                &[b.src(), s],
                nums![b.k("threshold"), b.k("soft")],
                [i32::from(b.on("dark")), 0, 0, 0],
            )
        }),
        fx(
            "GrainRemove",
            "Grain Remove",
            "Takes grain and noise out of surfaces, keeping edges and texture.",
        )
        .num("size", "Grain Size", 0.5, 0.1, 2.0)
        .within("amount", "Strength", 8.0, 1.0, 40.0)
        .pct("color", "Color Noise", 60.0)
        .num("detail", "Detail", 15.0, 0.0, 100.0)
        .preset("Light", &[("amount", &[4.0])])
        .preset("Strong", &[("amount", &[16.0]), ("size", &[0.8])])
        .draw(|b| {
            let o = surface(b, b.src(), b.len("size"), b.k("amount"));
            let wide = b.blur(b.src(), b.len("size") * (0.5 + b.k("color") * 2.5), 1);
            let c = b.mix(b.src(), wide, b.k("color"));
            let o = b.run(k::CHROMA_JOIN, &[o, c], nums![1.0, 1.0], [0; 4]);
            let d = b.k("detail");
            let soft = b.blur(o, 0.0015, 1);
            b.run(k::SHARPEN, &[o, soft], nums![d * 1.5, 0.004, 0.2], [0; 4])
        }),
        fx(
            "SkinSmooth",
            "Skin Smooth",
            "Finds skin by its color and smooths it, keeping its texture and glow.",
        )
        .color("skin", "Skin Color", [0.82, 0.6, 0.48])
        .within("range", "Range", 35.0, 5.0, 100.0)
        .pct("amount", "Smoothing", 75.0)
        .num("size", "Size", 0.9, 0.1, 4.0)
        .pct("texture", "Texture", 25.0)
        .pct("glow", "Glow", 12.0)
        .preset("Natural", &[])
        .preset(
            "Magazine",
            &[
                ("amount", &[100.0]),
                ("size", &[1.6]),
                ("texture", &[10.0]),
                ("glow", &[25.0]),
            ],
        )
        .draw(|b| {
            let s = surface(b, b.src(), b.len("size"), 0.09);
            let g = b.blur(b.src(), 0.02, 1);
            b.run(
                k::SKIN,
                &[b.src(), s, g],
                nums![
                    b.color("skin"),
                    b.k("range"),
                    b.k("amount"),
                    b.k("texture"),
                    b.k("glow") * 0.5
                ],
                [0; 4],
            )
        }),
        fx(
            "Median",
            "Median Filter",
            "Takes specks away: each pixel the middle one of those around.",
        )
        .num("size", "Size", 0.15, 0.05, 0.6)
        .int("passes", "Passes", 1, 1, 6)
        .preset("Despeckle", &[])
        .preset("Painterly", &[("size", &[0.3]), ("passes", &[4.0])])
        .draw(|b| {
            let gap = b.px_y(b.len("size")).round().max(1.0) as i32;
            let mut s = b.src();
            for _ in 0..b.i("passes").clamp(1, 6) {
                s = b.run(k::MEDIAN, &[s], nums![], [gap, 0, 0, 0]);
            }
            s
        }),
        fx(
            "Sharpen",
            "Detail Sharpen",
            "Sharpens the detail, leaving noise and halos alone.",
        )
        .num("amount", "Amount", 80.0, 0.0, 300.0)
        .num("size", "Size", 0.2, 0.02, 1.5)
        .within("threshold", "Threshold", 1.0, 0.0, 20.0)
        .hint("Differences smaller than this stay as they are.")
        .within("halo", "Halo Limit", 25.0, 1.0, 100.0)
        .preset("Fine", &[("size", &[0.1]), ("amount", &[100.0])])
        .preset(
            "Punch",
            &[("size", &[0.5]), ("amount", &[150.0]), ("halo", &[40.0])],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("size"), 1);
            b.run(
                k::SHARPEN,
                &[b.src(), s],
                nums![b.k("amount"), b.k("threshold"), b.k("halo")],
                [0; 4],
            )
        }),
        fx(
            "Clarity",
            "Clarity",
            "Contrast between neighboring areas, for the middle tones.",
        )
        .within("amount", "Amount", 45.0, -100.0, 200.0)
        .num("size", "Size", 4.0, 0.5, 20.0)
        .within("threshold", "Threshold", 0.5, 0.0, 10.0)
        .preset("Crisp", &[])
        .preset("Gritty", &[("amount", &[120.0]), ("size", &[2.0])])
        .preset("Soft Skin", &[("amount", &[-45.0]), ("size", &[2.5])])
        .draw(|b| {
            let s = b.blur(b.src(), b.len("size"), 2);
            b.run(
                k::SHARPEN,
                &[b.src(), s],
                nums![b.k("amount"), b.k("threshold"), 0.35],
                [1, 0, 0, 0],
            )
        }),
        fx(
            "SoftFocus",
            "Soft Focus",
            "A sharp picture with a soft, glowing one over it, as a filter on the lens.",
        )
        .num("size", "Size", 3.0, 0.2, 15.0)
        .pct("glow", "Glow", 45.0)
        .pct("soft", "Softness", 25.0)
        .within("contrast", "Contrast", 100.0, 50.0, 150.0)
        .preset("Portrait", &[])
        .preset(
            "Dream",
            &[("size", &[6.0]), ("glow", &[80.0]), ("soft", &[45.0])],
        )
        .preset(
            "Old Glamour",
            &[
                ("size", &[2.0]),
                ("glow", &[55.0]),
                ("soft", &[40.0]),
                ("contrast", &[112.0]),
            ],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("size"), 1);
            b.run(
                k::DREAM,
                &[b.src(), s],
                nums![b.k("glow"), b.k("soft"), b.k("contrast")],
                [0; 4],
            )
        }),
        fx(
            "Deband",
            "Deband",
            "Smooths the bands of skies and gradients, with a little noise.",
        )
        .num("size", "Size", 1.2, 0.1, 6.0)
        .within("threshold", "Threshold", 2.0, 0.1, 10.0)
        .within("noise", "Noise", 0.6, 0.0, 5.0)
        .seed()
        .preset("8-bit Sky", &[])
        .preset(
            "Strong",
            &[("size", &[2.5]), ("threshold", &[4.0]), ("noise", &[1.0])],
        )
        .draw(|b| {
            let t = b.k("threshold");
            let o = b.run(
                k::DEBAND,
                &[b.src()],
                nums![b.px_y(b.len("size")), t, 0.0],
                [0; 4],
            );
            let r = b.px_y(b.len("size")) * 0.4;
            let seed = b.seed_of("seed").wrapping_add(31);
            let size = [b.w(), b.h()];
            b.run_sized(
                k::DEBAND,
                &[o],
                nums![r, t, b.k("noise")],
                [0; 4],
                size,
                seed,
            )
        }),
        fx(
            "Convolution",
            "Kernel",
            "Each pixel worked out from the nine around it, by your own numbers.",
        )
        .choice(
            "kernel",
            "Kernel",
            0,
            &["Custom", "Sharpen", "Edges", "Emboss", "Outline", "Soften"],
        )
        .within("a", "Top Left", 0.0, -20.0, 20.0)
        .within("b", "Top", -1.0, -20.0, 20.0)
        .within("c", "Top Right", 0.0, -20.0, 20.0)
        .within("d", "Left", -1.0, -20.0, 20.0)
        .within("e", "Middle", 5.0, -20.0, 20.0)
        .within("f", "Right", -1.0, -20.0, 20.0)
        .within("g", "Bottom Left", 0.0, -20.0, 20.0)
        .within("h", "Bottom", -1.0, -20.0, 20.0)
        .within("i", "Bottom Right", 0.0, -20.0, 20.0)
        .num("size", "Spacing", 0.1, 0.05, 2.0)
        .within("offset", "Offset", 0.0, -100.0, 100.0)
        .toggle("normal", "Keep Brightness", true)
        .hint("Divides by the numbers' sum, so that the light stays.")
        .pct("amount", "Amount", 100.0)
        .preset("Emboss", &[("kernel", &[3.0]), ("offset", &[50.0])])
        .preset("Edges", &[("kernel", &[2.0])])
        .draw(|b| {
            let m: [f32; 9] = match b.i("kernel") {
                1 => [0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0],
                2 => [-1.0, -1.0, -1.0, -1.0, 8.0, -1.0, -1.0, -1.0, -1.0],
                3 => [-1.0, -1.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 1.0],
                4 => [1.0, 1.0, 1.0, 1.0, -7.0, 1.0, 1.0, 1.0, 1.0],
                5 => [1.0, 2.0, 1.0, 2.0, 4.0, 2.0, 1.0, 2.0, 1.0],
                _ => ["a", "b", "c", "d", "e", "f", "g", "h", "i"].map(|n| b.n(n)),
            };
            let sum: f32 = m.iter().sum();
            let div = if b.on("normal") && sum.abs() > 1e-3 {
                sum
            } else {
                1.0
            };
            let gap = b.px_y(b.len("size")).round().max(1.0) as i32;
            b.run(
                k::CONVOLVE,
                &[b.src()],
                nums![m.map(|v| v / div), b.k("offset"), b.k("amount")],
                [gap, 0, 0, 0],
            )
        }),
    ]
}

/// A picture smoothed but for its edges: `size` (frame heights) across,
/// edges past `edge`.
pub fn surface(b: &mut B<'_>, s: Slot, size: f32, edge: f32) -> Slot {
    let r = b.px_y(size);
    if r < 0.5 {
        return s;
    }
    // Two passes, the second between the first's steps.
    let gap = (r / 3.0).ceil().max(1.0) as i32;
    let n = ((r / gap as f32).round() as i32).clamp(1, 3);
    let numbers = nums![edge, n as f32 * 0.75];
    let a = b.run(k::SURFACE, &[s], numbers, [n, gap, 0, 0]);
    if gap == 1 {
        return a;
    }
    let fine = (gap / 2).max(1);
    b.run(k::SURFACE, &[a], nums![edge * 0.8, 1.2], [2, fine, 0, 0])
}
