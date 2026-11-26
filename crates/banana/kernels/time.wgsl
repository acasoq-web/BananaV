// Banana Split's effects of time: frames mixed with others, and frames
// changed as time goes.

// Three frames: f0-2 how much of each (they are made to sum to 1 when f3
// is 1).
fn k_frames(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y) * pf(0u);
    let b = at1(p.x, p.y) * pf(1u);
    let c = at2(p.x, p.y) * pf(2u);
    var k = 1.0;
    if pf(3u) > 0.5 {
        k = 1.0 / max(pf(0u) + pf(1u) + pf(2u), 1e-5);
    }
    return (a + b + c) * k;
}

// A trail (the second picture, moved) under or over the frame. f0: how
// strong, f1-3 its tint, f4: the least light that leaves a trail; i.x: 0
// the brighter, 1 added, 2 screened, 3 behind, 4 the darker.
fn k_trail(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b0 = at1(p.x, p.y);
    let k = ss(pf(4u) - 0.08, pf(4u) + 0.08, luma(b0.xyz) + step(pf(4u), 0.0));
    let b = vec4<f32>(b0.xyz * pf3(1u), b0.w) * (pf(0u) * k);
    if P.i.x == 1 {
        return vec4<f32>(a.xyz + b.xyz, a.w + b.w * (1.0 - a.w));
    }
    if P.i.x == 2 {
        let room = clamp(vec3<f32>(1.0) - a.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(a.xyz + b.xyz * room, a.w + b.w * (1.0 - a.w));
    }
    if P.i.x == 3 {
        return a + b * (1.0 - a.w);
    }
    if P.i.x == 4 {
        let c = mix4(a, vec4<f32>(b0.xyz * pf3(1u), b0.w), pf(0u));
        return vec4<f32>(min(a.xyz, c.xyz), max(a.w, b.w));
    }
    return max(a, b);
}

// A picture moved and turned (for the trails). f0-1 the middle, f2: scale,
// f3: turn, f4-5: move; f6: swirl of noise, f7: its size, f8: the light's
// pull.
fn k_drift(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p) - pf2(4u);
    var v = pf2(0u) + turn(q - pf2(0u), -pf(3u)) / max(pf(2u), 1e-3);
    if pf(6u) > 0.0 {
        let s = 1.0 / max(pf(7u), 1e-3);
        v += vec2<f32>(fbm(vec3<f32>(v * s, P.time * 0.3), 3, P.seed), fbm(vec3<f32>(v * s + vec2<f32>(5.0, 9.0), P.time * 0.3), 3, P.seed + 1u)) * pf(6u);
    }
    if pf(8u) != 0.0 {
        let l = luma(pic0(v - vec2<f32>(0.01, 0.0), 1).xyz);
        let r = luma(pic0(v + vec2<f32>(0.01, 0.0), 1).xyz);
        let u = luma(pic0(v - vec2<f32>(0.0, 0.01), 1).xyz);
        let d = luma(pic0(v + vec2<f32>(0.0, 0.01), 1).xyz);
        v += vec2<f32>(r - l, d - u) * pf(8u);
    }
    return pic0(v, 0);
}

// Red of one frame, green of another, blue of a third.
fn k_rgb_time(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let c = at2(p.x, p.y);
    return vec4<f32>(a.x, b.y, c.z, max(max(a.w, b.w), c.w));
}

// Frames across the picture: this one where the pattern is 0, the second
// at 1. f0: where this pair starts, f1: how wide each step, f2: the turn,
// f3: the steps' edges' softness; i.x: 0 a line, 1 round (f4-5 the
// middle), 2 by the light (the third picture), 3 noise.
fn k_slit(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var t = turn(q, -pf(2u)).x / max(P.shape, 1.0) + 0.5;
    if P.i.x == 1 {
        t = length(q - pf2(4u)) * 1.3;
    } else if P.i.x == 2 {
        t = clamp(luma(at2(p.x, p.y).xyz), 0.0, 1.0);
    } else if P.i.x == 3 {
        t = 0.5 + fbm(vec3<f32>(q * 2.0, 0.0), 3, P.seed) * 0.8;
    }
    let k0 = (clamp(t, 0.0, 1.0) - pf(0u)) / max(pf(1u), 1e-4);
    let w = max(pf(3u), 0.02) * 0.5;
    let k = ss(0.5 - w, 0.5 + w, k0);
    return mix4(at0(p.x, p.y), at1(p.x, p.y), k);
}

// What moves between frames (the first and the second). f0: gain, f1: the
// least change, f2-4 its color, f5: how much of the picture's color, f6:
// the still picture's share behind it.
fn k_motion(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let d = abs(a.xyz - b.xyz);
    let m = max(max(d.x, d.y), d.z);
    let k = clamp((m - pf(1u)) * pf(0u), 0.0, 1.0);
    let c = mix3(pf3(2u), straight(a), pf(5u));
    return vec4<f32>((c * k + straight(a) * pf(6u) * (1.0 - k)) * a.w, a.w);
}

