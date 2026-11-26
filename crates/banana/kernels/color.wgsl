// Banana Split's color effects: each pixel where it is, its own color
// (straight) changed and premultiplied again.

// f0-11: rows of red, green, blue and a constant for each channel; f12:
// contrast, f13: saturation, f14: gamma's power, f15: gain.
fn k_matrix(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    var o = vec3<f32>(
        dot(s, pf3(0u)) + pf(3u),
        dot(s, pf3(4u)) + pf(7u),
        dot(s, pf3(8u)) + pf(11u),
    ) * pf(15u);
    o = contrast3(o, pf(12u));
    o = saturate3(o, pf(13u));
    if pf(14u) != 1.0 {
        o = pw3(o, pf(14u));
    }
    return back(o, c.w);
}

// f0-2: powers, f3-5: gains, f6-8: lifts.
fn k_gamma(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let g = s * pf3(3u) + pf3(6u) * (vec3<f32>(1.0) - s);
    let o = vec3<f32>(pw(g.x, pf(0u)), pw(g.y, pf(1u)), pw(g.z, pf(2u)));
    return back(o, c.w);
}

// f0: the hue's turn, f1: saturation, f2: vibrance, f3: brightness, f4:
// offset, f5-7: a color it all takes, f8: how much of it.
fn k_hue(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    var o = hue_turn(straight(c), pf(0u));
    let hi = max(max(o.x, o.y), o.z);
    let lo = min(min(o.x, o.y), o.z);
    let dull = 1.0 - clamp(hi - lo, 0.0, 1.0);
    o = saturate3(o, max(pf(1u) * (1.0 + pf(2u) * dull), 0.0));
    o = mix3(o, pf3(5u) * luma(o), pf(8u));
    return back(o * pf(3u) + vec3<f32>(pf(4u)), c.w);
}

// f0-2: how much of each channel makes the gray, f3: brightness, f4:
// contrast, f5-7: the gray's tint, f8: how much.
fn k_mono(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let g = contrast3(vec3<f32>(dot(straight(c), pf3(0u)) * pf(3u)), pf(4u));
    return back(mix3(g, g * pf3(5u), pf(8u)), c.w);
}

// f0-2: how much of each channel turns over; i.x 1: only the light turns
// over, 2: only the colors.
fn k_negative(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let l = luma(s);
    if P.i.x == 1 {
        return back(s + vec3<f32>(1.0 - 2.0 * l), c.w);
    }
    if P.i.x == 2 {
        return back(vec3<f32>(2.0 * l) - s, c.w);
    }
    return back(mix3(s, vec3<f32>(1.0) - s, 1.0) * pf3(0u) + s * (vec3<f32>(1.0) - pf3(0u)), c.w);
}

// The light as a ramp of colors. i.x: how many (2 to 5); f0-14: the
// colors, f15: how soft between them (0 flat bands, 1 a smooth ramp), f16:
// the power of the light, f17: how much of the picture's own color stays,
// f18: how much of the picture's own light stays.
fn k_tones(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let n = P.i.x;
    let l = pw(clamp(luma(s), 0.0, 1.0), pf(16u));
    let t = l * f32(n - 1);
    var o = pf3(0u);
    let soft = clamp(pf(15u), 0.001, 1.0) * 0.5;
    for (var i = 1; i < n; i++) {
        let k = ss(0.5 - soft, 0.5 + soft, t - f32(i - 1));
        o = mix3(o, pf3(u32(i) * 3u), k);
    }
    let light = luma(s) / max(luma(o), 0.02);
    o = mix3(o, o * min(light, 4.0), pf(18u));
    return back(mix3(o, s, pf(17u)), c.w);
}

// Only the parts brighter than f0 (f1: how softly), their saturation f2
// and brightness f3; i.x 0 on black, 1 on nothing.
fn k_only_bright(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let k = ss(pf(0u) - pf(1u), pf(0u) + pf(1u) + 1e-4, luma(s));
    let o = saturate3(s, pf(2u)) * pf(3u);
    if P.i.x == 1 {
        return back(o, c.w * k);
    }
    return back(o * k, c.w);
}

// Light held under f0 and over f1, colors within f2 of gray, easing off
// over f3 before each.
fn k_limit(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let l = luma(s);
    let knee = max(pf(3u), 1e-4);
    var y = l;
    let over = y - (pf(0u) - knee);
    if over > 0.0 {
        y = pf(0u) - knee + knee * (1.0 - exp(-over / knee));
    }
    let under = pf(1u) + knee - y;
    if under > 0.0 {
        y = pf(1u) + knee - knee * (1.0 - exp(-under / knee));
    }
    let d = s - vec3<f32>(l);
    let far = max(max(abs(d.x), abs(d.y)), abs(d.z));
    var k = 1.0;
    if far > pf(2u) - knee && far > 1e-6 {
        k = (pf(2u) - knee + knee * (1.0 - exp(-(far - pf(2u) + knee) / knee))) / far;
    }
    return back(vec3<f32>(y) + d * max(k, 0.0), c.w);
}

