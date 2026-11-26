//! Distortions.

use crate::fx::{B, Fx, Group, Handle};
use crate::generated::k;
use crate::graph::Slot;
use crate::nums;

fn fx(id: &'static str, name: &'static str, about: &'static str) -> Fx {
    Fx::filter(id, name, Group::Distort, about)
}

/// The rows from a place to a picture's, given those from the picture's to
/// a place (`None`: no way back).
pub fn inverse(m: [f32; 9]) -> Option<[f32; 9]> {
    let [a, b, c, d, e, f, g, h, i] = m.map(f64::from);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let r = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    Some(r.map(|v| (v / det) as f32))
}

pub fn times(a: [f32; 9], b: [f32; 9]) -> [f32; 9] {
    std::array::from_fn(|n| {
        let (r, c) = (n / 3, n % 3);
        (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum()
    })
}

/// A plane in space seen by a camera: moved, turned (radians: about the
/// upright, the level axis, the line of sight), scaled; `view` how strong
/// the perspective. From the picture's places to the screen's.
pub fn plane(
    shift: [f32; 3],
    [yaw, pitch, roll]: [f32; 3],
    scale: [f32; 2],
    pivot: [f32; 2],
    view: f32,
) -> [f32; 9] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sr, cr) = roll.sin_cos();
    // The turn of the plane's own two axes.
    let x = [cy * cr + sy * sp * sr, cp * sr, -sy * cr + cy * sp * sr];
    let y = [-cy * sr + sy * sp * cr, cp * cr, sy * sr + cy * sp * cr];
    // The camera stands a distance away: 1 for a view of 1.
    let d = 1.0 / view.max(1e-4);
    let col = |v: [f32; 3], s: f32| [v[0] * s, v[1] * s, -v[2] * s];
    let (a, b) = (col(x, scale[0]), col(y, scale[1]));
    let o = [
        shift[0] + pivot[0] - a[0] * pivot[0] - b[0] * pivot[1],
        shift[1] + pivot[1] - a[1] * pivot[0] - b[1] * pivot[1],
        shift[2] - a[2] * pivot[0] - b[2] * pivot[1],
    ];
    // Screen = d · place / (d + depth), depth away from the camera.
    [
        a[0],
        b[0],
        o[0],
        a[1],
        b[1],
        o[1],
        a[2] / d,
        b[2] / d,
        1.0 + o[2] / d,
    ]
}

/// `s` on a plane (rows from the picture to the screen).
pub fn on_plane(b: &mut B<'_>, s: Slot, m: [f32; 9], spread: f32, edges: i32) -> Slot {
    let Some(inv) = inverse(m) else {
        return b.run(k::FILL, &[s], nums![0.0, 0.0, 0.0, 0.0], [0; 4]);
    };
    let n = nums![inv, spread, 2.0];
    b.run(k::PLANE, &[s], n, [edges, 0, 0, 0])
}

/// The rows taking the unit square's corners (top left, top right, bottom
/// right, bottom left) to `q`.
pub fn square_to(q: [[f32; 2]; 4]) -> [f32; 9] {
    let [p0, p1, p2, p3] = q.map(|p| p.map(f64::from));
    let d1 = [p1[0] - p2[0], p1[1] - p2[1]];
    let d2 = [p3[0] - p2[0], p3[1] - p2[1]];
    let s = [p0[0] - p1[0] + p2[0] - p3[0], p0[1] - p1[1] + p2[1] - p3[1]];
    let det = d1[0] * d2[1] - d2[0] * d1[1];
    let (g, h) = if det.abs() < 1e-12 {
        (0.0, 0.0)
    } else {
        (
            (s[0] * d2[1] - d2[0] * s[1]) / det,
            (d1[0] * s[1] - s[0] * d1[1]) / det,
        )
    };
    [
        p1[0] - p0[0] + g * p1[0],
        p3[0] - p0[0] + h * p3[0],
        p0[0],
        p1[1] - p0[1] + g * p1[1],
        p3[1] - p0[1] + h * p3[1],
        p0[1],
        g,
        h,
        1.0,
    ]
    .map(|v| v as f32)
}

/// The frame's places (frame heights) as the unit square's.
pub fn frame_to_square(shape: f32) -> [f32; 9] {
    [1.0 / shape, 0.0, 0.5, 0.0, 1.0, 0.5, 0.0, 0.0, 1.0]
}

/// A shake at a moment: how far across and down (frame heights), the turn,
/// the zoom's log.
fn shake_at(b: &B<'_>, t: f32) -> [f32; 4] {
    let seed = b.seed_of("seed");
    let rate = b.n("rate");
    let rough = b.k("rough");
    let style = b.i("style");
    let wave = |n: u32| -> f32 {
        let s = seed.wrapping_mul(31).wrapping_add(n * 101);
        let x = t * rate;
        match style {
            // A hand's: slow with a tremor over it.
            0 => {
                wander(x * 0.6, s) * 0.8
                    + wander(x * 2.3, s + 1) * (0.2 + 0.4 * rough)
                    + wander(x * 7.0, s + 2) * 0.25 * rough
            }
            // Blows that die away.
            1 => {
                let k = x * 0.5;
                let age = (k - k.floor()) * 2.0;
                let kick = (-age * 3.2).exp();
                wander(x * 5.0, s) * kick + wander(x * 13.0, s + 1) * kick * 0.6 * rough
            }
            // A machine's.
            2 => {
                (x * 2.0 * std::f32::consts::TAU + n as f32).sin() * 0.7
                    + wander(x * 9.0, s) * 0.5 * rough
            }
            // Each frame its own.
            _ => hash01((t * rate * 4.0).floor() as i32, s) * 2.0 - 1.0,
        }
    };
    [
        wave(0) * b.len("moveX"),
        wave(1) * b.len("moveY"),
        wave(2) * b.n("turn").to_radians(),
        wave(3) * b.k("zoom"),
    ]
}

