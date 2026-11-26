// Banana Split's blurs, defocuses and sharpening.

// Along a line. f0-1: the whole way in pixels, centered but for f2 (-0.5
// to 0.5 of it); i.x: the edges, i.y: how many samples.
fn k_smear(p: vec2<i32>) -> vec4<f32> {
    let n = max(P.i.y, 1);
    let d = pf2(0u);
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < n; i++) {
        let t = (f32(i) + 0.5) / f32(n) - 0.5 + pf(2u);
        let v = vec2<f32>(f32(p.x), f32(p.y)) + d * t;
        sum += pic0_uv((v + vec2<f32>(0.5)) / vec2<f32>(f32(P.w0), f32(P.h0)), P.i.x);
    }
    return sum * (1.0 / f32(n));
}

// Around a place: f0-1 the place, f2 the zoom's way (its scale's log), f3
// the turn's, all of it centered but for f4; i.x: the edges, i.y: how many
// samples; f5: from this far out on (frame heights, 0 everywhere), f6:
// growing to whole over this.
fn k_spin(p: vec2<i32>) -> vec4<f32> {
    let n = max(P.i.y, 1);
    let q = pos(p);
    let c = pf2(0u);
    let d = q - c;
    var k = 1.0;
    if pf(6u) > 0.0 {
        k = ss(pf(5u), pf(5u) + pf(6u), length(d));
    }
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < n; i++) {
        let t = ((f32(i) + 0.5) / f32(n) - 0.5 + pf(4u)) * k;
        sum += pic0(c + turn(d, pf(3u) * t) * exp(pf(2u) * t), P.i.x);
    }
    return sum * (1.0 / f32(n));
}

// How far out an iris of `n` blades reaches at angle `a` (1 at its
// corners; 0 blades: round), `round` of the way to a circle.
fn iris(a: f32, n: f32, round: f32) -> f32 {
    if n < 2.5 {
        return 1.0;
    }
    let s = PI / n;
    let t = a - 2.0 * s * floor(a / (2.0 * s)) - s;
    return mix1(cos(s) / cos(t), 1.0, round);
}

// How much of the defocus a place gets: all of it, or as far from a band
// in focus. f8-9: the band's middle, f10: its turn, f11: its half width,
// f12: how far from it the defocus is whole; i.y: 0 everywhere, 1 a band,
// 2 a circle, 3 one way from a line.
fn focus(q: vec2<f32>) -> f32 {
    if P.i.y == 0 {
        return 1.0;
    }
    let v = turn(q - pf2(8u), -pf(10u));
    var d = abs(v.y);
    if P.i.y == 2 {
        d = length(v);
    } else if P.i.y == 3 {
        d = v.y;
    }
    return ss(pf(11u), pf(11u) + max(pf(12u), 1e-3), d);
}

// A lens out of focus. f0: the radius in this picture's pixels, f1-2: its
// stretch across and down, f3: the iris's turn, f4: its blades, f5: their
// roundness, f6: how much brighter the bright parts weigh, f7: from this
// light on; f13: the colors' spread; i.x: the edges, i.z: how many
// samples.
fn k_lens(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let here = focus(q);
    let r = pf(0u) * here;
    let mid = vec2<f32>(f32(p.x) + 0.5, f32(p.y) + 0.5);
    let size = vec2<f32>(f32(P.w0), f32(P.h0));
    if r < 0.35 {
        return at0(p.x, p.y);
    }
    let n = P.i.z;
    var sum = vec4<f32>(0.0);
    var total = vec3<f32>(0.0);
    var alpha = 0.0;
    var weight = 0.0;
    for (var i = 0; i < n; i++) {
        // Sunflower seeds: an even disc.
        let t = (f32(i) + 0.5) / f32(n);
        let a = f32(i) * 2.39996323;
        let l = sqrt(t) * r;
        let o = turn(vec2<f32>(cos(a), sin(a)) * l * iris(a, pf(4u), pf(5u)) * pf2(1u), pf(3u));
        if pf(13u) == 0.0 {
            let c = pic0_uv((mid + o) / size, P.i.x);
            let w = 1.0 + pf(6u) * ss(pf(7u) - 0.15, pf(7u) + 0.15, luma(c.xyz));
            sum += c * w;
            weight += w;
        } else {
            let cr = pic0_uv((mid + o * (1.0 + pf(13u))) / size, P.i.x);
            let cg = pic0_uv((mid + o) / size, P.i.x);
            let cb = pic0_uv((mid + o * (1.0 - pf(13u))) / size, P.i.x);
            let wr = 1.0 + pf(6u) * ss(pf(7u) - 0.15, pf(7u) + 0.15, luma(cr.xyz));
            let wg = 1.0 + pf(6u) * ss(pf(7u) - 0.15, pf(7u) + 0.15, luma(cg.xyz));
            let wb = 1.0 + pf(6u) * ss(pf(7u) - 0.15, pf(7u) + 0.15, luma(cb.xyz));
            sum += vec4<f32>(cr.x * wr, cg.y * wg, cb.z * wb, 0.0);
            total += vec3<f32>(wr, wg, wb);
            alpha += (cr.w + cg.w + cb.w) / 3.0;
        }
    }
    if pf(13u) == 0.0 {
        return sum * (1.0 / weight);
    }
    return vec4<f32>(sum.xyz / total, alpha / f32(n));
}

