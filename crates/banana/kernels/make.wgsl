// Banana Split's generated pictures: clouds, textures, gradients, beams.
// Most share their numbers: f0-1 the middle (frame heights), f2 the size,
// f3 the turn, f4 the brightness, f5 the contrast, f6-8, f9-11, f12-14
// three colors, f15 how far in their time they are, f16 on each one's own;
// i.x: layers, i.y: how it is see-through (0 not at all, 1 by its light, 2
// by its pattern), i.z: which kind, i.w: how many colors (2 or 3).

fn place(p: vec2<i32>) -> vec2<f32> {
    return turn(pos(p) - pf2(0u), -pf(3u)) / max(pf(2u), 1e-4);
}

// A color as see-through as the pattern (its value t) is told to be.
fn seen(c: vec3<f32>, t: f32) -> vec4<f32> {
    if P.i.y == 1 {
        return vec4<f32>(c, clamp(max(max(c.x, c.y), c.z), 0.0, 1.0));
    }
    if P.i.y == 2 {
        return vec4<f32>(c * t, t);
    }
    return vec4<f32>(c, 1.0);
}

// The pattern (0 to 1) in its colors.
fn dyed(v: f32) -> vec4<f32> {
    let t = clamp((v - 0.5) * pf(5u) + 0.5, 0.0, 1.0);
    var c = mix3(pf3(6u), pf3(9u), t);
    if P.i.w == 3 {
        c = mix3(mix3(pf3(6u), pf3(9u), ss(0.0, 0.5, t)), pf3(12u), ss(0.5, 1.0, t));
    }
    return seen(c * pf(4u), t);
}

// Light alone.
fn glowing(c: vec3<f32>) -> vec4<f32> {
    let l = max(c * pf(4u), vec3<f32>(0.0));
    return vec4<f32>(l, clamp(max(max(l.x, l.y), l.z), 0.0, 1.0));
}

// Clouds; f16-17: how the wind moves them, f18: how they billow (0 to 1).
fn k_clouds(p: vec2<i32>) -> vec4<f32> {
    let q = place(p) + pf2(16u) * pf(15u);
    let z = pf(15u) * 0.25;
    let w = vec2<f32>(noise3(vec3<f32>(q * 0.7, z), P.seed + 21u), noise3(vec3<f32>(q * 0.7 + vec2<f32>(4.1, 8.3), z), P.seed + 22u));
    let n = fbm(vec3<f32>(q + w * 0.45 * pf(18u), z), P.i.x, P.seed);
    let r = ridges(vec3<f32>(q * 0.8 + w * 0.3, z), P.i.x, P.seed + 40u) - 0.62;
    return dyed(0.5 + n * 0.75 + r * 0.5 * pf(18u));
}

// Clouds whose colors each go their way.
fn k_clouds_color(p: vec2<i32>) -> vec4<f32> {
    let q = place(p) + pf2(16u) * pf(15u);
    let z = pf(15u) * 0.25;
    let r = 0.5 + fbm(vec3<f32>(q, z), P.i.x, P.seed) * 0.8;
    let g = 0.5 + fbm(vec3<f32>(q + vec2<f32>(5.2, 1.3), z), P.i.x, P.seed + 301u) * 0.8;
    let b = 0.5 + fbm(vec3<f32>(q + vec2<f32>(9.7, 8.1), z), P.i.x, P.seed + 601u) * 0.8;
    let c = clamp((vec3<f32>(r, g, b) - vec3<f32>(0.5)) * pf(5u) + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    let o = (pf3(6u) * c.x + pf3(9u) * c.y + pf3(12u) * c.z) * pf(4u);
    return seen(o, clamp(max(max(c.x, c.y), c.z), 0.0, 1.0));
}

// A nebula: gas lit from inside, stars through it. f18: the stars.
fn k_nebula(p: vec2<i32>) -> vec4<f32> {
    let q = place(p) + pf2(16u) * pf(15u);
    let z = pf(15u) * 0.12;
    let w = vec2<f32>(fbm(vec3<f32>(q * 0.6, z), 3, P.seed + 3u), fbm(vec3<f32>(q * 0.6 + vec2<f32>(7.7, 2.2), z), 3, P.seed + 4u));
    let d = clamp(0.55 + fbm(vec3<f32>(q + w * 1.1, z), P.i.x, P.seed) * 1.1, 0.0, 1.0);
    let e = ridges(vec3<f32>(q * 1.3 + w * 0.8, z), P.i.x, P.seed + 50u);
    let core = pw(clamp(0.6 + w.x * 1.2, 0.0, 1.0), 3.0);
    var c = mix3(pf3(6u), pf3(9u), ss(0.25, 0.85, d)) * (0.25 + d * 0.9);
    c = mix3(c, pf3(12u) * 1.25, core * e);
    let s = cells(pos(p) * 42.0, 0.9, 0.0, P.seed + 80u);
    let star = pw(ss(0.13, 0.0, s.x), 3.0) * step(0.55, s.z) * pf(18u);
    c = contrast3(c, pf(5u)) + vec3<f32>(star * (1.0 - d * 0.6));
    return seen(max(c, vec3<f32>(0.0)) * pf(4u), d);
}

// A sky of clouds going away to a horizon. f16-17: the wind, f18: where
// the horizon is (frame heights down from the middle), f19: how near the
// sky is; f12-14: the haze's color.
fn k_cloud_sky(p: vec2<i32>) -> vec4<f32> {
    let q = turn(pos(p) - pf2(0u), -pf(3u));
    let d = pf(18u) - q.y;
    let far = max(abs(d), 0.004);
    let u = vec2<f32>(q.x / far, pf(19u) / far) / max(pf(2u), 1e-3) * 0.5 + pf2(16u) * pf(15u);
    let fade_far = ss(0.0, 0.22, far);
    let n = fbm(vec3<f32>(u, pf(15u) * 0.2), P.i.x, P.seed);
    let t = clamp(n * 0.8 * pf(5u) + 0.5, 0.0, 1.0);
    var c = mix3(pf3(6u), pf3(9u), t * t * (3.0 - 2.0 * t));
    c = mix3(pf3(12u), c, fade_far);
    return seen(c * pf(4u), t);
}

// Clouds of turning colors. f18: how many times round the rainbow, f19:
// how vivid.
fn k_clouds_hue(p: vec2<i32>) -> vec4<f32> {
    let q = place(p) + pf2(16u) * pf(15u);
    let z = pf(15u) * 0.3;
    let w = fbm(vec3<f32>(q * 0.8, z), 3, P.seed + 9u);
    let n = fbm(vec3<f32>(q + vec2<f32>(w, -w) * 0.8, z), P.i.x, P.seed);
    let h = hsv(fr(n * pf(18u) + pf(15u) * 0.12), pf(19u), 1.0);
    let c = clamp(contrast3(h * (0.65 + 0.35 * cos((n * pf(18u) + w) * TAU)), pf(5u)), vec3<f32>(0.0), vec3<f32>(2.0));
    return seen(c * pf(4u), clamp(0.5 + n, 0.0, 1.0));
}

// A storm's eye. f18: how tightly it winds, f19: the eye's size.
fn k_cloud_storm(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let r = length(q);
    let a = pf(18u) / (r * 1.4 + 0.25) + pf(15u) * 0.6;
    let v = turn(q, a);
    let w = vec2<f32>(noise3(vec3<f32>(v * 0.9, pf(15u) * 0.2), P.seed + 7u), noise3(vec3<f32>(v * 0.9 + vec2<f32>(3.0, 8.0), pf(15u) * 0.2), P.seed + 8u));
    let n = fbm(vec3<f32>(v + w * 0.5, pf(15u) * 0.15), P.i.x, P.seed);
    let eye = ss(pf(19u) * 0.5, pf(19u) * 1.5 + 0.05, r);
    return dyed((0.5 + n * 0.8) * eye + (1.0 - eye) * 0.02);
}

// Curtains of light in the sky. f18: how tall, f19: rays in them, f20: how
// they sway, f21: where they hang (frame heights down); i.x: curtains.
fn k_aurora(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p) - pf2(0u);
    var sum = vec3<f32>(0.0);
    let s = 1.0 / max(pf(2u), 1e-3);
    for (var i = 0; i < P.i.x; i++) {
        let f = f32(i);
        let x = q.x * s * (0.8 + 0.25 * f) + f * 3.7;
        let sway = (noise3(vec3<f32>(x * 0.7, f, pf(15u) * 0.15), P.seed) * 0.5 + noise3(vec3<f32>(x * 1.9, f + 4.0, pf(15u) * 0.25), P.seed + 2u) * 0.2) * pf(20u);
        let base = pf(21u) + sway * 0.4 - f * 0.05;
        let h = (base - q.y) / max(pf(18u) * (0.7 + 0.25 * noise3(vec3<f32>(x * 0.4, f, pf(15u) * 0.1), P.seed + 5u)), 1e-3);
        let ray = mix1(1.0, 0.45 + 0.55 * pw(0.5 + 0.5 * noise3(vec3<f32>(x * 9.0 + sway * 5.0, h * 0.4, pf(15u) * 0.5), P.seed + 9u), 1.5) * 2.0, pf(19u));
        let k = ss(-0.05, 0.1, h) * exp(-max(h, 0.0) * 1.9) * 1.35 * ray * (0.6 + 0.4 * noise3(vec3<f32>(x * 0.5, 3.0 + f, pf(15u) * 0.2), P.seed + 11u));
        let c = mix3(pf3(6u), mix3(pf3(9u), pf3(12u), ss(0.3, 0.9, h)), ss(0.05, 0.45, h));
        sum += c * max(k, 0.0) / (1.0 + 0.35 * f);
    }
    return glowing(sum);
}

