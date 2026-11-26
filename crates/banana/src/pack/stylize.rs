//! Drawings, prints, damage and film.

use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::filter(id, name, Group::Stylize, about)
}

const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
const INK: [f64; 3] = [0.06, 0.06, 0.09];
const PAPER: [f64; 3] = [0.98, 0.96, 0.91];

/// A picture painted: the brush `size` (frame heights).
fn oil(b: &mut B<'_>, s: Slot, size: f32, sharp: f32, long: f32) -> Slot {
    let r = b.px_y(size).max(0.8);
    // Worked out small when the brush is big.
    let mut half = 0;
    while r / f32::from(1u16 << half) > 9.0 && half < 4 {
        half += 1;
    }
    let small = b.smaller(s, half, 1);
    let rr = r / f32::from(1u16 << half);
    let lean = b.blur_px(small, rr * 0.6 + 0.8, rr * 0.6 + 0.8, 1);
    let n = (rr.round() as i32).clamp(2, 6);
    let o = b.run_like(
        small,
        k::OIL,
        &[small, lean],
        nums![rr, sharp, long],
        [n, 0, 0, 0],
    );
    b.larger(o, half, 1)
}

pub fn list() -> Vec<Fx> {
    let mut v = drawn();
    v.extend(printed());
    v.extend(broken());
    v.extend(filmed());
    v
}