// The sharp picture (the first) where a lens leaves it in focus, the
// defocused one (the second, whole size) elsewhere: as `k_lens`'s numbers;
// f0: the radius in whole pixels.
fn k_lens_join(p: vec2<i32>) -> vec4<f32> {
    let k = ss(0.6, 2.2, focus(pos(p)) * pf(0u));
    return mix4(at0(p.x, p.y), at1(p.x, p.y), k);
}

// Blurred but for edges: the pixels within i.x of them, i.y pixels apart,
// each weighed by how near its color is. f0: how near, f1: how far each
// reaches (sigma, in steps).
fn k_surface(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let n = P.i.x;
    let gap = P.i.y;
    let kr = -0.5 / max(sq(pf(0u)), 1e-6);
    let ks = -0.5 / max(sq(pf(1u)), 1e-6);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var j = -n; j <= n; j++) {
        for (var i = -n; i <= n; i++) {
            let s = at0(p.x + i * gap, p.y + j * gap);
            let d = s.xyz - c.xyz;
            let w = exp(dot(d, d) * kr + f32(i * i + j * j) * ks);
            sum += s * w;
            total += w;
        }
    }
    return sum * (1.0 / total);
}

// The picture (first) against a blurred one (second): f0 how much the
// difference grows, f1 the difference left alone, f2 how far a halo may
// go; i.x 1: the mid tones most (clarity).
fn k_sharpen(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let s = straight(a);
    var d = luma(a.xyz) - luma(b.xyz);
    let over = max(abs(d) - pf(1u), 0.0);
    d = sign(d) * over;
    var k = pf(0u);
    if P.i.x == 1 {
        let l = clamp(luma(s), 0.0, 1.0);
        k = k * (1.0 - sq(2.0 * l - 1.0));
    }
    let add = clamp(d * k, -pf(2u), pf(2u));
    return vec4<f32>(max(a.xyz + vec3<f32>(add), vec3<f32>(0.0)), a.w);
}

// The picture's light (first) with a blurred one's colors (second). f0:
// how much color, f1: how much light.
fn k_chroma_join(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let l = luma(a.xyz);
    let d = b.xyz - vec3<f32>(luma(b.xyz));
    return vec4<f32>(vec3<f32>(l * pf(1u)) + d * pf(0u), a.w);
}

// Red from the first picture, green from the second, blue from the third.
fn k_channel_join(p: vec2<i32>) -> vec4<f32> {
    let r = at0(p.x, p.y);
    let g = at1(p.x, p.y);
    let b = at2(p.x, p.y);
    return vec4<f32>(r.x, g.y, b.z, max(max(r.w, g.w), b.w));
}

fn lo4(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return min(a, b);
}

fn hi4(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return max(a, b);
}

fn mid3(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>) -> vec4<f32> {
    return hi4(lo4(a, b), lo4(hi4(a, b), c));
}

// The middle of the nine pixels i.x apart (the exact median, by Paeth's
// exchanges).
fn k_median(p: vec2<i32>) -> vec4<f32> {
    let g = P.i.x;
    let v0 = at0(p.x - g, p.y - g);
    let v1 = at0(p.x, p.y - g);
    let v2 = at0(p.x + g, p.y - g);
    let v3 = at0(p.x - g, p.y);
    let v4 = at0(p.x, p.y);
    let v5 = at0(p.x + g, p.y);
    let v6 = at0(p.x - g, p.y + g);
    let v7 = at0(p.x, p.y + g);
    let v8 = at0(p.x + g, p.y + g);
    // Each row sorted.
    let a0 = lo4(lo4(v0, v1), v2);
    let a2 = hi4(hi4(v0, v1), v2);
    let a1 = mid3(v0, v1, v2);
    let b0 = lo4(lo4(v3, v4), v5);
    let b2 = hi4(hi4(v3, v4), v5);
    let b1 = mid3(v3, v4, v5);
    let c0 = lo4(lo4(v6, v7), v8);
    let c2 = hi4(hi4(v6, v7), v8);
    let c1 = mid3(v6, v7, v8);
    // The largest of the smallest, the middle of the middles, the smallest
    // of the largest: their middle.
    return mid3(hi4(hi4(a0, b0), c0), mid3(a1, b1, c1), lo4(lo4(a2, b2), c2));
}