// The light on a pool's floor. f18: how sharp.
fn k_caustics(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let t = pf(15u) * 0.6;
    var u = q;
    var sum = 1.0;
    // Waves bent by waves: where they meet, light gathers.
    for (var i = 0; i < 4; i++) {
        let f = f32(i) + 1.0;
        u = q * 2.2 + vec2<f32>(
            cos(t * (1.0 - 0.35 * f) - u.x) + sin(t * (1.0 - 0.35 * f) + u.y),
            sin(t * (1.0 - 0.35 * f) - u.y) + cos(t * (1.0 - 0.35 * f) + u.x),
        );
        let d = vec2<f32>(q.x * 2.2 / max(abs(sin(u.x + t)), 0.04), q.y * 2.2 / max(abs(cos(u.y + t)), 0.04));
        sum += 1.0 / max(length(d) * 0.28 + 0.12, 0.05) * 0.12;
    }
    let w = vec2<f32>(noise3(vec3<f32>(q * 1.4, t * 0.5), P.seed), noise3(vec3<f32>(q * 1.4 + vec2<f32>(5.0, 9.0), t * 0.5), P.seed + 1u));
    let a = cells(q * 1.5 + w * 0.55, 0.9, t * 1.4, P.seed + 2u);
    let b = cells(q * 1.9 - w * 0.45 + vec2<f32>(3.3, 1.7), 0.9, t * 1.7, P.seed + 3u);
    let line = pw(1.0 - clamp((a.y - a.x) * 1.5, 0.0, 1.0), pf(18u)) + pw(1.0 - clamp((b.y - b.x) * 1.5, 0.0, 1.0), pf(18u)) * 0.7;
    return dyed(clamp(line * 0.75 + 0.02 * sum, 0.0, 1.0));
}

// Lines. f18: how wide (parts of a square), f19: their softness, f20: a
// heavier line every this many (0 none); i.z: 0 squares, 1 triangles, 2
// hexagons, 3 in perspective (f21: the horizon).
fn k_grid(p: vec2<i32>) -> vec4<f32> {
    var q = place(p);
    var w = px() / max(pf(2u), 1e-4);
    var dim = 1.0;
    if P.i.z == 3 {
        let v = turn(pos(p) - pf2(0u), -pf(3u));
        let d = v.y - pf(21u);
        if d < 0.004 {
            return dyed(0.0) * step(0.5, f32(1 - min(P.i.y, 1)));
        }
        q = vec2<f32>(v.x / d, 0.5 / d) / max(pf(2u), 1e-4) * 0.35 + vec2<f32>(0.0, pf(15u));
        w = px() / max(pf(2u), 1e-4) * 0.35 / d * (1.0 + 0.5 / d);
        dim = ss(0.0, 0.18, d);
    }
    var e = 1.0;
    if P.i.z == 2 {
        let s = vec2<f32>(1.0, 1.7320508);
        let a = q - (floor(q / s) + vec2<f32>(0.5)) * s;
        let b = q - (floor(q / s - vec2<f32>(0.5)) + vec2<f32>(1.0)) * s;
        var o = a;
        if dot(b, b) < dot(a, a) {
            o = b;
        }
        let h = abs(o);
        e = 0.5 - max(h.x, h.x * 0.5 + h.y * 0.8660254);
    } else if P.i.z == 1 {
        let a = line1(q.y / 0.8660254);
        let b = line1(q.x + q.y * 0.57735027);
        let c = line1(q.x - q.y * 0.57735027);
        e = min(min(a, b), c);
    } else {
        e = min(line1(q.x), line1(q.y));
    }
    let wide = pf(18u) * 0.5;
    let soft = max(pf(19u) * 0.5, w * 0.75);
    var k = stroke(e, wide, soft);
    if pf(20u) > 1.5 && P.i.z != 2 && P.i.z != 1 {
        let g = min(line1(q.x / pf(20u)), line1(q.y / pf(20u))) * pf(20u);
        k = max(k * 0.55, stroke(g, wide, soft));
    }
    return dyed(k * dim);
}

// How far a number is from the nearest whole one.
fn line1(x: f32) -> f32 {
    return abs(fr(x + 0.5) - 0.5);
}

