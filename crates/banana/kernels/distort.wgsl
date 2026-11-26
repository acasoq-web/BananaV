// Banana Split's distortions: each pixel drawn from another place.

// The picture where a place reads (`to`), its colors pulled apart along
// the way there by `spread`.
fn bent(q: vec2<f32>, to: vec2<f32>, spread: f32, mode: i32) -> vec4<f32> {
    if spread == 0.0 {
        return pic0(to, mode);
    }
    let d = (to - q) * spread;
    let r = pic0(to + d, mode);
    let g = pic0(to, mode);
    let b = pic0(to - d, mode);
    return vec4<f32>(r.x, g.y, b.z, max(max(r.w, g.w), b.w));
}

// How the light of the second picture leans here, a step either way.
fn slope1(p: vec2<i32>) -> vec2<f32> {
    let l = luma(at1(p.x - 1, p.y).xyz);
    let r = luma(at1(p.x + 1, p.y).xyz);
    let u = luma(at1(p.x, p.y - 1).xyz);
    let d = luma(at1(p.x, p.y + 1).xyz);
    return vec2<f32>(r - l, d - u) * (0.5 * f32(P.h1));
}

// Pushed by the light of a map (the second picture, blurred). f0: how far,
// f1: the turn of the push, f2: the colors' spread, f3: how the map is
// read (0 its slopes, 1 its light along the turn), f4: blur along the
// push; i.x: the edges, i.y: samples.
fn k_displace(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    // Steep slopes push no farther than a few gentle ones.
    let g = slope1(p) * 0.05;
    var d = g / (1.0 + length(g) * 2.5);
    if pf(3u) > 0.5 {
        d = vec2<f32>(luma(at1(p.x, p.y).xyz) - 0.5, 0.0);
    }
    d = turn(d, pf(1u)) * pf(0u);
    let n = max(P.i.y, 1);
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < n; i++) {
        let t = 1.0 + pf(4u) * ((f32(i) + 0.5) / f32(n) - 0.5);
        sum += bent(q, q + d * t, pf(2u), P.i.x);
    }
    return sum * (1.0 / f32(n));
}

// Each color pushed its own way. f0-1 red's, f2-3 green's, f4-5 blue's
// (frame heights), f6-8: their zooms, f9-11: their turns, f12-13: the
// middle; i.x: the edges.
fn k_rgb_shift(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p) - pf2(12u);
    let c = pf2(12u);
    let r = pic0(c + turn(q, -pf(9u)) / pf(6u) - pf2(0u), P.i.x);
    let g = pic0(c + turn(q, -pf(10u)) / pf(7u) - pf2(2u), P.i.x);
    let b = pic0(c + turn(q, -pf(11u)) / pf(8u) - pf2(4u), P.i.x);
    return vec4<f32>(r.x, g.y, b.z, max(max(r.w, g.w), b.w));
}

// Noise that flows. f0: how far, f1: the noise's size, f2: the colors'
// spread, f3-4: how the noise drifts in a second, f5: how fast it changes,
// f6: the stretch down; i.x: the edges, i.y: layers, i.z: 0 waves, 1
// swirls, 2 ridges, 3 two flows against each other.
fn k_turbulence(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let s = 1.0 / max(pf(1u), 1e-3);
    let u = vec2<f32>(q.x, q.y * pf(6u)) * s + pf2(3u) * P.time;
    let z = P.time * pf(5u);
    var d = vec2<f32>(0.0);
    if P.i.z == 1 {
        let e = 0.05;
        let n0 = fbm(vec3<f32>(u, z), P.i.y, P.seed);
        let nx = fbm(vec3<f32>(u + vec2<f32>(e, 0.0), z), P.i.y, P.seed);
        let ny = fbm(vec3<f32>(u + vec2<f32>(0.0, e), z), P.i.y, P.seed);
        d = vec2<f32>(ny - n0, n0 - nx) * (0.35 / e);
    } else if P.i.z == 2 {
        d = vec2<f32>(ridges(vec3<f32>(u, z), P.i.y, P.seed), ridges(vec3<f32>(u + vec2<f32>(19.1, 7.7), z), P.i.y, P.seed + 9u)) - vec2<f32>(0.62);
        d = d * 1.6;
    } else {
        d = vec2<f32>(fbm(vec3<f32>(u, z), P.i.y, P.seed), fbm(vec3<f32>(u + vec2<f32>(31.7, 11.3), z), P.i.y, P.seed + 5u));
        if P.i.z == 3 {
            let v = u * 2.3 + d * 2.0 - pf2(3u) * P.time * 2.0;
            d += vec2<f32>(fbm(vec3<f32>(v, z * 1.7), P.i.y, P.seed + 17u), fbm(vec3<f32>(v + vec2<f32>(5.2, 9.4), z * 1.7), P.i.y, P.seed + 23u)) * 0.6;
        }
    }
    return bent(q, q + d * pf(0u), pf(2u), P.i.x);
}

