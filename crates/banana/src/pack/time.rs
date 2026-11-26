//! Time: other frames, and changes as time goes.

use super::distort::{hash01, wander};
use crate::fx::{B, Fx, Group, Handle, V};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::filter(id, name, Group::Time, about)
}

/// The frames before this one, `step` apart, `count` of them, nearest
/// first.
fn before(frame: f64, step: f64, count: usize) -> Vec<f64> {
    (1..=count).map(|n| frame - step * n as f64).collect()
}

/// What every trail has.
fn trail_settings(fx: Fx) -> Fx {
    fx.int("count", "Echoes", 5, 1, 12)
        .num("step", "Frames Apart", 2.0, 1.0, 12.0)
        .within("decay", "Decay", 75.0, 0.0, 100.0)
        .hint("How strong each echo is for the one after it.")
        .choice(
            "lay",
            "Lay",
            0,
            &["Brightest", "Add", "Screen", "Behind", "Darkest"],
        )
        .color("tint", "Tint", [1.0, 1.0, 1.0])
        .within("least", "Threshold", 0.0, 0.0, 100.0)
        .hint("Only what is brighter than this leaves a trail.")
}

fn trail_frames(v: &V<'_>, frame: f64) -> Vec<f64> {
    before(
        frame,
        v.f64("step").max(0.25),
        v.i("count").clamp(1, 12) as usize,
    )
}

/// The echoes laid on the frame, the oldest first, each through `change`.
fn trails(b: &mut B<'_>, change: impl Fn(&mut B<'_>, Slot, usize) -> Slot) -> Slot {
    let count = b.i("count").clamp(1, 12) as usize;
    let decay = b.k("decay");
    let tint = b.color("tint");
    let lay = b.i("lay");
    let least = b.k("least");
    let mut o = b.src();
    for n in 0..count {
        let e = change(b, b.other(n), n + 1);
        let k1 = decay.powi(n as i32 + 1);
        let t: [f32; 3] =
            std::array::from_fn(|i| 1.0 + (tint[i] - 1.0) * (n as f32 + 1.0) / count as f32);
        o = b.run(k::TRAIL, &[o, e], nums![k1, t, least], [lay, 0, 0, 0]);
    }
    o
}