// Wear: stains, scratches and specks. f18: the stains, f19: the scratches,
// f20: the specks.
fn k_grunge(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let w = vec2<f32>(noise3(vec3<f32>(q * 1.5, 1.0), P.seed), noise3(vec3<f32>(q * 1.5, 5.0), P.seed + 1u));
    let stain = ss(-0.05, 0.5, fbm(vec3<f32>(q + w * 0.6, 0.0), P.i.x, P.seed + 2u)) * (0.6 + 0.4 * ridges(vec3<f32>(q * 3.0, 2.0), 3, P.seed + 3u));
    let sq1 = turn(q, 0.5 + 0.4 * noise3(vec3<f32>(q * 0.3, 8.0), P.seed + 4u));
    let s1 = noise3(vec3<f32>(sq1.x * 26.0, sq1.y * 0.6, 3.0), P.seed + 5u);
    let s2 = noise3(vec3<f32>(sq1.y * 34.0 + 9.0, sq1.x * 0.8, 6.0), P.seed + 6u);
    let scratch = ss(0.5, 0.62, s1) + ss(0.56, 0.66, s2);
    let c = cells(q * 14.0, 0.95, 0.0, P.seed + 7u);
    let speck = ss(0.3 * c.z, 0.0, c.x) * step(0.6, c.z);
    let t = stain * pf(18u) + min(scratch, 1.0) * pf(19u) * 0.8 + speck * pf(20u);
    return dyed(clamp(t, 0.0, 1.0));
}

// Motes in a beam. f18: how many, f19-20: how they drift in a second,
// f21: how far out of focus some are; i.x: layers.
fn k_dust(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var sum = 0.0;
    for (var l = 0; l < P.i.x; l++) {
        let f = f32(l);
        let s = pf(2u) * (1.0 + 0.8 * f) * 9.0;
        let wind = pf2(19u) * pf(15u) / (1.0 + 0.5 * f);
        let g = (q - wind) / max(s, 1e-3) + vec2<f32>(f * 7.3, f * 3.1);
        let b = floor(g);
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let cx = i32(b.x) + i;
                let cy = i32(b.y) + j;
                let h = rnd2(cx, cy, P.seed + u32(l) * 41u);
                let o = rnd2(cx, cy, P.seed + u32(l) * 83u + 1u);
                if h.x < pf(18u) {
                    let a = h.y * TAU;
                    let c = vec2<f32>(f32(cx), f32(cy)) + vec2<f32>(0.25) + o * 0.5 + vec2<f32>(sin(pf(15u) * (0.4 + h.y) + a), cos(pf(15u) * (0.3 + h.x) + a * 1.7)) * 0.2;
                    let blur_by = 1.0 + pf(21u) * f * 1.4;
                    let r = (0.03 + 0.035 * o.x) * blur_by;
                    let d = length(g - c) / r;
                    let tw = 0.55 + 0.45 * sin(pf(15u) * (1.0 + 2.5 * h.y) + a * 3.0);
                    sum += (1.0 - ss(0.2, 1.0, d)) * tw / (blur_by * 1.2);
                }
            }
        }
    }
    return glowing(pf3(9u) * sum);
}

// A gradient. f0-1 where it starts, f2-3 where it ends, f6-9, f10-13,
// f14-17 its colors (premultiplied, with their alpha), f18: the middle
// one's place, f19: how smooth, f20: dither; i.z: 0 along a line, 1 round,
// 2 a cone, 3 a diamond, 4 mirrored; i.w: 2 or 3 colors; f21: how many
// times.
fn k_gradient(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = pf2(2u) - pf2(0u);
    let v = q - pf2(0u);
    let len = max(length(d), 1e-4);
    var t = dot(v, d) / (len * len);
    if P.i.z == 1 {
        t = length(v) / len;
    } else if P.i.z == 2 {
        t = fr((angle(v) - angle(d)) / TAU + 1.0);
    } else if P.i.z == 3 {
        let m = turn(v, -angle(d));
        t = (abs(m.x) + abs(m.y)) / len;
    } else if P.i.z == 4 {
        t = abs(t);
    }
    if pf(21u) > 1.0 {
        t = 1.0 - abs(md(t * pf(21u), 2.0) - 1.0);
    }
    t = clamp(t, 0.0, 1.0);
    let f = full_pixel(p);
    t = clamp(t + (rnd(f.x, f.y, 7u) - 0.5) * pf(20u), 0.0, 1.0);
    t = mix1(t, t * t * (3.0 - 2.0 * t), pf(19u));
    let a = vec4<f32>(pf3(6u), pf(9u));
    let b = vec4<f32>(pf3(10u), pf(13u));
    let c = vec4<f32>(pf3(14u), pf(17u));
    if P.i.w == 3 {
        let m = clamp(pf(18u), 0.001, 0.999);
        if t < m {
            return mix4(a, b, t / m);
        }
        return mix4(b, c, (t - m) / (1.0 - m));
    }
    return mix4(a, b, t);
}

// A moon. f0-1 its middle, f2 its radius, f16: where the light comes from
// (a turn), f17: how full (0 new, 1 full), f18: its seas, f19: its
// craters, f20: the glow round it, f21: earthshine.
fn k_moon(p: vec2<i32>) -> vec4<f32> {
    let v = turn((pos(p) - pf2(0u)) / max(pf(2u), 1e-4), -pf(3u));
    let r = length(v);
    let w = px() / max(pf(2u), 1e-4);
    let disc = 1.0 - ss(1.0 - w, 1.0 + w, r);
    let m = min(r, 0.9995);
    let dir = v / max(r, 1e-5) * m;
    let n = vec3<f32>(dir, sqrt(1.0 - m * m));
    let s = vec3<f32>(n.xy * 1.6, n.z * 1.6) + vec3<f32>(pf(15u) * 0.03, 0.0, 0.0);
    let sea = ss(-0.02, 0.32, fbm(s * 0.9, 5, P.seed));
    let rough = fbm(s * 5.0, 4, P.seed + 1u);
    var crater = 0.0;
    var bump = vec2<f32>(0.0);
    for (var i = 0; i < 3; i++) {
        let k = 2.4 + f32(i) * 3.3;
        let c = cells(vec2<f32>(atan2(n.x, n.z), asin(clamp(n.y, -1.0, 1.0))) * k, 0.8, 0.0, P.seed + u32(i) * 7u + 3u);
        let size = 0.18 + 0.16 * c.z;
        let t = c.x / size;
        let ring = exp(-sq((t - 1.0) * 4.0)) - 0.55 * (1.0 - ss(0.6, 1.0, t));
        crater += ring * step(0.35, c.z) / (1.0 + 0.6 * f32(i));
    }
    let a = (1.0 - pf(17u)) * PI;
    let light = vec3<f32>(turn(vec2<f32>(sin(a), 0.0), pf(16u)), cos(a));
    let lean = normalize(vec3<f32>(n.x + rough * 0.06, n.y + rough * 0.04, n.z));
    let lit_by = max(dot(lean, light), 0.0);
    let albedo = (0.82 - 0.38 * sea * pf(18u)) * (1.0 + rough * 0.22) + crater * 0.16 * pf(19u);
    let body = pf3(9u) * max(albedo, 0.05) * (pw(lit_by, 0.75) + pf(21u) * 0.06);
    let full = pf(17u) * pf(20u);
    let glow = exp(-max(r - 1.0, 0.0) * 5.0) * 0.28 * full + exp(-max(r - 1.0, 0.0) * 1.5) * 0.08 * full;
    let c = (body * disc + pf3(12u) * glow * (1.0 - disc)) * pf(4u);
    return vec4<f32>(c, max(disc, clamp(max(max(c.x, c.y), c.z), 0.0, 1.0)));
}