fn drawn() -> Vec<Fx> {
    vec![
        fx(
            "OilPaint",
            "Oil Paint",
            "Strokes of paint that follow the picture's shapes.",
        )
        .num("size", "Brush Size", 1.6, 0.2, 5.0)
        .num("sharp", "Sharpness", 100.0, 10.0, 300.0)
        .num("long", "Stroke Length", 60.0, 0.0, 150.0)
        .section("relief", "Relief", true)
        .num("bristles", "Bristles", 35.0, 0.0, 200.0)
        .num("bristleSize", "Bristle Size", 0.35, 0.05, 2.0)
        .angle("light", "Light From", 135.0)
        .pct("canvas", "Canvas", 15.0)
        .seed()
        .end()
        .preset("Canvas", &[])
        .preset(
            "Impressionist",
            &[("size", &[1.8]), ("long", &[100.0]), ("bristles", &[90.0])],
        )
        .preset(
            "Flat Gouache",
            &[("bristles", &[0.0]), ("canvas", &[0.0]), ("long", &[20.0])],
        )
        .preset(
            "Palette Knife",
            &[
                ("size", &[3.0]),
                ("sharp", &[250.0]),
                ("bristles", &[120.0]),
                ("bristleSize", &[0.9]),
            ],
        )
        .draw(|b| {
            let o = oil(b, b.src(), b.len("size"), b.k("sharp") * 0.1, b.k("long"));
            if b.k("bristles") <= 0.0 && b.k("canvas") <= 0.0 {
                return o;
            }
            let lean = b.blur(o, b.len("size") * 0.6, 1);
            let n = nums![
                b.len("bristleSize"),
                b.k("bristles"),
                b.a("light"),
                b.k("canvas")
            ];
            b.run(k::IMPASTO, &[o, lean], n, [0; 4])
        }),
        fx(
            "Watercolor",
            "Watercolor",
            "Pools of thin paint on rough paper.",
        )
        .num("size", "Brush Size", 1.4, 0.2, 6.0)
        .num("rims", "Dark Rims", 80.0, 0.0, 200.0)
        .num("pigment", "Pigment", 60.0, 0.0, 200.0)
        .num("scale", "Pigment Size", 18.0, 1.0, 80.0)
        .num("grain", "Paper Grain", 50.0, 0.0, 200.0)
        .pct("thin", "Thinness", 35.0)
        .seed()
        .preset("Sketchbook", &[])
        .preset(
            "Wet on Wet",
            &[("size", &[3.0]), ("pigment", &[120.0]), ("rims", &[40.0])],
        )
        .draw(|b| {
            let o = oil(b, b.src(), b.len("size"), 0.06, 0.3);
            let soft = b.blur(o, b.len("size") * 0.7, 1);
            let n = nums![
                b.k("rims"),
                b.k("pigment"),
                b.k("grain"),
                b.k("thin"),
                b.len("scale")
            ];
            b.run(k::WASH, &[b.src(), o, soft], n, [0; 4])
        }),
        fx("Cartoon", "Cartoon", "Flat colors and ink outlines.")
            .num("steps", "Color Steps", 5.0, 2.0, 16.0)
            .pct("soft", "Step Softness", 20.0)
            .num("smooth", "Smoothness", 0.8, 0.1, 4.0)
            .within("edge", "Smooth Within", 14.0, 2.0, 60.0)
            .num("saturation", "Saturation", 130.0, 0.0, 250.0)
            .section("inkLines", "Ink", true)
            .num("ink", "Ink", 100.0, 0.0, 200.0)
            .num("width", "Line Width", 0.2, 0.05, 1.5)
            .within("from", "Edge Threshold", 14.0, 0.0, 60.0)
            .color("inkColor", "Ink Color", [0.05, 0.04, 0.06])
            .end()
            .preset("Saturday Morning", &[])
            .preset(
                "Comic Book",
                &[
                    ("steps", &[4.0]),
                    ("soft", &[5.0]),
                    ("ink", &[140.0]),
                    ("width", &[0.3]),
                ],
            )
            .preset(
                "Anime",
                &[
                    ("steps", &[3.0]),
                    ("soft", &[35.0]),
                    ("ink", &[80.0]),
                    ("width", &[0.12]),
                    ("saturation", &[145.0]),
                ],
            )
            .preset("No Ink", &[("ink", &[0.0])])
            .draw(|b| {
                let s = super::blur::surface(b, b.src(), b.len("smooth"), b.k("edge"));
                cartoon(b, s)
            }),
        fx(
            "ComicPaint",
            "Comic Paint",
            "Painted strokes inside ink outlines.",
        )
        .num("size", "Brush Size", 1.2, 0.2, 5.0)
        .num("steps", "Color Steps", 7.0, 2.0, 16.0)
        .pct("soft", "Step Softness", 50.0)
        .num("saturation", "Saturation", 125.0, 0.0, 250.0)
        .num("ink", "Ink", 100.0, 0.0, 200.0)
        .num("width", "Line Width", 0.22, 0.05, 1.5)
        .within("from", "Edge Threshold", 14.0, 0.0, 60.0)
        .color("inkColor", "Ink Color", [0.05, 0.04, 0.06])
        .preset("Graphic Novel", &[])
        .preset(
            "Children's Book",
            &[
                ("size", &[2.0]),
                ("inkColor", &[0.3, 0.18, 0.1]),
                ("saturation", &[110.0]),
            ],
        )
        .draw(|b| {
            let s = oil(b, b.src(), b.len("size"), 0.1, 0.6);
            cartoon(b, s)
        }),
        fx(
            "PencilSketch",
            "Pencil Sketch",
            "A pencil's strokes on grainy paper.",
        )
        .num("strength", "Strength", 100.0, 0.0, 300.0)
        .num("size", "Detail Size", 1.2, 0.1, 6.0)
        .num("stroke", "Stroke Size", 0.35, 0.05, 2.0)
        .angle("angle", "Stroke Angle", 40.0)
        .color("ink", "Pencil", [0.16, 0.16, 0.18])
        .color("paper", "Paper", PAPER)
        .pct("color", "Picture's Colors", 0.0)
        .pct("grain", "Paper Grain", 55.0)
        .seed()
        .preset("Graphite", &[])
        .preset(
            "Colored Pencils",
            &[("color", &[100.0]), ("strength", &[130.0])],
        )
        .preset(
            "Red Chalk",
            &[
                ("ink", &[0.55, 0.16, 0.08]),
                ("paper", &[0.94, 0.87, 0.76]),
                ("stroke", &[0.6]),
            ],
        )
        .preset(
            "White on Black",
            &[("ink", &[0.92, 0.92, 0.95]), ("paper", &[0.05, 0.05, 0.06])],
        )
        .draw(|b| {
            let m = b.blur(b.src(), b.len("size"), 2);
            let n = nums![
                b.k("strength"),
                b.len("stroke"),
                b.a("angle"),
                b.color("ink"),
                b.color("paper"),
                b.k("color"),
                b.k("grain")
            ];
            b.run(k::PENCIL, &[b.src(), m], n, [0; 4])
        }),
        fx(
            "InkDrawing",
            "Ink Drawing",
            "Clean lines of ink, from the difference of two blurs.",
        )
        .num("strength", "Strength", 100.0, 0.0, 400.0)
        .num("width", "Line Width", 0.25, 0.05, 2.0)
        .within("from", "Threshold", 8.0, 0.0, 50.0)
        .pct("soft", "Softness", 25.0)
        .color("ink", "Ink", INK)
        .color("paper", "Paper", PAPER)
        .pct("keep", "Keep Picture", 0.0)
        .pct("grain", "Paper Grain", 25.0)
        .seed()
        .preset("Pen", &[])
        .preset("Brush", &[("width", &[0.7]), ("soft", &[10.0])])
        .preset("Lines Over", &[("keep", &[100.0]), ("grain", &[0.0])])
        .preset(
            "Blueprint",
            &[("ink", &[0.85, 0.93, 1.0]), ("paper", &[0.05, 0.16, 0.42])],
        )
        .draw(|b| {
            let w = b.len("width");
            let fine = b.blur(b.src(), w * 0.5, 1);
            let wide = b.blur(b.src(), w * 1.3, 1);
            let n = nums![
                b.k("strength") * 7.0,
                b.k("from"),
                b.k("soft") * 0.6,
                b.color("ink"),
                b.color("paper"),
                b.k("keep"),
                b.k("grain")
            ];
            b.run(k::INK, &[b.src(), fine, wide], n, [0; 4])
        }),
        fx(
            "Hatching",
            "Hatching",
            "Shading made of crossing pen strokes.",
        )
        .num("gap", "Line Spacing", 0.8, 0.2, 5.0)
        .within("width", "Line Width", 55.0, 5.0, 150.0)
        .int("ways", "Directions", 4, 1, 6)
        .angle("angle", "Angle", 45.0)
        .num("wobble", "Wobble", 40.0, 0.0, 200.0)
        .num("outline", "Outlines", 70.0, 0.0, 300.0)
        .color("ink", "Ink", INK)
        .color("paper", "Paper", PAPER)
        .pct("color", "Picture's Colors", 0.0)
        .seed()
        .preset("Pen & Ink", &[])
        .preset("Colored Hatching", &[("color", &[100.0]), ("ways", &[3.0])])
        .preset(
            "Rough",
            &[("gap", &[1.6]), ("wobble", &[140.0]), ("ways", &[2.0])],
        )
        .draw(|b| hatch(b, 0)),
        fx(
            "Engraving",
            "Engraving",
            "Lines that swell with the dark, as on a banknote.",
        )
        .num("gap", "Line Spacing", 0.7, 0.2, 5.0)
        .within("width", "Line Width", 85.0, 10.0, 150.0)
        .int("ways", "Directions", 1, 1, 3)
        .angle("angle", "Angle", 30.0)
        .num("wobble", "Wobble", 10.0, 0.0, 200.0)
        .num("outline", "Outlines", 30.0, 0.0, 300.0)
        .color("ink", "Ink", [0.08, 0.14, 0.12])
        .color("paper", "Paper", [0.93, 0.93, 0.85])
        .pct("color", "Picture's Colors", 0.0)
        .seed()
        .preset("Copper Plate", &[])
        .preset("Banknote", &[("ink", &[0.08, 0.3, 0.16]), ("gap", &[0.5])])
        .preset(
            "Woodcut",
            &[
                ("gap", &[1.6]),
                ("wobble", &[60.0]),
                ("ink", &[0.05, 0.03, 0.02]),
                ("paper", &[0.9, 0.82, 0.66]),
            ],
        )
        .draw(|b| hatch(b, 1)),
        fx(
            "Outlines",
            "Outlines",
            "The picture's outlines alone, in one color or their own.",
        )
        .num("strength", "Strength", 200.0, 0.0, 800.0)
        .within("from", "Threshold", 8.0, 0.0, 60.0)
        .pct("soft", "Softness", 30.0)
        .num("width", "Width", 0.15, 0.05, 1.5)
        .choice("colors", "Colors", 0, &["One Color", "By Direction"])
        .color("ink", "Line Color", WHITE)
        .pct("own", "Picture's Colors", 0.0)
        .color("paper", "Background", BLACK)
        .pct("keep", "Keep Picture", 0.0)
        .pct("strict", "Only One Direction", 0.0)
        .angle("angle", "Direction", 0.0)
        .handle(Handle::Arrow("", "angle"))
        .preset("White on Black", &[])
        .preset(
            "Black on White",
            &[("ink", &[0.0, 0.0, 0.0]), ("paper", &[1.0, 1.0, 1.0])],
        )
        .preset("Own Colors", &[("own", &[100.0])])
        .preset("By Direction", &[("colors", &[1.0])])
        .preset("Only Upright", &[("strict", &[100.0])])
        .draw(|b| {
            let n = nums![
                b.k("strength"),
                b.k("from"),
                b.k("soft") * 0.6,
                b.color("ink"),
                b.color("paper"),
                b.k("keep"),
                b.k("own"),
                b.a("angle"),
                b.k("strict"),
                b.px_y(b.len("width"))
            ];
            b.run(k::OUTLINE, &[b.src()], n, [0, b.i("colors"), 0, 0])
        }),
        fx(
            "DoubleOutlines",
            "Double Outlines",
            "The outlines of the outlines: thin twin lines.",
        )
        .num("strength", "Strength", 130.0, 0.0, 800.0)
        .num("smooth", "Smoothness", 0.4, 0.0, 3.0)
        .num("width", "Width", 0.15, 0.05, 1.0)
        .within("from", "Threshold", 8.0, 0.0, 60.0)
        .pct("soft", "Softness", 30.0)
        .color("ink", "Line Color", [0.6, 0.95, 1.0])
        .color("paper", "Background", [0.02, 0.03, 0.06])
        .pct("keep", "Keep Picture", 0.0)
        .preset("Wires", &[])
        .preset(
            "Drafting",
            &[("ink", &[0.1, 0.12, 0.2]), ("paper", &[0.95, 0.95, 0.92])],
        )
        .draw(|b| {
            let gap = b.px_y(b.len("width"));
            let s = b.blur(b.src(), b.len("smooth") * 0.5, 1);
            let n = nums![b.k("strength") * 4.0, gap.max(0.7)];
            let e = b.run(k::EDGES, &[s], n, [0; 4]);
            let n = nums![
                b.k("strength"),
                b.k("from"),
                b.k("soft") * 0.6,
                b.color("ink"),
                b.color("paper"),
                0.0,
                0.0,
                0.0,
                0.0,
                gap
            ];
            let o = b.run(k::OUTLINE, &[e], n, [0; 4]);
            b.mix(o, b.src(), b.k("keep"))
        }),
        emboss(
            fx("Emboss", "Emboss", "The picture's light raised as relief."),
            0.0,
            80.0,
        )
        .preset("Gray Plate", &[])
        .preset("In Color", &[("color", &[100.0])])
        .preset(
            "Coin",
            &[
                ("color", &[40.0]),
                ("shine", &[80.0]),
                ("smooth", &[0.5]),
                ("deep", &[200.0]),
            ],
        )
        .draw(|b| draw_emboss(b, 0)),
        emboss(
            fx(
                "Metal",
                "Metal",
                "Relief in polished metal, with a room reflected in it.",
            ),
            20.0,
            40.0,
        )
        .preset(
            "Chrome",
            &[("deep", &[160.0]), ("smooth", &[1.2]), ("shine", &[150.0])],
        )
        .preset(
            "Gold",
            &[
                ("shineColor", &[1.0, 0.8, 0.35]),
                ("color", &[10.0]),
                ("shine", &[180.0]),
                ("smooth", &[1.0]),
            ],
        )
        .draw(|b| draw_emboss(b, 1)),
        emboss(
            fx(
                "GlassRelief",
                "Glass Relief",
                "Relief in clear glass that bends what is behind it.",
            ),
            100.0,
            40.0,
        )
        .num("bend", "Bend", 1.5, -10.0, 10.0)
        .within("spread", "Color Spread", 20.0, -100.0, 100.0)
        .edges(2)
        .preset("Cast Glass", &[("smooth", &[1.2]), ("shine", &[80.0])])
        .preset(
            "Melting Ice",
            &[
                ("bend", &[4.0]),
                ("smooth", &[2.0]),
                ("shine", &[120.0]),
                ("shineColor", &[0.8, 0.95, 1.0]),
            ],
        )
        .draw(|b| draw_emboss(b, 2)),
    ]
}