pub fn list() -> Vec<Fx> {
    vec![
        trail_settings(fx(
            "Trails",
            "Trails",
            "Echoes of the frames before, fading.",
        ))
        .frames(trail_frames)
        .preset(
            "Light Painting",
            &[
                ("lay", &[0.0]),
                ("least", &[35.0]),
                ("decay", &[92.0]),
                ("count", &[10.0]),
            ],
        )
        .preset(
            "Ghosts",
            &[("lay", &[3.0]), ("decay", &[60.0]), ("step", &[4.0])],
        )
        .preset(
            "Blue Wake",
            &[("tint", &[0.15, 0.4, 1.0]), ("count", &[8.0])],
        )
        .draw(|b| trails(b, |_, s, _| s)),
        trail_settings(fx(
            "SoftTrails",
            "Soft Trails",
            "Echoes that blur as they fade.",
        ))
        .num("blur", "Blur Each", 0.6, 0.0, 6.0)
        .num("glow", "Glow", 0.0, 0.0, 100.0)
        .frames(trail_frames)
        .preset(
            "Smoke",
            &[("blur", &[1.4]), ("count", &[8.0]), ("decay", &[85.0])],
        )
        .preset(
            "Dream",
            &[("lay", &[2.0]), ("blur", &[1.0]), ("glow", &[35.0])],
        )
        .draw(|b| {
            let blur = b.len("blur");
            let o = trails(b, |b, s, n| b.blur(s, blur * n as f32, 1));
            let g = b.k("glow");
            if g <= 0.0 {
                return o;
            }
            let s = b.blur(o, 0.03, 1);
            b.run(k::GLOW, &[o, s], nums![g, [1.0; 3], 1.0], [1, 0, 0, 0])
        }),
        trail_settings(fx(
            "Feedback",
            "Feedback",
            "Echoes that zoom, turn and move as a camera's that films its own screen.",
        ))
        .point("center", "Center", [0.5, 0.5])
        .within("zoom", "Zoom Each", 104.0, 80.0, 130.0)
        .angle("turn", "Turn Each", 2.0)
        .num("moveX", "Move Horizontal", 0.0, -10.0, 10.0)
        .num("moveY", "Move Vertical", 0.0, -10.0, 10.0)
        .handle(Handle::Point("center"))
        .frames(trail_frames)
        .preset("Video Loop", &[("count", &[9.0]), ("lay", &[2.0])])
        .preset(
            "Tunnel",
            &[
                ("zoom", &[92.0]),
                ("turn", &[0.0]),
                ("count", &[10.0]),
                ("decay", &[90.0]),
                ("lay", &[3.0]),
            ],
        )
        .preset(
            "Rising",
            &[
                ("moveY", &[2.5]),
                ("zoom", &[101.0]),
                ("turn", &[0.0]),
                ("tint", &[1.0, 0.5, 0.15]),
            ],
        )
        .draw(|b| {
            let (c, z, t) = (b.p("center"), b.k("zoom"), b.a("turn"));
            let m = [b.len("moveX"), -b.len("moveY")];
            trails(b, |b, s, n| {
                let f = n as f32;
                let numbers = nums![c, z.powf(f), t * f, m[0] * f, m[1] * f, 0.0, 0.1, 0.0];
                b.run(k::DRIFT, &[s], numbers, [0; 4])
            })
        }),
        trail_settings(fx(
            "LiquidFeedback",
            "Liquid Feedback",
            "Echoes that flow: swirled by noise, pulled by the picture's own light.",
        ))
        .num("flow", "Flow", 1.5, 0.0, 15.0)
        .num("size", "Flow Size", 25.0, 2.0, 100.0)
        .num("pull", "Pull of Light", 2.0, -20.0, 20.0)
        .within("zoom", "Zoom Each", 101.0, 80.0, 130.0)
        .seed()
        .frames(trail_frames)
        .moving()
        .preset(
            "Ink in Water",
            &[("flow", &[3.0]), ("count", &[10.0]), ("decay", &[88.0])],
        )
        .preset(
            "Melting Light",
            &[("pull", &[8.0]), ("flow", &[0.5]), ("lay", &[2.0])],
        )
        .draw(|b| {
            let (f, s, p, z) = (b.len("flow"), b.len("size"), b.len("pull"), b.k("zoom"));
            trails(b, |b, slot, n| {
                let m = n as f32;
                let numbers = nums![
                    [0.0f32; 2],
                    z.powf(m),
                    0.0,
                    [0.0f32; 2],
                    f * m * 0.5,
                    s,
                    p * m * 0.5
                ];
                b.run(k::DRIFT, &[slot], numbers, [0; 4])
            })
        }),
        fx(
            "RgbTime",
            "RGB Time",
            "Red, green and blue each from its own moment.",
        )
        .num("red", "Red Frames", -3.0, -30.0, 30.0)
        .num("green", "Green Frames", 0.0, -30.0, 30.0)
        .num("blue", "Blue Frames", 3.0, -30.0, 30.0)
        .frames(|v, f| vec![f + v.f64("red"), f + v.f64("green"), f + v.f64("blue")])
        .preset("Apart", &[])
        .preset("Far Apart", &[("red", &[-10.0]), ("blue", &[10.0])])
        .draw(|b| {
            let i = [b.other(0), b.other(1), b.other(2)];
            b.run(k::RGB_TIME, &i, nums![], [0; 4])
        }),
        fx(
            "SlitScan",
            "Slit Scan",
            "Each part of the picture from another moment.",
        )
        .choice(
            "by",
            "Time By",
            0,
            &["A Line", "Distance", "Light", "Noise"],
        )
        .num("span", "Frames", 16.0, 1.0, 60.0)
        .int("steps", "Steps", 10, 2, 15)
        .angle("angle", "Angle", 90.0)
        .point("center", "Center", [0.5, 0.5])
        .pct("soft", "Softness", 100.0)
        .toggle("ahead", "Ahead", false)
        .hint("Reads frames to come instead of past ones.")
        .seed()
        .handle(Handle::Arrow("center", "angle"))
        .frames(|v, f| {
            let n = v.i("steps").clamp(2, 15);
            let way = if v.on("ahead") { 1.0 } else { -1.0 };
            (1..=n)
                .map(|i| f + way * v.f64("span") * f64::from(i) / f64::from(n))
                .collect()
        })
        .preset("Twisting", &[])
        .preset("Time Ripples", &[("by", &[1.0])])
        .preset("Light Delay", &[("by", &[2.0]), ("span", &[10.0])])
        .draw(|b| {
            let n = b.i("steps").clamp(2, 15) as usize;
            let wide = 1.0 / n as f32;
            let mut o = b.src();
            for i in 0..n {
                let numbers = nums![
                    i as f32 * wide,
                    wide,
                    b.a("angle"),
                    b.k("soft"),
                    b.p("center")
                ];
                let next = b.other(i);
                o = b.run(k::SLIT, &[o, next, b.src()], numbers, [b.i("by"), 0, 0, 0]);
            }
            o
        }),
        fx(
            "FrameBlend",
            "Frame Blend",
            "Frames before and after averaged, as a slow shutter's.",
        )
        .int("count", "Frames", 5, 2, 15)
        .num("step", "Frames Apart", 1.0, 0.25, 6.0)
        .choice("when", "Frames From", 0, &["Around", "Before", "After"])
        .frames(|v, f| {
            let n = v.i("count").clamp(2, 15) - 1;
            let s = v.f64("step").max(0.05);
            let first = match v.i("when") {
                1 => -f64::from(n),
                2 => 1.0,
                _ => -f64::from(n) * 0.5,
            };
            let mut t = Vec::new();
            let mut x = first;
            while t.len() < n as usize {
                if x.abs() > 1e-6 {
                    t.push(f + x * s);
                }
                x += 1.0;
            }
            t
        })
        .preset("Slow Shutter", &[("count", &[9.0])])
        .preset("Long Exposure", &[("count", &[15.0]), ("step", &[2.0])])
        .draw(|b| {
            // A running mean: each frame in turn weighs as the others.
            let n = b.at.others;
            let mut o = b.src();
            let mut have = 1.0f32;
            let mut i = 0;
            while i < n {
                let two = i + 1 < n;
                let (x, y) = (b.other(i), if two { b.other(i + 1) } else { b.other(i) });
                let add = if two { 2.0 } else { 1.0 };
                let t = have + add;
                let numbers = nums![have / t, 1.0 / t, if two { 1.0 / t } else { 0.0 }, 0.0];
                o = b.run(k::FRAMES, &[o, x, y], numbers, [0; 4]);
                have = t;
                i += if two { 2 } else { 1 };
            }
            o
        }),
        fx(
            "TimeOffset",
            "Time Offset",
            "Shows another moment: so many frames before or after.",
        )
        .num("frames", "Frames", -5.0, -120.0, 120.0)
        .frames(|v, f| vec![f + v.f64("frames")])
        .preset("Late", &[])
        .preset("Early", &[("frames", &[5.0])])
        .draw(|b| b.other(0)),
        fx(
            "FreezeFrame",
            "Freeze Frame",
            "Holds one frame, always or every so often.",
        )
        .num("frame", "Frame", 0.0, 0.0, 600.0)
        .choice(
            "mode",
            "Mode",
            0,
            &["Hold", "Hold From There", "Stop Motion"],
        )
        .num("every", "Hold For", 4.0, 1.0, 60.0)
        .hint("For Stop Motion: how many frames each picture stays.")
        .frames(|v, f| {
            let held = v.f64("frame").round();
            vec![match v.i("mode") {
                1 => f.min(held),
                2 => {
                    let n = v.f64("every").round().max(1.0);
                    (f / n).floor() * n
                }
                _ => held,
            }]
        })
        .preset("First Frame", &[])
        .preset("Stop Motion", &[("mode", &[2.0])])
        .preset("Twelve a Second", &[("mode", &[2.0]), ("every", &[2.0])])
        .draw(|b| b.other(0)),
        fx(
            "FrameJitter",
            "Frame Jitter",
            "Frames out of order, at random, around their own.",
        )
        .num("amount", "Amount", 4.0, 0.0, 30.0)
        .num("every", "Hold For", 2.0, 1.0, 20.0)
        .within("chance", "How Often", 70.0, 0.0, 100.0)
        .seed()
        .frames(|v, f| {
            let n = (f / v.f64("every").round().max(1.0)).floor() as i32;
            let s = v.i("seed").max(0) as u32;
            if f64::from(hash01(n, s.wrapping_add(9))) > v.f64("chance") / 100.0 {
                return vec![f];
            }
            let h = f64::from(hash01(n, s)) * 2.0 - 1.0;
            vec![(f + h * v.f64("amount")).round()]
        })
        .preset("Nervous", &[])
        .preset(
            "Broken Projector",
            &[("amount", &[12.0]), ("every", &[4.0])],
        )
        .moving()
        .draw(|b| b.other(0)),
        fx(
            "RandomCuts",
            "Random Cuts",
            "Jumps about the event, staying so long at each place.",
        )
        .num("every", "Stay For", 8.0, 1.0, 60.0)
        .num("span", "Reach", 60.0, 1.0, 600.0)
        .within("chance", "How Often", 100.0, 0.0, 100.0)
        .seed()
        .frames(|v, f| {
            let e = v.f64("every").round().max(1.0);
            let n = (f / e).floor();
            let s = v.i("seed").max(0) as u32;
            if f64::from(hash01(n as i32, s.wrapping_add(9))) > v.f64("chance") / 100.0 {
                return vec![f];
            }
            let h = f64::from(hash01(n as i32, s)) * 2.0 - 1.0;
            let start = (n * e + h * v.f64("span")).round();
            vec![(start + (f - n * e)).max(0.0)]
        })
        .preset("Music Video", &[])
        .preset("Frantic", &[("every", &[3.0]), ("span", &[120.0])])
        .moving()
        .draw(|b| b.other(0)),
        fx(
            "ReverseParts",
            "Reverse Parts",
            "Plays each stretch of frames backward (or to and fro).",
        )
        .num("every", "Length", 24.0, 2.0, 240.0)
        .choice("mode", "Play Mode", 0, &["Backward", "To and Fro"])
        .frames(|v, f| {
            let e = v.f64("every").round().max(1.0);
            let n = (f / e).floor();
            let x = f - n * e;
            vec![match v.i("mode") {
                1 => {
                    let t = (f / e).rem_euclid(2.0);
                    let p = if t > 1.0 { 2.0 - t } else { t };
                    (n - n.rem_euclid(2.0)) * e + p * e
                }
                _ => n * e + (e - 1.0 - x),
            }]
        })
        .preset("Backward Beats", &[("every", &[12.0])])
        .preset("Boomerang", &[("mode", &[1.0])])
        .draw(|b| b.other(0)),
        fx(
            "MotionDetect",
            "Motion Detect",
            "Shows what moves from frame to frame.",
        )
        .num("gain", "Gain", 400.0, 50.0, 2000.0)
        .within("least", "Threshold", 3.0, 0.0, 50.0)
        .num("step", "Frames Apart", 1.0, 1.0, 10.0)
        .num("smooth", "Smoothness", 0.2, 0.0, 4.0)
        .color("color", "Color", [0.2, 1.0, 0.45])
        .pct("own", "Picture's Colors", 0.0)
        .pct("keep", "Keep Picture", 25.0)
        .frames(|v, f| vec![f - v.f64("step").max(0.25)])
        .preset("Alarm", &[("color", &[1.0, 0.1, 0.05])])
        .preset(
            "Only What Moves",
            &[("own", &[100.0]), ("keep", &[0.0]), ("smooth", &[0.8])],
        )
        .draw(|b| {
            let a = b.blur(b.src(), b.len("smooth"), 1);
            let c = b.blur(b.other(0), b.len("smooth"), 1);
            let n = nums![
                b.k("gain"),
                b.k("least"),
                b.color("color"),
                b.k("own"),
                b.k("keep")
            ];
            b.run(k::MOTION, &[a, c], n, [0; 4])
        }),
        fx(
            "SteadyLight",
            "Steady Light",
            "Takes the flicker out of light that changes from frame to frame.",
        )
        .pct("amount", "Amount", 100.0)
        .int("reach", "Frames", 3, 1, 7)
        .hint("How many frames before and after the light is steadied over.")
        .choice("by", "Steady", 0, &["Brightness", "Each Channel"])
        .frames(|v, f| {
            let n = v.i("reach").clamp(1, 7);
            (1..=n)
                .flat_map(|i| [f - f64::from(i), f + f64::from(i)])
                .collect()
        })
        .preset("Time-Lapse", &[("reach", &[5.0])])
        .preset("Lamps", &[("by", &[1.0]), ("reach", &[3.0])])
        .draw(|b| {
            let n = b.halvings(60);
            let mean = |b: &mut B<'_>, s: Slot| {
                let small = b.smaller(s, n, 1);
                let [w, h] = b.size(small);
                b.run_sized(
                    k::MEAN,
                    &[small],
                    nums![],
                    [w.min(64) as i32, h.min(36) as i32, 0, 0],
                    [1, 1],
                    0,
                )
            };
            let own = mean(b, b.src());
            let mut sum = own;
            let mut have = 1.0f32;
            for i in (0..b.at.others).step_by(2) {
                let x = mean(b, b.other(i));
                let y = mean(b, b.other(i + 1));
                let w = 1.0 - (i / 2) as f32 / 8.0;
                sum = b.run_sized(k::MEAN3, &[sum, x, y], nums![have, w, w], [0; 4], [1, 1], 0);
                have += 2.0 * w;
            }
            let numbers = nums![b.k("amount")];
            b.run(
                k::STEADY,
                &[b.src(), own, sum],
                numbers,
                [b.i("by"), 0, 0, 0],
            )
        }),
        fx(
            "TemporalDenoise",
            "Temporal Denoise",
            "Takes noise away by comparing each frame with those around it.",
        )
        .within("level", "Noise Level", 6.0, 0.5, 40.0)
        .pct("amount", "Amount", 100.0)
        .int("reach", "Frames", 2, 1, 6)
        .frames(|v, f| {
            let n = v.i("reach").clamp(1, 6);
            (1..=n)
                .flat_map(|i| [f - f64::from(i), f + f64::from(i)])
                .collect()
        })
        .preset("Gentle", &[("level", &[3.0])])
        .preset("Strong", &[("level", &[12.0]), ("reach", &[4.0])])
        .draw(|b| {
            let mut o = b.src();
            for i in (0..b.at.others).step_by(2) {
                let s = [o, b.other(i), b.other(i + 1)];
                o = b.run(k::CALM, &s, nums![b.k("level"), b.k("amount")], [0; 4]);
            }
            o
        }),
        fx(
            "FrameRepair",
            "Frame Repair",
            "Mends a damaged frame (a flash, a drop-out) from those around it.",
        )
        .pct("amount", "Amount", 100.0)
        .num("step", "Frames Apart", 1.0, 1.0, 10.0)
        .frames(|v, f| {
            let s = v.f64("step").max(0.25);
            vec![f - s, f + s]
        })
        .preset("Whole Frame", &[])
        .draw(|b| {
            let s = [b.src(), b.other(0), b.other(1)];
            b.run(k::REPAIR, &s, nums![b.k("amount")], [0; 4])
        }),
        fx(
            "Deinterlace",
            "Deinterlace",
            "Takes the combing out of interlaced video, only where it moves.",
        )
        .choice("field", "Keep Field", 0, &["Upper", "Lower"])
        .choice("mode", "Mode", 0, &["Where It Moves", "Everywhere"])
        .within("level", "Motion Threshold", 4.0, 0.5, 30.0)
        .frames(|_, f| vec![f - 1.0])
        .preset("Adaptive", &[])
        .preset("One Field", &[("mode", &[1.0])])
        .draw(|b| {
            let s = [b.src(), b.other(0)];
            b.run(
                k::COMB,
                &s,
                nums![b.k("level")],
                [b.i("field"), b.i("mode"), 0, 0],
            )
        }),
        fx(
            "LampFlicker",
            "Lamp Flicker",
            "A light that flickers: a tube failing, a flame, a pulse.",
        )
        .num("amount", "Amount", 35.0, 0.0, 150.0)
        .choice(
            "kind",
            "Kind",
            0,
            &["Failing Tube", "Flame", "Pulse", "Strobe"],
        )
        .num("rate", "Rate", 8.0, 0.1, 30.0)
        .color("color", "Color", [1.0, 1.0, 1.0])
        .within("least", "Threshold", 0.0, 0.0, 100.0)
        .hint("Only what is brighter than this flickers.")
        .seed()
        .moving()
        .preset("Horror Corridor", &[("amount", &[80.0]), ("rate", &[10.0])])
        .preset(
            "Fireplace",
            &[
                ("kind", &[1.0]),
                ("color", &[1.0, 0.75, 0.45]),
                ("rate", &[5.0]),
            ],
        )
        .preset(
            "Alarm",
            &[
                ("kind", &[2.0]),
                ("rate", &[1.2]),
                ("color", &[1.0, 0.15, 0.1]),
                ("amount", &[70.0]),
            ],
        )
        .preset(
            "Club Strobe",
            &[("kind", &[3.0]), ("rate", &[10.0]), ("amount", &[120.0])],
        )
        .draw(|b| {
            let t = b.time() * b.n("rate");
            let s = b.seed_of("seed");
            let v = match b.i("kind") {
                1 => {
                    wander(t, s) * 0.6 + wander(t * 2.7, s + 1) * 0.3 + wander(t * 6.3, s + 2) * 0.1
                }
                2 => (t * std::f32::consts::TAU).sin(),
                3 => {
                    if t - t.floor() < 0.35 {
                        1.0
                    } else {
                        -0.5
                    }
                }
                _ => {
                    // Steady, then bursts of failing.
                    let calm = wander(t * 0.17, s + 3);
                    let buzz = wander(t * 0.9, s + 4) * 0.15;
                    if calm > 0.05 {
                        buzz
                    } else {
                        let h = hash01(t.floor() as i32, s);
                        if h < 0.45 { -0.9 } else { buzz }
                    }
                }
            };
            let n = nums![b.k("amount"), b.color("color"), v, b.k("least")];
            b.run(k::FLICKER, &[b.src()], n, [0; 4])
        }),
    ]
}
