//! Transitions of motion: the pictures slide, turn, whip and flip.

use super::distort::{hash01, inverse, on_plane, plane};
use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::transition(id, name, Group::Motion, about)
}

fn progress(b: &B<'_>) -> f32 {
    b.at.progress.clamp(0.0, 1.0) as f32
}

/// The first picture before the middle, the second from it.
fn half(b: &B<'_>) -> Slot {
    if b.at.progress < 0.5 { b.src() } else { b.to() }
}

/// A whip's way: how far a picture has gone, from 0 out to 1 (the first)
/// or from -1 in to 0 (the second), and how fast it goes now.
fn whip(t: f32, sharp: f32) -> (f32, f32) {
    let s = sharp.max(0.2);
    let f = |t: f32| -> f32 {
        if t < 0.5 {
            0.5 * (2.0 * t).powf(1.0 + s * 2.0)
        } else {
            1.0 - 0.5 * (2.0 * (1.0 - t)).powf(1.0 + s * 2.0)
        }
    };
    let e = 0.01;
    (
        f(t),
        (f((t + e).min(1.0)) - f((t - e).max(0.0))) / (2.0 * e),
    )
}

/// A picture on a plane over a color (or over nothing).
fn card(b: &mut B<'_>, s: Slot, m: [f32; 9], shade: f32) -> Slot {
    let Some(inv) = inverse(m) else {
        return b.run(k::FILL, &[s], nums![0.0, 0.0, 0.0, 0.0], [0; 4]);
    };
    b.run(k::CARD, &[s], nums![inv, shade, 0.0], [0, 2, 0, 0])
}

fn background(b: &mut B<'_>, s: Slot) -> Slot {
    let c = b.color("background");
    let fill = b.run(k::FILL, &[s], nums![c, 1.0], [0; 4]);
    b.run(k::LAY, &[fill, s], nums![1.0], [0; 4])
}

fn direction(b: &B<'_>) -> [f32; 2] {
    let a = b.a("angle");
    [a.cos(), a.sin()]
}