// A twist round a place. f0-1 the place, f2 the turn at its middle, f3 its
// radius, f4 how it falls off, f5 the colors' spread, f6: a pull inward.
fn k_twirl(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(0u);
    let t = clamp(1.0 - length(d) / max(pf(3u), 1e-4), 0.0, 1.0);
    let k = pw(t, pf(4u));
    let to = pf2(0u) + turn(d, -pf(2u) * k) * (1.0 + pf(6u) * k);
    return bent(q, to, pf(5u) * 0.2, P.i.x);
}

// A lens bending the whole picture. f0-1 its middle, f2 how much (above 0
// bulging, below 0 pinching), f3 the zoom, f4 the colors' spread, f5:
// squeeze sideways (1 none).
fn k_fisheye(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = vec2<f32>((q.x - pf(0u)) / pf(5u), q.y - pf(1u));
    let r = length(d);
    let k = pf(2u);
    var s = 1.0;
    let n = r * 2.0 / max(P.shape, 1.0);
    if k > 0.0 {
        let m = k * 1.35;
        s = tan(min(n * m, 1.5)) / max(n * tan(min(m, 1.5)), 1e-5);
        if n < 1e-4 {
            s = m / tan(min(m, 1.5));
        }
    } else if k < 0.0 {
        let m = -k * 1.35;
        s = atan(n * tan(min(m, 1.5))) / max(n * m, 1e-5);
        if n < 1e-4 {
            s = tan(min(m, 1.5)) / m;
        }
    }
    let o = d * s / pf(3u);
    let to = pf2(0u) + vec2<f32>(o.x * pf(5u), o.y);
    return bent(q, to, pf(4u) * 0.3, P.i.x);
}

// A magnifying glass. f0-1 its middle, f2 its radius, f3 how many times,
// f4 its stretch down, f5 how curved, f6 its rim's softness, f7-9 the
// rim's color, f10 the rim's width, f11 the colors' spread; i.y: round or
// square.
fn k_magnify(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = vec2<f32>(q.x - pf(0u), (q.y - pf(1u)) / pf(4u));
    var r = length(d);
    if P.i.y == 1 {
        r = max(abs(d.x), abs(d.y));
    }
    let rad = max(pf(2u), 1e-4);
    let n = r / rad;
    let soft = max(pf(6u), px() / rad);
    let k = 1.0 - ss(1.0 - soft, 1.0, n);
    // Flat in the middle, the curve near the rim.
    let z = mix1(1.0 / pf(3u), 1.0, pf(5u) * n * n * n);
    let o = d * z;
    let to = pf2(0u) + vec2<f32>(o.x, o.y * pf(4u));
    let here = pic0(q, 1);
    let there = bent(q, mix_v2(q, to, 1.0), pf(11u) * n * 0.15, P.i.x);
    var c = mix4(here, there, k);
    if pf(10u) > 0.0 {
        let w = pf(10u) / rad;
        let rim = 1.0 - ss(w * 0.5, w * 0.5 + px() / rad * 1.5, abs(n - 1.0));
        c = mix4(c, vec4<f32>(pf3(7u), 1.0), rim);
    }
    return c;
}

fn mix_v2(a: vec2<f32>, b: vec2<f32>, t: f32) -> vec2<f32> {
    return a + (b - a) * t;
}