fn hash(x: u32) -> u32 {
    let mut h = x;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// A number 0 to 1 for a whole one.
pub fn hash01(n: i32, seed: u32) -> f32 {
    let h = hash((n as u32).wrapping_mul(0x8da6_b343) ^ seed.wrapping_mul(0xcb1a_b31f));
    (h >> 8) as f32 / 16_777_216.0
}

/// A smooth walk through random numbers, -1 to 1.
pub fn wander(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let t = x - i;
    let t = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let a = hash01(i as i32, seed);
    let b = hash01(i as i32 + 1, seed);
    (a + (b - a) * t) * 2.0 - 1.0
}

pub fn list() -> Vec<Fx> {
    vec![
        fx(
            "Displace",
            "Displace",
            "Pushes the picture by its own light, as if seen through wavy glass.",
        )
        .num("amount", "Amount", 12.0, -60.0, 60.0)
        .choice("map", "Push By", 0, &["Slopes", "Light"])
        .angle("angle", "Angle", 0.0)
        .num("smooth", "Smoothness", 1.5, 0.1, 10.0)
        .within("spread", "Color Spread", 0.0, -100.0, 100.0)
        .num("blur", "Blur", 0.0, 0.0, 100.0)
        .edges(2)
        .preset(
            "Liquid Glass",
            &[("amount", &[25.0]), ("smooth", &[3.0]), ("spread", &[25.0])],
        )
        .preset("Etched", &[("amount", &[18.0]), ("smooth", &[0.4])])
        .preset("Light Slide", &[("map", &[1.0]), ("amount", &[10.0])])
        .preset(
            "Smeared",
            &[("amount", &[30.0]), ("smooth", &[2.0]), ("blur", &[100.0])],
        )
        .draw(|b| {
            let m = b.blur(b.src(), b.len("smooth"), 2);
            let blur = b.k("blur");
            let n = if blur > 0.0 { 12 } else { 1 };
            let numbers = nums![
                b.k("amount") * 2.0,
                b.a("angle"),
                b.k("spread"),
                b.i("map") as f32,
                blur * 2.0
            ];
            b.run(k::DISPLACE, &[b.src(), m], numbers, [b.i("edges"), n, 0, 0])
        }),
        fx(
            "RgbShift",
            "RGB Shift",
            "Moves, zooms and turns red, green and blue apart.",
        )
        .point("center", "Center", [0.5, 0.5])
        .section("red", "Red", true)
        .num("redX", "Horizontal", 0.8, -10.0, 10.0)
        .num("redY", "Vertical", 0.0, -10.0, 10.0)
        .within("redZoom", "Zoom", 100.0, 50.0, 150.0)
        .angle("redTurn", "Turn", 0.0)
        .section("green", "Green", false)
        .num("greenX", "Horizontal", 0.0, -10.0, 10.0)
        .num("greenY", "Vertical", 0.0, -10.0, 10.0)
        .within("greenZoom", "Zoom", 100.0, 50.0, 150.0)
        .angle("greenTurn", "Turn", 0.0)
        .section("blue", "Blue", true)
        .num("blueX", "Horizontal", -0.8, -10.0, 10.0)
        .num("blueY", "Vertical", 0.0, -10.0, 10.0)
        .within("blueZoom", "Zoom", 100.0, 50.0, 150.0)
        .angle("blueTurn", "Turn", 0.0)
        .end()
        .edges(1)
        .preset("Glasses", &[("redX", &[1.2]), ("blueX", &[-1.2])])
        .preset(
            "Zoomed Apart",
            &[
                ("redX", &[0.0]),
                ("blueX", &[0.0]),
                ("redZoom", &[102.0]),
                ("blueZoom", &[98.0]),
            ],
        )
        .preset(
            "Turned Apart",
            &[
                ("redX", &[0.0]),
                ("blueX", &[0.0]),
                ("redTurn", &[1.5]),
                ("blueTurn", &[-1.5]),
            ],
        )
        .draw(|b| {
            let n = nums![
                [b.len("redX"), -b.len("redY")],
                [b.len("greenX"), -b.len("greenY")],
                [b.len("blueX"), -b.len("blueY")],
                b.k("redZoom"),
                b.k("greenZoom"),
                b.k("blueZoom"),
                b.a("redTurn"),
                b.a("greenTurn"),
                b.a("blueTurn"),
                b.p("center"),
            ];
            b.run(k::RGB_SHIFT, &[b.src()], n, [b.i("edges"), 0, 0, 0])
        }),
        fx(
            "Turbulence",
            "Turbulence",
            "Flowing noise that bends the picture: water, heat, dreams.",
        )
        .num("amount", "Amount", 5.0, 0.0, 40.0)
        .num("size", "Size", 22.0, 2.0, 100.0)
        .choice(
            "flow",
            "Flow",
            0,
            &["Waves", "Swirls", "Ridges", "Two Flows"],
        )
        .int("detail", "Detail", 3, 1, 6)
        .within("stretch", "Stretch", 100.0, 20.0, 500.0)
        .within("spread", "Color Spread", 0.0, -100.0, 100.0)
        .section("motion", "Motion", true)
        .num("speed", "Change", 0.5, 0.0, 4.0)
        .num("driftX", "Drift Horizontal", 0.0, -2.0, 2.0)
        .num("driftY", "Drift Vertical", 0.0, -2.0, 2.0)
        .end()
        .seed()
        .edges(2)
        .moving()
        .preset("Under Water", &[("amount", &[4.0]), ("size", &[35.0])])
        .preset(
            "Smoke",
            &[("flow", &[1.0]), ("amount", &[10.0]), ("detail", &[4.0])],
        )
        .preset(
            "Crumpled",
            &[("flow", &[2.0]), ("amount", &[6.0]), ("speed", &[0.0])],
        )
        .preset(
            "Boiling",
            &[
                ("flow", &[3.0]),
                ("amount", &[7.0]),
                ("size", &[12.0]),
                ("speed", &[1.6]),
            ],
        )
        .preset(
            "Oil on Water",
            &[("flow", &[1.0]), ("amount", &[8.0]), ("spread", &[35.0])],
        )
        .draw(|b| turbulence(b, b.src())),
        fx(
            "HeatHaze",
            "Heat Haze",
            "The shimmer of hot air rising, more of it lower down.",
        )
        .num("amount", "Amount", 0.8, 0.0, 6.0)
        .num("size", "Size", 5.0, 0.5, 30.0)
        .num("speed", "Rise", 1.0, 0.0, 5.0)
        .num("blur", "Softness", 0.1, 0.0, 2.0)
        .point("top", "Cool", [0.5, 0.75])
        .point("bottom", "Hot", [0.5, 0.1])
        .seed()
        .handle(Handle::Line("bottom", "top"))
        .moving()
        .preset("Road", &[("amount", &[1.2]), ("size", &[4.0])])
        .preset(
            "Fire",
            &[("amount", &[2.5]), ("size", &[8.0]), ("speed", &[2.2])],
        )
        .draw(|b| {
            let soft = b.blur(b.src(), b.len("blur"), 1);
            let n = nums![
                b.len("amount"),
                b.len("size"),
                0.0,
                [0.0, b.n("speed") * 0.7],
                b.n("speed") * 0.8,
                0.55,
            ];
            let o = b.run(k::TURBULENCE, &[soft], n, [2, 3, 0, 0]);
            // Whole where it is hot, none where it is cool.
            let (hot, cool) = (b.p("bottom"), b.p("top"));
            let mid = [(hot[0] + cool[0]) * 0.5, (hot[1] + cool[1]) * 0.5];
            let d = [cool[0] - hot[0], cool[1] - hot[1]];
            let long = d[0].hypot(d[1]).max(0.02);
            let turn = d[1].atan2(d[0]);
            // A wide rectangle whose feathered side runs between them.
            let out = [mid[0] - d[0] / long * 4.0, mid[1] - d[1] / long * 4.0];
            let f = nums![1.0, out, [4.0, 8.0], turn, long * 0.5];
            b.run(k::FINISH, &[b.src(), o], f, [2, 0, 0, 0])
        }),
        fx("Twirl", "Twirl", "A twist round a place.")
            .point("center", "Center", [0.5, 0.5])
            .angle("turn", "Turn", 180.0)
            .num("radius", "Radius", 45.0, 1.0, 120.0)
            .within("falloff", "Falloff", 2.0, 0.5, 6.0)
            .within("pull", "Pull", 0.0, -100.0, 100.0)
            .within("spread", "Color Spread", 0.0, -100.0, 100.0)
            .edges(2)
            .handle(Handle::Point("center"))
            .handle(Handle::Ring("center", "radius"))
            .preset("Whirlpool", &[("turn", &[420.0]), ("pull", &[25.0])])
            .preset("Gentle", &[("turn", &[45.0]), ("radius", &[80.0])])
            .preset(
                "Black Hole",
                &[
                    ("turn", &[270.0]),
                    ("pull", &[90.0]),
                    ("falloff", &[3.0]),
                    ("spread", &[30.0]),
                ],
            )
            .draw(|b| {
                let n = nums![
                    b.p("center"),
                    b.a("turn"),
                    b.len("radius"),
                    b.n("falloff"),
                    b.k("spread"),
                    b.k("pull"),
                ];
                b.run(k::TWIRL, &[b.src()], n, [b.i("edges"), 0, 0, 0])
            }),
        fx(
            "Fisheye",
            "Fisheye",
            "A wide lens's bulge, or a pinch, over the whole picture.",
        )
        .within("amount", "Amount", 55.0, -100.0, 100.0)
        .point("center", "Center", [0.5, 0.5])
        .within("zoom", "Zoom", 100.0, 20.0, 300.0)
        .within("squeeze", "Squeeze", 100.0, 40.0, 250.0)
        .within("spread", "Color Spread", 0.0, -100.0, 100.0)
        .edges(0)
        .handle(Handle::Point("center"))
        .preset("Door Viewer", &[("amount", &[100.0]), ("zoom", &[85.0])])
        .preset("Action Camera", &[("amount", &[40.0]), ("zoom", &[105.0])])
        .preset("Pinched", &[("amount", &[-60.0])])
        .preset("Old Lens", &[("amount", &[25.0]), ("spread", &[25.0])])
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.k("amount"),
                b.k("zoom"),
                b.k("spread"),
                b.k("squeeze"),
            ];
            b.run(k::FISHEYE, &[b.src()], n, [b.i("edges"), 0, 0, 0])
        }),
        fx(
            "Magnifier",
            "Magnifier",
            "A magnifying glass over the picture.",
        )
        .point("center", "Center", [0.5, 0.5])
        .num("radius", "Radius", 22.0, 1.0, 80.0)
        .within("zoom", "Zoom", 200.0, 20.0, 1000.0)
        .choice("shape", "Shape", 0, &["Round", "Square"])
        .within("stretch", "Height", 100.0, 20.0, 300.0)
        .pct("curve", "Curve", 35.0)
        .pct("soft", "Edge Softness", 0.0)
        .within("spread", "Color Spread", 10.0, -100.0, 100.0)
        .num("rim", "Rim Width", 0.8, 0.0, 5.0)
        .color("rimColor", "Rim Color", [0.92, 0.9, 0.86])
        .handle(Handle::Point("center"))
        .handle(Handle::Ring("center", "radius"))
        .preset("Glass", &[])
        .preset(
            "Soft Lens",
            &[("rim", &[0.0]), ("soft", &[60.0]), ("curve", &[100.0])],
        )
        .preset(
            "Detail Window",
            &[("shape", &[1.0]), ("curve", &[0.0]), ("zoom", &[300.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.len("radius"),
                b.k("zoom"),
                b.k("stretch"),
                b.k("curve"),
                b.k("soft"),
                b.color("rimColor"),
                b.len("rim"),
                b.k("spread"),
            ];
            b.run(k::MAGNIFY, &[b.src()], n, [2, b.i("shape"), 0, 0])
        }),
        fx("Ripple", "Ripple", "Rings of water spreading from a place.")
            .point("center", "Center", [0.5, 0.5])
            .num("height", "Height", 1.2, 0.0, 10.0)
            .num("length", "Reach", 8.0, 1.0, 40.0)
            .num("speed", "Speed", 1.0, -5.0, 5.0)
            .num("phase", "Phase", 0.0, -2.0, 2.0)
            .num("fade", "Fade", 1.5, 0.0, 8.0)
            .num("radius", "Radius", 0.0, 0.0, 150.0)
            .hint("How far the rings reach: 0, all over.")
            .within("light", "Light", 30.0, 0.0, 100.0)
            .within("spread", "Color Spread", 0.0, -100.0, 100.0)
            .edges(2)
            .handle(Handle::Point("center"))
            .moving()
            .preset("Water Drop", &[("fade", &[3.0]), ("height", &[1.8])])
            .preset("Pond", &[("length", &[14.0]), ("fade", &[0.6])])
            .preset(
                "Sonar",
                &[
                    ("length", &[4.0]),
                    ("height", &[0.8]),
                    ("speed", &[2.0]),
                    ("fade", &[0.4]),
                ],
            )
            .draw(|b| {
                let n = nums![
                    b.p("center"),
                    b.len("height"),
                    b.len("length"),
                    b.n("phase") + b.run_time("speed"),
                    b.n("fade"),
                    b.k("spread"),
                    b.k("light"),
                    b.len("radius"),
                ];
                b.run(k::RIPPLE, &[b.src()], n, [b.i("edges"), 0, 0, 0])
            }),
        fx(
            "Waves",
            "Waves",
            "Waves through the picture, in one direction or two.",
        )
        .num("height", "Height", 2.0, 0.0, 20.0)
        .num("length", "Reach", 20.0, 1.0, 100.0)
        .num("height2", "Cross Height", 0.0, 0.0, 20.0)
        .num("length2", "Cross Length", 25.0, 1.0, 100.0)
        .angle("angle", "Angle", 0.0)
        .choice("wave", "Wave", 0, &["Sine", "Triangle", "Square", "Noise"])
        .choice("way", "Moves", 0, &["Sideways", "Along"])
        .num("speed", "Speed", 0.6, -5.0, 5.0)
        .num("phase", "Phase", 0.0, -2.0, 2.0)
        .within("spread", "Color Spread", 0.0, -100.0, 100.0)
        .seed()
        .edges(2)
        .handle(Handle::Arrow("", "angle"))
        .moving()
        .preset("Flag", &[("length", &[45.0]), ("height", &[3.0])])
        .preset(
            "Bad Reception",
            &[
                ("wave", &[3.0]),
                ("angle", &[90.0]),
                ("length", &[10.0]),
                ("height", &[1.5]),
                ("speed", &[2.0]),
            ],
        )
        .preset(
            "Woven",
            &[
                ("height2", &[2.0]),
                ("length", &[15.0]),
                ("length2", &[15.0]),
            ],
        )
        .preset(
            "Steps",
            &[("wave", &[2.0]), ("angle", &[90.0]), ("length", &[12.0])],
        )
        .draw(|b| {
            let n = nums![
                b.len("height"),
                b.len("length"),
                b.a("angle"),
                b.n("phase") + b.run_time("speed"),
                b.len("height2"),
                b.len("length2"),
                b.k("spread"),
            ];
            b.run(
                k::WAVES,
                &[b.src()],
                n,
                [b.i("edges"), b.i("wave"), b.i("way"), 0],
            )
        }),
        fx(
            "Polar",
            "Polar",
            "Wraps the picture round a place, or unrolls a round one.",
        )
        .choice("way", "Way", 0, &["Wrap", "Unroll"])
        .point("center", "Center", [0.5, 0.5])
        .angle("turn", "Turn", 0.0)
        .num("inner", "Inner Radius", 0.0, 0.0, 50.0)
        .num("outer", "Outer Radius", 50.0, 5.0, 120.0)
        .int("times", "Repeat", 1, 1, 8)
        .edges(1)
        .handle(Handle::Point("center"))
        .handle(Handle::Ring("center", "outer"))
        .preset("Small Planet", &[("outer", &[90.0])])
        .preset(
            "Ring",
            &[("inner", &[22.0]), ("outer", &[48.0]), ("edges", &[0.0])],
        )
        .preset("Unrolled", &[("way", &[1.0])])
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.a("turn"),
                b.len("inner"),
                b.len("outer").max(b.len("inner") + 0.01),
                b.n("times"),
            ];
            b.run(k::POLAR, &[b.src()], n, [b.i("edges"), b.i("way"), 0, 0])
        }),
        fx(
            "CornerPin",
            "Corner Pin",
            "Pulls the picture by its four corners, in perspective.",
        )
        .point("tl", "Top Left", [0.1, 0.9])
        .point("tr", "Top Right", [0.9, 0.9])
        .point("bl", "Bottom Left", [0.1, 0.1])
        .point("br", "Bottom Right", [0.9, 0.1])
        .toggle("inverse", "Straighten", false)
        .hint("The corners mark a place of the picture that is brought to the whole frame.")
        .edges(0)
        .handle(Handle::Quad(["tl", "tr", "br", "bl"]))
        .preset(
            "Screen",
            &[
                ("tl", &[0.2, 0.85]),
                ("tr", &[0.75, 0.75]),
                ("bl", &[0.22, 0.2]),
                ("br", &[0.78, 0.3]),
            ],
        )
        .preset(
            "Floor",
            &[
                ("tl", &[0.3, 0.55]),
                ("tr", &[0.7, 0.55]),
                ("bl", &[0.0, 0.0]),
                ("br", &[1.0, 0.0]),
            ],
        )
        .draw(|b| {
            let q = ["tl", "tr", "br", "bl"].map(|n| b.p(n));
            let m = times(square_to(q), frame_to_square(b.at.shape));
            let m = if b.on("inverse") {
                inverse(m).unwrap_or(m)
            } else {
                m
            };
            on_plane(b, b.src(), m, 0.0, b.i("edges"))
        }),
        fx(
            "Plane3D",
            "3D Plane",
            "The picture on a plane you turn, move and scale in space.",
        )
        .point("center", "Position", [0.5, 0.5])
        .angle("yaw", "Turn", 30.0)
        .angle("pitch", "Tilt", 0.0)
        .angle("roll", "Roll", 0.0)
        .num("depth", "Distance", 0.0, -80.0, 200.0)
        .within("scale", "Scale", 100.0, 0.0, 500.0)
        .within("view", "Perspective", 60.0, 0.0, 200.0)
        .within("spread", "Color Spread", 0.0, -100.0, 100.0)
        .edges(0)
        .handle(Handle::Point("center"))
        .preset(
            "Leaning",
            &[("yaw", &[25.0]), ("pitch", &[10.0]), ("roll", &[-4.0])],
        )
        .preset(
            "Title Crawl",
            &[("yaw", &[0.0]), ("pitch", &[-55.0]), ("view", &[110.0])],
        )
        .preset(
            "Tunnel Wall",
            &[("yaw", &[65.0]), ("view", &[120.0]), ("edges", &[2.0])],
        )
        .draw(|b| {
            let c = b.p("center");
            let m = plane(
                [c[0], c[1], b.k("depth")],
                [
                    b.f64("yaw").to_radians() as f32,
                    b.f64("pitch").to_radians() as f32,
                    b.a("roll"),
                ],
                [b.k("scale"); 2],
                [0.0; 2],
                b.k("view"),
            );
            on_plane(b, b.src(), m, b.k("spread"), b.i("edges"))
        }),
        fx(
            "Transform",
            "Transform",
            "Moves, scales, turns and skews the picture, with its own motion blur.",
        )
        .point("position", "Position", [0.5, 0.5])
        .point("pivot", "Pivot", [0.5, 0.5])
        .within("scale", "Scale", 100.0, 0.0, 1000.0)
        .within("scaleX", "Width", 100.0, -500.0, 500.0)
        .within("scaleY", "Height", 100.0, -500.0, 500.0)
        .angle("turn", "Turn", 0.0)
        .within("skew", "Skew", 0.0, -80.0, 80.0)
        .edges(0)
        .handle(Handle::Point("position"))
        .handle(Handle::Arrow("position", "turn"))
        .preset("Mirrored", &[("scaleX", &[-100.0])])
        .preset("Upside Down", &[("scaleY", &[-100.0])])
        .preset("Mirror Wall", &[("scale", &[34.0]), ("edges", &[2.0])])
        .draw(|b| {
            let (p, c) = (b.p("position"), b.p("pivot"));
            let s = b.k("scale");
            let (sx, sy) = (s * b.k("scaleX"), s * b.k("scaleY"));
            let (sin, cos) = b.a("turn").sin_cos();
            let skew = (-b.n("skew").to_radians()).tan();
            // Scaled and skewed, turned, then put in place.
            let (a, bb) = (sx, skew * sy);
            let m = [
                cos * a,
                cos * bb - sin * sy,
                p[0] - (cos * a * c[0] + (cos * bb - sin * sy) * c[1]),
                sin * a,
                sin * bb + cos * sy,
                p[1] - (sin * a * c[0] + (sin * bb + cos * sy) * c[1]),
                0.0,
                0.0,
                1.0,
            ];
            on_plane(b, b.src(), m, 0.0, b.i("edges"))
        }),
        fx(
            "Kaleidoscope",
            "Kaleidoscope",
            "A mirror tube turned on the picture.",
        )
        .point("center", "Center", [0.5, 0.5])
        .int("sides", "Sides", 6, 2, 32)
        .choice("kind", "Mirrors", 0, &["Wedges", "Rings", "Petals"])
        .angle("turn", "Turn", 0.0)
        .within("zoom", "Zoom", 100.0, 10.0, 500.0)
        .section("view", "Viewpoint", true)
        .point("look", "Looks At", [0.5, 0.5])
        .angle("lookTurn", "Look Turn", 0.0)
        .num("spin", "Spin", 0.0, -180.0, 180.0)
        .hint("Degrees a second.")
        .end()
        .edges(2)
        .handle(Handle::Point("center"))
        .handle(Handle::Arrow("center", "turn"))
        .moving()
        .preset("Classic", &[("spin", &[10.0])])
        .preset("Snowflake", &[("sides", &[12.0]), ("zoom", &[70.0])])
        .preset(
            "Mandala",
            &[("kind", &[1.0]), ("sides", &[10.0]), ("zoom", &[60.0])],
        )
        .preset("Flower", &[("kind", &[2.0]), ("sides", &[5.0])])
        .preset("Mirror", &[("sides", &[2.0])])
        .draw(|b| {
            let spin = -(b.run_time("spin")).to_radians();
            let n = nums![
                b.p("center"),
                b.a("turn"),
                b.k("zoom"),
                b.n("sides"),
                b.a("lookTurn") + spin,
                b.p("look"),
            ];
            b.run(k::KALEIDO, &[b.src()], n, [b.i("edges"), b.i("kind"), 0, 0])
        }),
        fx("InsectEye", "Insect Eye", "The picture in many small eyes.")
            .num("size", "Size", 9.0, 1.0, 50.0)
            .choice("shape", "Shape", 0, &["Hexagons", "Squares", "Circles"])
            .angle("turn", "Turn", 0.0)
            .within("zoom", "Each Sees", 220.0, 10.0, 1000.0)
            .within("curve", "Curve", 40.0, -100.0, 100.0)
            .pct("gap", "Gaps", 4.0)
            .color("gapColor", "Gap Color", [0.02, 0.02, 0.03])
            .pct("shade", "Shade", 30.0)
            .edges(2)
            .preset("Fly", &[("size", &[5.0]), ("zoom", &[300.0])])
            .preset(
                "Honeycomb",
                &[("size", &[12.0]), ("zoom", &[100.0]), ("gap", &[8.0])],
            )
            .preset(
                "Lens Array",
                &[
                    ("shape", &[2.0]),
                    ("size", &[10.0]),
                    ("zoom", &[350.0]),
                    ("gap", &[6.0]),
                ],
            )
            .draw(|b| {
                let n = nums![
                    b.len("size"),
                    b.a("turn"),
                    b.k("zoom"),
                    b.k("curve"),
                    b.k("gap"),
                    b.k("shade"),
                    b.color("gapColor"),
                ];
                b.run(k::FACETS, &[b.src()], n, [b.i("edges"), b.i("shape"), 0, 0])
            }),
        fx(
            "TileScramble",
            "Tile Scramble",
            "The picture in tiles thrown about.",
        )
        .num("rows", "Rows", 8.0, 1.0, 40.0)
        .within("stretch", "Tile Width", 100.0, 20.0, 400.0)
        .num("move", "Move", 8.0, 0.0, 60.0)
        .num("turn", "Turn", 0.0, 0.0, 180.0)
        .pct("share", "How Many", 60.0)
        .pct("gap", "Gaps", 0.0)
        .color("gapColor", "Gap Color", [0.0, 0.0, 0.0])
        .toggle("snap", "Swap Places", false)
        .num("rate", "Changes a Second", 0.0, 0.0, 30.0)
        .seed()
        .edges(2)
        .moving()
        .preset(
            "Puzzle",
            &[("snap", &[1.0]), ("move", &[30.0]), ("gap", &[4.0])],
        )
        .preset(
            "Broken Screen",
            &[("rate", &[8.0]), ("share", &[30.0]), ("move", &[4.0])],
        )
        .preset(
            "Loose Tiles",
            &[
                ("turn", &[8.0]),
                ("move", &[1.5]),
                ("share", &[100.0]),
                ("gap", &[6.0]),
            ],
        )
        .draw(|b| {
            let n = nums![
                b.n("rows"),
                b.k("stretch"),
                b.len("move"),
                b.n("turn").to_radians(),
                b.k("share"),
                b.k("gap"),
                b.n("rate"),
                b.color("gapColor"),
            ];
            b.run(
                k::TILES,
                &[b.src()],
                n,
                [b.i("edges"), i32::from(b.on("snap")), 0, 0],
            )
        }),
        fx(
            "Strips",
            "Strips",
            "The picture cut into strips that slide.",
        )
        .num("count", "Strips", 12.0, 1.0, 60.0)
        .angle("angle", "Angle", 0.0)
        .num("slide", "Slide", 6.0, -60.0, 60.0)
        .choice(
            "way",
            "Way",
            0,
            &["Each Its Own", "Alternate", "Further and Further"],
        )
        .pct("uneven", "Unevenness", 60.0)
        .num("speed", "Moving", 0.0, -40.0, 40.0)
        .within("tilt", "Tilt", 0.0, -100.0, 100.0)
        .pct("shade", "Shade", 0.0)
        .seed()
        .edges(2)
        .handle(Handle::Arrow("", "angle"))
        .moving()
        .preset("Sliced", &[])
        .preset(
            "Shutters",
            &[
                ("way", &[1.0]),
                ("uneven", &[0.0]),
                ("angle", &[90.0]),
                ("shade", &[50.0]),
            ],
        )
        .preset(
            "Staircase",
            &[("way", &[2.0]), ("uneven", &[0.0]), ("slide", &[12.0])],
        )
        .preset(
            "Parallax",
            &[("speed", &[4.0]), ("tilt", &[40.0]), ("shade", &[35.0])],
        )
        .draw(|b| {
            let n = nums![
                b.n("count"),
                b.a("angle"),
                b.len("slide"),
                b.k("uneven"),
                b.k("tilt"),
                b.k("shade"),
                b.len("speed"),
            ];
            b.run(k::STRIPS, &[b.src()], n, [b.i("edges"), b.i("way"), 0, 0])
        }),
        fx(
            "EndlessZoom",
            "Endless Zoom",
            "The picture inside itself, again and again, without end.",
        )
        .point("center", "Center", [0.5, 0.5])
        .within("step", "Each Smaller By", 250.0, 110.0, 1000.0)
        .num("speed", "Speed", 0.5, -4.0, 4.0)
        .num("phase", "Phase", 0.0, -2.0, 2.0)
        .angle("turn", "Turn Each", 0.0)
        .num("soft", "Edge Softness", 8.0, 0.0, 40.0)
        .int("count", "How Many", 6, 2, 10)
        .handle(Handle::Point("center"))
        .moving()
        .preset("Tunnel", &[("step", &[180.0]), ("turn", &[12.0])])
        .preset(
            "Picture in a Picture",
            &[("step", &[350.0]), ("soft", &[0.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.k("step"),
                b.n("phase") + b.run_time("speed"),
                b.a("turn"),
                b.len("soft"),
            ];
            b.run(k::ENDLESS, &[b.src()], n, [0, b.i("count"), 0, 0])
        }),
        fx(
            "Echoes",
            "Echoes",
            "The picture repeated, each time smaller, turned, moved or fainter.",
        )
        .int("count", "Count", 8, 2, 24)
        .point("center", "Center", [0.5, 0.5])
        .within("scale", "Scale Each", 88.0, 50.0, 150.0)
        .angle("turn", "Turn Each", 0.0)
        .num("moveX", "Move Horizontal", 0.0, -20.0, 20.0)
        .num("moveY", "Move Vertical", 0.0, -20.0, 20.0)
        .within("fade", "Fade Each", 85.0, 0.0, 100.0)
        .angle("hue", "Hue Each", 0.0)
        .choice("lay", "Lay", 0, &["Over", "Under", "Add", "Brightest"])
        .edges(0)
        .handle(Handle::Point("center"))
        .preset(
            "Corridor",
            &[("lay", &[0.0]), ("scale", &[85.0]), ("fade", &[90.0])],
        )
        .preset(
            "Spiral",
            &[("lay", &[0.0]), ("turn", &[12.0]), ("scale", &[84.0])],
        )
        .preset(
            "Rainbow Trail",
            &[
                ("lay", &[1.0]),
                ("scale", &[100.0]),
                ("moveX", &[2.0]),
                ("moveY", &[1.0]),
                ("hue", &[30.0]),
            ],
        )
        .preset(
            "Light Zoom",
            &[("lay", &[3.0]), ("scale", &[106.0]), ("fade", &[80.0])],
        )
        .draw(|b| {
            let n = nums![
                b.p("center"),
                b.k("scale"),
                b.a("turn"),
                [b.len("moveX"), -b.len("moveY")],
                b.k("fade"),
                b.i("lay") as f32,
                b.a("hue"),
            ];
            b.run(k::ECHOES, &[b.src()], n, [b.i("edges"), b.i("count"), 0, 0])
        }),
        fx(
            "Raindrops",
            "Raindrops",
            "Drops on the glass, with a misted window behind them.",
        )
        .num("size", "Size", 9.0, 1.0, 40.0)
        .pct("count", "How Many", 45.0)
        .within("bend", "Bend", 100.0, 0.0, 200.0)
        .num("speed", "Run", 1.0, 0.0, 20.0)
        .pct("shine", "Shine", 45.0)
        .pct("mist", "Misted Glass", 60.0)
        .num("blur", "Mist Blur", 1.2, 0.0, 8.0)
        .seed()
        .moving()
        .preset("Window", &[])
        .preset(
            "Downpour",
            &[("count", &[80.0]), ("speed", &[6.0]), ("mist", &[85.0])],
        )
        .preset(
            "Dew",
            &[
                ("size", &[4.0]),
                ("count", &[70.0]),
                ("speed", &[0.0]),
                ("mist", &[30.0]),
            ],
        )
        .draw(|b| {
            let m = b.blur(b.src(), b.len("blur"), 2);
            let n = nums![
                b.len("size"),
                b.k("count"),
                b.k("bend"),
                b.len("speed"),
                b.k("shine"),
                b.k("mist"),
            ];
            b.run(k::DROPS, &[b.src(), m], n, [0; 4])
        }),
        fx(
            "GlassBlocks",
            "Glass Blocks",
            "The picture through glass blocks, reeds or hammered glass.",
        )
        .num("count", "Rows", 14.0, 1.0, 60.0)
        .choice("kind", "Glass", 0, &["Blocks", "Reeds", "Hammered"])
        .within("bulge", "Bulge", 120.0, -300.0, 300.0)
        .angle("turn", "Turn", 0.0)
        .pct("shade", "Edge Shade", 25.0)
        .pct("rough", "Roughness", 0.0)
        .within("spread", "Color Spread", 0.0, -100.0, 100.0)
        .seed()
        .edges(2)
        .preset(
            "Bathroom Window",
            &[("kind", &[2.0]), ("count", &[22.0]), ("bulge", &[160.0])],
        )
        .preset(
            "Reeded Glass",
            &[("kind", &[1.0]), ("count", &[26.0]), ("bulge", &[200.0])],
        )
        .preset(
            "Crystal Blocks",
            &[("spread", &[60.0]), ("bulge", &[220.0])],
        )
        .draw(|b| {
            let n = nums![
                b.n("count"),
                b.k("bulge"),
                b.k("shade"),
                b.a("turn"),
                b.k("rough"),
                b.k("spread"),
            ];
            b.run(k::GLASS, &[b.src()], n, [b.i("edges"), b.i("kind"), 0, 0])
        }),
        fx(
            "Aberration",
            "Chromatic Aberration",
            "The whole rainbow a lens pulls out of edges: round a place or along a line.",
        )
        .choice("way", "Way", 0, &["Round a Place", "Along a Line"])
        .point("center", "Center", [0.5, 0.5])
        .num("zoom", "Amount", 2.5, -20.0, 20.0)
        .angle("turn", "Turn", 0.0)
        .angle("angle", "Line Angle", 0.0)
        .num("clear", "Clear Middle", 5.0, 0.0, 80.0)
        .int("samples", "Quality", 16, 5, 40)
        .edges(1)
        .handle(Handle::Point("center"))
        .preset("Wide Lens", &[])
        .preset(
            "Trip",
            &[
                ("zoom", &[12.0]),
                ("turn", &[4.0]),
                ("clear", &[0.0]),
                ("samples", &[28.0]),
            ],
        )
        .preset("Sideways", &[("way", &[1.0]), ("zoom", &[1.5])])
        .draw(|b| {
            let a = b.a("angle");
            let z = b.k("zoom");
            let n = nums![
                b.p("center"),
                z * 2.0,
                b.a("turn"),
                b.len("clear"),
                [a.cos() * z * 1.2, a.sin() * z * 1.2],
            ];
            b.run(
                k::RAINBOW,
                &[b.src()],
                n,
                [b.i("edges"), b.i("samples"), b.i("way"), 0],
            )
        }),
        fx(
            "EdgeStretch",
            "Edge Stretch",
            "Draws the picture's edges out over black bars or an uneven frame.",
        )
        .within("left", "Left", 12.5, 0.0, 50.0)
        .within("right", "Right", 12.5, 0.0, 50.0)
        .within("top", "Top", 0.0, 0.0, 50.0)
        .within("bottom", "Bottom", 0.0, 0.0, 50.0)
        .choice("kind", "Fill", 1, &["Hold", "Mirror", "Streaks"])
        .num("spread", "Spread", 60.0, 0.0, 200.0)
        .num("soft", "Softness", 4.0, 0.0, 30.0)
        .preset("Pillarbox", &[])
        .preset(
            "Letterbox",
            &[
                ("left", &[0.0]),
                ("right", &[0.0]),
                ("top", &[12.0]),
                ("bottom", &[12.0]),
            ],
        )
        .preset(
            "Upright Video",
            &[("left", &[34.2]), ("right", &[34.2]), ("kind", &[2.0])],
        )
        .draw(|b| {
            let n = nums![
                b.k("left"),
                b.k("top"),
                (1.0 - b.k("right")).max(b.k("left") + 0.01),
                (1.0 - b.k("bottom")).max(b.k("top") + 0.01),
                b.k("spread"),
                b.len("soft"),
            ];
            b.run(k::STRETCH_EDGES, &[b.src()], n, [0, b.i("kind"), 0, 0])
        }),
        fx(
            "CrystalPrism",
            "Crystal Prism",
            "The picture through a crystal's faces.",
        )
        .point("center", "Center", [0.5, 0.5])
        .int("faces", "Faces", 5, 2, 12)
        .num("offset", "Offset", 14.0, -60.0, 60.0)
        .angle("turn", "Turn", 0.0)
        .num("clear", "Clear Middle", 8.0, 0.0, 60.0)
        .within("spread", "Color Spread", 25.0, -100.0, 100.0)
        .pct("shade", "Face Shade", 25.0)
        .num("spin", "Spin", 0.0, -180.0, 180.0)
        .handle(Handle::Point("center"))
        .handle(Handle::Arrow("center", "turn"))
        .moving()
        .preset("Triangle", &[("faces", &[3.0]), ("offset", &[20.0])])
        .preset(
            "Music Video",
            &[("faces", &[6.0]), ("spin", &[15.0]), ("spread", &[50.0])],
        )
        .draw(|b| {
            let spin = -(b.run_time("spin")).to_radians();
            let n = nums![
                b.n("faces"),
                b.len("offset"),
                b.a("turn") + spin,
                b.k("spread"),
                b.k("shade"),
                b.p("center"),
                b.len("clear"),
            ];
            b.run(k::PRISM, &[b.src()], n, [2, 0, 0, 0])
        }),
        fx(
            "CameraShake",
            "Camera Shake",
            "A hand's, a blow's or a machine's shake, with motion blur.",
        )
        .choice("style", "Shake", 0, &["Hand", "Blows", "Machine", "Jitter"])
        .num("rate", "Rate", 2.0, 0.1, 12.0)
        .num("moveX", "Horizontal", 2.5, 0.0, 20.0)
        .num("moveY", "Vertical", 2.5, 0.0, 20.0)
        .num("turn", "Turn", 1.0, 0.0, 20.0)
        .num("zoom", "Zoom", 1.0, 0.0, 20.0)
        .pct("rough", "Roughness", 40.0)
        .pct("blur", "Motion Blur", 50.0)
        .within("scale", "Scale", 100.0, 80.0, 150.0)
        .hint("Enlarges the picture to hide the edges.")
        .seed()
        .edges(2)
        .moving()
        .preset(
            "Hand Held",
            &[
                ("moveX", &[1.2]),
                ("moveY", &[1.2]),
                ("turn", &[0.5]),
                ("rate", &[1.2]),
            ],
        )
        .preset(
            "Explosion",
            &[
                ("style", &[1.0]),
                ("moveX", &[7.0]),
                ("moveY", &[7.0]),
                ("turn", &[3.0]),
                ("zoom", &[5.0]),
                ("rate", &[3.0]),
            ],
        )
        .preset(
            "Earthquake",
            &[
                ("moveX", &[6.0]),
                ("moveY", &[3.0]),
                ("rate", &[6.0]),
                ("rough", &[80.0]),
            ],
        )
        .preset(
            "Engine",
            &[
                ("style", &[2.0]),
                ("moveX", &[0.4]),
                ("moveY", &[0.6]),
                ("rate", &[8.0]),
                ("turn", &[0.1]),
                ("zoom", &[0.0]),
            ],
        )
        .preset(
            "Music Hit",
            &[
                ("style", &[1.0]),
                ("zoom", &[8.0]),
                ("moveX", &[0.5]),
                ("moveY", &[0.5]),
                ("rate", &[4.0]),
            ],
        )
        .draw(|b| {
            let t = b.time();
            let now = shake_at(b, t);
            let e = b.i("edges");
            let scale = b.k("scale");
            let m = |v: [f32; 4]| {
                plane(
                    [v[0], v[1], 0.0],
                    [0.0, 0.0, v[2]],
                    [scale * v[3].exp(); 2],
                    [0.0; 2],
                    0.0,
                )
            };
            let o = on_plane(b, b.src(), m(now), 0.0, e);
            let blur = b.k("blur");
            if blur <= 0.0 {
                return o;
            }
            // The way it goes while the shutter is open.
            let open = blur / b.at.rate.max(1.0) as f32;
            let before = shake_at(b, t - open * 0.5);
            let after = shake_at(b, t + open * 0.5);
            let d = [after[0] - before[0], after[1] - before[1]];
            let edge = if e == 0 { 0 } else { 2 };
            let o = b.smear(o, d, 0.0, edge);
            b.spin(
                o,
                [now[0], now[1]],
                after[3] - before[3],
                after[2] - before[2],
                0.0,
                edge,
                (0.0, 0.0),
            )
        }),
    ]
}

/// Turbulence, from its settings.
fn turbulence(b: &mut B<'_>, s: Slot) -> Slot {
    let n = nums![
        b.len("amount") * 1.4,
        b.len("size"),
        b.k("spread") * 0.35,
        [-b.n("driftX"), b.n("driftY")],
        b.n("speed"),
        1.0 / b.k("stretch").max(0.05),
    ];
    b.run(
        k::TURBULENCE,
        &[s],
        n,
        [b.i("edges"), b.i("detail"), b.i("flow"), 0],
    )
}
