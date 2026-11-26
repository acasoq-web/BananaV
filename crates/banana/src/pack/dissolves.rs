//! Dissolves: one picture gives way to the other while an effect swells and
//! goes.

use super::distort::{hash01, wander};
use crate::bolt::{self, Bolts, Guide};
use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::transition(id, name, Group::Dissolves, about)
}

/// What every dissolve with an effect has.
fn swell(fx: Fx) -> Fx {
    fx.num("amount", "Amount", 100.0, 0.0, 300.0)
        .within("overlap", "Overlap", 50.0, 2.0, 100.0)
        .hint("How much of the transition the pictures change places over.")
}

/// The pictures mixed as far as the time says, over `overlap` of it.
fn mixed(b: &mut B<'_>, overlap: f32) -> Slot {
    let t = b.at.progress.clamp(0.0, 1.0) as f32;
    let o = overlap.clamp(0.02, 1.0);
    let x = ((t - 0.5) / o + 0.5).clamp(0.0, 1.0);
    b.mix(b.src(), b.to(), x * x * (3.0 - 2.0 * x))
}

/// The mix, and how strong the effect is now.
fn start(b: &mut B<'_>) -> (Slot, f32) {
    let o = b.k("overlap");
    let m = mixed(b, o);
    (m, b.bell() * b.k("amount"))
}

fn bloom(b: &mut B<'_>, s: Slot, size: f32, threshold: f32) -> Slot {
    let hot = b.bright(s, threshold, 0.25, 1.1, 1.0);
    let a = b.blur(hot, size * 0.4, 0);
    let c = b.blur(a, size * 1.4, 0);
    b.run(k::SUM3, &[a, c], nums![0.6, 0.7, 0.0], [0; 4])
}

fn star_light(b: &mut B<'_>, s: Slot, lines: i32, length: f32, rainbow: f32, angle: f32) -> Slot {
    let hot = b.bright(s, 0.55, 0.2, 1.0, 1.0);
    let n = b.halvings(270);
    let small = b.smaller(hot, n, 0);
    let small = super::light::points(b, small, 0.8);
    let len = length * b.size(small)[1] as f32;
    let numbers = nums![len, angle, 2.0, rainbow, 0.7];
    let samples = ((len * 0.9) as i32).clamp(12, 56);
    let o = b.run_like(small, k::STAR, &[small], numbers, [lines, samples, 0, 0]);
    b.larger(o, n, 0)
}

fn add(b: &mut B<'_>, s: Slot, light: Slot, how: f32, color: [f32; 3]) -> Slot {
    b.run(k::GLOW, &[s, light], nums![how, color, 1.0], [1, 0, 0, 0])
}

fn progress(b: &B<'_>) -> f32 {
    b.at.progress.clamp(0.0, 1.0) as f32
}