// Rings of water. f0-1 the middle, f2 how high, f3 how far apart, f4 how
// far they have come (rings), f5 how they fade outward, f6 the colors'
// spread, f7: the light on them; f8: only within this of the middle (0
// everywhere).
fn k_ripple(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(0u);
    let r = length(d);
    let a = (r / max(pf(3u), 1e-3) - pf(4u)) * TAU;
    var k = exp(-r * pf(5u));
    if pf(8u) > 0.0 {
        k = k * (1.0 - ss(pf(8u) * 0.7, pf(8u), r));
    }
    // Still where the rings start.
    k = k * ss(0.0, pf(3u) * 0.5, r);
    let n = d / max(r, 1e-5);
    let c = bent(q, q + n * (cos(a) * pf(2u) * k), pf(6u), P.i.x);
    let shade = 1.0 + sin(a) * k * pf(7u) * (n.x * 0.5 - n.y * 0.86);
    return vec4<f32>(c.xyz * max(shade, 0.0), c.w);
}

// Waves. f0 how high, f1 how long, f2 the way they run, f3 how far they
// have come (waves), f4 a second set's height, f5 its length, f6 the
// colors' spread; i.y: 0 sines, 1 triangles, 2 squares, 3 noise; i.z: 0
// sideways, 1 along.
fn k_waves(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let v = turn(q, -pf(2u));
    let a = wave1(v.x / max(pf(1u), 1e-3) - pf(3u)) * pf(0u);
    let b = wave1(v.y / max(pf(5u), 1e-3) - pf(3u) * 1.31) * pf(4u);
    var o = vec2<f32>(b, a);
    if P.i.z == 1 {
        o = vec2<f32>(a, b);
    }
    return bent(q, q + turn(o, pf(2u)), pf(6u), P.i.x);
}

fn wave1(t: f32) -> f32 {
    if P.i.y == 1 {
        return abs(fr(t) * 4.0 - 2.0) - 1.0;
    }
    if P.i.y == 2 {
        return ss(0.0, 0.04, fr(t)) * 2.0 - 1.0 - 2.0 * ss(0.5, 0.54, fr(t));
    }
    if P.i.y == 3 {
        return wander(t * 2.0, P.seed);
    }
    return sin(t * TAU);
}

// The frame wrapped round a place (i.y 0) or a ring unrolled (1). f0-1 the
// middle, f2 the turn, f3-4 the inner and outer radii, f5 how many times
// round.
fn k_polar(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    if P.i.y == 1 {
        let u = uv_of(p);
        let a = (u.x - 0.5) * TAU * pf(5u) + pf(2u);
        let r = mix1(pf(4u), pf(3u), u.y);
        return pic0(pf2(0u) + vec2<f32>(sin(a), -cos(a)) * r, P.i.x);
    }
    let d = q - pf2(0u);
    let r = length(d);
    let a = angle(vec2<f32>(-d.y, d.x)) + pf(2u);
    let u = fr(a / TAU + 0.5) * pf(5u);
    let v = 1.0 - (r - pf(3u)) / max(pf(4u) - pf(3u), 1e-4);
    var edge = P.i.x;
    var x = u;
    if pf(5u) > 1.0 || edge != 0 {
        // Mirrored at the seam, so that it does not show.
        x = 1.0 - abs(md(u, 2.0) - 1.0);
        if pf(5u) <= 1.0 {
            x = 1.0 - abs(u * 2.0 - 1.0);
        }
    }
    let k = clamp((r - pf(3u)) / px() + 0.5, 0.0, 1.0) * clamp((pf(4u) - r) / px() + 0.5, 0.0, 1.0);
    if edge == 0 {
        return pic0_uv(vec2<f32>(x, clamp(v, 0.0, 1.0)), 1) * k;
    }
    return pic0_uv(vec2<f32>(x, v), edge);
}

