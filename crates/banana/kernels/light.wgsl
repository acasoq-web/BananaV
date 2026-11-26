// Banana Split's light: glows, rays, flares, leaks and shadows.

// A glow (the second picture) on the first. f0: how bright, f1-3: its
// color, f4: how much of the picture stays; i.x: added or screened.
fn k_glow(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let base = vec4<f32>(a.xyz * pf(4u), a.w);
    return lit(base, at1(p.x, p.y).xyz * pf3(1u) * pf(0u), P.i.x);
}

// Two colors: f1-3 where the glow is strong, f5-7 where it fades; f8: how
// soon it fades to the second.
fn k_glow_two(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let g = at1(p.x, p.y).xyz;
    let t = ss(0.0, max(pf(8u), 1e-3), max(max(g.x, g.y), g.z));
    let base = vec4<f32>(a.xyz * pf(4u), a.w);
    return lit(base, g * mix3(pf3(5u), pf3(1u), t) * pf(0u), P.i.x);
}

// A rainbow by how strong the glow is: f5: how many times round, f6: where
// it starts, f7: how much color.
fn k_glow_rainbow(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let g = at1(p.x, p.y).xyz;
    let l = max(max(g.x, g.y), g.z);
    let h = hsv(fr(pw(l, 0.55) * pf(5u) + pf(6u)), pf(7u), 1.0);
    let base = vec4<f32>(a.xyz * pf(4u), a.w);
    return lit(base, h * l * pf3(1u) * pf(0u), P.i.x);
}

// Rings round the light: f5: how many, f6: how far they have moved, f7:
// how sharp, f8: how far their colors part.
fn k_glow_rings(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let g = at1(p.x, p.y).xyz;
    let l = pw(max(max(g.x, g.y), g.z), 0.4);
    let t = (l * pf(5u) + pf(6u)) * TAU;
    let ring = vec3<f32>(ring1(t - pf(8u)), ring1(t), ring1(t + pf(8u)));
    let base = vec4<f32>(a.xyz * pf(4u), a.w);
    return lit(base, g * ring * pf3(1u) * pf(0u), P.i.x);
}

fn ring1(t: f32) -> f32 {
    return pw(0.5 + 0.5 * cos(t), pf(7u));
}