// Stars. f18: how many, f19: their twinkle, f20: the milky way's strength,
// f21: its turn, f22: how many bright ones have points; i.y as ever.
fn k_stars(p: vec2<i32>) -> vec4<f32> {
    let q = turn(pos(p) - pf2(0u), -pf(3u) - pf(15u) * 0.01);
    var sum = vec3<f32>(0.0);
    for (var l = 0; l < 3; l++) {
        let f = f32(l);
        let g = q * (14.0 + 17.0 * f) / max(pf(2u), 1e-3);
        let c = floor(g);
        let h = rnd2(i32(c.x), i32(c.y), P.seed + u32(l) * 19u);
        let o = rnd2(i32(c.x), i32(c.y), P.seed + u32(l) * 23u + 5u);
        if h.x < pf(18u) {
            let d = (g - c - vec2<f32>(0.2) - o * 0.6) * max(pf(2u), 1e-3);
            let mag = pw(h.y, 5.0) + 0.07;
            let r = length(d);
            let tw = 1.0 - pf(19u) * (0.5 + 0.5 * sin(P.time * (2.0 + 6.0 * o.x) + h.y * 60.0));
            let size = 0.03 + 0.05 * mag;
            var k = exp(-sq(r / size)) * (0.35 + mag * 1.3);
            let spikes = exp(-abs(d.x) * 60.0) * exp(-abs(d.y) * 6.0) + exp(-abs(d.y) * 60.0) * exp(-abs(d.x) * 6.0);
            k += spikes * step(1.0 - pf(22u) * 0.15, h.y) * 0.8;
            sum += mix3(vec3<f32>(0.75, 0.85, 1.0), vec3<f32>(1.0, 0.86, 0.7), o.y) * k * tw;
        }
    }
    let b = turn(q, -pf(21u));
    let band = exp(-sq(b.y * 3.2 + noise3(vec3<f32>(b * 1.3, 2.0), P.seed + 60u) * 0.6));
    let dust = fbm(vec3<f32>(b * 3.5, 4.0), 5, P.seed + 61u);
    let way = band * max(0.35 + dust * 1.2, 0.0);
    let sky = pf3(6u) + pf3(12u) * way * pf(20u) * 0.5;
    let c = (sky + pf3(9u) * sum) * pf(4u);
    if P.i.y == 0 {
        return vec4<f32>(c, 1.0);
    }
    return glowing(pf3(9u) * sum + pf3(12u) * way * pf(20u) * 0.5);
}

// A shape. f0-1 its middle, f2 its size, f3 its turn, f16: its stretch
// across, f17: how round its corners, f18: its edge's softness, f19: an
// outline's width (0 filled), f20: its points' depth, f6-8 its color,
// f9-11 the color round it, f12: that one's opacity; i.x: points; i.z: 0
// ellipse, 1 rectangle, 2 polygon, 3 star, 4 heart, 5 cross, 6 ring.
fn k_shape(p: vec2<i32>) -> vec4<f32> {
    let v0 = turn(pos(p) - pf2(0u), -pf(3u));
    let s = max(pf(2u), 1e-4);
    let v = vec2<f32>(v0.x / max(pf(16u), 1e-3), v0.y) / s;
    var d = length(v) - 1.0;
    let n = max(f32(P.i.x), 3.0);
    if P.i.z == 1 {
        let c = pf(17u);
        let q = abs(v) - vec2<f32>(1.0 - c);
        d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - c;
    } else if P.i.z == 2 {
        d = mix1(poly1(v, n), length(v), pf(17u)) - 1.0;
    } else if P.i.z == 3 {
        d = star1(v, n, max(pf(20u), 0.05)) - 1.0;
    } else if P.i.z == 4 {
        let h = vec2<f32>(abs(v.x), -v.y + 0.12) * 1.05;
        let t = h.x * h.x + h.y * h.y - 0.75 * h.x * h.y * 1.35;
        d = sqrt(max(t, 0.0)) - 0.86;
    } else if P.i.z == 5 {
        let q = abs(v);
        let a = max(q.x - max(pf(20u), 0.05), q.y - 1.0);
        let b = max(q.x - 1.0, q.y - max(pf(20u), 0.05));
        d = min(a, b);
    } else if P.i.z == 6 {
        d = abs(length(v) - 1.0 + max(pf(20u), 0.02) * 0.5) - max(pf(20u), 0.02) * 0.5;
    }
    if pf(19u) > 0.0 {
        d = abs(d + pf(19u) * 0.5) - pf(19u) * 0.5;
    }
    let w = max(pf(18u), px() / s * 0.75);
    let k = 1.0 - ss(-w, w, d);
    let o = pf(12u);
    return mix4(vec4<f32>(pf3(9u) * o, o), vec4<f32>(pf3(6u), 1.0), k);
}

// How far out a place is in a star of n points, its notches `deep` out
// (1 on its sides).
fn star1(v: vec2<f32>, n: f32, deep: f32) -> f32 {
    let b = PI / n;
    let t = angle(vec2<f32>(-v.y, v.x));
    let a = abs(t - 2.0 * b * floor(t / (2.0 * b) + 0.5));
    let notch = vec2<f32>(cos(b), sin(b)) * deep;
    let e = notch - vec2<f32>(1.0, 0.0);
    // Where the ray meets the side from the point (1, 0) to the notch.
    let r = notch.y / max(cos(a) * e.y - sin(a) * e.x, 1e-5);
    return length(v) / max(r, 1e-4);
}

fn poly1(v: vec2<f32>, n: f32) -> f32 {
    let a = angle(vec2<f32>(-v.y, v.x));
    let s = PI / n;
    let t = a - 2.0 * s * floor(a / (2.0 * s)) - s;
    return length(v) * cos(t) / cos(s);
}

