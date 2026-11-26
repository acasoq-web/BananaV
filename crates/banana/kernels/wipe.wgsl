// Banana Split's transitions: from the first picture to the second. f0: how
// far (0 to 1). The wipes share: f1: their edge's softness, f2: a glow's
// width on it, f3-5 its color, f6: its strength, f7: the turn, f8-9 the
// middle, f10: the pattern's size, f11: its stretch, f12: how uneven, f13:
// how far it moves meanwhile; i.y: which kind, i.z 1: the other way.

// The pictures where a pattern (0 first to change, 1 last) says.
fn wiped(p: vec2<i32>, field: f32) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    var f = clamp(field, 0.0, 1.0);
    if P.i.z == 1 {
        f = 1.0 - f;
    }
    let soft = max(pf(1u), 0.003);
    // The edge starts before the first place and ends past the last.
    let edge = pf(0u) * (1.0 + soft) - f;
    let k = ss(0.0, soft, edge);
    var c = mix4(a, b, k);
    if pf(6u) > 0.0 {
        let d = (edge - soft * 0.5) / max(pf(2u), 1e-3);
        let live = ss(0.0, 0.06, pf(0u)) * ss(1.0, 0.94, pf(0u));
        c = lit(c, pf3(3u) * (exp(-d * d * 3.0) * pf(6u) * live * 1.4), 0);
    }
    return c;
}

fn wipe_at(p: vec2<i32>) -> vec2<f32> {
    return turn(pos(p) - pf2(8u), -pf(7u));
}

// How far the frame's farthest corner is from the middle.
fn reach() -> f32 {
    return length(vec2<f32>(P.shape * 0.5 + abs(pf(8u)), 0.5 + abs(pf(9u))));
}

// A line. i.y: 0 one, 1 from the middle both ways, 2 an elbow (a corner).
fn k_wipe_line(p: vec2<i32>) -> vec4<f32> {
    let q = turn(pos(p), -pf(7u));
    let w = 0.5 * (P.shape * abs(cos(pf(7u))) + abs(sin(pf(7u))));
    let h = 0.5 * (P.shape * abs(sin(pf(7u))) + abs(cos(pf(7u))));
    let wave = wander(q.y / max(pf(10u), 1e-3) + pf(0u) * pf(13u), P.seed) * pf(12u) * 0.25;
    var f = (q.x / w + 1.0) * 0.5;
    if P.i.y == 1 {
        f = abs(q.x) / w;
    } else if P.i.y == 2 {
        f = max((q.x / w + 1.0) * 0.5, (q.y / h + 1.0) * 0.5);
    }
    return wiped(p, f + wave);
}

// A shape that grows. i.y: 0 a circle, 1 a box, 2 a diamond, 3 a star, 4 a
// heart, 5 a polygon; f14: points, f15: their depth.
fn k_wipe_shape(p: vec2<i32>) -> vec4<f32> {
    var q = wipe_at(p);
    q = turn(q, pf(0u) * pf(13u));
    q = vec2<f32>(q.x / max(pf(11u), 0.05), q.y);
    var d = length(q);
    var far = reach() / min(max(pf(11u), 0.05), 1.0);
    if P.i.y == 1 {
        d = max(abs(q.x), abs(q.y));
    } else if P.i.y == 2 {
        d = abs(q.x) + abs(q.y);
        far = far * 1.42;
    } else if P.i.y == 3 {
        let n = max(floor(pf(14u) + 0.5), 3.0);
        let b = PI / n;
        let t = angle(vec2<f32>(-q.y, q.x));
        let a = abs(t - 2.0 * b * floor(t / (2.0 * b) + 0.5));
        let deep = max(pf(15u), 0.1);
        let notch = vec2<f32>(cos(b), sin(b)) * deep;
        let e = notch - vec2<f32>(1.0, 0.0);
        d = length(q) / max(notch.y / max(cos(a) * e.y - sin(a) * e.x, 1e-5), 1e-4);
        far = far / deep;
    } else if P.i.y == 4 {
        let h = vec2<f32>(abs(q.x), -q.y + 0.04);
        d = sqrt(max(h.x * h.x + h.y * h.y - 1.0 * h.x * h.y, 0.0));
        far = far * 1.25;
    } else if P.i.y == 5 {
        let n = max(floor(pf(14u) + 0.5), 3.0);
        let a = angle(vec2<f32>(-q.y, q.x));
        let s = PI / n;
        let t = a - 2.0 * s * floor(a / (2.0 * s)) - s;
        d = length(q) * cos(t) / cos(s);
        far = far / cos(s);
    }
    return wiped(p, d / far);
}