// Darkness that spreads (the second picture: the dark parts, blurred). f0:
// how much, f1-3: what it takes away most (a color kept).
fn k_glow_dark(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let g = at1(p.x, p.y).xyz;
    let d = clamp(g * pf(0u) * (vec3<f32>(1.0) - pf3(1u) * 0.85), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(a.xyz * (vec3<f32>(1.0) - d), a.w);
}

// A glow full of smoke: f5: the smoke's size, f6: how much it shows, f7-8:
// how it drifts in a second, f9: how fast it changes; i.y: layers.
fn k_glow_smoke(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let q = pos(p) / max(pf(5u), 1e-3) + pf2(7u) * P.time / max(pf(5u), 1e-3);
    let w = vec2<f32>(noise3(vec3<f32>(q * 0.7, P.time * pf(9u)), P.seed + 3u), noise3(vec3<f32>(q * 0.7 + vec2<f32>(9.2, 3.3), P.time * pf(9u)), P.seed + 4u));
    let n = fbm(vec3<f32>(q + w * 0.7, P.time * pf(9u)), P.i.y, P.seed);
    let k = max(1.0 + n * 2.4 * pf(6u), 0.0);
    let base = vec4<f32>(a.xyz * pf(4u), a.w);
    return lit(base, at1(p.x, p.y).xyz * pf3(1u) * (pf(0u) * k), P.i.x);
}

// Light scattered in film: the glow past where the light itself is, f1-3
// its color.
fn k_halation(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let g = at1(p.x, p.y).xyz;
    let here = at2(p.x, p.y).xyz;
    let l = luma(g);
    let spill = max(l - luma(here) * pf(5u), 0.0);
    return lit(a, pf3(1u) * spill * pf(0u), P.i.x);
}

// A tube camera's glow: light round the bright parts, a dark rim past it.
// f0: light, f1-3 its color, f5: the dark rim's strength.
fn k_glow_tube(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let near = at1(p.x, p.y).xyz;
    let far = at2(p.x, p.y).xyz;
    let rim = clamp((luma(far) - luma(near) * 0.6) * pf(5u), 0.0, 0.95);
    let base = vec4<f32>(a.xyz * (1.0 - rim), a.w);
    return lit(base, near * pf3(1u) * pf(0u), P.i.x);
}

// Light that fades between two places: f0-1 where it is whole, f2-3 where
// it ends.
fn k_light_fade(p: vec2<i32>) -> vec4<f32> {
    let d = pf2(2u) - pf2(0u);
    let t = clamp(dot(pos(p) - pf2(0u), d) / max(dot(d, d), 1e-6), 0.0, 1.0);
    return at0(p.x, p.y) * (1.0 - t * t * (3.0 - 2.0 * t));
}

// Outlines as light: f0: how strong, f1-3: their color, f4: how much of
// the picture's own color, f5: the least change that counts; i.x: how many
// pixels apart.
fn k_neon(p: vec2<i32>) -> vec4<f32> {
    let g = edge_gap(pf(6u));
    let q = vec2<f32>(f32(p.x), f32(p.y));
    let l = bil0(vec2<f32>(q.x - g, q.y)).xyz;
    let r = bil0(vec2<f32>(q.x + g, q.y)).xyz;
    let u = bil0(vec2<f32>(q.x, q.y - g)).xyz;
    let d = bil0(vec2<f32>(q.x, q.y + g)).xyz;
    let e = sqrt((r - l) * (r - l) + (d - u) * (d - u));
    let k = max(max(max(e.x, e.y), e.z) - pf(5u), 0.0) * pf(0u);
    let c = at0(p.x, p.y);
    let own = c.xyz / max(max(max(c.x, c.y), c.z), 0.08);
    return vec4<f32>(mix3(pf3(1u), own, pf(4u)) * min(k, 1.5) * edge_thin(pf(6u)), c.w);
}

// A picture's bright points alone: what is brighter (f0 times) than what
// is around it (the second picture: it blurred). f1: gain.
fn k_points(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let o = max(a.xyz - at1(p.x, p.y).xyz * pf(0u), vec3<f32>(0.0)) * pf(1u);
    return vec4<f32>(o, a.w);
}

// Points of light drawn out as stars (from the bright parts, small). f0:
// the points' length in this picture's pixels, f1: the turn, f2: how they
// fade, f3: their rainbow, f4: the odd ones' length; i.x: how many lines,
// i.y: samples each way.
fn k_star(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(f32(P.w0), f32(P.h0));
    let here = vec2<f32>(f32(p.x) + 0.5, f32(p.y) + 0.5);
    var sum = vec3<f32>(0.0);
    let n = P.i.x;
    let m = P.i.y;
    for (var r = 0; r < n; r++) {
        let a = pf(1u) + f32(r) * PI / f32(n);
        var len = pf(0u);
        if (r & 1) == 1 {
            len = len * pf(4u);
        }
        let d = vec2<f32>(cos(a), sin(a)) * len;
        for (var s = 0; s < m; s++) {
            let t = (f32(s) + 0.5) / f32(m);
            let w = pw(1.0 - t, pf(2u));
            let tint = mix3(vec3<f32>(1.0), hsv(0.72 - t * 0.72, 0.9, 1.35), pf(3u));
            let c = pic0_uv((here + d * t) / size, 0).xyz + pic0_uv((here - d * t) / size, 0).xyz;
            sum += c * tint * w;
        }
    }
    // Wide bright areas add up their stars: held back, points are not.
    let l = sum * (7.5 / f32(m));
    return vec4<f32>(l / (1.0 + 0.45 * max(max(l.x, l.y), l.z)), 1.0);
}

// Shafts of light from a place (from the bright parts, small). f0-1 the
// place, f2: how long (parts of the way to it), f3: how they fade, f4:
// shimmer, f5: its fineness, f6: how fast it moves, f7-9 their color near
// the place, f10-12 far from it; i.y: samples.
fn k_rays(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(0u);
    let n = P.i.y;
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    for (var i = 0; i < n; i++) {
        let t = (f32(i) + 0.5) / f32(n);
        let w = pw(1.0 - t, pf(3u));
        sum += pic0(q - d * (t * pf(2u)), 0).xyz * w;
        total += w;
    }
    var k = 1.0;
    if pf(4u) > 0.0 {
        let a = angle(d) / TAU * pf(5u);
        let z = P.time * pf(6u);
        let m = wander(a * 3.0 + z, P.seed) * 0.6 + wander(a * 7.0 - z * 1.7, P.seed + 5u) * 0.4;
        // The seam where the angle starts over is smoothed away.
        let seam = ss(0.0, 0.06, 0.5 - abs(angle(d) / TAU));
        k = max(1.0 + m * seam * pf(4u), 0.0);
    }
    let tint = mix3(pf3(7u), pf3(10u), ss(0.0, 0.9, length(d)));
    return vec4<f32>(sum / max(total, 1e-4) * tint * k, 1.0);
}

// Where a picture's light is: its brightest parts' middle (x, y in frame
// heights), how bright they are (z). f0: how much the brightest count; i.x,
// i.y: samples across and down. Drawn 1 pixel big.
fn k_find_light(p: vec2<i32>) -> vec4<f32> {
    var sum = vec3<f32>(0.0);
    var peak = 0.0;
    for (var j = 0; j < P.i.y; j++) {
        for (var i = 0; i < P.i.x; i++) {
            let u = vec2<f32>((f32(i) + 0.5) / f32(P.i.x), (f32(j) + 0.5) / f32(P.i.y));
            let l = max(luma(pic0_uv(u, 1).xyz), 0.0);
            let w = pw(l, pf(0u));
            sum += vec3<f32>(u * w, w);
            peak = max(peak, l);
        }
    }
    let m = sum.xy / max(sum.z, 1e-6);
    return vec4<f32>((m.x - 0.5) * P.shape, m.y - 0.5, peak, 1.0);
}

fn poly(v: vec2<f32>, n: f32, a0: f32) -> f32 {
    let a = angle(v) + a0;
    let s = PI / n;
    let t = a - 2.0 * s * floor(a / (2.0 * s)) - s;
    return length(v) * cos(t) / cos(s);
}

// A lens's flare as light. f0-1: the light's place, f2-3: the frame's
// middle it mirrors through, f4: its size, f5-7: its color, f8: the core,
// f9: the star's points' strength, f10: the halo, f11: the streak, f12:
// the ghosts, f13: the ghosts' colors, f14: the turn, f15: the glow; i.x:
// points, i.y: ghosts, i.z 1: the place is the second picture's
// (`k_find_light`'s), its brightness f16 times the rest.
fn k_flare(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var at = pf2(0u);
    var power = 1.0;
    if P.i.z == 1 {
        let f = at1(0, 0);
        at = f.xy;
        power = ss(pf(16u) - 0.15, pf(16u) + 0.15, f.z);
    }
    let s = max(pf(4u), 1e-3);
    let d = (q - at) / s;
    let r = length(d);
    let a = angle(d) + pf(14u);
    var l = vec3<f32>(0.0);
    l += vec3<f32>(exp(-r * r * 260.0) * 2.4 + 0.012 / (r * r + 0.012) * 0.25) * pf(8u);
    l += vec3<f32>(exp(-r * 3.6) * 0.45) * pf(15u);
    let n = max(f32(P.i.x), 1.0);
    let star = pw(abs(cos(a * n * 0.5)), 60.0) * 0.7 + pw(abs(cos(a * n * 1.5 + 1.3)), 24.0) * 0.12;
    l += vec3<f32>(star * exp(-r * 4.2) * (1.0 - exp(-r * 30.0))) * pf(9u);
    let streak = exp(-abs(d.y) * 55.0) * exp(-abs(d.x) * 1.5) + exp(-abs(d.y) * 160.0) * exp(-abs(d.x) * 0.7) * 0.7;
    l += vec3<f32>(0.35, 0.6, 1.0) * streak * pf(11u) * 0.8;
    let hr = vec3<f32>(r - 0.415, r - 0.43, r - 0.445);
    l += exp(-hr * hr * 2600.0) * 0.4 * pf(10u) * (0.55 + 0.45 * cos(a * 2.0 + 0.6));
    l = l * pf3(5u);
    let axis = pf2(2u) - at;
    for (var i = 0; i < P.i.y; i++) {
        let h = rnd2(i, 5, P.seed);
        let t = 0.35 + 2.1 * (f32(i) + 0.5 * h.x) / max(f32(P.i.y), 1.0);
        let gs = (0.025 + 0.085 * h.y * h.y) * s;
        let g = (q - at - axis * t) / gs;
        let e = poly(g, 6.0, pf(14u) + 0.3);
        let w = px() / gs;
        let disc = (1.0 - ss(1.0 - 0.1 - w, 1.0 + w, e)) * (0.3 + 0.7 * ss(0.3, 1.0, e));
        let c = mix3(vec3<f32>(1.0), hsv(fr(h.x * 0.9 + 0.35), 0.75, 1.0), pf(13u));
        l += c * disc * (0.05 + 0.12 * h.x) * pf(12u);
    }
    return vec4<f32>(l * power, 1.0);
}

// A spotlight. f0-1 the lamp, f2-3 where it points, f4: the pool's radius,
// f5: how soft, f6: its brightness, f7: the light elsewhere, f8-10: its
// color, f11: the beam seen in the air, f12: how round the pool is (1 a
// circle), f13: dust in the beam.
fn k_spot(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let q = pos(p);
    let axis = pf2(2u) - pf2(0u);
    let len = max(length(axis), 1e-4);
    let n = axis / len;
    let v = q - pf2(2u);
    let m = vec2<f32>(dot(v, n), dot(v, vec2<f32>(-n.y, n.x)));
    let r = max(pf(4u), 1e-4);
    let d = length(vec2<f32>(m.x * pf(12u), m.y)) / r;
    let pool = 1.0 - ss(1.0 - clamp(pf(5u), 0.0, 1.0), 1.0 + pf(5u) * 0.35 + px() / r, d);
    // The cone from the lamp to the pool.
    let w = q - pf2(0u);
    let along = dot(w, n) / len;
    let across = abs(dot(w, vec2<f32>(-n.y, n.x)));
    let wide = mix1(0.012, r, clamp(along, 0.0, 1.2));
    var beam = exp(-sq(across / max(wide, 1e-4)) * 1.8) * ss(0.0, 0.04, along) * (1.0 - ss(0.75, 1.05, along)) / (0.55 + along * 1.6);
    if pf(13u) > 0.0 {
        let dust = fbm(vec3<f32>(q * 5.0 + vec2<f32>(P.time * 0.12, -P.time * 0.07), P.time * 0.2), 4, P.seed);
        beam = beam * max(1.0 + dust * 2.0 * pf(13u), 0.0);
    }
    let k = vec3<f32>(pf(7u)) + pf3(8u) * (pool * pf(6u));
    return lit(vec4<f32>(c.xyz * k, c.w), pf3(8u) * (beam * pf(11u) * pf(6u) * 0.5) * max(c.w, 0.0), 1);
}

// The picture lit from a place as if its light were its relief (the second
// picture: its light, blurred). f0-1: the lamp, f2: its height, f3: how
// deep the relief, f4: brightness, f5: the light elsewhere, f6: the shine,
// f7: how tight it is, f8-10: the color, f11: how far the lamp reaches.
fn k_relight(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let q = pos(p);
    let l = luma(at1(p.x - 1, p.y).xyz);
    let r = luma(at1(p.x + 1, p.y).xyz);
    let u = luma(at1(p.x, p.y - 1).xyz);
    let d = luma(at1(p.x, p.y + 1).xyz);
    let k = pf(3u) * 0.5 * f32(P.h) * 0.06;
    let n = normalize(vec3<f32>((l - r) * k, (u - d) * k, 1.0));
    let v = vec3<f32>(pf2(0u) - q, pf(2u));
    let far = length(v);
    let dir = v / max(far, 1e-4);
    let diff = max(dot(n, dir), 0.0);
    let h = normalize(dir + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pw(max(dot(n, h), 0.0), pf(7u)) * pf(6u);
    let fall = 1.0 / (1.0 + sq(far / max(pf(11u), 1e-3)));
    let light = vec3<f32>(pf(5u)) + pf3(8u) * (diff * pf(4u) * fall);
    return vec4<f32>(c.xyz * light + pf3(8u) * (spec * fall * pf(4u) * c.w), c.w);
}

// A shadow under the picture (its alpha, blurred: the second). f0-1: its
// move, f2: how dark, f3-5: its color; i.x 1: alone; f6-7 its scale, 1
// the picture's.
fn k_shadow(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let a = clamp(pic1(pos(p) - pf2(0u), 0).w * pf(2u), 0.0, 1.0);
    let s = vec4<f32>(pf3(3u) * a, a);
    if P.i.x == 1 {
        return s;
    }
    return c + s * (1.0 - clamp(c.w, 0.0, 1.0));
}

// Light leaking in as light. f0: its size, f1: the turn, f2: how far it
// has moved, f3: the edge it comes from (0 anywhere, 1 the sides), f4-6,
// f7-9, f10-12: its colors, f13: how ragged; i.y: 0 clouds, 1 bands, 2 a
// burn's hole.
fn k_leak(p: vec2<i32>) -> vec4<f32> {
    let q = turn(pos(p), -pf(1u));
    let s = 1.0 / max(pf(0u), 1e-3);
    let t = pf(2u);
    let warp = vec2<f32>(noise3(vec3<f32>(q * s * 0.8, t * 0.6), P.seed), noise3(vec3<f32>(q * s * 0.8 + vec2<f32>(7.1, 2.9), t * 0.6), P.seed + 1u)) * pf(13u);
    let u = q + warp * 0.35 / s;
    var k = 0.0;
    var h = 0.0;
    if P.i.y == 1 {
        let b = u.x * s * 0.8 + wander(t * 0.7, P.seed + 2u) * 0.8;
        k = pw(0.5 + 0.5 * cos(b * TAU * 0.5 + t), 3.0);
        h = 0.5 + 0.5 * noise3(vec3<f32>(u.x * s, 0.0, t * 0.5), P.seed + 8u);
    } else if P.i.y == 2 {
        let n = fbm(vec3<f32>(u * s, t * 0.5), 5, P.seed);
        let open = length(q - vec2<f32>(wander(t * 0.4, P.seed + 3u), wander(t * 0.4 + 4.0, P.seed + 4u)) * 0.35) * s * 0.55;
        k = ss(1.0, 0.0, open - n * 0.9 + 0.1);
        h = ss(0.0, 1.0, k);
    } else {
        let n = fbm(vec3<f32>(u * s * 0.55, t * 0.45), 3, P.seed);
        let m = fbm(vec3<f32>(u * s * 0.4 + vec2<f32>(13.0, 5.0), t * 0.3), 2, P.seed + 6u);
        k = ss(-0.3, 0.55, n + m * 0.5);
        h = 0.5 + m * 1.4;
    }
    let side = clamp(abs(q.x) / max(P.shape * 0.5, 0.5), 0.0, 1.2);
    k = k * mix1(1.0, ss(0.15, 1.0, side), pf(3u));
    let c = mix3(mix3(pf3(4u), pf3(7u), ss(0.0, 0.55, h)), pf3(10u), ss(0.5, 1.0, h));
    return vec4<f32>(c * k, 1.0);
}

// Discs of light out of focus, as light. f0: their size, f1: how many (0
// to 1), f2: how far they have drifted, f3: the way they drift, f4: their
// blades, f5: their turn, f6-8 and f9-11: their colors, f12: how bright
// their rims, f13: how much they twinkle, f14: how soft; i.y: layers.
fn k_bokeh(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var sum = vec3<f32>(0.0);
    let go = vec2<f32>(cos(pf(3u)), sin(pf(3u)));
    for (var l = 0; l < P.i.y; l++) {
        let f = f32(l);
        let s = pf(0u) * (1.0 + 0.55 * f) * 2.4;
        let g = q / max(s, 1e-3) + go * (pf(2u) / (1.0 + 0.6 * f)) + vec2<f32>(f * 5.3, f * 2.1);
        let b = floor(g);
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let cx = i32(b.x) + i;
                let cy = i32(b.y) + j;
                let id = rnd(cx, cy, P.seed + u32(l) * 37u);
                if id < pf(1u) {
                    let o = rnd2(cx, cy, P.seed + u32(l) * 91u + 1u);
                    let h = rnd2(cx, cy, P.seed + u32(l) * 53u + 2u);
                    let c = vec2<f32>(f32(cx), f32(cy)) + vec2<f32>(0.2) + o * 0.6;
                    let r = (0.2 + 0.26 * h.x);
                    let v = (g - c) / r;
                    var e = length(v);
                    if pf(4u) > 2.5 {
                        e = poly(v, pf(4u), pf(5u));
                    }
                    let w = max(pf(14u) + 0.12 * f, px() / (s * r));
                    let disc = (1.0 - ss(1.0 - w, 1.0 + w * 0.3, e)) * mix1(1.0, 0.35 + 0.65 * sq(min(e, 1.0)), pf(12u));
                    let tw = 1.0 - pf(13u) * (0.5 + 0.5 * sin(P.time * (1.5 + 3.0 * h.y) + id * 40.0));
                    sum += mix3(pf3(6u), pf3(9u), h.y) * disc * tw * (0.35 + 0.65 * h.x) / (1.0 + 0.4 * f);
                }
            }
        }
    }
    return vec4<f32>(sum, 1.0);
}