// Sparkles. f18: how many, f19: how often each shines (a second), f20:
// their points' length, f21: every color (0 to 1), f22-23: how they drift;
// i.x: layers.
fn k_sparkles(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var sum = vec3<f32>(0.0);
    for (var l = 0; l < P.i.x; l++) {
        let f = f32(l);
        let s = max(pf(2u), 1e-3) * (1.0 - 0.22 * f) * 4.0;
        let g = (q - pf2(22u) * pf(15u)) / s + vec2<f32>(f * 3.9, f * 8.2);
        let b = floor(g);
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let cx = i32(b.x) + i;
                let cy = i32(b.y) + j;
                let h = rnd2(cx, cy, P.seed + u32(l) * 29u);
                let t = pf(15u) * pf(19u) + h.x * 13.0;
                let n = floor(t);
                let o = rnd2(cx + i32(n) * 17, cy - i32(n) * 5, P.seed + u32(l) * 61u + 2u);
                if o.x < pf(18u) {
                    let c = vec2<f32>(f32(cx), f32(cy)) + vec2<f32>(0.25) + vec2<f32>(h.y, o.y) * 0.5;
                    let life = t - n;
                    let k = pw(sin(life * PI), 2.0);
                    let d = (g - c) * 4.0;
                    let r = length(d);
                    let z = turn(d, 0.35 * (h.x - 0.5));
                    let len = max(pf(20u), 0.05);
                    let star = exp(-abs(z.x) * 30.0) * exp(-abs(z.y) * 4.0 / len) + exp(-abs(z.y) * 30.0) * exp(-abs(z.x) * 4.0 / len);
                    let core = exp(-r * r * 120.0) * 1.4 + exp(-r * 12.0) * 0.35;
                    let tint = mix3(pf3(9u), hsv(h.y, 0.75, 1.15), pf(21u));
                    sum += tint * (core + star * 0.75) * k * (0.4 + 0.6 * o.y);
                }
            }
        }
    }
    return glowing(sum);
}

// Cells. f18: how uneven, f19: the walls' width; i.z: 0 bubbles, 1 walls,
// 2 stones (each its own shade), 3 crystals.
fn k_cells(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let c = cells(q, pf(18u), pf(15u), P.seed);
    var v = 1.0 - clamp(c.x, 0.0, 1.0);
    if P.i.z == 1 {
        v = 1.0 - ss(pf(19u) * 0.5, pf(19u) * 0.5 + 0.05, c.y - c.x);
    } else if P.i.z == 2 {
        v = (0.25 + 0.75 * c.z) * ss(pf(19u) * 0.4, pf(19u) * 0.4 + 0.07, c.y - c.x);
    } else if P.i.z == 3 {
        let e = 0.02;
        let dx = cells(q + vec2<f32>(e, 0.0), pf(18u), pf(15u), P.seed);
        v = 0.5 + (dx.y - dx.x - c.y + c.x) / e * 0.35 + (c.z - 0.5) * 0.3;
    }
    return dyed(v);
}

// A spiral of colors. f18: how many arms, f19: how tight, f20: how vivid.
fn k_spiral(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let r = length(q);
    let t = angle(q) / TAU * max(floor(pf(18u) + 0.5), 1.0) + log(max(r, 1e-4)) * pf(19u) - pf(15u);
    if P.i.z == 1 {
        return dyed(0.5 + 0.5 * cos(t * TAU));
    }
    let c = hsv(fr(t), pf(20u), 1.0);
    return vec4<f32>(clamp(contrast3(c, pf(5u)), vec3<f32>(0.0), vec3<f32>(2.0)) * pf(4u), 1.0);
}

// A liquid folded into itself. f18: how much.
fn k_flux(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let z = pf(15u) * 0.2;
    let a = vec2<f32>(fbm(vec3<f32>(q, z), 4, P.seed), fbm(vec3<f32>(q + vec2<f32>(5.2, 1.3), z), 4, P.seed + 1u)) * pf(18u);
    let b = vec2<f32>(fbm(vec3<f32>(q + a * 2.2 + vec2<f32>(1.7, 9.2), z), 4, P.seed + 2u), fbm(vec3<f32>(q + a * 2.2 + vec2<f32>(8.3, 2.8), z), 4, P.seed + 3u)) * pf(18u);
    let n = fbm(vec3<f32>(q + b * 2.2, z), P.i.x, P.seed + 4u);
    return dyed(0.5 + n * 0.9 + (a.x + b.y) * 0.25);
}

// Folds of silk. f18: how deep, f19: how they lean.
fn k_silk(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let z = pf(15u) * 0.2;
    let w = fbm(vec3<f32>(q * 0.6, z), 3, P.seed) * pf(18u);
    let t = q.x * 1.2 + w * 2.6 + q.y * pf(19u);
    let a = sin(t * TAU);
    let b = sin((t * 1.7 + q.y * 0.3 + w) * TAU + 1.2);
    let sheen = pw(0.5 + 0.5 * cos(t * TAU * 1.0 + 0.9), 6.0);
    return dyed(0.42 + a * 0.2 + b * 0.12 + sheen * 0.35);
}

// Lines of equal height. f18: how many, f19: how wide.
fn k_contours(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let z = pf(15u) * 0.15;
    let e = px() / max(pf(2u), 1e-4);
    let n = fbm(vec3<f32>(q, z), P.i.x, P.seed) * pf(18u);
    let nx = fbm(vec3<f32>(q + vec2<f32>(e, 0.0), z), P.i.x, P.seed) * pf(18u);
    let ny = fbm(vec3<f32>(q + vec2<f32>(0.0, e), z), P.i.x, P.seed) * pf(18u);
    let g = max(length(vec2<f32>(nx - n, ny - n)), 1e-5);
    let d = line1(n) / g;
    // Half as wide, in pixels: a line thinner than one is fainter.
    let wide = pf(19u) * 0.02 / e;
    return dyed(stroke(d, wide, 0.75));
}

// Fibers and grains, close up. f18: how stringy.
fn k_fibers(p: vec2<i32>) -> vec4<f32> {
    let q = place(p) + vec2<f32>(0.0, pf(15u) * 0.2);
    let v = q + vec2<f32>(noise3(vec3<f32>(q * 0.35, 2.0), P.seed), noise3(vec3<f32>(q * 0.35, 8.0), P.seed + 9u)) * 1.6;
    let f = noise3(vec3<f32>(v.x * 9.0, v.y * mix1(9.0, 0.7, pf(18u)), 0.0), P.seed + 1u);
    let g = fbm(vec3<f32>(q * 6.0, 1.0), P.i.x, P.seed + 2u);
    return dyed(0.5 + f * 0.38 + g * 0.3);
}

// Two sets of rings crossing. f0-1 and f16-17: their middles, f18: how
// many rings, f19: the second's against the first; i.z 1: lines (f3, f20
// the turn between them).
fn k_moire(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var a = length(q - pf2(0u)) * pf(18u);
    var b = length(q - pf2(16u)) * pf(18u) * pf(19u);
    if P.i.z == 1 {
        a = turn(q - pf2(0u), -pf(3u)).x * pf(18u);
        b = turn(q - pf2(16u), -pf(3u) - pf(20u)).x * pf(18u) * pf(19u);
    }
    let s = sin((a - pf(15u)) * TAU) * sin((b + pf(15u)) * TAU);
    return dyed(0.5 + 0.5 * s);
}