// A hand going round. i.y: how many hands; i.x 1: both ways from each;
// i.w: 1 a spiral (f14 its turns).
fn k_wipe_clock(p: vec2<i32>) -> vec4<f32> {
    let q = wipe_at(p);
    let n = f32(max(P.i.y, 1));
    var t = fr(angle(vec2<f32>(-q.y, q.x)) / TAU * n + 1.0);
    if P.i.x == 1 {
        t = abs(t * 2.0 - 1.0);
        t = 1.0 - t;
    }
    if P.i.w == 1 {
        let turns = max(pf(14u), 1.0);
        let r = length(q) / reach();
        t = (floor(r * turns - t + 1.0) + t) / (turns + 1.0);
        t = clamp(t, 0.0, 1.0);
    }
    return wiped(p, t);
}

// Blinds and checks. i.y: 0 blinds, 1 a checkerboard, 2 blinds that open
// in turn, 3 a weave, 4 bars each at its own time.
fn k_wipe_bars(p: vec2<i32>) -> vec4<f32> {
    let q = wipe_at(p) / max(pf(10u), 1e-3);
    let w = 0.5 * (P.shape * abs(cos(pf(7u))) + abs(sin(pf(7u))));
    let all = clamp((wipe_at(p).x / w + 1.0) * 0.5, 0.0, 1.0);
    var f = fr(q.x);
    if P.i.y == 1 {
        let c = md(floor(q.x) + floor(q.y), 2.0);
        f = fr(q.x) * 0.5 + c * 0.5;
    } else if P.i.y == 2 {
        f = mix1(fr(q.x), all, clamp(pf(12u), 0.0, 1.0) * 0.8 + 0.1);
    } else if P.i.y == 3 {
        let over = md(floor(q.x) + floor(q.y), 2.0);
        f = abs(fr(q.x) - 0.5) * 2.0;
        if over > 0.5 {
            f = abs(fr(q.y) - 0.5) * 2.0;
        }
    } else if P.i.y == 4 {
        f = rnd1(i32(floor(q.x)), P.seed) * 0.6 + clamp((wipe_at(p).y + 0.5), 0.0, 1.0) * 0.4;
    }
    return wiped(p, f);
}

// Tiles in an order. i.y: 0 at random, 1 along a line, 2 from the middle,
// 3 a spiral, 4 each growing; f14: each tile's own growing.
fn k_wipe_tiles(p: vec2<i32>) -> vec4<f32> {
    let s = max(pf(10u), 1e-3);
    let t = vec2<f32>(s * max(pf(11u), 0.05), s);
    let q = pos(p) / t;
    let c = floor(q);
    let o = q - c - vec2<f32>(0.5);
    let h = rnd(i32(c.x), i32(c.y), P.seed);
    let m = (c + vec2<f32>(0.5)) * t;
    var f = h;
    if P.i.y == 1 {
        let w = 0.5 * (P.shape * abs(cos(pf(7u))) + abs(sin(pf(7u)))) + s;
        f = mix1((turn(m, -pf(7u)).x / w + 1.0) * 0.5, h, pf(12u) * 0.5);
    } else if P.i.y == 2 {
        f = mix1(length(m - pf2(8u)) / reach(), h, pf(12u) * 0.5);
    } else if P.i.y == 3 {
        let a = fr(angle(m - pf2(8u)) / TAU + 1.0);
        f = mix1(fr(a + length(m - pf2(8u)) * 1.5), h, pf(12u) * 0.5);
    }
    let own = max(abs(o.x), abs(o.y)) * 2.0;
    let k = clamp(pf(14u), 0.0, 0.9);
    return wiped(p, f * (1.0 - k) + own * k);
}