// Stripes where the light is past f0 or under f1, as wide as f2, moving
// with the time (f3 stripes a second); i.x 1: the colors too far from
// gray (f4).
fn k_zebra(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let q = pos(p);
    let stripe = fr((q.x + q.y) / max(pf(2u), 1e-4) - P.time * pf(3u));
    let w = min(fwd(stripe) / max(px() / max(pf(2u), 1e-4), 1e-4), 1.0);
    let l = luma(s);
    let d = s - vec3<f32>(l);
    let far = max(max(abs(d.x), abs(d.y)), abs(d.z));
    var o = s;
    if l > pf(0u) {
        o = mix3(s, vec3<f32>(1.0, 0.15, 0.1), w);
    } else if l < pf(1u) {
        o = mix3(s, vec3<f32>(0.1, 0.35, 1.0), w);
    } else if P.i.x == 1 && far > pf(4u) {
        o = mix3(s, vec3<f32>(0.1, 1.0, 0.3), w);
    }
    return back(o, c.w);
}

// A stripe's half, its edge in stripes.
fn fwd(t: f32) -> f32 {
    return max(min(t, 0.5 - t), 0.0);
}

// Black or white (i.x 1: each channel) on either side of f0, f1 wide; f2:
// 1 the sides swapped, f3: how much of the picture's color stays.
fn k_threshold(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let w = max(pf(1u), 0.002);
    let l = luma(s);
    var k = vec3<f32>(ss(pf(0u) - w, pf(0u) + w, l));
    if P.i.x == 1 {
        k = vec3<f32>(ss(pf(0u) - w, pf(0u) + w, s.x), ss(pf(0u) - w, pf(0u) + w, s.y), ss(pf(0u) - w, pf(0u) + w, s.z));
    }
    k = mix3(k, vec3<f32>(1.0) - k, pf(2u));
    return back(mix3(k, k * s, pf(3u)), c.w);
}

// The dark parts' tint (f0-2) and the light parts' (f3-5), each as much as
// f6 and f7; f8: where one gives way to the other, f9: saturation.
fn k_split_tint(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = saturate3(straight(c), pf(9u));
    let l = luma(s);
    let t = ss(pf(8u) - 0.35, pf(8u) + 0.35, l);
    let dark = mix3(s, s * 0.45 + pf3(0u) * (0.18 + l * 0.75), pf(6u));
    let light = mix3(s, s * 0.45 + pf3(3u) * l * 0.62, pf(7u));
    return back(mix3(dark, light, t), c.w);
}

// f0: how many steps, f1: how soft between them; i.x 1: the light alone
// steps.
fn k_posterize(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let n = max(pf(0u) - 1.0, 1.0);
    if P.i.x == 1 {
        let l = luma(s);
        let o = posterize1(l, n, pf(1u));
        return back(s * (o / max(l, 1e-3)), c.w);
    }
    return back(vec3<f32>(posterize1(s.x, n, pf(1u)), posterize1(s.y, n, pf(1u)), posterize1(s.z, n, pf(1u))), c.w);
}

fn posterize1(v: f32, n: f32, soft: f32) -> f32 {
    let t = v * n;
    let b = floor(t);
    let w = clamp(soft, 0.01, 1.0) * 0.5;
    return (b + ss(0.5 - w, 0.5 + w, t - b)) / n;
}

// The light folded over past f0, f1 times; f2: how much.
fn k_solarize(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let o = vec3<f32>(solar1(s.x), solar1(s.y), solar1(s.z));
    return back(mix3(s, o, pf(2u)), c.w);
}

fn solar1(v: f32) -> f32 {
    let t = max(pf(0u), 0.02);
    let k = v / t * pf(1u);
    let w = k - 2.0 * floor(k * 0.5);
    return min(1.0 - abs(w - 1.0), 1.0);
}

// Silver left in the film: f0 how much, f1 the color left, f2 contrast.
fn k_bleach(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let l = luma(s);
    let g = vec3<f32>(l);
    let low = 2.0 * s * g;
    let high = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - s) * (vec3<f32>(1.0) - g);
    let o = mix3(low, high, ss(0.35, 0.65, l));
    let m = saturate3(mix3(s, o, pf(0u)), mix1(1.0, pf(1u), pf(0u)));
    return back(contrast3(m, pf(2u)), c.w);
}