// Nerves. f18: how thick, f19: how they pulse.
fn k_nerves(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let w = vec2<f32>(noise3(vec3<f32>(q * 1.6, pf(15u) * 0.2), P.seed), noise3(vec3<f32>(q * 1.6 + vec2<f32>(7.0, 3.0), pf(15u) * 0.2), P.seed + 1u)) * 0.45;
    let c = cells(q + w, 0.95, pf(15u) * 0.5, P.seed + 2u);
    let d = cells(q * 2.3 - w, 0.95, pf(15u) * 0.7, P.seed + 3u);
    let a = exp(-(c.y - c.x) / max(pf(18u), 1e-3) * 2.2);
    let b = exp(-(d.y - d.x) / max(pf(18u), 1e-3) * 3.2) * 0.4;
    let body = exp(-c.x * c.x * 26.0);
    let beat = 1.0 + pf(19u) * sin(pf(15u) * 4.0 - c.x * 9.0 + c.z * TAU);
    return dyed(clamp((a * beat + b + body * 1.3) * 0.8, 0.0, 1.0));
}

// A rough wall, lit from one side (f16, a turn; f17 its height). f18: how
// deep; i.z: 0 stone, 1 stucco, 2 hammered, 3 paper.
fn k_relief(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let e = max(px() / max(pf(2u), 1e-4), 0.012);
    // The wall here, and a little to each side (one loop: it is compiled
    // once).
    var h = 0.0;
    var d = vec2<f32>(0.0);
    for (var i = 0; i < 3; i++) {
        let r = relief1(q + vec2<f32>(f32(i % 2), f32(i / 2)) * e);
        if i == 0 {
            h = r;
        } else if i == 1 {
            d.x = (r - h) / e;
        } else {
            d.y = (r - h) / e;
        }
    }
    let n = normalize(vec3<f32>(-d * pf(18u), 1.0));
    let l = normalize(vec3<f32>(cos(pf(16u)), sin(pf(16u)), max(pf(17u), 0.05)));
    return dyed(clamp(dot(n, l), 0.0, 1.0) * 0.8 + h * 0.15 + 0.05);
}

fn relief1(q: vec2<f32>) -> f32 {
    if P.i.z == 1 {
        return ridges(vec3<f32>(q * 1.2, 0.0), P.i.x, P.seed) * 0.5;
    }
    if P.i.z == 2 {
        return cells(q * 1.6, 0.9, 0.0, P.seed).x * 0.5;
    }
    if P.i.z == 3 {
        return fbm(vec3<f32>(q * 5.0, 0.0), P.i.x, P.seed) * 0.12 + noise3(vec3<f32>(q * 1.1, 4.0), P.seed + 5u) * 0.05;
    }
    return fbm(vec3<f32>(q, 0.0), P.i.x, P.seed) * 0.5;
}

// Paint. f18: the strokes' length, f19: their relief.
fn k_paint(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    var u = q;
    for (var i = 0; i < 4; i++) {
        let a = noise3(vec3<f32>(u * 0.55, pf(15u) * 0.1), P.seed) * TAU;
        u += vec2<f32>(cos(a), sin(a)) * 0.12 * pf(18u);
    }
    let v = 0.5 + fbm(vec3<f32>(u * 0.9, pf(15u) * 0.1), 3, P.seed + 2u) * 0.9;
    let a = noise3(vec3<f32>(q * 0.55, pf(15u) * 0.1), P.seed) * TAU;
    let h = turn(q, -a);
    let hair = noise3(vec3<f32>(h.x * 1.5, h.y * 16.0, 0.0), P.seed + 3u);
    return dyed(v + hair * 0.16 * pf(19u));
}

// Waves meeting. f18: how many bends.
fn k_plasma(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let t = pf(15u);
    var v = sin(q.x * 2.0 + t) + sin((q.y * 2.0 + t) * 0.9);
    v += sin((q.x + q.y) * 1.4 * pf(18u) + t * 0.7);
    let c = q + vec2<f32>(sin(t * 0.33) * 1.2, cos(t * 0.5) * 1.2);
    v += sin(sqrt(dot(c, c) * 5.0 * pf(18u) + 1.0) + t);
    return dyed(0.5 + 0.5 * sin(v * 1.25 + t * 0.3));
}

// Ridges and veins. f18: how sharp.
fn k_veins(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let w = fbm(vec3<f32>(q * 0.7, pf(15u) * 0.1), 3, P.seed + 8u);
    let r = ridges(vec3<f32>(q + vec2<f32>(w, -w) * 0.6, pf(15u) * 0.15), P.i.x, P.seed);
    return dyed(pw(r, pf(18u)) * 1.1);
}

// Spots. f18: their size, f19: how uneven, f20: how soft; i.z: 0 in rows,
// 1 staggered, 2 scattered.
fn k_spots(p: vec2<i32>) -> vec4<f32> {
    let q = place(p) + vec2<f32>(pf(15u), 0.0);
    var r = pf(18u) * 0.5;
    var d = 1.0;
    if P.i.z == 2 {
        let c = cells(q, 0.85, pf(15u), P.seed);
        r = r * (1.0 - pf(19u) * c.z);
        d = c.x;
    } else {
        var u = q;
        if P.i.z == 1 {
            u = vec2<f32>(q.x + 0.5 * md(floor(q.y), 2.0), q.y);
        }
        let c = floor(u);
        r = r * (1.0 - pf(19u) * rnd(i32(c.x), i32(c.y), P.seed));
        d = length(u - c - vec2<f32>(0.5));
    }
    let w = max(pf(20u) * 0.5, px() / max(pf(2u), 1e-4));
    return dyed(1.0 - ss(r - w, r + w, d));
}

// Tiles. f18: their width over height, f19: the joints, f20: their bevel,
// f21: how their shades differ; i.z: 0 in rows, 1 bricks, 2 a herringbone.
fn k_tiling(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    var t = vec2<f32>(max(pf(18u), 0.1), 1.0);
    var u = q;
    if P.i.z == 2 {
        let v = turn(q, PI * 0.25);
        let c = floor(v);
        let m = i32(md(c.x + c.y, 4.0));
        // Boards two long, laid across and along in turn.
        u = v;
        if m < 2 {
            let f = v.x - (c.x - f32(m & 1));
            let o = vec2<f32>(f - 1.0, v.y - c.y - 0.5);
            return tile_done(o, vec2<f32>(2.0, 1.0), i32(c.x) - (m & 1), i32(c.y));
        }
        let f = v.y - (c.y - f32(m & 1));
        let o = vec2<f32>(v.x - c.x - 0.5, f - 1.0);
        return tile_done(o, vec2<f32>(1.0, 2.0), i32(c.x) + 400, i32(c.y) - (m & 1));
    }
    if P.i.z == 1 {
        u = vec2<f32>(q.x + 0.5 * t.x * md(floor(q.y), 2.0), q.y);
    }
    let c = floor(u / t);
    return tile_done(u - (c + vec2<f32>(0.5)) * t, t, i32(c.x), i32(c.y));
}