// A plane in space. f0-8: the rows from this place to the picture's
// (frame heights, then divided); f9: the colors' spread; f10: how many
// samples a side (antialiased); i.x: the edges, i.y 1: the back too.
fn k_plane(p: vec2<i32>) -> vec4<f32> {
    let n = max(i32(pf(10u)), 1);
    var sum = vec4<f32>(0.0);
    for (var j = 0; j < n; j++) {
        for (var i = 0; i < n; i++) {
            let o = (vec2<f32>(f32(i), f32(j)) + vec2<f32>(0.5)) / f32(n) - vec2<f32>(0.5);
            let q = pos(p) + o * px();
            let w = pf(6u) * q.x + pf(7u) * q.y + pf(8u);
            if abs(w) > 1e-6 && (w > 0.0 || P.i.y == 1) {
                let to = vec2<f32>(pf(0u) * q.x + pf(1u) * q.y + pf(2u), pf(3u) * q.x + pf(4u) * q.y + pf(5u)) / w;
                sum += bent(q, to, pf(9u), P.i.x);
            }
        }
    }
    return sum * (1.0 / f32(n * n));
}

// A mirror tube. f0-1 its middle, f2 the turn, f3 its zoom, f4 how many
// sides, f5 where it looks (turn), f6-7: where its picture's middle is;
// i.y: 0 wedges, 1 rings folded too, 2 petals.
fn k_kaleido(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(0u);
    var r = length(d) / pf(3u);
    let wedge = TAU / max(pf(4u), 2.0);
    var a = angle(d) - pf(2u);
    if P.i.y == 2 {
        r = r * (0.75 + 0.25 * cos(a * max(pf(4u), 2.0)));
    }
    a = a - wedge * floor(a / wedge);
    a = abs(a - wedge * 0.5) + pf(5u);
    if P.i.y == 1 {
        r = abs(md(r, 0.6) - 0.3);
    }
    return pic0(pf2(6u) + vec2<f32>(cos(a), sin(a)) * r, P.i.x);
}

// Many eyes. f0: their size, f1: the turn, f2: how much of the picture
// each sees (zoom), f3: how round each is, f4: the gaps between them, f5:
// shade at their rims, f6-8 the gaps' color; i.y: 0 hexagons, 1 squares, 2
// circles.
fn k_facets(p: vec2<i32>) -> vec4<f32> {
    let q = turn(pos(p), -pf(1u)) / max(pf(0u), 1e-3);
    var c = floor(q) + vec2<f32>(0.5);
    var o = q - c;
    var edge = max(abs(o.x), abs(o.y)) * 2.0;
    if P.i.y != 1 {
        // The nearer of two lattices of middles.
        let s = vec2<f32>(1.0, 1.7320508);
        let a = floor(q / s) + vec2<f32>(0.5);
        let b = floor(q / s - vec2<f32>(0.5)) + vec2<f32>(1.0);
        let oa = q - a * s;
        let ob = q - b * s;
        if dot(oa, oa) < dot(ob, ob) {
            c = a * s;
            o = oa;
        } else {
            c = b * s;
            o = ob;
        }
        let h = abs(o);
        edge = max(h.x, h.x * 0.5 + h.y * 0.8660254) * 2.0;
        if P.i.y == 2 {
            edge = length(o) * 2.0;
        }
    }
    let bulge = 1.0 + pf(3u) * (edge * edge - 1.0);
    let seen = turn((c + o * bulge * pf(2u)) * pf(0u), pf(1u));
    var col = pic0(seen, P.i.x);
    col = vec4<f32>(col.xyz * (1.0 - pf(5u) * edge * edge), col.w);
    let w = px() / max(pf(0u), 1e-3) * 2.0;
    let gap = ss(1.0 - pf(4u) - w, 1.0 - pf(4u) + w, edge) * step(1e-4, pf(4u));
    return mix4(col, vec4<f32>(pf3(6u), 1.0), gap);
}