// Flashes of cameras, as light. f0: their size, f1: how many a second in
// each place, f2: how long each lasts (seconds), f3-5: their color, f6:
// how tall the crowd (0 to 1 of the frame from the bottom), f7: their
// star's strength.
fn k_flashes(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let s = max(pf(0u), 1e-3) * 1.6;
    let g = q / s;
    let b = floor(g);
    var sum = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let cx = i32(b.x) + i;
            let cy = i32(b.y) + j;
            let h = rnd2(cx, cy, P.seed);
            let t = P.time * pf(1u) + h.x * 19.0;
            let n = floor(t);
            let o = rnd2(cx * 7 + i32(n), cy * 13 - i32(n), P.seed + 5u);
            let c = (vec2<f32>(f32(cx), f32(cy)) + vec2<f32>(0.2) + o * 0.6) * s;
            let on = step(0.5 - pf(6u), c.y) * step(o.x * 0.5 + h.y * 0.5, 0.62);
            let age = (t - n) / max(pf(1u), 1e-3);
            let k = exp(-age / max(pf(2u), 1e-3) * 3.0) * on;
            let d = (q - c) / max(pf(0u), 1e-3);
            let r = length(d);
            let star = exp(-abs(d.x) * 26.0) * exp(-abs(d.y) * 3.0) + exp(-abs(d.y) * 26.0) * exp(-abs(d.x) * 3.0);
            sum += k * (exp(-r * 6.0) * 1.8 + exp(-r * r * 45.0) * 1.5 + star * 0.8 * pf(7u));
        }
    }
    return vec4<f32>(pf3(3u) * sum, 1.0);
}

// A light drawn round a matte: the background's light (second picture,
// blurred) on the first's edges.  f0: how strong, f1-3 its color, f4: how
// far in.
fn k_rim(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let in_by = clamp(a.w - at1(p.x, p.y).w, 0.0, 1.0);
    return vec4<f32>(a.xyz + pf3(1u) * (in_by * pf(0u) * a.w), a.w);
}