// Dots that grow. i.y: 0 in rows, 1 staggered, 2 scattered, 3 rings of
// them; f14: how long the wave of them takes (0 all at once).
fn k_wipe_dots(p: vec2<i32>) -> vec4<f32> {
    let s = max(pf(10u), 1e-3);
    var q = wipe_at(p) / s;
    var d = 0.0;
    var m = q;
    if P.i.y == 2 {
        let c = cells(q, 0.85, 0.0, P.seed);
        d = c.x / 0.95;
    } else {
        if P.i.y == 1 {
            q = vec2<f32>(q.x + 0.5 * md(floor(q.y), 2.0), q.y);
        }
        m = floor(q) + vec2<f32>(0.5);
        d = length(q - m) / 0.72;
    }
    let w = 0.5 * (P.shape + 1.0);
    var all = clamp(m.x * s / w * 0.5 + 0.5, 0.0, 1.0);
    if P.i.y == 3 {
        all = clamp(length(m) * s / reach(), 0.0, 1.0);
    }
    let k = clamp(pf(14u), 0.0, 0.95);
    return wiped(p, clamp(d, 0.0, 1.0) * (1.0 - k) + all * k);
}

// Rings. i.y: 0 each opens outward, 1 a ripple from the middle.
fn k_wipe_rings(p: vec2<i32>) -> vec4<f32> {
    let r = length(wipe_at(p));
    let t = fr(r / max(pf(10u), 1e-3) - pf(0u) * pf(13u));
    let all = r / reach();
    let k = clamp(pf(12u), 0.0, 1.0);
    if P.i.y == 1 {
        return wiped(p, mix1(all, 0.5 + 0.5 * sin(r / max(pf(10u), 1e-3) * TAU), 0.25 + k * 0.35));
    }
    return wiped(p, mix1(t, all, 0.35 + k * 0.5));
}

// Noise that eats the picture. i.y: 0 clouds, 1 blobs, 2 grains, 3
// liquid, 4 ridges, 5 fire; f14: how much of a line's way is in it; i.x:
// layers.
fn k_wipe_noise(p: vec2<i32>) -> vec4<f32> {
    let q = wipe_at(p) / max(pf(10u), 1e-3);
    let z = pf(0u) * pf(13u);
    var n = 0.0;
    if P.i.y == 1 {
        n = noise3(vec3<f32>(q, z), P.seed) * 0.9;
    } else if P.i.y == 2 {
        let f = full_pixel(p);
        let g = max(i32(pf(10u) * 120.0), 1);
        n = rnd(f.x / g, f.y / g, P.seed) - 0.5;
    } else if P.i.y == 3 {
        let w = vec2<f32>(fbm(vec3<f32>(q * 0.8, z), 3, P.seed + 1u), fbm(vec3<f32>(q * 0.8 + vec2<f32>(3.1, 7.4), z), 3, P.seed + 2u));
        n = fbm(vec3<f32>(q + w * 1.9, z), P.i.x, P.seed) * 1.0;
    } else if P.i.y == 4 {
        n = (ridges(vec3<f32>(q, z), P.i.x, P.seed) - 0.6) * 1.1;
    } else if P.i.y == 5 {
        n = fbm(vec3<f32>(q.x, q.y + z * 2.0, z), P.i.x, P.seed) * 0.9 + q.y * max(pf(10u), 1e-3) * 0.35;
    } else {
        n = fbm(vec3<f32>(q, z), P.i.x, P.seed) * 0.95;
    }
    let w = 0.5 * (P.shape * abs(cos(pf(7u))) + abs(sin(pf(7u))));
    let all = clamp((wipe_at(p).x / w + 1.0) * 0.5, 0.0, 1.0);
    let k = clamp(pf(14u), 0.0, 1.0);
    return wiped(p, mix1(0.5 + n * 0.62, all + n * 0.25, k));
}

// Cells. i.y: 0 bubbles that grow, 1 walls first, 2 each cell at once, 3
// cracks from the middle.
fn k_wipe_cells(p: vec2<i32>) -> vec4<f32> {
    let q = wipe_at(p) / max(pf(10u), 1e-3);
    let c = cells(q, 0.95, pf(0u) * pf(13u), P.seed);
    var f = clamp(c.x / 0.95, 0.0, 1.0);
    if P.i.y == 1 {
        f = clamp((c.y - c.x) * 1.4, 0.0, 1.0);
    } else if P.i.y == 2 {
        f = c.z;
    } else if P.i.y == 3 {
        f = clamp(length(wipe_at(p)) / reach() * 0.7 + (c.y - c.x) * 0.25 + c.z * 0.15, 0.0, 1.0);
    }
    return wiped(p, f);
}