/// Steps and ink, from the effect's settings.
fn cartoon(b: &mut B<'_>, s: Slot) -> Slot {
    let gap = b.px_y(b.len("width")) * 0.7;
    let soft = b.blur(s, b.len("width") * 0.5, 1);
    let e = b.run(k::EDGES, &[soft], nums![1.6, gap], [0; 4]);
    let n = nums![
        b.n("steps"),
        b.k("soft"),
        b.k("ink"),
        b.k("from"),
        b.color("inkColor"),
        b.k("saturation")
    ];
    b.run(k::CARTOON, &[b.src(), s, e], n, [0; 4])
}

fn hatch(b: &mut B<'_>, kind: i32) -> Slot {
    let s = b.blur(b.src(), b.len("gap") * 0.3, 1);
    let n = nums![
        b.len("gap"),
        b.k("width"),
        b.a("angle"),
        b.color("ink"),
        b.color("paper"),
        b.k("color"),
        b.k("wobble"),
        b.k("outline")
    ];
    b.run(k::HATCH, &[s], n, [b.i("ways"), kind, 0, 0])
}

fn emboss(fx: Fx, color: f64, deep: f64) -> Fx {
    fx.angle("light", "Light From", 135.0)
        .within("low", "Light Height", 60.0, 5.0, 200.0)
        .num("deep", "Depth", deep, -400.0, 400.0)
        .num("smooth", "Smoothness", 0.5, 0.02, 5.0)
        .pct("color", "Picture's Colors", color)
        .num("shine", "Shine", 30.0, 0.0, 300.0)
        .within("tight", "Shine Tightness", 20.0, 2.0, 120.0)
        .color("shineColor", "Shine Color", WHITE)
        .handle(Handle::Arrow("", "light"))
}

fn draw_emboss(b: &mut B<'_>, kind: i32) -> Slot {
    let m = b.blur(b.src(), b.len("smooth"), 1);
    let (bend, spread, edges) = if kind == 2 {
        (b.len("bend"), b.k("spread"), b.i("edges"))
    } else {
        (0.0, 0.0, 1)
    };
    let n = nums![
        b.a("light"),
        b.k("low"),
        b.k("deep"),
        b.k("color"),
        b.k("shine"),
        b.n("tight"),
        b.color("shineColor"),
        bend,
        spread
    ];
    b.run(k::EMBOSS, &[b.src(), m], n, [edges, kind, 0, 0])
}