// Banding smoothed: where the pixels around (f0 pixels away, turned at
// random) are within f1 of this one, their mean; then noise of f2.
fn k_deband(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let f = full_pixel(p);
    let h = rnd2(f.x, f.y, P.seed);
    let a = h.x * TAU;
    let r = (0.35 + 0.65 * h.y) * pf(0u);
    let o = vec2<f32>(cos(a), sin(a)) * r;
    let q = vec2<f32>(f32(p.x), f32(p.y));
    let m = (bil0(q + o) + bil0(q - o) + bil0(q + vec2<f32>(-o.y, o.x)) + bil0(q - vec2<f32>(-o.y, o.x))) * 0.25;
    let d = abs(m.xyz - c.xyz);
    let k = 1.0 - ss(pf(1u) * 0.5, pf(1u), max(max(d.x, d.y), d.z));
    let grain = (rnd(f.x, f.y, P.seed + 77u) - 0.5) * pf(2u) * c.w;
    return vec4<f32>(mix3(c.xyz, m.xyz, k) + vec3<f32>(grain), c.w);
}

// The nine pixels i.x apart, weighed by f0-8, plus f9; f10: how much.
fn k_convolve(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let g = P.i.x;
    var sum = vec3<f32>(0.0);
    for (var j = 0; j < 3; j++) {
        for (var i = 0; i < 3; i++) {
            sum += at0(p.x + (i - 1) * g, p.y + (j - 1) * g).xyz * pf(u32(j * 3 + i));
        }
    }
    return vec4<f32>(mix3(c.xyz, sum + vec3<f32>(pf(9u) * c.w), pf(10u)), c.w);
}

// Skin smoothed: the smooth picture (second) where a color is like f0-2,
// within f3, f4 of the way, f5 of its own texture kept, f6 of glow (the
// third, blurred wide) on it.
fn k_skin(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    let g = at2(p.x, p.y);
    let s = straight(a);
    let t = to_hsv(max(s, vec3<f32>(0.0)));
    let want = to_hsv(pf3(0u));
    var dh = abs(t.x - want.x);
    dh = min(dh, 1.0 - dh);
    let d = sqrt(sq(dh * 5.0) + sq((t.y - want.y) * 0.7));
    let like = 1.0 - ss(pf(3u) * 0.5, pf(3u), d);
    let k = like * pf(4u) * ss(0.05, 0.2, t.z);
    var o = mix3(a.xyz, b.xyz, k);
    o += (a.xyz - b.xyz) * (k * pf(5u));
    let light = g.xyz * (pf(6u) * like);
    o = o + light * clamp(vec3<f32>(a.w) - o, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(o, a.w);
}

// How strong the edges are here (0 to 1), from the first picture's light.
fn edge_here(p: vec2<i32>, gap: f32) -> f32 {
    let q = vec2<f32>(f32(p.x), f32(p.y));
    let l = luma(bil0(vec2<f32>(q.x - gap, q.y)).xyz);
    let r = luma(bil0(vec2<f32>(q.x + gap, q.y)).xyz);
    let u = luma(bil0(vec2<f32>(q.x, q.y - gap)).xyz);
    let d = luma(bil0(vec2<f32>(q.x, q.y + gap)).xyz);
    return length(vec2<f32>(r - l, d - u));
}

// The blurred picture (second) where the first's edges (third, spread)
// are: f0 how strong they count; i.x 1: everywhere but there.
fn k_edge_join(p: vec2<i32>) -> vec4<f32> {
    var k = clamp(at2(p.x, p.y).x * pf(0u), 0.0, 1.0);
    if P.i.x == 1 {
        k = 1.0 - k;
    }
    return mix4(at0(p.x, p.y), at1(p.x, p.y), k);
}

// The edges' strength as light. f0: times; f1: how many pixels apart.
fn k_edges(p: vec2<i32>) -> vec4<f32> {
    let e = edge_here(p, edge_gap(pf(1u))) * pf(0u) * edge_thin(pf(1u));
    return vec4<f32>(e, e, e, 1.0);
}

// The blurred picture (second) where the light is past f0 (f1 softly):
// highlights that bloom out; i.x 1: below it.
fn k_light_join(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let l = max(luma(straight(a)), luma(at1(p.x, p.y).xyz) * 0.8);
    var k = ss(pf(0u) - pf(1u), pf(0u) + pf(1u) + 1e-4, l);
    if P.i.x == 1 {
        k = 1.0 - k;
    }
    return mix4(a, at1(p.x, p.y), k);
}

// A dreamy focus: the blurred picture (second) screened over the first, f0
// how much, f1 how much of the blur replaces it, f2 its contrast.
fn k_dream(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    var o = mix3(a.xyz, b.xyz, pf(1u));
    let l = max(b.xyz * b.xyz * (1.0 / max(b.w, 1e-3)), vec3<f32>(0.0)) * pf(0u);
    let room = clamp(vec3<f32>(a.w) - o, vec3<f32>(0.0), vec3<f32>(1.0));
    o = o + min(l, vec3<f32>(1.0)) * room;
    let m = vec3<f32>(0.5 * a.w);
    return vec4<f32>(m + (o - m) * pf(2u), a.w);
}