// Tiles thrown about. f0: how many down, f1: their width over height, f2:
// how far they go (frame heights), f3: how much they turn, f4: how many of
// them (0 to 1), f5: a gap round each, f6: how often they change (a
// second), f7-9 the gap's color; i.x: the edges, i.y 1: their places
// swapped.
fn k_tiles(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let th = 1.0 / max(pf(0u), 1.0);
    let t = vec2<f32>(th * pf(1u), th);
    let c = floor(q / t);
    let tick = i32(floor(P.time * pf(6u) + 1e-4));
    let s = P.seed + u32(max(tick, 0)) * 7919u;
    let h = rnd2(i32(c.x), i32(c.y), s);
    let mid = (c + vec2<f32>(0.5)) * t;
    let o = q - mid;
    let on = step(rnd(i32(c.x), i32(c.y), s + 11u), pf(4u) - 1e-4);
    var seen = mid + turn(o, (h.x - 0.5) * 2.0 * pf(3u) * on);
    var move_by = (h - vec2<f32>(0.5)) * 2.0 * pf(2u) * on;
    if P.i.y == 1 {
        move_by = floor(move_by / t + vec2<f32>(0.5)) * t;
    }
    seen += move_by;
    let col = pic0(seen, P.i.x);
    let d = min(t.x * 0.5 - abs(o.x), t.y * 0.5 - abs(o.y));
    let g = pf(5u) * th * 0.5;
    let k = (1.0 - ss(g - px(), g + px(), d)) * step(1e-4, pf(5u));
    return mix4(col, vec4<f32>(pf3(7u), 1.0), k);
}

// Strips that slide. f0: how many, f1: the turn, f2: how far (frame
// heights), f3: how unevenly (0 to 1), f4: a tilt of each, f5: shade at
// their sides, f6: how far each has slid by the time (a second); i.y: 0
// each its own way, 1 every other the other way, 2 all together further
// and further.
fn k_strips(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let v = turn(q, -pf(1u));
    let w = 1.0 / max(pf(0u), 1.0);
    let n = floor(v.y / w + 1e-4);
    let o = v.y / w - n - 0.5;
    let h = rnd1(i32(n), P.seed) * 2.0 - 1.0;
    var s = 1.0;
    if P.i.y == 0 {
        s = h;
    } else if P.i.y == 1 {
        s = 1.0 - 2.0 * md(n, 2.0);
        s = s * (1.0 + h * pf(3u));
    } else {
        s = n * w * 2.0 + h * pf(3u);
    }
    if P.i.y == 0 {
        s = mix1(sign(h), h, pf(3u));
    }
    let slide = s * (pf(2u) + pf(6u) * P.time);
    let depth = 1.0 + o * pf(4u);
    let to = turn(vec2<f32>(v.x * depth - slide, v.y), pf(1u));
    let c = pic0(to, P.i.x);
    let shade = 1.0 - pf(5u) * sq(o * 2.0) * 0.7;
    return vec4<f32>(c.xyz * shade, c.w);
}

// A picture inside itself without end. f0-1 the middle, f2: how much
// smaller each is, f3: how far in (the pictures come), f4: each one's
// turn, f5: how soft their edges; i.y: how many.
fn k_endless(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p) - pf2(0u);
    let ratio = max(pf(2u), 1.05);
    let t = fr(pf(3u));
    var out = vec4<f32>(0.0);
    for (var i = -1; i < P.i.y; i++) {
        // The largest first, each next inside it.
        let k = exp(-(f32(i) - t) * log(ratio));
        let v = turn(q, -pf(4u) * (f32(i) - t)) / k + pf2(0u);
        let u = uv_at(v);
        let edge = min(min(u.x, 1.0 - u.x) * P.shape, min(u.y, 1.0 - u.y));
        let w = max(pf(5u), px() / max(k, 1e-3));
        let a = ss(0.0, w, edge);
        out = mix4(out, pic0_uv(u, 1), a);
    }
    return out;
}

// The picture again and again, each smaller (or larger), turned and moved.
// f0-1 the middle, f2: each one's scale, f3: its turn, f4-5: its move, f6:
// how it fades, f7: how it is laid (0 over, 1 under, 2 added, 3 the
// brightest); i.x: the edges, i.y: how many; f8: the colors turned a
// little more each.
fn k_echoes(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    var out = vec4<f32>(0.0);
    let n = max(P.i.y, 1);
    for (var j = 0; j < n; j++) {
        var i = j;
        if pf(7u) > 0.5 && pf(7u) < 1.5 {
            i = n - 1 - j;
        }
        let f = f32(i);
        let k = exp(f * log(max(pf(2u), 1e-3)));
        let d = turn(q - pf2(0u) - pf2(4u) * f, -pf(3u) * f) / k;
        var mode = P.i.x;
        if i > 0 && mode == 1 {
            mode = 0;
        }
        var c = pic0(pf2(0u) + d, mode);
        if pf(8u) != 0.0 {
            c = vec4<f32>(hue_turn(c.xyz, pf(8u) * f), c.w);
        }
        let w = pw(pf(6u), f);
        if i == 0 {
            c = c * 1.0;
        } else {
            c = c * w;
        }
        if pf(7u) > 2.5 {
            out = max(out, c);
        } else if pf(7u) > 1.5 {
            out = vec4<f32>(out.xyz + c.xyz, out.w + c.w * (1.0 - out.w));
        } else {
            out = c + out * (1.0 - c.w);
        }
    }
    return out;
}