fn printed() -> Vec<Fx> {
    vec![
        halftone(fx(
            "Halftone",
            "Halftone",
            "A printer's dots in one ink, two inks or four.",
        ))
        .choice(
            "inks",
            "Inks",
            0,
            &["One Ink", "Red Green Blue", "Four Inks"],
        )
        .preset("Newspaper", &[])
        .preset("Comic Print", &[("inks", &[2.0]), ("size", &[1.0])])
        .preset("Screens of Light", &[("inks", &[1.0]), ("size", &[0.8])])
        .preset("Lines", &[("shape", &[1.0]), ("angle", &[30.0])])
        .preset(
            "Banana Pop",
            &[
                ("ink", &[0.25, 0.1, 0.4]),
                ("paper", &[1.0, 0.86, 0.2]),
                ("size", &[1.6]),
            ],
        )
        .draw(|b| {
            let i = b.i("inks");
            draw_halftone(b, i)
        }),
        fx(
            "RingHalftone",
            "Ring Halftone",
            "One ink in rings round a place, thick and thin by the light.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("size", "Ring Spacing", 1.2, 0.2, 6.0)
        .pct("hard", "Hardness", 70.0)
        .within("contrast", "Contrast", 110.0, 20.0, 300.0)
        .color("ink", "Ink", INK)
        .color("paper", "Paper", PAPER)
        .handle(Handle::Point("center"))
        .preset("Vinyl", &[])
        .preset(
            "Radar",
            &[
                ("ink", &[0.0, 0.08, 0.03]),
                ("paper", &[0.3, 1.0, 0.45]),
                ("size", &[0.8]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.len("size"),
                0.0,
                b.k("hard"),
                b.color("ink"),
                b.color("paper"),
                b.k("contrast"),
                b.p("center")
            ];
            b.run(k::HALFTONE, &[b.src()], n, [0, 3, 0, 0])
        }),
        fx(
            "Misprint",
            "Misprint",
            "Cheap printing: the inks out of place on rough paper.",
        )
        .num("amount", "Amount", 0.5, 0.0, 5.0)
        .angle("angle", "Angle", 20.0)
        .num("rough", "Roughness", 45.0, 0.0, 200.0)
        .color("paper", "Paper", [0.97, 0.94, 0.86])
        .num("steps", "Color Steps", 6.0, 2.0, 32.0)
        .pct("soft", "Step Softness", 60.0)
        .seed()
        .preset("Zine", &[])
        .preset(
            "Riso",
            &[
                ("steps", &[3.0]),
                ("soft", &[20.0]),
                ("rough", &[110.0]),
                ("amount", &[0.9]),
            ],
        )
        .draw(|b| {
            let p = b.run(
                k::POSTERIZE,
                &[b.src()],
                nums![b.n("steps"), b.k("soft")],
                [0; 4],
            );
            let (l, a) = (b.len("amount"), b.a("angle"));
            let v = |t: f32| [(a + t).cos() * l, (a + t).sin() * l];
            let n = nums![v(0.0), v(2.1), v(4.2), b.k("rough"), b.color("paper")];
            b.run(k::MISPRINT, &[p], n, [0; 4])
        }),
        fx(
            "Mosaic",
            "Mosaic",
            "Blocks: squares, dots, diamonds, stones or hexagons.",
        )
        .choice(
            "shape",
            "Shape",
            0,
            &["Squares", "Dots", "Diamonds", "Stones", "Hexagons"],
        )
        .num("size", "Size", 3.0, 0.5, 20.0)
        .within("stretch", "Width", 100.0, 20.0, 500.0)
        .pct("gap", "Gaps", 0.0)
        .color("gapColor", "Gap Color", [0.05, 0.05, 0.06])
        .num("relief", "Relief", 0.0, 0.0, 200.0)
        .seed()
        .preset("Pixels", &[])
        .preset(
            "Roman Floor",
            &[
                ("shape", &[3.0]),
                ("size", &[2.4]),
                ("gap", &[16.0]),
                ("gapColor", &[0.82, 0.78, 0.7]),
                ("relief", &[30.0]),
            ],
        )
        .preset(
            "Building Blocks",
            &[("size", &[2.6]), ("gap", &[5.0]), ("relief", &[90.0])],
        )
        .preset("Hexagons", &[("shape", &[4.0]), ("gap", &[8.0])])
        .preset(
            "Pointillism",
            &[
                ("shape", &[1.0]),
                ("size", &[1.5]),
                ("gapColor", &[0.98, 0.96, 0.9]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.len("size"),
                b.k("stretch"),
                b.k("gap"),
                b.color("gapColor"),
                b.k("relief")
            ];
            b.run(k::MOSAIC, &[b.src()], n, [0, b.i("shape"), 0, 0])
        }),
        fx("LampWall", "Lamp Wall", "The picture on a wall of lamps.")
            .num("size", "Size", 1.8, 0.4, 10.0)
            .within("fill", "Lamp Size", 82.0, 10.0, 140.0)
            .num("glow", "Glow", 70.0, 0.0, 200.0)
            .color("wall", "Wall", [0.02, 0.02, 0.025])
            .num("steps", "Light Steps", 0.0, 0.0, 16.0)
            .hint("0: every shade.")
            .preset("LED Wall", &[])
            .preset(
                "Theater Bulbs",
                &[
                    ("size", &[4.0]),
                    ("glow", &[110.0]),
                    ("wall", &[0.12, 0.02, 0.02]),
                ],
            )
            .preset(
                "Toy Pegs",
                &[
                    ("size", &[2.4]),
                    ("glow", &[20.0]),
                    ("steps", &[4.0]),
                    ("fill", &[85.0]),
                ],
            )
            .draw(|b| {
                let n = nums![
                    b.len("size"),
                    b.k("fill"),
                    b.k("glow"),
                    b.color("wall"),
                    b.n("steps")
                ];
                b.run(k::LAMPS, &[b.src()], n, [0; 4])
            }),
        fx(
            "BandPass",
            "Detail Band",
            "Only the detail between two sizes, on gray or on the picture.",
        )
        .num("fine", "Finest", 0.3, 0.0, 10.0)
        .num("coarse", "Coarsest", 3.0, 0.1, 30.0)
        .num("amount", "Amount", 150.0, 0.0, 600.0)
        .pct("color", "Color", 0.0)
        .pct("keep", "Keep Picture", 0.0)
        .preset("Gray", &[])
        .preset(
            "Texture Boost",
            &[("keep", &[100.0]), ("amount", &[80.0]), ("color", &[100.0])],
        )
        .draw(|b| {
            let f = b.blur(b.src(), b.len("fine"), 2);
            let c = b.blur(b.src(), b.len("coarse"), 2);
            let n = nums![b.k("amount"), b.k("color"), b.k("keep")];
            b.run(k::BAND, &[b.src(), f, c], n, [0; 4])
        }),
        fx(
            "Psychedelic",
            "Psychedelic",
            "Colors run wild over the picture, by noise and by its light.",
        )
        .choice("kind", "Kind", 0, &["Clouds", "Stripes"])
        .num("size", "Size", 25.0, 2.0, 100.0)
        .num("times", "Rainbows", 1.4, 0.1, 6.0)
        .num("pull", "Follow Light", 100.0, -300.0, 300.0)
        .pct("vivid", "Vividness", 85.0)
        .pct("amount", "Amount", 80.0)
        .angle("turn", "Turn", 0.0)
        .num("speed", "Speed", 0.2, -3.0, 3.0)
        .num("phase", "Phase", 0.0, -10.0, 10.0)
        .seed()
        .moving()
        .preset("Lava Lamp", &[])
        .preset(
            "Trip Stripes",
            &[("kind", &[1.0]), ("size", &[12.0]), ("times", &[2.0])],
        )
        .preset(
            "Heat Dream",
            &[("times", &[0.5]), ("pull", &[220.0]), ("vivid", &[100.0])],
        )
        .draw(|b| {
            let n = nums![
                b.len("size"),
                b.n("times"),
                b.n("phase") + b.run_time("speed"),
                b.k("vivid"),
                b.k("amount"),
                b.k("pull"),
                b.a("turn")
            ];
            b.run(k::PSYCHE, &[b.src()], n, [0, b.i("kind"), 0, 0])
        }),
        fx(
            "LightZebra",
            "Light Zebra",
            "Stripes that follow the picture's light.",
        )
        .num("count", "Stripes", 6.0, 0.5, 40.0)
        .num("speed", "Speed", 0.3, -4.0, 4.0)
        .num("phase", "Phase", 0.0, -10.0, 10.0)
        .pct("hard", "Hardness", 60.0)
        .choice("by", "Stripe", 0, &["Two Colors", "Each Channel"])
        .color("c1", "Color 1", [0.05, 0.03, 0.12])
        .color("c2", "Color 2", [1.0, 0.85, 0.3])
        .pct("amount", "Amount", 100.0)
        .num("smooth", "Smooth First", 0.6, 0.0, 6.0)
        .moving()
        .preset("Contours", &[])
        .preset(
            "Solar Colors",
            &[("by", &[1.0]), ("count", &[3.0]), ("hard", &[0.0])],
        )
        .draw(|b| {
            let s = b.blur(b.src(), b.len("smooth") * 0.5, 1);
            let n = nums![
                b.n("count"),
                b.n("phase") + b.run_time("speed"),
                b.k("hard"),
                b.color("c1"),
                b.color("c2"),
                b.k("amount")
            ];
            b.run(k::ZEBRA_LIGHT, &[s], n, [0, b.i("by"), 0, 0])
        }),
        fx(
            "FrostedGlass",
            "Frosted Glass",
            "Pixels scattered at random, as by rough glass.",
        )
        .num("amount", "Amount", 1.5, 0.0, 15.0)
        .int("grain", "Grain", 1, 1, 12)
        .pct("along", "Along One Way", 0.0)
        .angle("angle", "Angle", 0.0)
        .num("rate", "Changes a Second", 0.0, 0.0, 60.0)
        .num("blur", "Smooth After", 0.0, 0.0, 3.0)
        .seed()
        .edges(2)
        .moving()
        .preset("Shower Door", &[("amount", &[2.5]), ("blur", &[0.15])])
        .preset("Sand", &[("amount", &[0.8]), ("grain", &[2.0])])
        .preset("Wind", &[("along", &[100.0]), ("amount", &[4.0])])
        .draw(|b| {
            let n = nums![
                b.len("amount"),
                b.n("grain"),
                b.n("rate"),
                b.a("angle"),
                b.k("along")
            ];
            let o = b.run(k::SCATTER, &[b.src()], n, [b.i("edges"), 0, 0, 0]);
            b.blur(o, b.len("blur"), 1)
        }),
    ]
}

fn halftone(fx: Fx) -> Fx {
    fx.num("size", "Size", 1.2, 0.2, 8.0)
        .choice("shape", "Shape", 0, &["Dots", "Lines", "Squares"])
        .angle("angle", "Angle", 45.0)
        .pct("hard", "Hardness", 75.0)
        .within("contrast", "Contrast", 105.0, 20.0, 300.0)
        .color("ink", "Ink", INK)
        .color("paper", "Paper", PAPER)
}

fn draw_halftone(b: &mut B<'_>, inks: i32) -> Slot {
    let s = b.blur(b.src(), b.len("size") * 0.2, 1);
    let n = nums![
        b.len("size"),
        b.a("angle"),
        b.k("hard"),
        b.color("ink"),
        b.color("paper"),
        b.k("contrast"),
        [0.0f32; 2]
    ];
    b.run(k::HALFTONE, &[s], n, [inks, b.i("shape"), 0, 0])
}

fn broken() -> Vec<Fx> {
    vec![
        fx(
            "DigitalGlitch",
            "Digital Glitch",
            "A broken digital signal: torn bands, moved blocks, lost colors.",
        )
        .num("amount", "Amount", 60.0, 0.0, 150.0)
        .num("size", "Block Size", 5.0, 0.5, 30.0)
        .num("rate", "Changes a Second", 10.0, 0.0, 60.0)
        .num("bands", "Torn Bands", 100.0, 0.0, 200.0)
        .num("blocks", "Moved Blocks", 100.0, 0.0, 200.0)
        .num("colors", "Lost Colors", 100.0, 0.0, 200.0)
        .num("split", "Colors Apart", 100.0, 0.0, 300.0)
        .seed()
        .moving()
        .preset("Bad Cable", &[])
        .preset(
            "Corrupted File",
            &[("blocks", &[200.0]), ("colors", &[200.0]), ("size", &[3.0])],
        )
        .preset(
            "Only Colors Apart",
            &[
                ("bands", &[0.0]),
                ("blocks", &[0.0]),
                ("colors", &[0.0]),
                ("split", &[200.0]),
            ],
        )
        .preset("Meltdown", &[("amount", &[130.0]), ("rate", &[24.0])])
        .draw(|b| {
            let n = nums![
                b.k("amount"),
                b.len("size"),
                b.n("rate"),
                b.k("split"),
                b.k("bands"),
                b.k("blocks"),
                b.k("colors")
            ];
            b.run(k::GLITCH, &[b.src()], n, [0; 4])
        }),
        fx(
            "Compression",
            "Compression",
            "A picture compressed too much: its 8 × 8 blocks show.",
        )
        .pct("quality", "Quality", 15.0)
        .num("color", "Color Loss", 200.0, 100.0, 800.0)
        .num("fine", "Detail Loss", 60.0, 0.0, 300.0)
        .pct("errors", "Broken Blocks", 0.0)
        .num("size", "Block Size", 100.0, 100.0, 800.0)
        .hint("100 as real ones; more draws the picture smaller first.")
        .num("rate", "Changes a Second", 0.0, 0.0, 60.0)
        .seed()
        .moving()
        .preset("Saved Too Often", &[])
        .preset("First Phones", &[("quality", &[6.0]), ("size", &[250.0])])
        .preset(
            "Broken Stream",
            &[("quality", &[10.0]), ("errors", &[20.0]), ("rate", &[8.0])],
        )
        .draw(|b| {
            // The blocks are of the full frame's pixels: a preview's
            // smaller picture is one drawn smaller already.
            let down = (b.k("size") * b.at.scale).max(0.01);
            let w = ((b.w() as f32 / down).round() as u32).clamp(8, b.w() * 8);
            let h = ((b.h() as f32 / down).round() as u32).clamp(8, b.h() * 8);
            let size = [w.min(8192), h.min(8192)];
            let s = resized(b, b.src(), size);
            let q = 1.0 - b.k("quality");
            let step = 0.008 + q * q * 0.75;
            let tick = (b.time() * b.n("rate")).floor().max(0.0) as u32;
            let seed = b.seed_of("seed").wrapping_add(tick.wrapping_mul(31));
            let a = b.run_sized(k::DCT_ROWS, &[s], nums![], [0; 4], size, seed);
            let n = nums![step, b.k("color"), b.k("errors") * 0.5, b.k("fine") * 0.5];
            let c = b.run_sized(k::DCT_COLUMNS, &[a], n, [0; 4], size, seed);
            let d = b.run_sized(k::IDCT_COLUMNS, &[c], nums![], [0; 4], size, seed);
            let e = b.run_sized(k::IDCT_ROWS, &[d], nums![], [0; 4], size, seed);
            let frame = [b.w(), b.h()];
            resized(b, e, frame)
        }),
        fx(
            "DataMosh",
            "Data Mosh",
            "Blocks that go on moving as they should not.",
        )
        .num("amount", "Amount", 6.0, 0.0, 40.0)
        .num("size", "Block Size", 3.5, 0.5, 20.0)
        .pct("share", "How Many", 55.0)
        .num("rate", "Rate", 1.5, 0.1, 15.0)
        .pct("follow", "Follow the Picture", 60.0)
        .seed()
        .moving()
        .preset("Mosh", &[])
        .preset(
            "Melting",
            &[("follow", &[100.0]), ("amount", &[16.0]), ("rate", &[0.6])],
        )
        .draw(|b| {
            let m = b.blur(b.src(), b.len("size") * 0.5, 1);
            let n = nums![
                b.len("amount"),
                b.len("size"),
                b.k("share"),
                b.n("rate"),
                b.k("follow")
            ];
            b.run(k::MOSH, &[b.src(), m], n, [0; 4])
        }),
        fx(
            "PixelSort",
            "Pixel Sort",
            "Runs of pixels sorted by their light.",
        )
        .pct("low", "From", 25.0)
        .pct("high", "To", 90.0)
        .num("length", "Reach", 20.0, 1.0, 100.0)
        .choice("way", "Direction", 0, &["Horizontal", "Vertical"])
        .toggle("reverse", "Reverse", false)
        .preset("Streaks", &[])
        .preset("Waterfall", &[("way", &[1.0]), ("length", &[40.0])])
        .preset("Only Highlights", &[("low", &[65.0]), ("high", &[100.0])])
        .draw(|b| {
            let long = b.px_y(b.len("length")) * 0.5;
            let n = (long as i32).clamp(6, 48);
            let numbers = nums![b.k("low"), b.k("high"), long];
            let ints = [b.i("way"), i32::from(b.on("reverse")), n, 0];
            b.run(k::SORT, &[b.src()], numbers, ints)
        }),
        fx(
            "Videotape",
            "Videotape",
            "A home tape: bleeding colors, wavering lines, tracking noise.",
        )
        .num("bleed", "Color Bleed", 0.6, 0.0, 4.0)
        .num("soft", "Softness", 100.0, 0.0, 300.0)
        .num("waver", "Waver", 100.0, 0.0, 400.0)
        .num("lines", "Tracking Noise", 60.0, 0.0, 200.0)
        .pct("bottom", "Bottom Band", 70.0)
        .num("noise", "Noise", 70.0, 0.0, 300.0)
        .num("drops", "Drop-Outs", 60.0, 0.0, 400.0)
        .pct("faded", "Faded", 35.0)
        .seed()
        .moving()
        .preset("Rental", &[])
        .preset(
            "Camcorder 1988",
            &[("lines", &[10.0]), ("bottom", &[40.0]), ("faded", &[50.0])],
        )
        .preset(
            "Worn Out",
            &[
                ("waver", &[260.0]),
                ("lines", &[150.0]),
                ("drops", &[250.0]),
                ("noise", &[150.0]),
            ],
        )
        .draw(|b| {
            let s = b.len("bleed") * 0.6 + 0.0016 * b.k("soft");
            let soft = b.blur_xy(b.src(), s, 0.0004, 1);
            let n = nums![
                b.len("bleed") * 0.6,
                b.k("waver"),
                b.k("lines"),
                b.k("bottom"),
                b.k("noise"),
                (b.k("soft") * 0.45).min(1.0),
                b.k("faded"),
                b.k("drops")
            ];
            b.run(k::TAPE, &[b.src(), soft], n, [0; 4])
        }),
        fx(
            "BadReception",
            "Bad Reception",
            "A television with a bad aerial: snow, rolling, ghosts and hum.",
        )
        .num("snow", "Snow", 45.0, 0.0, 200.0)
        .num("roll", "Roll", 0.0, -3.0, 3.0)
        .num("jitter", "Jitter", 60.0, 0.0, 300.0)
        .pct("ghosts", "Ghosts", 50.0)
        .num("hum", "Hum Bars", 60.0, 0.0, 200.0)
        .pct("lines", "Lines", 50.0)
        .pct("colors", "Failing Colors", 40.0)
        .seed()
        .moving()
        .preset("Countryside", &[])
        .preset(
            "Losing the Channel",
            &[("snow", &[130.0]), ("jitter", &[200.0]), ("roll", &[0.6])],
        )
        .preset(
            "Between Channels",
            &[
                ("snow", &[200.0]),
                ("colors", &[100.0]),
                ("jitter", &[300.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.k("snow"),
                b.n("roll"),
                b.k("jitter"),
                b.k("ghosts"),
                b.k("hum"),
                b.k("lines"),
                b.k("colors")
            ];
            b.run(k::AERIAL, &[b.src()], n, [0; 4])
        }),
        fx(
            "ScanLines",
            "Scan Lines",
            "A picture tube's lines, dots, curve and glow.",
        )
        .num("count", "Lines", 240.0, 20.0, 800.0)
        .pct("dark", "Darkness", 55.0)
        .pct("round", "Roundness", 60.0)
        .num("roll", "Roll", 0.0, -10.0, 10.0)
        .pct("dots", "Color Dots", 0.0)
        .pct("one", "One Color", 0.0)
        .color("color", "Color", [0.3, 1.0, 0.45])
        .pct("curve", "Glass Curve", 0.0)
        .pct("corners", "Dark Corners", 0.0)
        .num("glow", "Glow", 20.0, 0.0, 100.0)
        .moving()
        .preset("Lines", &[])
        .preset(
            "Arcade Cabinet",
            &[
                ("dots", &[70.0]),
                ("curve", &[45.0]),
                ("corners", &[60.0]),
                ("count", &[224.0]),
                ("glow", &[40.0]),
            ],
        )
        .preset(
            "Green Terminal",
            &[
                ("one", &[100.0]),
                ("glow", &[60.0]),
                ("count", &[200.0]),
                ("curve", &[30.0]),
            ],
        )
        .preset(
            "Amber Terminal",
            &[
                ("one", &[100.0]),
                ("color", &[1.0, 0.65, 0.15]),
                ("glow", &[60.0]),
                ("count", &[200.0]),
            ],
        )
        .preset(
            "Security Camera",
            &[
                ("one", &[100.0]),
                ("color", &[0.85, 0.92, 1.0]),
                ("roll", &[2.0]),
                ("corners", &[70.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.n("count"),
                b.k("dark"),
                b.k("round"),
                b.n("roll"),
                b.k("dots"),
                b.color("color"),
                b.k("one"),
                b.k("curve"),
                b.k("corners")
            ];
            let o = b.run(k::TUBE, &[b.src()], n, [0; 4]);
            let g = b.k("glow");
            if g <= 0.0 {
                return o;
            }
            let s = b.blur(o, 0.006, 1);
            b.run(k::GLOW, &[o, s], nums![g, [1.0; 3], 1.0], [1, 0, 0, 0])
        }),
    ]
}

/// A picture at another size.
fn resized(b: &mut B<'_>, s: Slot, size: [u32; 2]) -> Slot {
    let [w, h] = b.size(s);
    if [w, h] == size {
        return s;
    }
    // Smoothed first by as much as it shrinks.
    let k_x = w as f32 / size[0] as f32;
    let k_y = h as f32 / size[1] as f32;
    let soft = |r: f32| if r > 1.0 { r * 0.35 } else { 0.0 };
    let a = b.blur_px(s, soft(k_x), soft(k_y), 1);
    // A plane as it is: each place from the same place.
    let same = [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    b.run_sized(k::PLANE, &[a], nums![same, 0.0, 1.0], [1, 0, 0, 0], size, 0)
}

fn filmed() -> Vec<Fx> {
    vec![
        fx(
            "OldFilm",
            "Old Film",
            "Film that has been shown too often: scratches, dust, flicker and sway.",
        )
        .num("scratches", "Scratches", 60.0, 0.0, 150.0)
        .num("dust", "Dust", 60.0, 0.0, 300.0)
        .num("hairs", "Hairs", 40.0, 0.0, 200.0)
        .num("stains", "Stains", 20.0, 0.0, 150.0)
        .num("flicker", "Flicker", 60.0, 0.0, 300.0)
        .num("sway", "Sway", 0.3, 0.0, 5.0)
        .num("grain", "Grain", 60.0, 0.0, 300.0)
        .pct("faded", "Faded", 40.0)
        .pct("corners", "Dark Corners", 45.0)
        .within("fps", "Frames a Second", 18.0, 1.0, 60.0)
        .seed()
        .moving()
        .preset("Family Reel", &[])
        .preset(
            "Silent Era",
            &[
                ("faded", &[100.0]),
                ("flicker", &[120.0]),
                ("fps", &[16.0]),
                ("scratches", &[110.0]),
                ("corners", &[80.0]),
            ],
        )
        .preset(
            "Grindhouse",
            &[
                ("scratches", &[140.0]),
                ("dust", &[150.0]),
                ("stains", &[90.0]),
                ("faded", &[20.0]),
                ("fps", &[24.0]),
            ],
        )
        .preset(
            "Clean Projection",
            &[
                ("scratches", &[10.0]),
                ("dust", &[15.0]),
                ("hairs", &[0.0]),
                ("faded", &[0.0]),
                ("sway", &[0.08]),
                ("flicker", &[20.0]),
                ("fps", &[24.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.k("scratches"),
                b.k("dust"),
                b.k("flicker"),
                b.len("sway"),
                b.k("grain"),
                b.k("stains"),
                b.k("faded"),
                b.k("hairs"),
                b.k("corners"),
                b.n("fps")
            ];
            b.run(k::FILM, &[b.src()], n, [0; 4])
        }),
        fx(
            "FilmGrain",
            "Film Stock Grain",
            "Grain, by how bright the picture is, in color or not.",
        )
        .num("amount", "Amount", 22.0, 0.0, 100.0)
        .num("size", "Size", 1.6, 0.5, 8.0)
        .pct("color", "Color", 25.0)
        .pct("soft", "Softness", 60.0)
        .num("shadows", "Shadows", 90.0, 0.0, 200.0)
        .num("mids", "Middle", 100.0, 0.0, 200.0)
        .num("highs", "Highlights", 40.0, 0.0, 200.0)
        .num("rate", "Changes a Second", 24.0, 0.0, 60.0)
        .hint("0 keeps the grain still.")
        .seed()
        .moving()
        .preset("35 mm", &[("amount", &[14.0]), ("size", &[1.2])])
        .preset("16 mm", &[("amount", &[26.0]), ("size", &[2.2])])
        .preset(
            "Super 8",
            &[("amount", &[42.0]), ("size", &[3.5]), ("color", &[45.0])],
        )
        .preset(
            "Pushed Black & White",
            &[("amount", &[40.0]), ("color", &[0.0]), ("size", &[2.0])],
        )
        .preset("Still Texture", &[("rate", &[0.0])])
        .draw(|b| {
            let n = nums![
                b.k("amount") * 0.6,
                b.n("size"),
                b.k("color"),
                b.k("shadows"),
                b.k("mids"),
                b.k("highs"),
                b.k("soft"),
                b.n("rate")
            ];
            b.run(k::GRAIN, &[b.src()], n, [0; 4])
        }),
        fx(
            "SensorNoise",
            "Sensor Noise",
            "A camera's noise in the dark: colored specks and blotches.",
        )
        .num("amount", "Amount", 22.0, 0.0, 100.0)
        .num("blotch", "Blotches", 40.0, 0.0, 200.0)
        .num("size", "Blotch Size", 5.0, 1.0, 20.0)
        .num("shadows", "Shadows", 150.0, 0.0, 300.0)
        .num("highs", "Highlights", 20.0, 0.0, 200.0)
        .num("rate", "Changes a Second", 30.0, 0.0, 60.0)
        .seed()
        .moving()
        .preset("High ISO", &[])
        .preset(
            "Night Phone",
            &[("amount", &[38.0]), ("blotch", &[110.0]), ("size", &[8.0])],
        )
        .draw(|b| {
            let mid = (b.k("shadows") + b.k("highs")) * 0.5;
            let fine = nums![
                b.k("amount") * 0.55,
                1.0,
                0.7,
                b.k("shadows"),
                mid,
                b.k("highs"),
                0.0,
                b.n("rate")
            ];
            let o = b.run(k::GRAIN, &[b.src()], fine, [0; 4]);
            let big = nums![
                b.k("amount") * b.k("blotch") * 0.5,
                b.n("size"),
                1.0,
                b.k("shadows"),
                mid,
                b.k("highs"),
                1.0,
                b.n("rate")
            ];
            let seed = b.seed_of("seed").wrapping_add(4001);
            let size = [b.w(), b.h()];
            b.run_sized(k::GRAIN, &[o], big, [0; 4], size, seed)
        }),
        fx(
            "LensVignette",
            "Lens Vignette",
            "The dark corners of a lens, with a place and a color.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("radius", "Radius", 30.0, 0.0, 120.0)
        .num("soft", "Softness", 45.0, 1.0, 150.0)
        .pct("amount", "Amount", 70.0)
        .pct("round", "Roundness", 35.0)
        .color("color", "Color", BLACK)
        .pct("lights", "Protect Highlights", 30.0)
        .toggle("invert", "Invert", false)
        .handle(Handle::Point("center"))
        .handle(Handle::Ring("center", "radius"))
        .preset("Subtle", &[("amount", &[40.0]), ("soft", &[70.0])])
        .preset(
            "Porthole",
            &[
                ("round", &[100.0]),
                ("soft", &[2.0]),
                ("amount", &[100.0]),
                ("radius", &[40.0]),
                ("lights", &[0.0]),
            ],
        )
        .preset(
            "Faded Edges",
            &[("color", &[0.96, 0.93, 0.86]), ("amount", &[85.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.len("radius"),
                b.len("soft"),
                b.k("amount"),
                b.k("round"),
                b.color("color"),
                b.k("lights")
            ];
            b.run(
                k::VIGNETTE,
                &[b.src()],
                n,
                [i32::from(b.on("invert")), 0, 0, 0],
            )
        }),
        fx(
            "FilmBurn",
            "Film Burn",
            "The projector's light burning through: leaks and flashes that come and go.",
        )
        .num("brightness", "Brightness", 120.0, 0.0, 300.0)
        .num("rate", "Rate", 0.7, 0.05, 5.0)
        .pct("share", "How Often", 60.0)
        .num("size", "Size", 60.0, 5.0, 200.0)
        .color("c1", "Color 1", [0.75, 0.03, 0.0])
        .color("c2", "Color 2", [1.0, 0.45, 0.05])
        .color("c3", "Color 3", [1.0, 0.95, 0.7])
        .num("flash", "Flash", 35.0, 0.0, 200.0)
        .seed()
        .moving()
        .preset("End of the Reel", &[("rate", &[1.6]), ("share", &[90.0])])
        .preset("Rare", &[("rate", &[0.3]), ("share", &[40.0])])
        .draw(|b| {
            let t = b.time() * b.n("rate");
            let n = t.floor();
            let seed = b
                .seed_of("seed")
                .wrapping_add((n.max(0.0) as u32).wrapping_mul(313));
            let on = super::distort::hash01(3, seed) < b.k("share");
            let x = t - n;
            let power = if on {
                (x * std::f32::consts::PI).sin().powi(2) * b.k("brightness")
            } else {
                0.0
            };
            let numbers = nums![
                b.len("size"),
                0.4 + super::distort::hash01(5, seed) * 2.0,
                b.time() * 1.6,
                0.45,
                b.color("c1"),
                b.color("c2"),
                b.color("c3"),
                0.6
            ];
            let h = b.halvings(240);
            let size = [b.w() >> h, b.h() >> h].map(|n| n.max(8));
            let l = b.run_sized(k::LEAK, &[b.src()], numbers, [0, 2, 0, 0], size, seed);
            let l = super::light::enlarged(b, l);
            let o = b.run(
                k::GLOW,
                &[b.src(), l],
                nums![power, [1.0; 3], 1.0],
                [1, 0, 0, 0],
            );
            let c = b.color("c3");
            let f = power * b.k("flash") * 0.4;
            let wash = b.run(
                k::FILL,
                &[b.src()],
                nums![c[0] * f, c[1] * f, c[2] * f, 1.0],
                [0; 4],
            );
            b.run(k::GLOW, &[o, wash], nums![1.0, [1.0; 3], 1.0], [1, 0, 0, 0])
        }),
    ]
}