pub fn list() -> Vec<Fx> {
    vec![
        fx(
            "SlidePush",
            "Slide Push",
            "The pictures slide: both, the new one over, or the old one off.",
        )
        .choice("kind", "Moves", 0, &["Both", "New One Over", "Old One Off"])
        .angle("angle", "Angle", 0.0)
        .num("gap", "Gap", 0.0, 0.0, 30.0)
        .color("gapColor", "Gap Color", [0.0, 0.0, 0.0])
        .pct("blur", "Motion Blur", 40.0)
        .ease(1)
        .handle(Handle::Arrow("", "angle"))
        .preset("Push", &[])
        .preset("Cover", &[("kind", &[1.0])])
        .preset("Reveal", &[("kind", &[2.0])])
        .preset("Film Strip", &[("gap", &[6.0]), ("angle", &[90.0])])
        .draw(|b| {
            let n = direction(b);
            let t = b.t();
            let len = b.at.shape * n[0].abs() + n[1].abs();
            let numbers = nums![t, n, b.len("gap"), b.color("gapColor")];
            let kind = b.i("kind");
            let o = b.run(k::SLIDE, &[b.src(), b.to()], numbers, [2, kind, 0, 0]);
            if kind != 0 {
                return o;
            }
            let frames = (b.at.length as f32).max(2.0);
            let d = len * b.k("blur") * b.bell() * 2.0 / frames;
            b.smear(o, [n[0] * d, n[1] * d], 0.0, 1)
        }),
        fx(
            "WhipPan",
            "Whip Pan",
            "The camera whips away from one picture onto the other.",
        )
        .angle("angle", "Angle", 0.0)
        .num("length", "Distance", 100.0, 20.0, 400.0)
        .within("sharp", "Snap", 100.0, 0.0, 300.0)
        .num("blur", "Motion Blur", 100.0, 0.0, 300.0)
        .within("spread", "Color Spread", 0.0, 0.0, 100.0)
        .handle(Handle::Arrow("", "angle"))
        .preset("Left", &[])
        .preset("Down", &[("angle", &[90.0])])
        .preset("Far Whip", &[("length", &[250.0]), ("blur", &[150.0])])
        .draw(|b| {
            let n = direction(b);
            let (p, v) = whip(progress(b), b.k("sharp"));
            let len = (b.at.shape * n[0].abs() + n[1].abs()) * b.k("length");
            let off = if b.at.progress < 0.5 { p } else { p - 1.0 };
            let m = plane(
                [-n[0] * off * len, -n[1] * off * len, 0.0],
                [0.0; 3],
                [1.0; 2],
                [0.0; 2],
                0.0,
            );
            let o = on_plane(b, half(b), m, 0.0, 2);
            let d = v * len * 0.16 * b.k("blur");
            let o = b.smear(o, [n[0] * d, n[1] * d], 0.0, 2);
            let s = b.k("spread") * b.bell();
            if s <= 0.0 {
                return o;
            }
            let line = [n[0] * s * 0.12, n[1] * s * 0.12];
            b.run(
                k::RAINBOW,
                &[o],
                nums![[0.0f32; 2], 0.0, 0.0, 0.0, line],
                [2, 14, 1, 0],
            )
        }),
        fx(
            "ZoomPunch",
            "Zoom Punch",
            "Rushes into one picture and out of the other.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("zoom", "Zoom", 250.0, 110.0, 800.0)
        .choice("way", "Way", 0, &["In", "Out"])
        .angle("spin", "Spin", 0.0)
        .within("sharp", "Snap", 100.0, 0.0, 300.0)
        .num("blur", "Motion Blur", 100.0, 0.0, 300.0)
        .within("spread", "Color Spread", 15.0, 0.0, 100.0)
        .handle(Handle::Point("center"))
        .preset("Punch In", &[])
        .preset("Pull Out", &[("way", &[1.0])])
        .preset("Drill", &[("spin", &[90.0]), ("zoom", &[350.0])])
        .draw(|b| {
            let (p, v) = whip(progress(b), b.k("sharp"));
            let c = b.p("center");
            let z = b.k("zoom").max(1.05).ln() * if b.i("way") == 1 { -1.0 } else { 1.0 };
            let off = if b.at.progress < 0.5 { p } else { p - 1.0 };
            let turn = b.a("spin") * off;
            let m = plane(
                [c[0], c[1], 0.0],
                [0.0, 0.0, turn],
                [(z * off * 2.0).exp(); 2],
                [0.0; 2],
                0.0,
            );
            // About the place, not the frame's middle.
            let back = plane([-c[0], -c[1], 0.0], [0.0; 3], [1.0; 2], [0.0; 2], 0.0);
            let m = super::distort::times(m, back);
            let s = b.k("spread") * b.bell();
            let o = on_plane(b, half(b), m, 0.0, 2);
            let d = v * 0.22 * b.k("blur");
            let o = b.spin(o, c, z * d, b.a("spin") * d * 0.5, 0.0, 2, (0.0, 0.0));
            if s <= 0.0 {
                return o;
            }
            b.run(
                k::RAINBOW,
                &[o],
                nums![c, s * 0.25, 0.0, 0.0, [0.0f32; 2]],
                [2, 14, 0, 0],
            )
        }),
        fx("SpinWhip", "Spin Whip", "Spins one picture into the other.")
            .point("center", "Center", [0.5, 0.5])
            .angle("spin", "Spin", 180.0)
            .num("zoom", "Zoom", 120.0, 50.0, 300.0)
            .within("sharp", "Snap", 100.0, 0.0, 300.0)
            .num("blur", "Motion Blur", 100.0, 0.0, 300.0)
            .handle(Handle::Point("center"))
            .preset("Half Turn", &[])
            .preset("Full Turn", &[("spin", &[360.0])])
            .draw(|b| {
                let (p, v) = whip(progress(b), b.k("sharp"));
                let c = b.p("center");
                let off = if b.at.progress < 0.5 { p } else { p - 1.0 };
                let a = b.a("spin");
                let z = 1.0 + (b.k("zoom") - 1.0) * b.bell();
                let m = plane(
                    [c[0], c[1], 0.0],
                    [0.0, 0.0, a * off],
                    [z; 2],
                    [0.0; 2],
                    0.0,
                );
                let back = plane([-c[0], -c[1], 0.0], [0.0; 3], [1.0; 2], [0.0; 2], 0.0);
                let o = on_plane(b, half(b), super::distort::times(m, back), 0.0, 2);
                let d = v * 0.16 * b.k("blur");
                b.spin(o, c, 0.0, a * d, 0.0, 2, (0.0, 0.0))
            }),
        fx(
            "ElasticWhip",
            "Elastic Whip",
            "The picture stretches, snaps and settles as the other.",
        )
        .angle("angle", "Angle", 0.0)
        .num("stretch", "Stretch", 250.0, 110.0, 800.0)
        .num("blur", "Motion Blur", 100.0, 0.0, 300.0)
        .within("spread", "Color Spread", 25.0, 0.0, 100.0)
        .handle(Handle::Arrow("", "angle"))
        .preset("Snap", &[])
        .draw(|b| {
            let t = progress(b);
            let (p, v) = whip(t, 1.0);
            let a = b.a("angle");
            let bell = b.bell();
            let k1 = 1.0 + (b.k("stretch") - 1.0) * bell;
            let turned = plane([0.0; 3], [0.0, 0.0, a], [1.0; 2], [0.0; 2], 0.0);
            let back = plane([0.0; 3], [0.0, 0.0, -a], [1.0; 2], [0.0; 2], 0.0);
            let off = if t < 0.5 { p } else { p - 1.0 };
            let wide = plane([-off * 0.8, 0.0, 0.0], [0.0; 3], [k1, 1.0], [0.0; 2], 0.0);
            let m = super::distort::times(turned, super::distort::times(wide, back));
            let o = on_plane(b, half(b), m, 0.0, 2);
            let d = v * 0.1 * b.k("blur") * k1;
            let n = direction(b);
            let o = b.smear(o, [n[0] * d, n[1] * d], 0.0, 2);
            let s = b.k("spread") * bell;
            let line = [n[0] * s * 0.15, n[1] * s * 0.15];
            b.run(
                k::RAINBOW,
                &[o],
                nums![[0.0f32; 2], 0.0, 0.0, 0.0, line],
                [2, 14, 1, 0],
            )
        }),
        fx(
            "CardFlip",
            "Card Flip",
            "The picture turns over in space; the other is on its back.",
        )
        .choice("axis", "Turns About", 0, &["Upright", "Level", "Diagonal"])
        .num("depth", "Move Away", 40.0, 0.0, 200.0)
        .within("view", "Perspective", 80.0, 0.0, 200.0)
        .within("tilt", "Tilt", 0.0, -60.0, 60.0)
        .pct("shade", "Shade", 50.0)
        .color("background", "Background", [0.0, 0.0, 0.0])
        .ease(1)
        .preset("Flip Over", &[])
        .preset("Over the Top", &[("axis", &[1.0])])
        .preset("Playing Card", &[("depth", &[90.0]), ("tilt", &[15.0])])
        .draw(|b| {
            let t = b.t().clamp(0.0, 1.0);
            let bell = (t * std::f32::consts::PI).sin();
            let first = t < 0.5;
            let a = if first { t } else { t - 1.0 } * std::f32::consts::PI;
            let s = if first { b.src() } else { b.to() };
            let tilt = b.n("tilt").to_radians() * bell;
            let turns = match b.i("axis") {
                1 => [tilt, a, 0.0],
                2 => [a, 0.0, 0.6 * bell],
                _ => [a, tilt, 0.0],
            };
            let m = plane(
                [0.0, 0.0, b.k("depth") * bell],
                turns,
                [1.0; 2],
                [0.0; 2],
                b.k("view"),
            );
            let shade = 1.0 - b.k("shade") * bell;
            let o = card(b, s, m, shade);
            background(b, o)
        }),
        fx(
            "CubeTurn",
            "Cube Turn",
            "The pictures are two sides of a cube that turns.",
        )
        .choice("way", "Turns To", 0, &["Left", "Right", "Up", "Down"])
        .num("depth", "Move Away", 25.0, 0.0, 200.0)
        .within("view", "Perspective", 70.0, 5.0, 200.0)
        .pct("shade", "Shade", 60.0)
        .color("background", "Background", [0.0, 0.0, 0.0])
        .ease(1)
        .preset("Left", &[])
        .preset("Up", &[("way", &[2.0])])
        .draw(|b| {
            let t = b.t().clamp(0.0, 1.0);
            let way = b.i("way");
            let level = way < 2;
            let sign = if way == 0 || way == 2 { 1.0 } else { -1.0 };
            let half_size = if level { b.at.shape * 0.5 } else { 0.5 };
            let bell = (t * std::f32::consts::PI).sin();
            let q = std::f32::consts::FRAC_PI_2;
            let away = b.k("depth") * bell;
            let view = b.k("view");
            let shade = b.k("shade");
            // A face whose turn is `a`: its middle on the cube's round.
            let face = |a: f32| -> [f32; 9] {
                let side = -a.sin() * half_size;
                let back = (1.0 - a.cos()) * half_size + away;
                if level {
                    plane([side, 0.0, back], [-a, 0.0, 0.0], [1.0; 2], [0.0; 2], view)
                } else {
                    plane([0.0, side, back], [0.0, a, 0.0], [1.0; 2], [0.0; 2], view)
                }
            };
            let a = sign * t * q;
            let (fa, fb) = (face(a), face(a - sign * q));
            let one = card(b, b.src(), fa, 1.0 - shade * (1.0 - (t * q).cos()));
            let two = card(b, b.to(), fb, 1.0 - shade * (1.0 - ((1.0 - t) * q).cos()));
            // The side that faces more is over the other.
            let o = if t < 0.5 {
                b.run(k::LAY, &[two, one], nums![1.0], [0; 4])
            } else {
                b.run(k::LAY, &[one, two], nums![1.0], [0; 4])
            };
            background(b, o)
        }),
        fx(
            "SwingIn",
            "Swing",
            "The camera swings past one picture onto the other, with blur.",
        )
        .angle("turn", "Swing", 55.0)
        .angle("roll", "Roll", 10.0)
        .within("view", "Perspective", 100.0, 10.0, 200.0)
        .num("blur", "Motion Blur", 100.0, 0.0, 300.0)
        .preset("Right", &[])
        .preset("Left", &[("turn", &[-55.0])])
        .draw(|b| {
            let t = progress(b);
            let (p, v) = whip(t, 0.8);
            let off = if t < 0.5 { p } else { p - 1.0 };
            let a = b.f64("turn").to_radians() as f32;
            let bell = b.bell();
            let m = plane(
                [-off * b.at.shape * 0.8, 0.0, 0.0],
                [a * off * 2.0, 0.0, b.a("roll") * bell],
                [1.0 + 0.25 * bell; 2],
                [0.0; 2],
                b.k("view"),
            );
            let o = on_plane(b, half(b), m, 0.0, 2);
            let d = v * 0.12 * b.k("blur") * a.signum();
            b.smear(o, [d, 0.0], 0.0, 2)
        }),
        fx(
            "DoorsOpen",
            "Doors",
            "The picture splits like doors, sliding, swinging or squeezed.",
        )
        .choice("kind", "Doors", 1, &["Slide", "Swing", "Squeeze"])
        .angle("angle", "Angle", 0.0)
        .pct("shade", "Shade", 60.0)
        .ease(1)
        .preset("Swing Open", &[])
        .preset("Elevator", &[("kind", &[0.0])])
        .preset("Hatch", &[("kind", &[0.0]), ("angle", &[90.0])])
        .draw(|b| {
            let n = nums![b.t().clamp(0.0, 1.0), b.a("angle"), b.k("shade")];
            b.run(k::DOORS, &[b.src(), b.to()], n, [0, b.i("kind"), 0, 0])
        }),
        fx(
            "StripSlide",
            "Strip Slide",
            "Strips of the picture slide away (or turn over) one by one.",
        )
        .choice(
            "kind",
            "Strips",
            0,
            &["Alternate", "One Way", "Random", "Turn Over"],
        )
        .num("count", "Strips", 12.0, 1.0, 60.0)
        .angle("angle", "Angle", 0.0)
        .num("uneven", "Delay", 60.0, 0.0, 200.0)
        .seed()
        .ease(1)
        .handle(Handle::Arrow("", "angle"))
        .preset("Interleaved", &[])
        .preset("Venetian", &[("kind", &[3.0]), ("uneven", &[30.0])])
        .preset("Scattered", &[("kind", &[2.0]), ("count", &[24.0])])
        .draw(|b| {
            let n = nums![
                b.t().clamp(0.0, 1.0),
                b.n("count"),
                b.a("angle"),
                b.k("uneven")
            ];
            b.run(
                k::STRIP_SLIDE,
                &[b.src(), b.to()],
                n,
                [0, b.i("kind"), 0, 0],
            )
        }),
        fx(
            "TileFlip",
            "Tile Flip",
            "Tiles turn over in a wave; the other picture is on their backs.",
        )
        .choice("axis", "Turn About", 0, &["Upright", "Level", "Random"])
        .num("rows", "Rows", 8.0, 1.0, 40.0)
        .angle("angle", "Wave Angle", 0.0)
        .pct("uneven", "Unevenness", 25.0)
        .pct("gap", "Behind", 0.0)
        .hint("How bright what shows between the tiles is.")
        .seed()
        .handle(Handle::Arrow("", "angle"))
        .preset("Wave", &[])
        .preset("Random", &[("axis", &[2.0]), ("uneven", &[100.0])])
        .draw(|b| {
            let n = nums![
                progress(b),
                b.n("rows"),
                b.k("uneven"),
                b.a("angle"),
                b.k("gap")
            ];
            b.run(k::FLIP_TILES, &[b.src(), b.to()], n, [0, b.i("axis"), 0, 0])
        }),
        fx(
            "ParallaxCut",
            "Parallax Strips",
            "Strips at different depths slide past as the picture changes.",
        )
        .num("count", "Strips", 10.0, 1.0, 60.0)
        .angle("angle", "Angle", 0.0)
        .num("slide", "Slide", 60.0, 0.0, 300.0)
        .pct("shade", "Shade", 40.0)
        .seed()
        .preset("Parallax", &[])
        .draw(|b| {
            let t = progress(b);
            let (p, _) = whip(t, 0.8);
            let off = if t < 0.5 { p } else { p - 1.0 };
            let bell = b.bell();
            let n = nums![
                b.n("count"),
                b.a("angle"),
                b.len("slide") * off * 2.0,
                0.7,
                0.0,
                b.k("shade") * bell,
                0.0
            ];
            let s = half(b);
            b.run(k::STRIPS, &[s], n, [2, 0, 0, 0])
        }),
        fx(
            "FilmRoll",
            "Film Roll",
            "The film slips in the projector: frames roll past to the next picture.",
        )
        .num("rolls", "Frames", 2.0, 1.0, 10.0)
        .num("gap", "Gap", 5.0, 0.0, 20.0)
        .choice("way", "Way", 0, &["Up", "Down"])
        .num("blur", "Motion Blur", 100.0, 0.0, 300.0)
        .preset("Slip", &[])
        .preset("Fast Roll", &[("rolls", &[6.0])])
        .draw(|b| {
            let t = progress(b);
            let (p, v) = whip(t, 0.7);
            let rolls = b.n("rolls").round().max(1.0);
            let at = p * rolls;
            let n = at.floor().min(rolls - 1.0);
            let x = at - n;
            // Frames of the first until half, then of the second.
            let pick = |b: &B<'_>, i: f32| {
                if i >= rolls * 0.5 + 0.25 || i >= rolls {
                    b.to()
                } else {
                    b.src()
                }
            };
            let way = if b.i("way") == 1 { -1.0 } else { 1.0 };
            let (s0, s1) = (pick(b, n), pick(b, n + 1.0));
            let numbers = nums![x, [0.0f32, -way], b.len("gap"), [0.0f32; 3]];
            let o = b.run(k::SLIDE, &[s0, s1], numbers, [0, 0, 0, 0]);
            let d = v * rolls * 0.04 * b.k("blur");
            b.smear(o, [0.0, d], 0.0, 1)
        }),
        fx(
            "FlutterCut",
            "Flutter Cut",
            "Cuts back and forth, faster or slower, before it stays.",
        )
        .num("beats", "Cuts", 8.0, 2.0, 40.0)
        .num("flash", "Flash", 25.0, 0.0, 200.0)
        .color("color", "Flash Color", [1.0, 1.0, 1.0])
        .toggle("random", "Random", true)
        .seed()
        .preset("Flutter", &[])
        .preset(
            "Strobe",
            &[("beats", &[14.0]), ("flash", &[120.0]), ("random", &[0.0])],
        )
        .draw(|b| {
            let t = progress(b);
            let x = t * b.n("beats");
            let n = x.floor();
            let on = if b.on("random") {
                hash01(n as i32, b.seed_of("seed")) < t * 1.1 - 0.05
            } else {
                (n as i32) & 1 == 1
            };
            let s = if t > 0.93 || (on && t > 0.07) {
                b.to()
            } else {
                b.src()
            };
            let f = b.k("flash") * (-(x - n) * 4.0).exp() * b.bell();
            if f <= 0.0 {
                return s;
            }
            let c = b.color("color");
            let wash = b.run(
                k::FILL,
                &[s],
                nums![c[0] * f, c[1] * f, c[2] * f, 1.0],
                [0; 4],
            );
            b.run(k::GLOW, &[s, wash], nums![1.0, [1.0; 3], 1.0], [1, 0, 0, 0])
        }),
        fx(
            "ChannelChange",
            "Channel Change",
            "A television changes channel: snow, a jump, the other picture.",
        )
        .num("snow", "Snow", 180.0, 0.0, 400.0)
        .num("roll", "Roll", 1.5, -4.0, 4.0)
        .pct("off", "Goes Dark", 60.0)
        .hint("How dark the moment between channels is.")
        .seed()
        .preset("Old Set", &[])
        .draw(|b| {
            let bell = b.bell();
            let s = half(b);
            let n = nums![
                b.k("snow") * bell,
                b.n("roll") * bell,
                bell * 4.0,
                bell * 0.7,
                bell,
                bell.min(1.0) * 0.6,
                bell.min(1.0)
            ];
            let o = b.run(k::AERIAL, &[s], n, [0; 4]);
            let x = ((bell - 0.75) / 0.25).clamp(0.0, 1.0) * b.k("off");
            b.run(k::SUM3, &[o], nums![1.0 - x * 0.85, 0.0, 0.0], [0; 4])
        }),
        fx(
            "BurnThrough",
            "Burn Through",
            "The picture catches fire and the other is behind it.",
        )
        .num("size", "Size", 45.0, 5.0, 150.0)
        .num("rim", "Fire Width", 5.0, 0.5, 30.0)
        .color("fire", "Fire", [1.0, 0.42, 0.08])
        .color("char", "Char", [0.03, 0.015, 0.01])
        .pct("line", "Along a Line", 30.0)
        .angle("angle", "Angle", 90.0)
        .num("heat", "Heat", 100.0, 0.0, 400.0)
        .seed()
        .ease(0)
        .handle(Handle::Arrow("", "angle"))
        .preset("Paper", &[])
        .preset("From the Bottom", &[("line", &[80.0])])
        .preset(
            "Spell",
            &[("fire", &[0.3, 0.6, 1.0]), ("char", &[0.02, 0.02, 0.06])],
        )
        .draw(|b| {
            let n = nums![
                b.t().clamp(0.0, 1.0),
                b.len("size"),
                b.len("rim"),
                b.color("fire"),
                b.color("char"),
                b.a("angle"),
                b.k("line"),
                b.k("heat")
            ];
            b.run(k::BURN, &[b.src(), b.to()], n, [0; 4])
        }),
        fx(
            "Squash",
            "Squash",
            "One picture is squeezed away by the other.",
        )
        .angle("angle", "Angle", 0.0)
        .ease(1)
        .handle(Handle::Arrow("", "angle"))
        .preset("From the Right", &[])
        .preset("From Below", &[("angle", &[90.0])])
        .draw(|b| {
            let t = b.t().clamp(0.0, 1.0);
            let a = b.a("angle");
            // Whole quarter turns only: the frame stays a frame.
            let q = (a / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2;
            let n = [q.cos().round(), q.sin().round()];
            let shape = b.at.shape;
            let len = if n[0] != 0.0 { shape } else { 1.0 };
            let squeeze = |k1: f32, at: f32| {
                let s = if n[0] != 0.0 { [k1, 1.0] } else { [1.0, k1] };
                plane([n[0] * at, n[1] * at, 0.0], [0.0; 3], s, [0.0; 2], 0.0)
            };
            let m1 = squeeze((1.0 - t).max(1e-3), len * t * 0.5);
            let m2 = squeeze(t.max(1e-3), -len * (1.0 - t) * 0.5);
            let o1 = on_plane(b, b.src(), m1, 0.0, 0);
            let o2 = on_plane(b, b.to(), m2, 0.0, 0);
            b.run(k::LAY, &[o1, o2], nums![1.0], [0; 4])
        }),
        fx(
            "ZoomThrough",
            "Zoom Through",
            "One picture grows past the camera and the other is behind it.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("zoom", "Zoom", 400.0, 110.0, 1500.0)
        .pct("fade", "Fade", 60.0)
        .ease(1)
        .handle(Handle::Point("center"))
        .preset("Through", &[])
        .draw(|b| {
            let t = b.t().clamp(0.0, 1.0);
            let c = b.p("center");
            let z = b.k("zoom").max(1.05).powf(t);
            let m = plane([c[0], c[1], 0.0], [0.0; 3], [z; 2], [0.0; 2], 0.0);
            let back = plane([-c[0], -c[1], 0.0], [0.0; 3], [1.0; 2], [0.0; 2], 0.0);
            let a = on_plane(b, b.src(), super::distort::times(m, back), 0.0, 1);
            let y = 0.85 + 0.15 * t;
            let m = plane([0.0; 3], [0.0; 3], [y; 2], [0.0; 2], 0.0);
            let o = on_plane(b, b.to(), m, 0.0, 2);
            let start = 1.0 - b.k("fade").max(0.02);
            let x = ((t - start) / (1.0 - start)).clamp(0.0, 1.0);
            b.mix(a, o, x)
        }),
        fx(
            "Rotate3D",
            "Turn in Space",
            "One picture turns away in space while the other turns in.",
        )
        .choice(
            "axis",
            "Turns About",
            0,
            &["Left Edge", "Right Edge", "Top Edge", "Bottom Edge"],
        )
        .within("view", "Perspective", 80.0, 5.0, 200.0)
        .pct("shade", "Shade", 60.0)
        .ease(1)
        .preset("Page", &[])
        .preset("Trapdoor", &[("axis", &[2.0])])
        .draw(|b| {
            let t = b.t().clamp(0.0, 1.0);
            let a = t * std::f32::consts::FRAC_PI_2;
            let w = b.at.shape * 0.5;
            let (pivot, turns) = match b.i("axis") {
                1 => ([w, 0.0], [-a, 0.0, 0.0]),
                2 => ([0.0, -0.5], [0.0, a, 0.0]),
                3 => ([0.0, 0.5], [0.0, -a, 0.0]),
                _ => ([-w, 0.0], [a, 0.0, 0.0]),
            };
            let m = plane([0.0; 3], turns, [1.0; 2], pivot, b.k("view"));
            let shade = 1.0 - b.k("shade") * t;
            let Some(inv) = inverse(m) else {
                return b.to();
            };
            let n = nums![inv, shade, b.k("shade") * (1.0 - t) * 0.6];
            b.run(k::CARD, &[b.src(), b.to()], n, [0, 0, 0, 0])
        }),
    ]
}