// Waves meeting. i.y: 0 plasma, 1 moiré.
fn k_wipe_waves(p: vec2<i32>) -> vec4<f32> {
    let q = wipe_at(p) / max(pf(10u), 1e-3);
    let z = pf(0u) * pf(13u);
    var v = 0.0;
    if P.i.y == 1 {
        v = sin(length(q) * TAU) * sin(length(q + vec2<f32>(0.8, 0.3)) * TAU);
    } else {
        v = (sin(q.x * 2.1 + z) + sin(q.y * 1.7 - z) + sin((q.x + q.y) * 1.3) + sin(length(q) * 2.4 - z)) * 0.25;
    }
    return wiped(p, 0.5 + 0.5 * v);
}

// By the light of the pictures. i.y: 0 the first's, 1 the second's, 2 both;
// i.z 1: the light parts last. The third picture: their light, blurred.
fn k_wipe_luma(p: vec2<i32>) -> vec4<f32> {
    var l = luma(at0(p.x, p.y).xyz);
    if P.i.y == 1 {
        l = luma(at1(p.x, p.y).xyz);
    } else if P.i.y == 2 {
        l = 0.5 * (l + luma(at1(p.x, p.y).xyz));
    }
    if P.i.x == 1 {
        l = luma(at2(p.x, p.y).xyz);
    }
    return wiped(p, 1.0 - clamp(l, 0.0, 1.0));
}

// The middle of a transition: each picture as its own moves leave it. f0
// how far; the first until f1, the second from then, f2 how softly they
// change places.
fn k_swap(p: vec2<i32>) -> vec4<f32> {
    let k = ss(pf(1u) - pf(2u), pf(1u) + pf(2u) + 1e-4, pf(0u));
    return mix4(at0(p.x, p.y), at1(p.x, p.y), k);
}

// Two pictures sliding, one leaving and one coming, the gap between them
// f3 wide and its color f4-6. f0: how far, f1-2 the way (its length 1);
// i.y: 0 both, 1 the second over, 2 the first off; i.x: past the frame
// each repeats mirrored (2) or not (0).
fn k_slide(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let n = pf2(1u);
    let w = P.shape * abs(n.x) + abs(n.y);
    let t = pf(0u);
    let gap = pf(3u);
    let far = w + gap;
    if P.i.y == 1 {
        let b = pic1(q + n * (w * (1.0 - t)), 0);
        return b + at0(p.x, p.y) * (1.0 - b.w);
    }
    if P.i.y == 2 {
        let a = pic0(q - n * (w * t), 0);
        return a + at1(p.x, p.y) * (1.0 - a.w);
    }
    let a = pic0(q - n * (far * t), P.i.x);
    let b = pic1(q + n * (far * (1.0 - t)), P.i.x);
    // Which side of the seam this place is.
    let s = dot(q, n) - (far * t - w * 0.5 - gap);
    let k = clamp((s - gap * 0.5) / px() + 0.5, 0.0, 1.0);
    var c = mix4(b, a, k);
    if gap > 0.0 {
        let g = clamp((gap * 0.5 - abs(s - gap * 0.5)) / px() + 0.5, 0.0, 1.0);
        c = mix4(c, vec4<f32>(pf3(4u), 1.0), g);
    }
    return c;
}

// A picture on a plane in space over another: f0-8 the rows from a place
// to the first picture's, f9: its shade; the second behind, f10 darker by.
// i.x 1: the plane's picture is the second, behind it the first; i.y 2:
// nothing behind.
fn k_card(p: vec2<i32>) -> vec4<f32> {
    var sum = vec4<f32>(0.0);
    for (var j = 0; j < 2; j++) {
        for (var i = 0; i < 2; i++) {
            let o = (vec2<f32>(f32(i), f32(j)) + vec2<f32>(0.5)) * 0.5 - vec2<f32>(0.5);
            let q = pos(p) + o * px();
            let w = pf(6u) * q.x + pf(7u) * q.y + pf(8u);
            if w > 1e-5 {
                let to = vec2<f32>(pf(0u) * q.x + pf(1u) * q.y + pf(2u), pf(3u) * q.x + pf(4u) * q.y + pf(5u)) / w;
                if P.i.x == 1 {
                    sum += pic1(to, 0);
                } else {
                    sum += pic0(to, 0);
                }
            }
        }
    }
    let c = vec4<f32>(sum.xyz * (0.25 * pf(9u)), sum.w * 0.25);
    if P.i.y == 2 {
        return c;
    }
    var under = at1(p.x, p.y);
    if P.i.x == 1 {
        under = at0(p.x, p.y);
    }
    return c + vec4<f32>(under.xyz * (1.0 - pf(10u)), under.w) * (1.0 - c.w);
}