// Drops on the glass. f0: their size, f1: how many (0 to 1), f2: how much
// they bend, f3: how fast they run, f4: their shine, f5: the mist between
// them (the second picture, blurred).
fn k_drops(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let base = mix4(at0(p.x, p.y), at1(p.x, p.y), pf(5u));
    var out = base;
    for (var l = 0; l < 3; l++) {
        let s = pf(0u) * (1.0 - 0.28 * f32(l));
        let g = q / max(s, 1e-3) + vec2<f32>(f32(l) * 3.7, f32(l) * 1.9);
        let cx = floor(g.x);
        let run = P.time * pf(3u) / max(s, 1e-3) * (0.5 + rnd1(i32(cx), P.seed + u32(l) * 31u));
        let y = g.y - run;
        let cy = floor(y);
        let id = rnd(i32(cx), i32(cy), P.seed + u32(l) * 101u);
        if id < pf(1u) {
            let o = rnd2(i32(cx), i32(cy), P.seed + u32(l) * 211u + 5u);
            let r = 0.18 + 0.24 * rnd(i32(cx), i32(cy), P.seed + u32(l) * 17u + 3u);
            let c = vec2<f32>(cx + r + (1.0 - 2.0 * r) * o.x, cy + r + (1.0 - 2.0 * r) * o.y);
            let d = (vec2<f32>(g.x, y) - c) / r;
            let m = length(vec2<f32>(d.x, d.y * 0.92));
            let w = px() / max(s * r, 1e-4);
            let k = 1.0 - ss(1.0 - w * 1.5, 1.0, m);
            if k > 0.0 {
                let z = sqrt(max(1.0 - m * m, 0.0));
                let to = q + d * (s * r * (pf(2u) * (z - 1.6)));
                var drop = pic0(to, 2);
                let shine = pf(4u) * pw(max(dot(vec3<f32>(d * m, z), normalize(vec3<f32>(-0.45, -0.65, 0.6))), 0.0), 18.0);
                drop = vec4<f32>(drop.xyz * (0.72 + 0.38 * z) + vec3<f32>(shine), drop.w);
                out = mix4(out, drop, k);
            }
        }
    }
    return out;
}

// Glass blocks. f0: how many down, f1: how they bulge, f2: their edges'
// shade, f3: their turn, f4: how rough, f5: the colors' spread; i.y: 0
// squares, 1 reeds (upright), 2 hammered.
fn k_glass(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let v = turn(q, -pf(3u)) * max(pf(0u), 1.0);
    var o = vec2<f32>(0.0);
    var far = 0.0;
    if P.i.y == 2 {
        let c = cells(v, 0.9, 0.0, P.seed);
        let e = 0.04;
        let cx = cells(v + vec2<f32>(e, 0.0), 0.9, 0.0, P.seed).x;
        let cy = cells(v + vec2<f32>(0.0, e), 0.9, 0.0, P.seed).x;
        o = vec2<f32>(cx - c.x, cy - c.x) * (0.22 / e);
        far = c.x * 0.6;
    } else {
        let f = v - floor(v) - vec2<f32>(0.5);
        o = f * (f * f * 4.0 - vec2<f32>(1.0));
        far = max(abs(f.x), abs(f.y));
        if P.i.y == 1 {
            o = vec2<f32>(o.x, 0.0);
            far = abs(f.x);
        }
    }
    var d = o * pf(1u);
    if pf(4u) > 0.0 {
        d += vec2<f32>(noise3(vec3<f32>(v * 6.0, 0.0), P.seed), noise3(vec3<f32>(v * 6.0, 9.0), P.seed)) * pf(4u) * 0.15;
    }
    let to = q + turn(d, pf(3u)) / max(pf(0u), 1.0);
    let c = bent(q, to, pf(5u), P.i.x);
    let shade = 1.0 - pf(2u) * ss(0.32, 0.5, far);
    return vec4<f32>(c.xyz * shade, c.w);
}