pub fn list() -> Vec<Fx> {
    vec![
        fx(
            "CrossDissolve",
            "Eased Dissolve",
            "One picture fades into the other, along the curve you choose.",
        )
        .choice(
            "curve",
            "Curve",
            1,
            &["Linear", "Smooth", "Film", "Light First", "Dark First"],
        )
        .preset("Smooth", &[])
        .preset("Film", &[("curve", &[2.0])])
        .draw(|b| {
            let t = progress(b);
            match b.i("curve") {
                0 => b.mix(b.src(), b.to(), t),
                // Film's: each fades through its own exposure.
                2 => {
                    let x = (t * std::f32::consts::FRAC_PI_2).sin().powi(2);
                    let m = b.mix(b.src(), b.to(), x);
                    let hot = b.bright(m, 0.7, 0.3, 1.0, 1.0);
                    let g = b.blur(hot, 0.02, 0);
                    let k = b.bell() * 0.25;
                    add(b, m, g, k, [1.0, 0.95, 0.88])
                }
                n @ (3 | 4) => {
                    let numbers = nums![
                        t,
                        0.6,
                        0.01,
                        [0.0f32; 3],
                        0.0,
                        0.0,
                        [0.0f32; 2],
                        1.0,
                        1.0,
                        0.0,
                        0.0,
                        0.0
                    ];
                    let s = b.blur(b.src(), 0.01, 1);
                    b.run(
                        k::WIPE_LUMA,
                        &[b.src(), b.to(), s],
                        numbers,
                        [1, 0, i32::from(n == 4), 0],
                    )
                }
                _ => b.mix(b.src(), b.to(), t * t * (3.0 - 2.0 * t)),
            }
        }),
        fx(
            "DipToColor",
            "Dip to Color",
            "Fades to a color and out of it to the other picture.",
        )
        .color("color", "Color", [0.0, 0.0, 0.0])
        .pct("hold", "Hold", 0.0)
        .hint("How long it stays at the color.")
        .preset("Black", &[])
        .preset("White", &[("color", &[1.0, 1.0, 1.0])])
        .preset("Banana", &[("color", &[1.0, 0.84, 0.15])])
        .draw(|b| {
            let t = progress(b);
            let h = b.k("hold") * 0.9;
            let half = 0.5 * (1.0 - h);
            let c = b.color("color");
            let (s, x) = if t < 0.5 {
                (b.src(), (t / half).min(1.0))
            } else {
                (b.to(), ((1.0 - t) / half).min(1.0))
            };
            let fill = b.run(k::FILL, &[s], nums![c, 1.0], [0; 4]);
            b.mix(s, fill, x * x * (3.0 - 2.0 * x))
        }),
        swell(fx(
            "BlurDissolve",
            "Blur Dissolve",
            "Both pictures blur as they change places.",
        ))
        .num("size", "Blur", 5.0, 0.0, 30.0)
        .preset("Soft", &[("size", &[2.5])])
        .preset("Deep", &[("size", &[14.0]), ("overlap", &[30.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let s = b.len("size") * x;
            b.blur(m, s, 2)
        }),
        swell(fx(
            "DefocusDissolve",
            "Defocus Dissolve",
            "A lens pulls out of focus on one picture and in on the other.",
        ))
        .num("size", "Defocus", 3.0, 0.0, 8.0)
        .int("blades", "Blades", 6, 0, 12)
        .num("boost", "Highlights", 400.0, 0.0, 1000.0)
        .preset("Rack Focus", &[])
        .preset("City Lights", &[("size", &[5.0]), ("boost", &[900.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let r = b.px_y(b.len("size") * x);
            if r < 0.5 {
                return m;
            }
            let mut half = 0;
            while r / f32::from(1u16 << half) > 13.0 && half < 6 {
                half += 1;
            }
            let small = b.smaller(m, half, 1);
            let rr = r / f32::from(1u16 << half);
            let n = nums![
                rr,
                [1.0f32, 1.0],
                0.2,
                b.n("blades"),
                0.2,
                b.k("boost"),
                0.75,
                [0.0f32; 5],
                0.0
            ];
            let samples = ((rr * rr * 1.6) as i32).clamp(24, 130);
            let o = b.run_like(small, k::LENS, &[small], n, [1, 0, samples, 0]);
            let o = b.blur_px(o, 0.55, 0.55, 1);
            b.larger(o, half, 1)
        }),
        swell(fx(
            "TurbulentDissolve",
            "Turbulent Dissolve",
            "The pictures flow and bubble as they change.",
        ))
        .num("flow", "Flow", 12.0, 0.0, 60.0)
        .num("size", "Size", 22.0, 2.0, 100.0)
        .within("spread", "Color Spread", 10.0, -100.0, 100.0)
        .seed()
        .preset("Bubbling", &[])
        .preset("Vapor", &[("size", &[40.0]), ("flow", &[25.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![
                b.len("flow") * x,
                b.len("size"),
                b.k("spread") * 0.35,
                [0.0f32, 0.0],
                0.8,
                1.0
            ];
            b.run(k::TURBULENCE, &[m], n, [2, 3, 0, 0])
        }),
        swell(fx(
            "GlassDissolve",
            "Glass Dissolve",
            "The pictures bend through their own light, as molten glass.",
        ))
        .num("bend", "Bend", 25.0, 0.0, 100.0)
        .num("smooth", "Smoothness", 2.0, 0.1, 10.0)
        .within("spread", "Color Spread", 30.0, -100.0, 100.0)
        .preset("Molten", &[])
        .preset("Etched", &[("smooth", &[0.5]), ("bend", &[35.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let map = b.blur(m, b.len("smooth"), 2);
            let n = nums![b.k("bend") * 2.0 * x, 0.0, b.k("spread"), 0.0, 0.6];
            b.run(k::DISPLACE, &[m, map], n, [2, 5, 0, 0])
        }),
        swell(fx(
            "FrostDissolve",
            "Frost Dissolve",
            "The pictures break into scattered grains and gather again.",
        ))
        .num("reach", "Reach", 12.0, 0.0, 60.0)
        .int("grain", "Grain", 2, 1, 12)
        .seed()
        .preset("Frost", &[])
        .preset("Sandstorm", &[("reach", &[30.0]), ("grain", &[3.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![b.len("reach") * x, b.n("grain"), 0.0, 0.0, 0.0];
            b.run(k::SCATTER, &[m], n, [2, 0, 0, 0])
        }),
        swell(fx(
            "GlitchDissolve",
            "Glitch Dissolve",
            "The signal breaks on one picture and comes back on the other.",
        ))
        .num("size", "Block Size", 5.0, 0.5, 30.0)
        .num("rate", "Changes", 16.0, 1.0, 60.0)
        .seed()
        .preset("Hacked", &[])
        .preset("Meltdown", &[("amount", &[200.0]), ("rate", &[30.0])])
        .draw(|b| {
            let t = progress(b);
            // It jumps from one to the other and back before it stays.
            let tick = (t * b.n("rate")).floor();
            let h = hash01(tick as i32, b.seed_of("seed").wrapping_add(77));
            let o = b.k("overlap").max(0.02);
            let x = ((t - 0.5) / o + 0.5).clamp(0.0, 1.0);
            let s = if h < x { b.to() } else { b.src() };
            let p = b.bell() * b.k("amount");
            let n = nums![
                p * 1.3,
                b.len("size"),
                b.n("rate") / (1.0f32).max(b.at.length as f32 / b.at.rate as f32),
                1.0,
                1.0,
                1.0,
                1.0
            ];
            b.run(k::GLITCH, &[s], n, [0; 4])
        }),
        swell(fx(
            "PixelateDissolve",
            "Pixelate Dissolve",
            "The pictures' blocks grow, change and shrink.",
        ))
        .num("size", "Largest Block", 12.0, 1.0, 50.0)
        .choice(
            "shape",
            "Shape",
            0,
            &["Squares", "Dots", "Diamonds", "Stones", "Hexagons"],
        )
        .seed()
        .preset("Pixels", &[])
        .preset("Hexagons", &[("shape", &[4.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let size = b.len("size") * x;
            if b.px_y(size) < 1.5 {
                return m;
            }
            let n = nums![size, 1.0, 0.0, [0.0f32; 3], 0.0];
            b.run(k::MOSAIC, &[m], n, [0, b.i("shape"), 0, 0])
        }),
        swell(fx(
            "MoshDissolve",
            "Mosh Dissolve",
            "Blocks of one picture drag into the other.",
        ))
        .num("reach", "Reach", 18.0, 0.0, 60.0)
        .num("size", "Block Size", 4.0, 0.5, 20.0)
        .seed()
        .preset("Mosh", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let g = b.blur(m, b.len("size") * 0.5, 1);
            let n = nums![
                b.len("reach") * x,
                b.len("size"),
                0.4 + 0.6 * x.min(1.0),
                0.4,
                0.6
            ];
            b.run(k::MOSH, &[m, g], n, [0; 4])
        }),
        swell(fx(
            "SortDissolve",
            "Sort Dissolve",
            "Runs of pixels melt out of order and back.",
        ))
        .num("length", "Reach", 45.0, 1.0, 100.0)
        .choice("way", "Direction", 1, &["Horizontal", "Vertical"])
        .preset("Melt", &[])
        .preset("Sideways", &[("way", &[0.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let long = b.px_y(b.len("length")) * 0.5 * x;
            if long < 2.0 {
                return m;
            }
            let n = (long as i32).clamp(4, 44);
            let numbers = nums![0.05, 0.98, long];
            b.run(k::SORT, &[m], numbers, [b.i("way"), 1, n, 0])
        }),
        swell(fx(
            "GlowDissolve",
            "Glow Dissolve",
            "The pictures bloom into light and out of it.",
        ))
        .num("size", "Glow Size", 6.0, 0.5, 30.0)
        .pct("threshold", "Threshold", 30.0)
        .color("color", "Color", [1.0, 0.94, 0.84])
        .preset("White Hot", &[])
        .preset(
            "Magic",
            &[("color", &[0.8, 0.6, 1.0]), ("threshold", &[15.0])],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let (s, t) = (b.len("size"), b.k("threshold"));
            let g = bloom(b, m, s, t);
            let c = b.color("color");
            add(b, m, g, x * 1.35, c)
        }),
        swell(fx(
            "SuperGlowDissolve",
            "Super Glow Dissolve",
            "A bloom in two widths that washes over the pictures.",
        ))
        .num("size", "Glow Size", 3.0, 0.5, 20.0)
        .color("color", "Near Color", [1.0, 0.95, 0.85])
        .color("far", "Far Color", [0.55, 0.7, 1.0])
        .preset("Cinema", &[])
        .preset(
            "Fire",
            &[("color", &[1.0, 0.85, 0.4]), ("far", &[1.0, 0.25, 0.05])],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let s = b.len("size");
            let hot = b.bright(m, 0.25, 0.25, 1.1, 1.0);
            let n = b.blur(hot, s, 0);
            let f = b.blur(n, s * 4.0, 0);
            let (c, d) = (b.color("color"), b.color("far"));
            let o = add(b, m, n, x * 1.2, c);
            add(b, o, f, x * 1.0, d)
        }),
        swell(fx(
            "ExposureDissolve",
            "Exposure Dissolve",
            "The film is overexposed for a moment, and the other picture is there.",
        ))
        .color("color", "Color", [1.0, 0.92, 0.78])
        .num("wash", "Wash", 60.0, 0.0, 200.0)
        .preset("Film Flash", &[])
        .preset("Blue Flash", &[("color", &[0.75, 0.88, 1.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let c = b.color("color");
            let w = b.k("wash") * x * 0.6;
            let gain = 1.0 + x * 0.8;
            let rows = [
                [1.0f32, 0.0, 0.0, c[0] * w],
                [0.0, 1.0, 0.0, c[1] * w],
                [0.0, 0.0, 1.0, c[2] * w],
            ];
            let n = nums![rows[0], rows[1], rows[2], 1.0, 1.0, 1.0, gain];
            b.run(k::MATRIX, &[m], n, [0; 4])
        }),
        swell(fx(
            "RaysDissolve",
            "Rays Dissolve",
            "Light streams out of one picture and the other comes in it.",
        ))
        .point("center", "Center", [0.5, 0.5])
        .color("color", "Color", [1.0, 0.94, 0.8])
        .within("length", "Reach", 80.0, 0.0, 150.0)
        .pct("threshold", "Threshold", 35.0)
        .handle(Handle::Point("center"))
        .preset("Heaven", &[])
        .preset(
            "Edges",
            &[("threshold", &[15.0]), ("color", &[0.7, 0.9, 1.0])],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let hot = b.bright(m, b.k("threshold"), 0.2, 0.8, 1.0);
            let n = b.halvings(360);
            let small = b.smaller(hot, n, 0);
            let (c, col) = (b.p("center"), b.color("color"));
            let long = nums![c, b.k("length") * x.min(1.2), 1.0, 0.0, 1.0, 0.0, col, col];
            let a = b.run_like(small, k::RAYS, &[small], long, [0, 40, 0, 0]);
            let one = [1.0f32; 3];
            let short = nums![c, b.k("length") / 40.0, 0.2, 0.0, 1.0, 0.0, one, one];
            let r = b.run_like(small, k::RAYS, &[a], short, [0, 8, 0, 0]);
            let r = b.larger(r, n, 0);
            add(b, m, r, x * 1.4, one)
        }),
        swell(fx(
            "GlintDissolve",
            "Glint Dissolve",
            "Stars flare on the bright points while the pictures change.",
        ))
        .int("lines", "Lines", 2, 1, 8)
        .num("length", "Reach", 35.0, 1.0, 80.0)
        .angle("angle", "Angle", 20.0)
        .within("rainbow", "Rainbow", 0.0, 0.0, 100.0)
        .color("color", "Color", [1.0, 0.97, 0.9])
        .preset("Four Points", &[])
        .preset("Rainbow", &[("rainbow", &[90.0]), ("lines", &[3.0])])
        .preset(
            "Glare",
            &[("lines", &[6.0]), ("rainbow", &[35.0]), ("length", &[50.0])],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let (n, l, r, a) = (
                b.i("lines"),
                b.len("length") * x.clamp(0.05, 1.2),
                b.k("rainbow"),
                b.a("angle"),
            );
            let s = star_light(b, m, n, l, r, a);
            let c = b.color("color");
            add(b, m, s, x * 1.3, c)
        }),
        swell(fx(
            "FlareDissolve",
            "Flare Dissolve",
            "A lens flare sweeps across and the picture has changed.",
        ))
        .point("from", "From", [0.05, 0.7])
        .point("to", "To", [0.95, 0.6])
        .color("color", "Color", [1.0, 0.9, 0.75])
        .num("streak", "Streak", 150.0, 0.0, 300.0)
        .seed()
        .handle(Handle::Line("from", "to"))
        .preset("Sweep", &[])
        .preset(
            "Blue Sweep",
            &[("color", &[0.6, 0.8, 1.0]), ("streak", &[280.0])],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b);
            let (f, g) = (b.p("from"), b.p("to"));
            let at = [f[0] + (g[0] - f[0]) * t, f[1] + (g[1] - f[1]) * t];
            let n = nums![
                at,
                [0.0f32; 2],
                0.7 + 0.35 * x,
                b.color("color"),
                1.0,
                0.9,
                0.5,
                b.k("streak"),
                1.2,
                0.6,
                0.0,
                1.1,
                0.0
            ];
            let l = b.run(k::FLARE, &[m], n, [10, 9, 0, 0]);
            let o = add(b, m, l, x * 1.2, [1.0; 3]);
            let w = b.blur(l, 0.25, 0);
            add(b, o, w, x * 1.6, [1.0; 3])
        }),
        swell(fx(
            "LeakDissolve",
            "Leak Dissolve",
            "A light leak washes over the cut.",
        ))
        .num("size", "Size", 60.0, 5.0, 200.0)
        .color("c1", "Color 1", [0.8, 0.05, 0.02])
        .color("c2", "Color 2", [1.0, 0.45, 0.1])
        .color("c3", "Color 3", [1.0, 0.92, 0.6])
        .num("speed", "Speed", 2.0, 0.0, 8.0)
        .seed()
        .preset("Warm", &[])
        .preset(
            "Pink",
            &[
                ("c1", &[0.8, 0.05, 0.35]),
                ("c2", &[1.0, 0.4, 0.55]),
                ("c3", &[1.0, 0.88, 0.8]),
            ],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b);
            let n = nums![
                b.len("size"),
                0.5,
                t * b.n("speed") + b.seed_of("seed") as f32,
                0.25,
                b.color("c1"),
                b.color("c2"),
                b.color("c3"),
                0.6
            ];
            let h = b.halvings(240);
            let size = [b.w() >> h, b.h() >> h].map(|n| n.max(8));
            let seed = b.seed_of("seed");
            let l = b.run_sized(k::LEAK, &[m], n, [0, 0, 0, 0], size, seed);
            let l = super::light::enlarged(b, l);
            let c = b.color("c3");
            let o = add(b, m, l, x * 2.6, [1.0; 3]);
            let f = x * 0.45;
            let wash = b.run(
                k::FILL,
                &[m],
                nums![c[0] * f, c[1] * f, c[2] * f, 1.0],
                [0; 4],
            );
            add(b, o, wash, 1.0, [1.0; 3])
        }),
        swell(fx(
            "FlashDissolve",
            "Flash Dissolve",
            "Camera flashes burst over the cut.",
        ))
        .num("size", "Size", 10.0, 1.0, 40.0)
        .num("rate", "Flashes", 14.0, 2.0, 40.0)
        .color("color", "Color", [1.0, 0.98, 0.94])
        .seed()
        .preset("Paparazzi", &[])
        .draw(|b| {
            let t = progress(b);
            let tick = (t * b.n("rate")).floor();
            let age = t * b.n("rate") - tick;
            let seed = b.seed_of("seed").wrapping_add(tick as u32 * 131);
            let o = b.k("overlap").max(0.02);
            let x = ((t - 0.5) / o + 0.5).clamp(0.0, 1.0);
            let s = if hash01(tick as i32, seed) < x {
                b.to()
            } else {
                b.src()
            };
            let x = b.bell() * b.k("amount") * (-age * 2.2).exp();
            let shape = b.at.shape;
            let c = b.color("color");
            let mut o = s;
            for i in 0..2 {
                let at = [
                    (hash01(i * 3 + 1, seed) - 0.5) * shape * 0.8,
                    (hash01(i * 3 + 2, seed) - 0.5) * 0.7,
                ];
                let n = nums![
                    at,
                    [0.0f32; 2],
                    b.len("size") * 12.0,
                    c,
                    2.5,
                    1.2,
                    0.0,
                    0.6,
                    0.0,
                    0.0,
                    hash01(i, seed),
                    2.5,
                    0.0
                ];
                let l = b.run(k::FLARE, &[s], n, [8, 0, 0, 0]);
                o = b.run(k::GLOW, &[o, l], nums![x, [1.0; 3], 1.0], [1, 0, 0, 0]);
            }
            o
        }),
        swell(fx(
            "ShakeDissolve",
            "Shake Dissolve",
            "A blow shakes one picture into the other.",
        ))
        .num("move", "Move", 12.0, 0.0, 60.0)
        .num("turn", "Turn", 6.0, 0.0, 45.0)
        .num("rate", "Rate", 7.0, 1.0, 30.0)
        .pct("blur", "Motion Blur", 70.0)
        .seed()
        .preset("Impact", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b) * b.n("rate");
            let s = b.seed_of("seed").wrapping_mul(17);
            let at = |t: f32| {
                [
                    wander(t, s) * b.len("move") * x,
                    wander(t, s + 1) * b.len("move") * x,
                    wander(t, s + 2) * b.n("turn").to_radians() * x,
                ]
            };
            let (now, a, c) = (at(t), at(t - 0.12), at(t + 0.12));
            let p = super::distort::plane(
                [now[0], now[1], 0.0],
                [0.0, 0.0, now[2]],
                [1.0 + 0.1 * x; 2],
                [0.0; 2],
                0.0,
            );
            let blur = b.k("blur") * 2.0;
            let d = [(c[0] - a[0]) * blur, (c[1] - a[1]) * blur];
            let o = super::distort::on_plane(b, m, p, 0.0, 2);
            b.smear(o, d, 0.0, 2)
        }),
        swell(fx(
            "GrainDissolve",
            "Grain Dissolve",
            "Grain grows until it is the picture, and the other comes out of it.",
        ))
        .num("size", "Grain Size", 2.0, 0.5, 8.0)
        .pct("color", "Color", 0.0)
        .seed()
        .preset("Film", &[])
        .preset("Color Static", &[("color", &[100.0]), ("size", &[3.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![x * 1.1, b.n("size"), b.k("color"), 1.0, 1.0, 1.0, 0.4, 30.0];
            b.run(k::GRAIN, &[m], n, [0; 4])
        }),
        swell(fx(
            "StaticDissolve",
            "Static Dissolve",
            "The signal is lost in snow and another is found.",
        ))
        .num("snow", "Snow", 150.0, 0.0, 400.0)
        .num("jitter", "Jitter", 150.0, 0.0, 400.0)
        .num("roll", "Roll", 1.0, -4.0, 4.0)
        .seed()
        .preset("Lost Signal", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![
                b.k("snow") * x,
                b.n("roll") * x,
                b.k("jitter") * x,
                x.min(1.0) * 0.6,
                x * 0.8,
                x.min(1.0) * 0.5,
                x.min(1.0)
            ];
            b.run(k::AERIAL, &[m], n, [0; 4])
        }),
        swell(fx(
            "TileDissolve",
            "Tile Dissolve",
            "Tiles scatter out of one picture and settle as the other.",
        ))
        .num("rows", "Rows", 9.0, 1.0, 40.0)
        .num("move", "Move", 25.0, 0.0, 100.0)
        .num("turn", "Turn", 60.0, 0.0, 180.0)
        .seed()
        .preset("Scatter", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![
                b.n("rows"),
                1.0,
                b.len("move") * x,
                b.n("turn").to_radians() * x,
                1.0,
                0.0,
                0.0,
                [0.0f32; 3]
            ];
            b.run(k::TILES, &[m], n, [2, 0, 0, 0])
        }),
        swell(fx(
            "TwirlDissolve",
            "Twirl Dissolve",
            "A whirlpool takes one picture and gives back the other.",
        ))
        .point("center", "Center", [0.5, 0.5])
        .angle("turn", "Turn", 360.0)
        .num("radius", "Radius", 90.0, 5.0, 150.0)
        .handle(Handle::Point("center"))
        .preset("Whirlpool", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![
                b.p("center"),
                b.a("turn") * x,
                b.len("radius"),
                1.5,
                0.15 * x,
                0.0
            ];
            b.run(k::TWIRL, &[m], n, [2, 0, 0, 0])
        }),
        swell(fx(
            "WaveDissolve",
            "Wave Dissolve",
            "Waves run through the pictures as they change.",
        ))
        .num("height", "Height", 5.0, 0.0, 40.0)
        .num("length", "Reach", 25.0, 2.0, 100.0)
        .angle("angle", "Angle", 20.0)
        .preset("Sea", &[])
        .preset(
            "Flashback",
            &[("angle", &[90.0]), ("height", &[4.0]), ("length", &[18.0])],
        )
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b);
            let n = nums![
                b.len("height") * x,
                b.len("length"),
                b.a("angle"),
                t * 2.5,
                b.len("height") * x * 0.4,
                b.len("length") * 1.3,
                0.0
            ];
            b.run(k::WAVES, &[m], n, [2, 0, 0, 0])
        }),
        fx(
            "WaterRipple",
            "Water Ripple",
            "A ring of water spreads; inside it is the other picture.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("height", "Height", 2.5, 0.0, 12.0)
        .num("length", "Reach", 7.0, 1.0, 40.0)
        .num("width", "Ring Width", 22.0, 3.0, 80.0)
        .ease(0)
        .handle(Handle::Point("center"))
        .preset("Water Drop", &[])
        .draw(|b| {
            let n = nums![
                b.t().clamp(0.0, 1.0),
                b.p("center"),
                b.len("height"),
                b.len("length"),
                b.len("width")
            ];
            b.run(k::WATER, &[b.src(), b.to()], n, [0; 4])
        }),
        swell(fx(
            "LightningDissolve",
            "Lightning Dissolve",
            "Lightning strikes across and the picture has changed.",
        ))
        .point("from", "From", [0.2, 1.05])
        .point("to", "To", [0.8, -0.05])
        .int("bolts", "Bolts", 3, 1, 6)
        .color("color", "Color", [0.5, 0.65, 1.0])
        .num("flash", "Flash", 80.0, 0.0, 200.0)
        .seed()
        .handle(Handle::Line("from", "to"))
        .preset("Strike", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b);
            let tick = (t * 12.0).floor();
            let seed = b.seed_of("seed").wrapping_add(tick as u32 * 131);
            let reach = (t * 3.2).min(1.0);
            let look = Bolts {
                bolts: b.i("bolts"),
                branches: 4,
                branch: 0.7,
                branch_long: 1.0,
                stray: 0.22,
                fine: 1.0,
                hold: 0.0,
                reach,
                thin: 0.3,
                flicker: 0.45,
                width: 0.006,
                glow: 0.04,
                writhe: 0.0,
                strike: 0,
                seed,
            };
            let light = bolt::Light {
                brightness: x * 1.6,
                core: [1.0; 3],
                glow: b.color("color"),
            };
            let guide = Guide::through(&[b.p("from"), b.p("to")], false);
            let l = bolt::draw(b, &guide, &look, light);
            let o = b.run(k::GLOW, &[m, l], nums![1.0, [1.0; 3], 1.0], [0; 4]);
            let c = b.color("color");
            let beat = if hash01(tick as i32, seed) < 0.6 {
                1.0
            } else {
                0.3
            };
            let f = b.k("flash") * x * beat * 0.8;
            let wash = b.run(
                k::FILL,
                &[m],
                nums![c[0] * f, c[1] * f, c[2] * f, 1.0],
                [0; 4],
            );
            add(b, o, wash, 1.0, [1.0; 3])
        }),
        swell(fx(
            "PaintDissolve",
            "Paint Dissolve",
            "The pictures melt into strokes of paint and out of them.",
        ))
        .num("size", "Brush Size", 4.0, 0.5, 10.0)
        .preset("Canvas", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let r = b.px_y(b.len("size") * x);
            if r < 1.0 {
                return m;
            }
            let mut half = 0;
            while r / f32::from(1u16 << half) > 8.0 && half < 4 {
                half += 1;
            }
            let small = b.smaller(m, half, 1);
            let rr = r / f32::from(1u16 << half);
            let lean = b.blur_px(small, rr * 0.6 + 0.8, rr * 0.6 + 0.8, 1);
            let n = (rr.round() as i32).clamp(2, 6);
            let o = b.run_like(
                small,
                k::OIL,
                &[small, lean],
                nums![rr, 0.1, 0.7],
                [n, 0, 0, 0],
            );
            let o = b.larger(o, half, 1);
            // The finer the brush the more of the picture itself.
            b.mix(m, o, (r / 3.0).min(1.0))
        }),
        swell(fx(
            "KaleidoDissolve",
            "Kaleidoscope Dissolve",
            "The pictures turn in a kaleidoscope as they change.",
        ))
        .int("sides", "Sides", 6, 2, 24)
        .angle("turn", "Turn", 120.0)
        .preset("Six Sides", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b);
            let n = nums![
                [0.0f32; 2],
                0.0,
                1.0 + x * 0.3,
                b.n("sides"),
                b.a("turn") * (t - 0.5),
                [0.0f32; 2]
            ];
            let o = b.run(k::KALEIDO, &[m], n, [2, 0, 0, 0]);
            b.mix(m, o, (x * 1.6).min(1.0))
        }),
        swell(fx(
            "AberrationDissolve",
            "Aberration Dissolve",
            "The colors pull apart and come together on the other picture.",
        ))
        .num("zoom", "Zoom", 25.0, 0.0, 100.0)
        .angle("turn", "Turn", 0.0)
        .preset("Burst", &[])
        .preset("Twist", &[("turn", &[15.0])])
        .draw(|b| {
            let (m, x) = start(b);
            let n = nums![
                [0.0f32; 2],
                b.k("zoom") * x,
                b.a("turn") * x,
                0.0,
                [0.0f32; 2]
            ];
            b.run(k::RAINBOW, &[m], n, [2, 24, 0, 0])
        }),
        swell(fx(
            "TapeDissolve",
            "Tape Dissolve",
            "The tape loses its tracking and finds another picture.",
        ))
        .num("bleed", "Color Bleed", 1.5, 0.0, 6.0)
        .seed()
        .preset("Recorded Over", &[])
        .draw(|b| {
            let (m, x) = start(b);
            let s = b.len("bleed") * x * 0.6 + 0.001;
            let soft = b.blur_xy(m, s, 0.0004, 1);
            let y = x.min(1.0);
            let n = nums![
                b.len("bleed") * x * 0.6,
                x * 8.0,
                x * 2.5,
                y,
                x * 2.0,
                y * 0.6,
                y * 0.3,
                x * 6.0
            ];
            let o = b.run(k::TAPE, &[m, soft], n, [0; 4]);
            b.mix(m, o, (x * 3.0).min(1.0))
        }),
        swell(fx(
            "SmokeDissolve",
            "Smoke Dissolve",
            "Smoke rolls over one picture and clears on the other.",
        ))
        .color("color", "Color", [0.82, 0.84, 0.88])
        .num("size", "Size", 45.0, 5.0, 150.0)
        .num("rise", "Rise", 0.6, -3.0, 3.0)
        .seed()
        .preset("Gray Smoke", &[])
        .preset("Ink", &[("color", &[0.02, 0.03, 0.08]), ("rise", &[-0.4])])
        .preset("Purple", &[("color", &[0.5, 0.25, 0.8])])
        .draw(|b| {
            let (m, x) = start(b);
            let t = progress(b);
            let c = b.color("color");
            let n = nums![
                [0.0f32; 2],
                b.len("size"),
                0.0,
                1.0,
                1.6,
                c.map(|v| v * 0.55),
                c,
                [0.0f32; 3],
                t * 3.0,
                0.0,
                b.n("rise") * 0.6,
                0.9
            ];
            let s = b.run(k::CLOUDS, &[m], n, [6, 2, 0, 2]);
            // The smoke's own pattern thickens until nothing shows.
            let thick = x.min(1.0);
            let o = b.run(k::LAY, &[m, s], nums![(x * 1.8).min(1.0)], [0; 4]);
            let fill = b.run(k::FILL, &[m], nums![c.map(|v| v * 0.8), 1.0], [0; 4]);
            let k1 = ((thick - 0.55) / 0.45).clamp(0.0, 1.0);
            let dense = b.run(k::LAY, &[fill, s], nums![1.0], [0; 4]);
            b.mix(o, dense, k1 * k1 * (3.0 - 2.0 * k1))
        }),
    ]
}