// Strips that slide away, the second picture under. f0 how far, f1: how
// many, f2: the turn, f3: how unevenly they start; i.y: 0 alternate ways,
// 1 one way, 2 at random, 3 they turn over (blinds in space).
fn k_strip_slide(p: vec2<i32>) -> vec4<f32> {
    let v = turn(pos(p), -pf(2u));
    let w = 1.0 / max(pf(1u), 1.0);
    let n = floor(v.y / w + 1e-4);
    let o = v.y / w - n - 0.5;
    let h = rnd1(i32(n), P.seed);
    let t = ss(0.0, 1.0, clamp((pf(0u) * (1.0 + pf(3u)) - h * pf(3u)), 0.0, 1.0));
    let len = P.shape * abs(cos(pf(2u))) + abs(sin(pf(2u)));
    if P.i.y == 3 {
        let a = t * PI;
        let c = cos(a);
        let u = o / max(abs(c), 1e-3);
        if abs(u) > 0.5 {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        let seen = turn(vec2<f32>(v.x, (n + 0.5 + u) * w), pf(2u));
        let shade = 0.35 + 0.65 * abs(c);
        if c > 0.0 {
            let x = pic0(seen, 1);
            return vec4<f32>(x.xyz * shade, x.w);
        }
        let x = pic1(seen, 1);
        return vec4<f32>(x.xyz * shade, x.w);
    }
    var s = 1.0;
    if P.i.y == 0 {
        s = 1.0 - 2.0 * md(n, 2.0);
    } else if P.i.y == 2 {
        s = sign(h - 0.5);
    }
    let q = turn(vec2<f32>(v.x - s * len * t, v.y), pf(2u));
    let a = pic0(q, 0);
    return a + at1(p.x, p.y) * (1.0 - a.w);
}

// Squares that flip over in space, each at its time. f0 how far, f1: how
// many down, f2: how unevenly, f3: the way the wave runs, f4: the color
// between; i.y: 0 about their upright, 1 about their level axis, 2 at
// random.
fn k_flip_tiles(p: vec2<i32>) -> vec4<f32> {
    let s = 1.0 / max(pf(1u), 1.0);
    let q = pos(p) / s;
    let c = floor(q);
    let o = q - c - vec2<f32>(0.5);
    let m = (c + vec2<f32>(0.5)) * s;
    let h = rnd(i32(c.x), i32(c.y), P.seed);
    let w = 0.5 * (P.shape * abs(cos(pf(3u))) + abs(sin(pf(3u)))) + s;
    let order = mix1((turn(m, -pf(3u)).x / w + 1.0) * 0.5, h, pf(2u));
    let t = clamp(pf(0u) * 2.2 - order * 1.2, 0.0, 1.0);
    let a = t * t * (3.0 - 2.0 * t) * PI;
    var axis = P.i.y;
    if axis == 2 {
        axis = i32(step(0.5, rnd(i32(c.x), i32(c.y), P.seed + 3u)));
    }
    let k = cos(a);
    let depth = sin(a);
    var u = o;
    if axis == 0 {
        let z = 1.0 + depth * o.x * 0.5;
        u = vec2<f32>(o.x / max(abs(k), 1e-3), o.y * z);
    } else {
        let z = 1.0 + depth * o.y * 0.5;
        u = vec2<f32>(o.x * z, o.y / max(abs(k), 1e-3));
    }
    let e = 0.5 - max(abs(u.x), abs(u.y));
    let cover = clamp(e / (px() / s) + 0.5, 0.0, 1.0);
    let seen = m + u * s;
    var x = pic0(seen, 1);
    if k < 0.0 {
        x = pic1(seen, 1);
    }
    let shade = 0.45 + 0.55 * abs(k);
    let lit_by = mix1(shade, 1.0, step(t, 0.0) + step(1.0, t));
    return mix4(vec4<f32>(vec3<f32>(pf(4u)), 1.0), vec4<f32>(x.xyz * lit_by, x.w), cover);
}

// The picture burnt through: noise that opens, its rim alight. f0 how far,
// f1: the noise's size, f2: the rim's width, f3-5 its color, f6-8 the
// char's, f9: its way (a turn), f10: how much of it is that way, f11: heat
// that bends the picture near the rim.
fn k_burn(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let n = fbm(vec3<f32>(q / max(pf(1u), 1e-3), 0.0), 5, P.seed);
    let w = 0.5 * (P.shape * abs(cos(pf(9u))) + abs(sin(pf(9u))));
    let all = (turn(q, -pf(9u)).x / w + 1.0) * 0.5;
    let f = mix1(0.5 + n * 0.75, all + n * 0.28, pf(10u));
    let rim = max(pf(2u), 1e-3);
    let d = pf(0u) * (1.4 + rim * 4.0) - 0.2 - rim * 2.0 - f;
    let heat = exp(-sq((d + rim) / (rim * 2.5))) * pf(11u);
    let bend_by = vec2<f32>(noise3(vec3<f32>(q * 14.0, pf(0u) * 9.0), P.seed + 1u), noise3(vec3<f32>(q * 14.0 + vec2<f32>(3.0, 8.0), pf(0u) * 9.0), P.seed + 2u)) * heat * 0.02;
    let a = pic0(q + bend_by, 1);
    let b = at1(p.x, p.y);
    let char_by = ss(-rim * 2.6, -rim * 0.3, d);
    let hole = ss(-rim * 0.15, rim * 0.15, d);
    var c = mix4(a, vec4<f32>(pf3(6u), max(a.w, 0.0)), char_by * 0.92);
    let ember = exp(-sq((d + rim * 0.35) / (rim * 0.5))) * (0.7 + 0.6 * noise3(vec3<f32>(q * 30.0, pf(0u) * 14.0), P.seed + 4u));
    c = lit(c, pf3(3u) * max(ember, 0.0) * 1.8, 0);
    let o = mix4(c, b, hole);
    let spill = exp(-max(d, 0.0) / rim * 1.4) * hole * 0.5;
    return lit(o, pf3(3u) * spill, 0);
}

// Ripples of water with one picture giving way to the other where they
// have passed. f0 how far, f1-2 the middle, f3: how high, f4: how far
// apart, f5: the ring's width.
fn k_water(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(1u);
    let r = length(d);
    let n = d / max(r, 1e-5);
    let far = length(vec2<f32>(P.shape * 0.5 + abs(pf(1u)), 0.5 + abs(pf(2u)))) + pf(5u) * 2.0;
    let front = pf(0u) * far - pf(5u);
    let x = (r - front) / max(pf(5u), 1e-3);
    let env = exp(-x * x * 2.0);
    let w = sin((r - front) / max(pf(4u), 1e-3) * TAU) * env * pf(3u);
    let to = q + n * w;
    let k = ss(0.35, -0.35, x);
    let c = mix4(pic0(to, 2), pic1(to, 2), k);
    let live = ss(0.0, 0.05, pf(0u)) * ss(1.0, 0.9, pf(0u));
    return vec4<f32>(c.xyz * (1.0 + w / max(pf(3u), 1e-4) * 0.22 * live), c.w);
}

// Doors: the first picture split and swung or slid apart. f0 how far, f1:
// the turn of the split; i.y: 0 slid, 1 swung in space, 2 squeezed; f2:
// shade.
fn k_doors(p: vec2<i32>) -> vec4<f32> {
    let v = turn(pos(p), -pf(1u));
    let half_w = 0.5 * (P.shape * abs(cos(pf(1u))) + abs(sin(pf(1u))));
    let t = pf(0u);
    let s = sign(v.x + 1e-6);
    var x = abs(v.x);
    var y = v.y;
    var shade = 1.0;
    if P.i.y == 0 {
        x = x - half_w * t;
    } else if P.i.y == 2 {
        x = half_w - (half_w - x) / max(1.0 - t, 1e-3);
    } else {
        // Hinged at the sides, opening away.
        let a = t * PI * 0.5;
        let h = half_w - x;
        let den = cos(a) - h * sin(a) * 0.8;
        let u = h / max(den, 1e-3);
        if den <= 1e-3 || u > half_w {
            return at1(p.x, p.y);
        }
        y = v.y * (1.0 + u * sin(a) * 0.8);
        x = half_w - u;
        shade = 1.0 - pf(2u) * t * 0.8;
    }
    if x < 0.0 {
        return at1(p.x, p.y);
    }
    let a = pic0(turn(vec2<f32>(s * x, y), pf(1u)), 0);
    let edge = clamp(x / px() + 0.5, 0.0, 1.0);
    let c = vec4<f32>(a.xyz * shade, a.w) * edge;
    return c + at1(p.x, p.y) * (1.0 - c.w);
}