// Combed lines smoothed where the frame moves (the second picture: the
// frame before). f0: how little counts as moving; i.x: which lines are
// made anew (0 odd, 1 even), i.y: 0 only where it moves, 1 everywhere.
fn k_comb(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    if (p.y & 1) == P.i.x {
        return a;
    }
    let u = at0(p.x, p.y - 1);
    let d = at0(p.x, p.y + 1);
    // Along the edge, not across it.
    let ul = at0(p.x - 1, p.y - 1);
    let dr = at0(p.x + 1, p.y + 1);
    let ur = at0(p.x + 1, p.y - 1);
    let dl = at0(p.x - 1, p.y + 1);
    let e0 = abs(luma(u.xyz) - luma(d.xyz));
    let e1 = abs(luma(ul.xyz) - luma(dr.xyz));
    let e2 = abs(luma(ur.xyz) - luma(dl.xyz));
    var m = (u + d) * 0.5;
    if e1 < e0 * 0.8 && e1 <= e2 {
        m = (ul + dr) * 0.5;
    } else if e2 < e0 * 0.8 && e2 < e1 {
        m = (ur + dl) * 0.5;
    }
    if P.i.y == 1 {
        return m;
    }
    let before = at1(p.x, p.y);
    let bu = at1(p.x, p.y - 1);
    let bd = at1(p.x, p.y + 1);
    let c = abs(luma(a.xyz) - luma(before.xyz));
    let n = max(abs(luma(u.xyz) - luma(bu.xyz)), abs(luma(d.xyz) - luma(bd.xyz)));
    return mix4(a, m, ss(pf(0u) * 0.5, pf(0u) * 1.5 + 1e-4, max(c, n)));
}

// A lamp failing. f0: how much, f1-3 how much of it each color takes, f4:
// the light now (-1 to 1), f5: only where the light is past this.
fn k_flicker(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let k = ss(pf(5u) - 0.1, pf(5u) + 0.1, luma(straight(c)) + step(pf(5u), 0.0));
    let g = vec3<f32>(1.0) + pf3(1u) * (pf(4u) * pf(0u) * k);
    return vec4<f32>(c.xyz * max(g, vec3<f32>(0.0)), c.w);
}

// The mean of a picture, drawn 1 pixel big. i.x, i.y: samples.
fn k_mean(p: vec2<i32>) -> vec4<f32> {
    var sum = vec4<f32>(0.0);
    for (var j = 0; j < P.i.y; j++) {
        for (var i = 0; i < P.i.x; i++) {
            let u = vec2<f32>((f32(i) + 0.5) / f32(P.i.x), (f32(j) + 0.5) / f32(P.i.y));
            sum += pic0_uv(u, 1);
        }
    }
    return sum * (1.0 / f32(P.i.x * P.i.y));
}

// Three means (of the frame and its neighbors) averaged: f0-2 their
// weights.
fn k_mean3(p: vec2<i32>) -> vec4<f32> {
    let k = 1.0 / max(pf(0u) + pf(1u) + pf(2u), 1e-5);
    return (at0(0, 0) * pf(0u) + at1(0, 0) * pf(1u) + at2(0, 0) * pf(2u)) * k;
}

// A frame brought to the light its neighbors have: its own mean the second
// picture, theirs the third. f0: how much; i.x 1: each color apart.
fn k_steady(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let own = at1(0, 0).xyz;
    let want = at2(0, 0).xyz;
    var g = vec3<f32>(luma(want) / max(luma(own), 1e-3));
    if P.i.x == 1 {
        g = want / max(own, vec3<f32>(1e-3));
    }
    g = clamp(g, vec3<f32>(0.4), vec3<f32>(2.5));
    return vec4<f32>(c.xyz * mix3(vec3<f32>(1.0), g, pf(0u)), c.w);
}

// Noise taken away over time: each pixel with its neighbors in time where
// they are alike. f0: how alike (sigma), f1: how much.
fn k_calm(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let c = at2(p.x, p.y);
    let s = -0.5 / max(sq(pf(0u)), 1e-6);
    let db = b.xyz - a.xyz;
    let dc = c.xyz - a.xyz;
    let wb = exp(dot(db, db) * s) * pf(1u);
    let wc = exp(dot(dc, dc) * s) * pf(1u);
    return (a + b * wb + c * wc) * (1.0 / (1.0 + wb + wc));
}

// A frame made from those before and after it, where they agree it is
// damaged: the middle of the three values. f0: how much.
fn k_repair(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let c = at2(p.x, p.y);
    let m = max(min(a, b), min(max(a, b), c));
    return mix4(a, m, pf(0u));
}