fn tile_done(o: vec2<f32>, t: vec2<f32>, cx: i32, cy: i32) -> vec4<f32> {
    let e = min(t.x * 0.5 - abs(o.x), t.y * 0.5 - abs(o.y));
    let w = px() / max(pf(2u), 1e-4);
    let g = pf(19u) * 0.5;
    let k = ss(g - w, g + w, e);
    let b = max(pf(20u), 1e-3);
    let up = 1.0 - ss(g, g + b, e);
    var lean = -o.y / t.y - o.x / t.x;
    lean = sign(lean) * up * step(0.002, pf(20u));
    let shade = 0.72 + (rnd(cx, cy, P.seed) - 0.5) * pf(21u) * 0.6 + lean * 0.25;
    return dyed(shade * k);
}

// Cloth. f18: how tight, f19: how uneven its threads; i.z: 0 plain, 1
// twill, 2 basket.
fn k_weave(p: vec2<i32>) -> vec4<f32> {
    let q = place(p);
    let c = floor(q);
    let f = q - c - vec2<f32>(0.5);
    var over = md(c.x + c.y, 2.0);
    if P.i.z == 1 {
        over = step(1.5, md(c.x + c.y, 4.0));
    } else if P.i.z == 2 {
        over = md(floor(c.x * 0.5) + floor(c.y * 0.5), 2.0);
    }
    let w = mix1(0.36, 0.5, pf(18u));
    let a = ss(w, w - 0.12, abs(f.y)) * (0.62 + 0.38 * cos(f.x * PI)) * cos(clamp(f.y / max(w, 0.1), -1.0, 1.0) * 1.2);
    let b = ss(w, w - 0.12, abs(f.x)) * (0.62 + 0.38 * cos(f.y * PI)) * cos(clamp(f.x / max(w, 0.1), -1.0, 1.0) * 1.2);
    let ta = 1.0 - pf(19u) * 0.5 * rnd1(i32(c.y), P.seed) - pf(19u) * 0.2 * noise3(vec3<f32>(q.x * 0.6, c.y * 3.1, 0.0), P.seed + 2u);
    let tb = 0.82 - pf(19u) * 0.5 * rnd1(i32(c.x), P.seed + 5u) - pf(19u) * 0.2 * noise3(vec3<f32>(q.y * 0.6, c.x * 3.1, 5.0), P.seed + 2u);
    var v = max(a * ta, b * tb * 0.55);
    if over < 0.5 {
        v = max(b * tb, a * ta * 0.55);
    }
    return dyed(v);
}

// A beam. f0-1 where it starts, f16-17 where it ends, f18: its width, f19:
// its glow's, f20: how it flickers, f21: how it thins to its end, f22: how
// much of it has been shot (0 to 1), f23: energy running along it; f6-8
// its core, f9-11 its glow.
fn k_beam(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let a = pf2(0u);
    let b = a + (pf2(16u) - a) * clamp(pf(22u), 0.0, 1.0);
    let axis = b - a;
    let len = max(length(axis), 1e-5);
    let n = axis / len;
    let t = clamp(dot(q - a, n) / len, 0.0, 1.0);
    let d = length(q - a - axis * t);
    let thin = 1.0 - pf(21u) * t;
    let run = 1.0 + pf(23u) * 0.6 * noise3(vec3<f32>(t * len * 14.0 - pf(15u) * 6.0, 0.0, pf(15u)), P.seed);
    let flick = 1.0 - pf(20u) * 0.45 * (0.5 + 0.5 * wander(pf(15u) * 18.0, P.seed + 3u));
    let w = max(pf(18u) * 0.5 * thin, 1e-4);
    let core = exp(-sq(d / w) * 1.3) * run;
    let g = max(pf(19u), 1e-4) * mix1(1.0, thin, 0.6);
    let glow = exp(-d / g * 1.6) * 0.65 + exp(-d / g * 0.45) * 0.12;
    // The flare where it hits.
    let hit = length(q - b);
    let spot = exp(-sq(hit / (w * 2.2 + g * 0.35))) * 0.9 * step(0.01, len);
    let tip = ss(0.0, 0.02, len);
    let c = pf3(6u) * (core * 1.3 + spot * 0.6) + pf3(9u) * (glow + spot * 0.8);
    return glowing(c * flick * tip);
}

// A gun's flash. f0-1 the muzzle, f2 its size, f3 the turn, f16: tongues
// of flame, f17: the side jets, f18: smoke's share, f19: how old the flash
// is (0 to 1), f20: how often it fires a second (0: held).
fn k_muzzle(p: vec2<i32>) -> vec4<f32> {
    var age = pf(19u);
    var shot = P.seed;
    if pf(20u) > 0.0 {
        let t = pf(15u) * pf(20u);
        age = clamp(fr(t) * 2.2, 0.0, 1.0);
        shot = P.seed + u32(i32(floor(t))) * 97u;
    }
    let v = turn(pos(p) - pf2(0u), -pf(3u)) / max(pf(2u), 1e-4);
    let power = 1.0 - ss(0.0, 1.0, age);
    let grow = 0.65 + 0.5 * age;
    let r = length(v) / grow;
    let a = angle(v);
    let n = fbm(vec3<f32>(v * 3.2 - vec2<f32>(age * 1.5, 0.0), f32(shot % 97u)), 4, shot);
    let x = v.x / grow;
    let wide = 0.1 + max(x, 0.0) * 0.34;
    let jet = exp(-sq(v.y / grow / wide)) * ss(-0.06, 0.1, x) * exp(-max(x, 0.0) * 1.55);
    let k = max(floor(pf(16u) + 0.5), 2.0);
    let tongue = pw(0.5 + 0.5 * cos(a * k + wander(a * 1.6, shot + 2u) * 1.6), 5.0) * exp(-r * 2.4);
    let side = exp(-sq((abs(a) - 1.15) * 4.5)) * exp(-r * 2.2);
    let flame = (jet * 1.5 + tongue * 0.75 + side * pf(17u)) * (0.75 + n * 1.3);
    let hot = exp(-r * r * 22.0) * 2.0;
    let body = max(flame, 0.0) * power;
    let c = pf3(6u) * hot * power + mix3(pf3(12u), pf3(9u), clamp(body, 0.0, 1.0)) * body * 1.6 + pf3(9u) * exp(-r * 1.7) * 0.25 * power;
    let l = max(c * pf(4u), vec3<f32>(0.0));
    let smoke = max(0.25 + n, 0.0) * exp(-r * 1.3) * pf(18u) * ss(0.1, 0.8, age) * (1.0 - ss(0.85, 1.0, age));
    let sa = clamp(smoke, 0.0, 0.9);
    return vec4<f32>(l + vec3<f32>(0.32) * sa, clamp(max(max(max(l.x, l.y), l.z), sa), 0.0, 1.0));
}