// The light as a map of colors. i.x: 0 heat, 1 rainbow, 2 night vision, 3
// ice, 4 sunset; f0, f1: the light the map runs over, f2: how many times,
// f3: how much.
fn k_false_color(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let t0 = (luma(s) - pf(0u)) / max(pf(1u) - pf(0u), 1e-3) * pf(2u);
    var t = clamp(t0, 0.0, 1.0);
    if pf(2u) > 1.0 {
        t = 1.0 - abs(md(t0, 2.0) - 1.0);
    }
    var o = vec3<f32>(0.0);
    if P.i.x == 0 {
        o = vec3<f32>(ss(0.15, 0.55, t), ss(0.45, 0.85, t), ss(0.0, 0.25, t) - ss(0.3, 0.6, t) + ss(0.85, 1.0, t));
    } else if P.i.x == 1 {
        o = hsv(0.75 - 0.75 * t, 0.95, 0.25 + 0.75 * ss(0.0, 0.2, t));
    } else if P.i.x == 2 {
        o = vec3<f32>(0.12, 1.0, 0.25) * pw(t, 0.8) + vec3<f32>(0.6, 0.25, 0.5) * ss(0.75, 1.0, t);
    } else if P.i.x == 3 {
        o = mix3(mix3(vec3<f32>(0.01, 0.03, 0.12), vec3<f32>(0.1, 0.45, 0.85), ss(0.0, 0.6, t)), vec3<f32>(0.95, 1.0, 1.0), ss(0.5, 1.0, t));
    } else {
        o = mix3(mix3(vec3<f32>(0.12, 0.02, 0.25), vec3<f32>(0.9, 0.25, 0.3), ss(0.0, 0.55, t)), vec3<f32>(1.0, 0.88, 0.45), ss(0.45, 1.0, t));
    }
    return back(mix3(s, o, pf(3u)), c.w);
}

// A print's answer to light: f0 contrast, f1 how far the highlights roll
// off, f2 how far the blacks lift, f3 saturation, f4-6 the shadows' tint,
// f7-9 the highlights', f10 how much.
fn k_print(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    var o = contrast3(s, pf(0u));
    let roll = pf(1u);
    let top = max(o - vec3<f32>(1.0 - roll), vec3<f32>(0.0));
    o = o - top + top / (vec3<f32>(1.0) + top / max(roll, 1e-3) * 0.9);
    o = mix3(o, vec3<f32>(1.0), 0.0) * (1.0 - pf(2u)) + vec3<f32>(pf(2u));
    o = saturate3(o, pf(3u));
    let l = clamp(luma(o), 0.0, 1.0);
    o += (pf3(4u) - vec3<f32>(0.5)) * (1.0 - l) * (1.0 - l) * 0.5;
    o += (pf3(7u) - vec3<f32>(0.5)) * l * l * 0.5;
    return back(mix3(s, o, pf(10u)), c.w);
}

// Mist over a picture between two places: f0-1 where it starts, f2-3 where
// it is whole, f4-6 its color, f7 how much; f8: 0 evenly, 1 thickening as
// real mist does; f9: patches of noise in it, f10: their size, f11: how
// fast they drift.
fn k_mist(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let q = pos(p);
    let d = pf2(2u) - pf2(0u);
    var t = clamp(dot(q - pf2(0u), d) / max(dot(d, d), 1e-6), 0.0, 1.0);
    if pf(9u) > 0.0 {
        let n = fbm(vec3<f32>(q / max(pf(10u), 1e-3) + vec2<f32>(P.time * pf(11u), 0.0), P.time * 0.15), 4, P.seed);
        t = clamp(t * (1.0 + n * pf(9u) * 1.6), 0.0, 1.0);
    }
    let even = t;
    let thick = 1.0 - exp(-3.0 * t * t);
    let k = mix1(even, thick, pf(8u)) * pf(7u);
    return vec4<f32>(c.xyz + (pf3(4u) * c.w - c.xyz) * k, c.w);
}

// Dark and light shades of a picture's own colors: f0-2 the color the
// colors lean to in the dark, f3-5 in the light, f6 how much, f7 a glow of
// its highlights (the second picture, blurred) added.
fn k_fuse(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let s = straight(c);
    let l = clamp(luma(s), 0.0, 1.0);
    let h = to_hsv(max(s, vec3<f32>(0.0)));
    let lean = mix3(pf3(0u), pf3(3u), ss(0.15, 0.85, l));
    let base = mix3(vec3<f32>(l), s, 0.55 + 0.45 * h.y);
    let o = base * mix3(vec3<f32>(1.0), lean * 1.8, 0.75);
    let m = mix3(s, o * (l / max(luma(o), 0.02)), pf(6u));
    return lit(back(m, c.w), at1(p.x, p.y).xyz * pf(7u), 1);
}