// The rainbow of a lens: each color a little larger than the one before.
// f0-1 the middle, f2: how much larger the last is, f3: how much it turns,
// f4: from this far out; i.x: the edges, i.y: how many samples, i.z 1:
// along a line (f5-6, frame heights) instead.
fn k_rainbow(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(0u);
    let k = ss(pf(4u), pf(4u) + 0.35, length(d));
    let n = max(P.i.y, 3);
    var sum = vec3<f32>(0.0);
    var total = vec3<f32>(0.0);
    var a = 0.0;
    for (var i = 0; i < n; i++) {
        let t = (f32(i) + 0.5) / f32(n);
        let w = vec3<f32>(
            max(1.0 - abs(t - 0.14) * 3.4, 0.0),
            max(1.0 - abs(t - 0.5) * 3.4, 0.0),
            max(1.0 - abs(t - 0.86) * 3.4, 0.0),
        );
        let s = t - 0.5;
        var c = vec4<f32>(0.0);
        if P.i.z == 1 {
            c = pic0(q + pf2(5u) * s, P.i.x);
        } else {
            c = pic0(pf2(0u) + turn(d, pf(3u) * s * k) * (1.0 - pf(2u) * s * k), P.i.x);
        }
        sum += c.xyz * w;
        total += w;
        a += c.w;
    }
    return vec4<f32>(sum / total, a / f32(n));
}

// The edges' color drawn out past a window. f0-3: the window's left, top,
// right and bottom (parts of the frame); i.y: 0 held, 1 mirrored, 2
// blurred away (f4 how far, frame heights); f5: how soft.
fn k_stretch_edges(p: vec2<i32>) -> vec4<f32> {
    let u = uv_of(p);
    let lo = pf2(0u);
    let hi = pf2(2u);
    var t = clamp(u, lo, hi);
    if P.i.y == 1 {
        t = lo + (hi - lo) * vec2<f32>(wrap1((u.x - lo.x) / max(hi.x - lo.x, 1e-4), 2), wrap1((u.y - lo.y) / max(hi.y - lo.y, 1e-4), 2));
    }
    let c = pic0_uv(t, 1);
    if P.i.y == 2 {
        let out = u - clamp(u, lo, hi);
        let far = length(vec2<f32>(out.x * P.shape, out.y));
        let d = normalize(vec2<f32>(out.y, out.x) + vec2<f32>(1e-6, 0.0)) * far * pf(4u);
        var sum = vec4<f32>(0.0);
        for (var i = 0; i < 9; i++) {
            let s = f32(i) / 4.0 - 1.0;
            let v = clamp(t + vec2<f32>(d.x / P.shape, d.y) * s, lo, hi);
            sum += pic0_uv(v, 1);
        }
        return mix4(c, sum * (1.0 / 9.0), ss(0.0, max(pf(5u), 1e-4), far));
    }
    return c;
}

// A crystal's faces. f0: how many, f1: how far each looks, f2: the turn,
// f3: the colors' spread, f4: the faces' shade, f5-6: the middle, f7: the
// clear middle's radius.
fn k_prism(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let d = q - pf2(5u);
    let r = length(d);
    let n = max(pf(0u), 2.0);
    let a = angle(d) - pf(2u);
    let face = floor(a / TAU * n + 0.5);
    let mid = face / n * TAU + pf(2u);
    let within = abs(a / TAU * n - face) * 2.0;
    let k = ss(pf(7u), pf(7u) + 0.06, r);
    let o = vec2<f32>(cos(mid), sin(mid)) * pf(1u) * k;
    let c = bent(q, q - o, pf(3u) * k, 2);
    let light = 1.0 + pf(4u) * k * (cos(mid * 1.7 + 0.6) * 0.6 - ss(0.9, 1.0, within) * 0.5);
    return vec4<f32>(c.xyz * max(light, 0.0), c.w);
}