// Lightning's light before its glow. The picture holds its path's pieces
// (bolt.rs), eight a row: a circle round them (its middle, its radius, how
// wide the widest is), their nine places (x, y, half how wide, how bright)
// and, on every eighth row, the circle round the eight rows' pieces. f0-1:
// where the first pixel is, f2-3: how far apart pixels are, f4: how soft
// they are (frame heights); i.x: rows.
fn k_bolt(p: vec2<i32>) -> vec4<f32> {
    let q = pf2(0u) + vec2<f32>(f32(p.x), f32(p.y)) * pf2(2u);
    let soft = pf(4u) * pf(4u);
    var v = 0.0;
    let groups = (P.i.x + 7) / 8;
    for (var g = 0; g < groups; g++) {
        let all = at0(10, g * 8);
        if length(q - all.xy) < all.z + 3.1 * sqrt(all.w * all.w + soft) {
            let last = min(g * 8 + 8, P.i.x);
            for (var r = g * 8; r < last; r++) {
                let c = at0(0, r);
                if length(q - c.xy) < c.z + 3.1 * sqrt(c.w * c.w + soft) {
                    var a = at0(1, r);
                    for (var s = 2; s < 10; s++) {
                        let b = at0(s, r);
                        let d = b.xy - a.xy;
                        let t = clamp(dot(q - a.xy, d) / max(dot(d, d), 1e-12), 0.0, 1.0);
                        let e = q - a.xy - d * t;
                        let w = mix1(a.z, b.z, t);
                        let k = w * w + soft;
                        v = max(v, mix1(a.w, b.w, t) * w * exp(-dot(e, e) / k) / sqrt(k));
                        a = b;
                    }
                }
            }
        }
    }
    return vec4<f32>(v, v, v, v);
}

// Between the pixels of a picture, as smoothly as a spline goes.
fn halo1(v: vec2<f32>) -> f32 {
    let b = floor(v);
    let t = v - b;
    let u = vec2<f32>(1.0) - t;
    let m = (vec2<f32>(4.0) - t * t * 6.0 + t * t * t * 3.0) / 6.0;
    let lo = u * u * u / 6.0 + m;
    let hi = vec2<f32>(1.0) - lo;
    let p0 = b + m / lo - vec2<f32>(1.0);
    let p1 = b + t * t * t / 6.0 / hi + vec2<f32>(1.0);
    let top = bil1(p0).x * lo.x + bil1(vec2<f32>(p1.x, p0.y)).x * hi.x;
    let low = bil1(vec2<f32>(p0.x, p1.y)).x * lo.x + bil1(p1).x * hi.x;
    return top * lo.y + low * hi.y;
}

fn halo2(v: vec2<f32>) -> f32 {
    let b = floor(v);
    let t = v - b;
    let u = vec2<f32>(1.0) - t;
    let m = (vec2<f32>(4.0) - t * t * 6.0 + t * t * t * 3.0) / 6.0;
    let lo = u * u * u / 6.0 + m;
    let hi = vec2<f32>(1.0) - lo;
    let p0 = b + m / lo - vec2<f32>(1.0);
    let p1 = b + t * t * t / 6.0 / hi + vec2<f32>(1.0);
    let top = bil2(p0).x * lo.x + bil2(vec2<f32>(p1.x, p0.y)).x * hi.x;
    let low = bil2(vec2<f32>(p0.x, p1.y)).x * lo.x + bil2(p1).x * hi.x;
    return top * lo.y + low * hi.y;
}

// Lightning lit: its light and two glows, each a picture of a size of its
// own. f0: the brightness, f1-3 its core, f4-6 its glow, f8-9 where the
// first glow's first pixel is, f10 its pixels a frame height, f11 how
// bright; f12-15 the second's.
fn k_bolt_light(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let c = at0(p.x, p.y).x;
    let near = halo1((q - pf2(8u)) * pf(10u)) * pf(11u);
    let wide = halo2((q - pf2(12u)) * pf(14u)) * pf(15u);
    let l = pf3(1u) * (c * 1.25) + pf3(4u) * (near + wide + c * 0.3);
    let o = max(l * pf(0u), vec3<f32>(0.0));
    return vec4<f32>(o, clamp(max(max(o.x, o.y), o.z), 0.0, 1.0));
}

// Discharges along a picture's outlines (the second picture: its edges,
// spread). f16-17: the way they jump, f18: their width, f19: their glow,
// f20: how far they stray, f21: how fine, f22: how often a second; f6-8
// and f9-11 as a bolt's.
fn k_arc_edges(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let beat = P.time * pf(22u);
    let z = floor(beat);
    let w = vec2<f32>(fbm(vec3<f32>(q * pf(21u), z * 3.7), 4, P.seed), fbm(vec3<f32>(q * pf(21u) + vec2<f32>(4.0, 7.0), z * 3.7), 4, P.seed + 1u)) * pf(20u);
    let e = pic1(q + w, 0).x;
    let r = ridges(vec3<f32>((q + w * 0.6) * pf(21u) * 1.3 + pf2(16u) * beat * 0.1, z * 2.1), 3, P.seed + 5u);
    let thin = exp(-(1.0 - r) / max(pf(18u), 1e-3) * 0.8);
    let k = thin * ss(0.01, 0.25, e) * 1.6;
    let halo = exp(-(1.0 - r) / max(pf(19u), 1e-3) * 0.6) * ss(0.0, 0.4, e);
    let alive = 0.7 + 0.3 * wander(beat * 1.3, P.seed + 8u);
    return glowing((pf3(6u) * k * 1.6 + pf3(9u) * halo * 0.8) * alive);
}

// Rain or snow. f16: the fall's turn, f17: how fast, f18: how many, f19:
// how long the streaks, f20: how it sways; i.x: layers; f9-11 the color.
fn k_fall(p: vec2<i32>) -> vec4<f32> {
    let q = turn(pos(p), -pf(16u));
    var sum = 0.0;
    for (var l = 0; l < P.i.x; l++) {
        let f = f32(l);
        let s = max(pf(2u), 1e-3) * (2.2 - f * 0.32) * 3.0;
        let long = 1.0 + pf(19u) * 7.0;
        let t = pf(15u) * pf(17u) * (1.0 - f * 0.13);
        let sway = sin(q.y * 2.0 + pf(15u) * 0.8 + f) * pf(20u) * 0.06;
        var g = vec2<f32>((q.x + sway) / s + f * 5.1, (q.y - t) / (s * long));
        let col = floor(g.x);
        g = vec2<f32>(g.x, g.y - rnd1(i32(col), P.seed + u32(l) * 3u) * 7.0);
        let c = floor(g);
        let h = rnd2(i32(c.x), i32(c.y), P.seed + u32(l) * 47u);
        if h.x < pf(18u) {
            let o = rnd2(i32(c.x), i32(c.y), P.seed + u32(l) * 59u + 1u);
            let d = vec2<f32>((g.x - c.x - 0.25 - o.x * 0.5) * 1.0, g.y - c.y - 0.5);
            let w = 0.05 + 0.07 * h.y;
            let a = ss(w, w * 0.2, abs(d.x)) * ss(0.45, 0.1, abs(d.y));
            let b = ss(w * 1.6, w * 0.3, length(vec2<f32>(d.x, d.y * 1.0)));
            sum += mix1(b, a, ss(0.0, 0.25, pf(19u))) * (0.35 + 0.65 * h.y) / (1.0 + f * 0.35);
        }
    }
    return glowing(pf3(9u) * sum);
}
