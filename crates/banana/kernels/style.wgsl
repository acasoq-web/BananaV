// Banana Split's stylized pictures: drawings, prints, damage and film.

fn unmul(c: vec4<f32>) -> vec3<f32> {
    return straight(c);
}

// Paint: the calmest of eight slices of a disc around, the strokes' way
// following the picture's (the second picture: its light, blurred). f0:
// the brush in pixels, f1: how sharp the choice, f2: how long the strokes;
// i.x: samples out.
fn k_oil(p: vec2<i32>) -> vec4<f32> {
    let gx = luma(at1(p.x + 1, p.y).xyz) - luma(at1(p.x - 1, p.y).xyz);
    let gy = luma(at1(p.x, p.y + 1).xyz) - luma(at1(p.x, p.y - 1).xyz);
    let way = angle(vec2<f32>(-gy, gx));
    let long = 1.0 + pf(2u) * ss(0.0, 0.02, length(vec2<f32>(gx, gy)));
    let n = P.i.x;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var k = 0; k < 8; k++) {
        let a = f32(k) * TAU / 8.0;
        var m = vec4<f32>(0.0);
        var s = vec3<f32>(0.0);
        var c = 0.0;
        for (var i = 0; i < n; i++) {
            for (var j = -1; j <= 1; j++) {
                let r = (f32(i) + 0.5) / f32(n) * pf(0u);
                let t = a + f32(j) * 0.26;
                var o = vec2<f32>(cos(t) * long, sin(t) / long) * r;
                o = turn(o, way);
                let x = at0(p.x + i32(floor(o.x + 0.5)), p.y + i32(floor(o.y + 0.5)));
                m += x;
                s += x.xyz * x.xyz;
                c += 1.0;
            }
        }
        m = m * (1.0 / c);
        let v = abs(s / c - m.xyz * m.xyz);
        let w = 1.0 / (1.0 + pw(255.0 * pf(1u) * (sqrt(v.x) + sqrt(v.y) + sqrt(v.z)), 4.0));
        sum += m * w;
        total += w;
    }
    return sum * (1.0 / max(total, 1e-9));
}

// The brush's hairs as relief on paint (the second picture: the paint's
// light, blurred). f0: the hairs' size, f1: how deep, f2: the light's
// turn, f3: canvas.
fn k_impasto(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let gx = luma(at1(p.x + 1, p.y).xyz) - luma(at1(p.x - 1, p.y).xyz);
    let gy = luma(at1(p.x, p.y + 1).xyz) - luma(at1(p.x, p.y - 1).xyz);
    let way = angle(vec2<f32>(-gy, gx));
    let q = pos(p);
    let s = 1.0 / max(pf(0u), 1e-4);
    let v = turn(q, -way);
    let e = 0.35 / s;
    let n0 = noise3(vec3<f32>(v.x * s * 0.15, v.y * s, 0.0), P.seed);
    let n1 = noise3(vec3<f32>(v.x * s * 0.15, (v.y + e) * s, 0.0), P.seed);
    let g = (n1 - n0) * cos(way - pf(2u));
    let w = sin(q.x * s * 1.6) * sin(q.y * s * 1.6);
    let fine = ss(1.0, 3.0, pf(0u) * f32(P.h));
    let k = 1.0 + (g * pf(1u) * 0.9 + w * pf(3u) * 0.12) * fine;
    return vec4<f32>(c.xyz * max(k, 0.0), c.w);
}

// Flat colors with ink on their edges (the second picture: soft, the
// third: its edges). f0: how many steps, f1: how soft they are, f2: the
// ink's strength, f3: where it starts, f4-6 its color, f7: saturation;
// i.x 1: no steps.
fn k_cartoon(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let s = straight(at1(p.x, p.y));
    let l = luma(s);
    let n = max(pf(0u) - 1.0, 1.0);
    var t = l;
    if P.i.x == 0 {
        let k = l * n;
        let b = floor(k);
        let w = clamp(pf(1u), 0.02, 1.0) * 0.5;
        t = (b + ss(0.5 - w, 0.5 + w, k - b)) / n;
    }
    var o = saturate3(s * (t / max(l, 0.02)), pf(7u));
    let e = at2(p.x, p.y).x;
    let ink = ss(pf(3u), pf(3u) + 0.18, e) * pf(2u);
    o = mix3(o, pf3(4u), clamp(ink, 0.0, 1.0));
    return back(o, a.w);
}

// How the light changes here, at `g` pixels (Sobel).
fn sobel0(p: vec2<i32>, g: f32) -> vec2<f32> {
    let q = vec2<f32>(f32(p.x), f32(p.y));
    let a = luma(bil0(q + vec2<f32>(-g, -g)).xyz);
    let b = luma(bil0(q + vec2<f32>(0.0, -g)).xyz);
    let c = luma(bil0(q + vec2<f32>(g, -g)).xyz);
    let d = luma(bil0(q + vec2<f32>(-g, 0.0)).xyz);
    let e = luma(bil0(q + vec2<f32>(g, 0.0)).xyz);
    let f = luma(bil0(q + vec2<f32>(-g, g)).xyz);
    let h = luma(bil0(q + vec2<f32>(0.0, g)).xyz);
    let i = luma(bil0(q + vec2<f32>(g, g)).xyz);
    return vec2<f32>(c + 2.0 * e + i - a - 2.0 * d - f, f + 2.0 * h + i - a - 2.0 * b - c) * 0.25;
}

// The outlines of a picture. f0: strength, f1: where they start, f2: how
// soft, f3-5: their color, f6-8: what is behind, f9: how much of the
// picture is behind instead, f10: how much of the picture's color they
// take, f11: the way they must run (a turn), f12: how strictly, f13: how
// many pixels apart; i.y 1: their rainbow by the way they run.
fn k_outline(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let g = sobel0(p, edge_gap(pf(13u)));
    let m = length(g) * pf(0u);
    var k = ss(pf(1u), pf(1u) + max(pf(2u), 0.01), m);
    if pf(12u) > 0.0 {
        let d = abs(dot(g / max(length(g), 1e-5), vec2<f32>(cos(pf(11u)), sin(pf(11u)))));
        k = k * mix1(1.0, pw(d, 6.0), pf(12u));
    }
    var ink = pf3(3u);
    let s = straight(a);
    ink = mix3(ink, s / max(max(max(s.x, s.y), s.z), 0.05), pf(10u));
    if P.i.y == 1 {
        ink = hsv(fr(angle(g) / TAU + 0.5), 0.9, 1.0);
    }
    let paper = mix3(pf3(6u), s, pf(9u));
    return back(mix3(paper, ink, k * edge_thin(pf(13u))), a.w);
}

// Ink lines from the difference of two blurs (the second, finer, less the
// third): f0: how much, f1: where ink starts, f2: how soft, f3-5 the ink,
// f6-8 the paper, f9: how much of the picture's color on the paper, f10:
// the paper's grain.
fn k_ink(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let d = (luma(at2(p.x, p.y).xyz) - luma(at1(p.x, p.y).xyz)) * pf(0u);
    var k = ss(pf(1u), pf(1u) + max(pf(2u), 0.005), d);
    let dark = ss(0.12, 0.02, luma(at1(p.x, p.y).xyz)) * 0.8;
    k = max(k, dark * step(0.001, pf(0u)));
    let f = full_pixel(p);
    let grain = 1.0 + (rnd(f.x, f.y, P.seed) - 0.5) * pf(10u) * 0.3;
    let paper = mix3(pf3(6u), straight(a), pf(9u)) * grain;
    return back(mix3(paper, pf3(3u), k), a.w);
}

// Hatching. f0: how far apart the lines, f1: their width, f2: their turn,
// f3-5 the ink, f6-8 the paper, f9: how much of the picture's color in the
// ink, f10: the hand's wobble, f11: the outlines' strength; i.x: how many
// directions, i.y: 0 hatching, 1 engraving (one line, its width the
// light).
fn k_hatch(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let s = straight(a);
    let l = clamp(luma(s), 0.0, 1.0);
    let q = pos(p);
    let gap = max(pf(0u), 1e-4);
    let wob = vec2<f32>(noise3(vec3<f32>(q * 6.0, 0.0), P.seed), noise3(vec3<f32>(q * 6.0, 3.0), P.seed + 1u)) * pf(10u) * gap * 0.7;
    let w = px() / gap;
    var ink = 0.0;
    for (var i = 0; i < P.i.x; i++) {
        let d = turn(q + wob, -pf(2u) - f32(i) * PI / f32(P.i.x)).y / gap;
        let t = abs(fr(d) - 0.5) * 2.0;
        var wide = (1.0 - l) * pf(1u) * 1.15;
        if P.i.y == 0 {
            // Each direction begins at a darker shade.
            let start = 1.0 - (f32(i) + 0.6) / f32(P.i.x);
            wide = ss(start + 0.08, start - 0.22, l) * pf(1u);
        }
        // Lines finer than the pixels: their mean.
        let k = mix1(min(wide, 1.0), 1.0 - ss(wide - w, wide + w, t), ss(1.2, 2.4, gap * f32(P.h)));
        ink = 1.0 - (1.0 - ink) * (1.0 - k * step(0.015, wide));
    }
    let e = length(sobel0(p, edge_gap(P.scale * 1.5)));
    ink = max(ink, ss(0.12, 0.4, e * pf(11u)) * edge_thin(P.scale * 1.5));
    let c = mix3(pf3(3u), s * 0.55, pf(9u));
    return back(mix3(pf3(6u), c, clamp(ink, 0.0, 1.0)), a.w);
}

// A printer's dots. f0: their size, f1: the turn, f2: how hard their
// edges, f3-5 the ink, f6-8 the paper, f9: contrast; i.x: 0 one ink, 1 red
// green and blue screens, 2 four inks; i.y: 0 dots, 1 lines, 2 squares, 3
// rings (f10-11 their middle).
fn k_halftone(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let q = pos(p);
    if P.i.x == 0 {
        let l = tone(straight(a), 0u);
        return back(mix3(pf3(3u), pf3(6u), screen1(q, pf(1u), l)), a.w);
    }
    let s = clamp((straight(a) - vec3<f32>(0.5)) * pf(9u) + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    let r = screen1(q, pf(1u) + 0.2618, s.x);
    let g = screen1(q, pf(1u) + 1.309, s.y);
    let b = screen1(q, pf(1u), s.z);
    if P.i.x == 1 {
        return back(vec3<f32>(r, g, b), a.w);
    }
    let k = 1.0 - max(max(s.x, s.y), s.z);
    let black = screen1(q, pf(1u) + 0.7854, 1.0 - k * 0.85);
    let o = pf3(6u) * vec3<f32>(mix1(0.05, 1.0, r), mix1(0.08, 1.0, g), mix1(0.12, 1.0, b));
    return back(mix3(pf3(3u), o, black), a.w);
}

fn tone(c: vec3<f32>, unused: u32) -> f32 {
    return clamp((luma(c) - 0.5) * pf(9u) + 0.5, 0.0, 1.0);
}

// How much of the paper shows for a light `l`.
fn screen1(q: vec2<f32>, a: f32, l: f32) -> f32 {
    let s = max(pf(0u), 1e-4);
    let v = turn(q, -a) / s;
    var t = 0.0;
    if P.i.y == 1 {
        t = abs(fr(v.y) - 0.5) * 2.0;
    } else if P.i.y == 2 {
        let f = abs(v - floor(v) - vec2<f32>(0.5)) * 2.0;
        t = max(f.x, f.y);
    } else if P.i.y == 3 {
        t = abs(fr(length(q - pf2(10u)) / s) - 0.5) * 2.0;
    } else {
        let f = v - floor(v) - vec2<f32>(0.5);
        // Ink dots in the light, paper dots in the dark.
        t = 0.5 + 0.25 * (cos(f.x * TAU) + cos(f.y * TAU));
        t = 1.0 - t;
    }
    let w = max(1.0 - pf(2u), 0.0) * 0.5 + px() / s * 1.6;
    let m = 1.0 - l;
    return ss(m - w, m + w, t) * step(0.004, l);
}

// Blocks. f0: their size, f1: their width over height, f2: their gaps,
// f3-5 the gaps' color, f6: relief; i.y: 0 squares, 1 round, 2 diamonds, 3
// bricks set unevenly (mosaic stones), 4 hexagons.
fn k_mosaic(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let t = vec2<f32>(max(pf(0u) * pf(1u), px() * 1.5), max(pf(0u), px() * 1.5));
    let w = px() / t.y;
    if P.i.y == 4 {
        let s = vec2<f32>(1.0, 1.7320508);
        let u = q / t;
        let oa = u - (floor(u / s) + vec2<f32>(0.5)) * s;
        let ob = u - (floor(u / s - vec2<f32>(0.5)) + vec2<f32>(1.0)) * s;
        var o = oa;
        if dot(ob, ob) < dot(oa, oa) {
            o = ob;
        }
        let h = abs(o);
        let e = 0.5 - max(h.x, h.x * 0.5 + h.y * 0.8660254);
        let c = block_color((u - o) * t, t * 0.7);
        return framed(c, o, e, w);
    }
    if P.i.y == 3 {
        let u = q / t;
        let c = cells(u, 0.75, 0.0, P.seed);
        // The nearest stone's middle, found again.
        let b = floor(u - vec2<f32>(0.5));
        var best = vec2<f32>(0.0);
        var d1 = 64.0;
        for (var j = -1; j <= 2; j++) {
            for (var i = -1; i <= 2; i++) {
                let cx = i32(b.x) + i;
                let cy = i32(b.y) + j;
                let h = hash2(cx, cy, P.seed);
                let a = unit(h) * TAU;
                let r = vec2<f32>(unit(hash(h + 0x2545f491u)), unit(hash(h + 0x6c8e9cf5u))) - vec2<f32>(0.5);
                let o = r * 0.72 + vec2<f32>(cos(a), sin(a * 1.7)) * 0.18;
                let m = vec2<f32>(f32(cx) + 0.5, f32(cy) + 0.5) + o * 0.75;
                let d = length(m - u);
                if d < d1 {
                    d1 = d;
                    best = m;
                }
            }
        }
        let col = block_color(best * t, t * 0.5);
        return framed(col, u - best, (c.y - c.x) * 0.5, w);
    }
    var u = q / t;
    if P.i.y == 2 {
        u = turn(u, PI * 0.25);
    }
    let c = floor(u) + vec2<f32>(0.5);
    let o = u - c;
    var e = 0.5 - max(abs(o.x), abs(o.y));
    if P.i.y == 1 {
        e = 0.5 - length(o);
    }
    var m = c;
    if P.i.y == 2 {
        m = turn(c, -PI * 0.25);
    }
    let col = block_color(m * t, t * 0.8);
    if P.i.y == 1 {
        let k = ss(pf(2u) * 0.5 - w, pf(2u) * 0.5 + w, e);
        return mix4(back(pf3(3u), at0(p.x, p.y).w), col * (1.0 + (o.y * -1.0 - o.x) * pf(6u) * 0.6), k);
    }
    return framed(col, o, e, w);
}

// A block's color: sixteen places of it.
fn block_color(m: vec2<f32>, t: vec2<f32>) -> vec4<f32> {
    var sum = vec4<f32>(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let o = (vec2<f32>(f32(i), f32(j)) + vec2<f32>(0.5)) / 4.0 - vec2<f32>(0.5);
            sum += pic0(m + o * t, 1);
        }
    }
    return sum * (1.0 / 16.0);
}

fn framed(c: vec4<f32>, o: vec2<f32>, e: f32, w: f32) -> vec4<f32> {
    let g = pf(2u) * 0.5;
    let k = ss(g - w, g + w, e) + step(pf(2u), 0.0);
    let lean = (-o.y - o.x) * pf(6u) * 0.7 + (1.0 - ss(g, g + 0.12, e)) * sign(-o.y - o.x) * pf(6u) * 0.25;
    let face = vec4<f32>(c.xyz * max(1.0 + lean, 0.0), c.w);
    return mix4(back(pf3(3u), c.w), face, clamp(k, 0.0, 1.0));
}

// Frosted: each pixel from a random place near. f0: how far, f1: how fine
// the grains (pixels), f2: how often they change a second; f3: the way (a
// turn), f4: how much along it only; i.x: the edges.
fn k_scatter(p: vec2<i32>) -> vec4<f32> {
    let f = full_pixel(p);
    let g = max(i32(pf(1u)), 1);
    let tick = u32(max(i32(floor(P.time * pf(2u) + 1e-3)), 0));
    let h = rnd2(f.x / g, f.y / g, P.seed + tick * 131u);
    let a = h.x * TAU;
    var o = vec2<f32>(cos(a), sin(a)) * sqrt(h.y) * pf(0u);
    let along = vec2<f32>(cos(pf(3u)), sin(pf(3u))) * (h.x * 2.0 - 1.0) * pf(0u);
    o = o + (along - o) * pf(4u);
    return pic0(pos(p) + o, P.i.x);
}

// A broken signal. f0: how much, f1: the blocks' size, f2: how often it
// breaks a second, f3: colors apart, f4: bands torn, f5: blocks moved, f6:
// blocks' colors lost.
fn k_glitch(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let tick = u32(max(i32(floor(P.time * pf(2u) + 1e-3)), 0));
    let s = P.seed + tick * 911u;
    let b = max(pf(1u), 1e-3);
    let amount = pf(0u) * (0.45 + 0.55 * rnd1(7, s));
    let row = i32(floor(q.y / (b * 0.5 * (0.4 + 1.6 * rnd1(3, s + 1u)))));
    let hr = rnd2(row, 11, s + 2u);
    var o = vec2<f32>(0.0);
    if hr.x < amount * 0.4 * pf(4u) {
        o = vec2<f32>((hr.y - 0.5) * 0.55 * amount, 0.0);
    }
    let size = vec2<f32>(b * 2.0, b);
    let c = floor(q / size);
    let hb = rnd2(i32(c.x), i32(c.y), s + 3u);
    let id = rnd(i32(c.x), i32(c.y), s + 4u);
    if id < amount * 0.22 * pf(5u) {
        o += floor((hb - vec2<f32>(0.5)) * 5.0) * size * vec2<f32>(1.0, 0.5);
    }
    let split = pf(3u) * amount * 0.03 * (0.4 + rnd1(row, s + 5u));
    let r = pic0(q + o + vec2<f32>(split, 0.0), 3);
    let g = pic0(q + o, 3);
    let bl = pic0(q + o - vec2<f32>(split, 0.0), 3);
    var out = vec4<f32>(r.x, g.y, bl.z, max(max(r.w, g.w), bl.w));
    if id > 1.0 - amount * 0.1 * pf(6u) {
        let k = floor(hb.x * 4.0);
        if k < 1.0 {
            out = vec4<f32>(out.y, out.x, out.z, out.w);
        } else if k < 2.0 {
            out = vec4<f32>(out.xyz * vec3<f32>(0.1, 1.2, 0.9), out.w);
        } else if k < 3.0 {
            out = vec4<f32>(floor(out.xyz * 3.0 + vec3<f32>(0.5)) / 3.0, out.w);
        } else {
            out = vec4<f32>(max(vec3<f32>(out.w) - out.xyz, vec3<f32>(0.0)), out.w);
        }
    }
    return out;
}

// The 8 x 8 blocks of a compressed picture, in four steps. A block is `b`
// pixels: 8 of the full frame's.
fn dct_w(x: f32, u: f32) -> f32 {
    var k = 0.5;
    if u < 0.5 {
        k = 0.35355339;
    }
    return cos((2.0 * x + 1.0) * u * PI / 16.0) * k;
}

// Rows to frequencies. The picture read in eight columns a block.
fn k_dct_rows(p: vec2<i32>) -> vec4<f32> {
    let bx = (p.x / 8) * 8;
    let u = f32(p.x - bx);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let c = at0(bx + i, p.y);
        let y = vec3<f32>(luma(c.xyz), c.z - luma(c.xyz), c.x - luma(c.xyz));
        sum += y * dct_w(f32(i), u);
    }
    return vec4<f32>(sum, at0(p.x, p.y).w);
}

// Columns to frequencies, then rounded. f0: the steps' size, f1: how much
// coarser the colors, f2: how many blocks come out wrong, f3: how much
// coarser the fine detail.
fn k_dct_columns(p: vec2<i32>) -> vec4<f32> {
    let by = (p.y / 8) * 8;
    let bx = (p.x / 8) * 8;
    let v = f32(p.y - by);
    let u = f32(p.x - bx);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        sum += at0(p.x, by + i).xyz * dct_w(f32(i), v);
    }
    let q = pf(0u) * (1.0 + (u + v) * pf(3u));
    let k = vec3<f32>(q, q * pf(1u), q * pf(1u));
    var o = floor(sum / k + vec3<f32>(0.5)) * k;
    let h = rnd2(bx / 8, by / 8, P.seed);
    if h.x < pf(2u) && u + v < 2.5 {
        o += vec3<f32>(h.y - 0.5, rnd(bx, by, P.seed + 1u) - 0.5, rnd(bx, by, P.seed + 2u) - 0.5) * 2.2 / (1.0 + u + v);
    }
    return vec4<f32>(o, at0(p.x, p.y).w);
}

fn k_idct_columns(p: vec2<i32>) -> vec4<f32> {
    let by = (p.y / 8) * 8;
    let y = f32(p.y - by);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        sum += at0(p.x, by + i).xyz * dct_w(y, f32(i));
    }
    return vec4<f32>(sum, at0(p.x, p.y).w);
}

fn k_idct_rows(p: vec2<i32>) -> vec4<f32> {
    let bx = (p.x / 8) * 8;
    let x = f32(p.x - bx);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        sum += at0(bx + i, p.y).xyz * dct_w(x, f32(i));
    }
    let r = sum.x + sum.z;
    let b = sum.x + sum.y;
    let g = (sum.x - 0.2126 * r - 0.0722 * b) / 0.7152;
    return vec4<f32>(max(vec3<f32>(r, g, b), vec3<f32>(0.0)), at0(p.x, p.y).w);
}

// Blocks that keep where they were going. f0: how far, f1: the blocks'
// size, f2: how many, f3: how often they change a second, f4: the pull of
// the picture's own edges (the second picture: its light, blurred).
fn k_mosh(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let b = max(pf(1u), 1e-3);
    let t = P.time * pf(3u);
    var go = vec2<f32>(0.0);
    for (var l = 0; l < 2; l++) {
        let s = b * (1.0 + f32(l));
        let c = floor(q / s);
        let z = floor(t + rnd(i32(c.x), i32(c.y), P.seed + u32(l)) * 3.0);
        let h = rnd2(i32(c.x) + i32(z) * 3, i32(c.y) - i32(z) * 7, P.seed + 5u + u32(l) * 3u);
        let on = step(rnd(i32(c.x), i32(c.y) + i32(z), P.seed + 9u), pf(2u));
        let m = (c + vec2<f32>(0.5)) * s;
        let lx = luma(pic1(m + vec2<f32>(s, 0.0), 1).xyz) - luma(pic1(m - vec2<f32>(s, 0.0), 1).xyz);
        let ly = luma(pic1(m + vec2<f32>(0.0, s), 1).xyz) - luma(pic1(m - vec2<f32>(0.0, s), 1).xyz);
        let pull = vec2<f32>(lx, ly) * 2.0 * pf(4u);
        let frac_t = fr(t);
        go += ((h - vec2<f32>(0.5)) * mix1(1.0, 0.25, pf(4u)) + pull) * pf(0u) * on * (0.35 + 0.65 * frac_t);
    }
    return pic0(q - go, 1);
}

// Runs of pixels sorted by their light. f0: the light they start at, f1:
// where they end, f2: how long (pixels); i.x: 0 across, 1 down; i.y 1: the
// other way; i.z: samples.
fn k_sort(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let l = sort_key(c);
    if l < pf(0u) || l > pf(1u) {
        return c;
    }
    var d = vec2<i32>(1, 0);
    if P.i.x == 1 {
        d = vec2<i32>(0, 1);
    }
    // The run this pixel is in: its ends either way, in steps.
    let n = P.i.z;
    let gap = max(i32(pf(2u) / f32(n)), 1);
    var back_by = 0;
    for (var i = 1; i <= n; i++) {
        let k = sort_key(at0(p.x - d.x * i * gap, p.y - d.y * i * gap));
        if k < pf(0u) || k > pf(1u) || p.x - d.x * i * gap < 0 || p.y - d.y * i * gap < 0 {
            break;
        }
        back_by = i;
    }
    var fore = 0;
    for (var i = 1; i <= n; i++) {
        let k = sort_key(at0(p.x + d.x * i * gap, p.y + d.y * i * gap));
        if k < pf(0u) || k > pf(1u) || p.x + d.x * i * gap >= i32(P.w0) || p.y + d.y * i * gap >= i32(P.h0) {
            break;
        }
        fore = i;
    }
    let len = back_by + fore + 1;
    if len < 2 {
        return c;
    }
    var want = back_by;
    if P.i.y == 1 {
        want = fore;
    }
    // The pixel of the run with as many darker ones as this one's place
    // asks for: its light found by halves, then the nearest to it.
    var lo = pf(0u);
    var hi = min(pf(1u), 1.2);
    for (var s = 0; s < 7; s++) {
        let m = (lo + hi) * 0.5;
        var under = 0;
        for (var i = -back_by; i <= fore; i++) {
            if sort_key(at0(p.x + d.x * i * gap, p.y + d.y * i * gap)) < m {
                under += 1;
            }
        }
        if under <= want {
            lo = m;
        } else {
            hi = m;
        }
    }
    let m = (lo + hi) * 0.5;
    var best = c;
    var far = 9.0;
    for (var i = -back_by; i <= fore; i++) {
        let x = at0(p.x + d.x * i * gap, p.y + d.y * i * gap);
        let e = abs(sort_key(x) - m);
        if e < far {
            far = e;
            best = x;
        }
    }
    return best;
}

fn sort_key(c: vec4<f32>) -> f32 {
    return luma(straight(c));
}

// A home tape. f0: the colors' bleed (frame heights), f1: how the lines
// waver, f2: tracking noise, f3: the band at the bottom, f4: the grain,
// f5: softness, f6: how faded, f7: drop-outs. The second picture: blurred
// across.
fn k_tape(p: vec2<i32>) -> vec4<f32> {
    let u = uv_of(p);
    let f = full_pixel(p);
    let frame = floor(P.time * 29.97 + 1e-3);
    let s = P.seed + u32(max(i32(frame), 0)) * 7u;
    let line = floor(u.y * 240.0);
    var x = (wander(u.y * 22.0 + P.time * 4.0, P.seed) * 0.0009 + wander(u.y * 3.0 - P.time * 1.4, P.seed + 1u) * 0.0016 + (rnd1(i32(line), s) - 0.5) * 0.0016) * pf(1u);
    // The head's switch at the bottom.
    let low = ss(1.0 - 0.055 * pf(3u), 1.0, u.y) * step(0.001, pf(3u));
    x += low * (0.03 + 0.02 * rnd1(i32(line), s + 2u)) * (1.0 + wander(P.time * 2.0, P.seed + 4u) * 0.5);
    // A band of noise rolling up.
    let at = fr(-P.time * 0.11 + rnd1(3, P.seed));
    let band = exp(-sq((u.y - at) * 26.0)) * pf(2u);
    x += band * (rnd1(i32(line), s + 3u) - 0.5) * 0.06;
    let v = vec2<f32>(u.x + x, u.y);
    let sharp = pic0_uv(v, 1);
    let soft = pic1_uv(v, 1);
    let cr = pic1_uv(v - vec2<f32>(pf(0u) / P.shape, 0.0), 1);
    let y = luma(mix4(sharp, soft, pf(5u)).xyz);
    let col = cr.xyz - vec3<f32>(luma(cr.xyz));
    var o = vec3<f32>(y) + col * 1.05;
    o += vec3<f32>((rnd(f.x / 2, f.y, s + 9u) - 0.5) * 0.14 * pf(4u) * specks(1.4));
    let tint = (rnd2(f.x / 9, f.y / 2, s + 10u) - vec2<f32>(0.5)) * 0.1 * pf(4u);
    o += vec3<f32>(tint.x, 0.0, tint.y);
    let snow = step(0.72, rnd(f.x / 5, f.y / 2, s + 11u)) * (band * 0.9 + low * 0.7);
    o = mix3(o, vec3<f32>(0.85), clamp(snow, 0.0, 1.0) * 0.7);
    let drop = step(1.0 - 0.004 * pf(7u), rnd1(i32(line), s + 12u)) * step(fr(u.x * 1.7 + rnd1(i32(line), s + 13u)), 0.25);
    o = mix3(o, vec3<f32>(0.95), drop);
    o = mix3(o, vec3<f32>(0.1) + o * 0.82, pf(6u));
    o = saturate3(o, 1.0 - 0.3 * pf(6u));
    return vec4<f32>(max(o, vec3<f32>(0.0)), max(sharp.w, soft.w));
}

// A bad aerial. f0: snow, f1: how the picture rolls (frames a second), f2:
// the lines' jitter, f3: ghosts, f4: bars of hum, f5: lines of a tube, f6:
// how the colors fail.
fn k_aerial(p: vec2<i32>) -> vec4<f32> {
    let u = uv_of(p);
    let f = full_pixel(p);
    let s = P.seed + u32(max(i32(floor(P.time * 30.0 + 1e-3)), 0)) * 11u;
    let line = i32(floor(u.y * 480.0));
    let burst = ss(0.35, 0.9, wander(P.time * 1.3, P.seed + 2u) * 0.5 + 0.5);
    var x = (rnd1(line, s) - 0.5) * 0.012 * pf(2u) * (0.4 + burst * 1.6);
    x += wander(u.y * 5.0 + P.time * 7.0, P.seed + 3u) * 0.012 * pf(2u) * burst;
    let y = fr(u.y + P.time * pf(1u));
    let v = vec2<f32>(u.x + x, y);
    var c = pic0_uv(v, 1);
    let g = pic0_uv(v - vec2<f32>(0.022, 0.0), 1);
    c = mix4(c, g, 0.32 * pf(3u));
    let gap = step(0.02 * step(0.001, abs(pf(1u))), min(y, 1.0 - y));
    var o = c.xyz * gap;
    let l = luma(o);
    o = mix3(o, vec3<f32>(l) + (hue_turn(o, wander(P.time * 3.0, P.seed + 5u) * 1.5 * pf(6u)) - vec3<f32>(l)) * (1.0 - 0.7 * pf(6u) * burst), min(pf(6u), 1.0));
    let n = rnd(f.x / 2, f.y / 2, s + 4u) - 0.5;
    o += vec3<f32>(n * 0.55 * pf(0u) * (0.5 + burst * 0.8) * specks(2.0));
    o = o * (1.0 + sin((u.y * 2.0 - P.time * 0.8) * TAU) * 0.16 * pf(4u));
    let tube = mix1(0.5, 0.5 + 0.5 * cos(u.y * 240.0 * TAU), ss(1.6, 3.2, f32(P.h) / 240.0));
    o = o * (1.0 - pf(5u) * 0.5 * tube);
    return vec4<f32>(max(o, vec3<f32>(0.0)), c.w);
}

// A tube's lines. f0: how many, f1: how dark between, f2: their roundness,
// f3: how they roll a second, f4: the dots of color, f5-7: a single color
// (f8 how much), f9: the glass's curve, f10: its corners' shade.
fn k_tube(p: vec2<i32>) -> vec4<f32> {
    var q = pos(p);
    let k = pf(9u) * 0.35;
    q = q * (1.0 + k * dot(q, q));
    let c = pic0(q, 0);
    let u = uv_at(q);
    let l = luma(c.xyz);
    var o = mix3(c.xyz, pf3(5u) * l, pf(8u));
    let t = fr(u.y * pf(0u) - P.time * pf(3u));
    let beam = pw(0.5 + 0.5 * cos((t - 0.5) * TAU), mix1(0.35, 2.2, pf(2u)));
    // The brighter the picture the wider its lines.
    let wide = mix1(beam, 1.0, clamp(l * 0.45, 0.0, 0.6));
    let seen = ss(1.6, 3.2, f32(P.h) / max(pf(0u), 1.0));
    o = o * mix1(1.0, mix1(0.72, wide * 1.25, seen), pf(1u));
    if pf(4u) > 0.0 {
        let m = fr(u.x * pf(0u) * P.shape * 0.75) * 3.0;
        let dots = vec3<f32>(ss(1.0, 0.4, abs(m - 0.5)), ss(1.0, 0.4, abs(m - 1.5)), ss(1.0, 0.4, abs(m - 2.5)));
        let fit = ss(2.0, 4.0, f32(P.w) / max(pf(0u) * P.shape * 0.75, 1.0));
        o = o * mix3(vec3<f32>(1.0), mix3(vec3<f32>(0.8), dots * 1.7 + vec3<f32>(0.15), fit), pf(4u));
    }
    let e = 1.0 - ss(0.35, 0.95, length(vec2<f32>(q.x / P.shape, q.y) * 1.35)) * pf(10u);
    return vec4<f32>(o * e, c.w);
}

// Film that has been shown too often. f0: scratches, f1: dust, f2:
// flicker, f3: weave (the picture's sway, frame heights), f4: grain, f5:
// stains, f6: how faded, f7: hairs, f8: the light's fall at the sides, f9:
// frames a second.
fn k_film(p: vec2<i32>) -> vec4<f32> {
    let fps = max(pf(9u), 1.0);
    let n = i32(floor(P.time * fps + 1e-3));
    let s = P.seed + u32(max(n, 0)) * 57u;
    let sway = vec2<f32>(wander(P.time * 2.6, P.seed + 1u) + (rnd1(n, P.seed + 2u) - 0.5) * 0.8, wander(P.time * 3.4, P.seed + 3u) * 0.7 + (rnd1(n, P.seed + 4u) - 0.5)) * pf(3u);
    let q = pos(p);
    let c = pic0(q + sway, 1);
    var o = c.xyz;
    let f = full_pixel(p);
    o = o * (1.0 + ((rnd1(n, P.seed + 5u) - 0.5) * 0.28 + wander(P.time * 5.0, P.seed + 6u) * 0.06) * pf(2u));
    let l = luma(o);
    o = mix3(o, vec3<f32>(l) * vec3<f32>(1.06, 0.98, 0.86) * 0.9 + vec3<f32>(0.06, 0.045, 0.03), pf(6u));
    let g = (rnd(f.x / 2, f.y / 2, s + 7u) - 0.5) * 0.6 * specks(2.0) + (rnd(f.x, f.y, s + 8u) - 0.5) * 0.4 * specks(1.0);
    o += vec3<f32>(g * 0.22 * pf(4u) * (1.1 - clamp(l, 0.0, 1.0) * 0.6));
    var mark = 0.0;
    for (var i = 0; i < 4; i++) {
        // Scratches last some frames and drift.
        let life = n / (5 + i * 4);
        let h = rnd2(life, i, P.seed + 11u);
        if rnd1(life * 3 + i, P.seed + 13u) < pf(0u) * 0.65 {
            let x = (h.x - 0.5) * P.shape + (rnd1(n + i * 50, P.seed + 14u) - 0.5) * 0.006 + wander(q.y * 1.5, P.seed + u32(i) + u32(max(life, 0))) * 0.002;
            let w = 0.0005 + 0.0011 * h.y;
            let d = abs(q.x - x);
            let gone = step(0.3, noise3(vec3<f32>(q.y * 3.0, f32(life), f32(i)), P.seed) + 0.75);
            mark += (1.0 - ss(w, w + px() * 1.2, d)) * gone * (h.y * 1.6 - 0.45);
        }
    }
    o += vec3<f32>(mark * 0.5);
    // Dust: dark and light specks, new each frame.
    let cell = floor(q * 11.0);
    let hd = rnd2(i32(cell.x), i32(cell.y), s + 21u);
    if hd.x < pf(1u) * 0.07 {
        let od = rnd2(i32(cell.x), i32(cell.y), s + 22u);
        let m = (cell + vec2<f32>(0.2) + od * 0.6) / 11.0;
        let v = (q - m) / (0.0022 + 0.006 * hd.y);
        let e = length(v * (1.0 + 0.5 * vec2<f32>(noise3(vec3<f32>(v, 1.0), s), noise3(vec3<f32>(v, 5.0), s))));
        o = mix3(o, vec3<f32>(step(0.6, od.x) * 0.9), (1.0 - ss(0.7, 1.1, e)) * 0.85);
    }
    if rnd1(n, P.seed + 31u) < pf(7u) * 0.5 {
        let h = rnd2(n, 3, P.seed + 32u);
        let m = vec2<f32>((h.x - 0.5) * P.shape * 0.9, (h.y - 0.5) * 0.9);
        let v = turn(q - m, h.x * 9.0);
        let bend_by = v.y - sin(v.x * 22.0 + h.y * 9.0) * 0.016;
        let hair = (1.0 - ss(0.0009, 0.0009 + px() * 1.3, abs(bend_by))) * (1.0 - ss(0.05, 0.06, abs(v.x)));
        o = o * (1.0 - hair * 0.85);
    }
    let stain = ss(0.2, 0.6, fbm(vec3<f32>(q * 1.6, f32(n / 3) * 1.7), 3, P.seed + 41u)) * pf(5u);
    o = mix3(o, o * vec3<f32>(1.12, 0.92, 0.7) + vec3<f32>(0.05, 0.02, 0.0), stain);
    let e = length(vec2<f32>(q.x / max(P.shape, 1.0), q.y));
    o = o * (1.0 - pf(8u) * ss(0.28, 0.72, e));
    return vec4<f32>(max(o, vec3<f32>(0.0)) , c.w);
}

// Grain. f0: how much, f1: its size in full pixels, f2: how much of it is
// in color, f3: in the dark, f4: in the middle, f5: in the light, f6: how
// soft, f7: frames a second (0 still).
fn k_grain(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let f = full_pixel(p);
    var s = P.seed;
    if pf(7u) > 0.0 {
        s = s + u32(max(i32(floor(P.time * pf(7u) + 1e-3)), 0)) * 977u;
    }
    // (A hair past the pixel's corner: an exact edge falls either way.)
    let g = (vec2<f32>(f32(f.x), f32(f.y)) + vec2<f32>(0.01)) / max(pf(1u), 0.5);
    let l = luma(straight(c));
    let k = mix1(mix1(pf(3u), pf(4u), ss(0.0, 0.5, l)), pf(5u), ss(0.5, 1.0, l)) * pf(0u) * specks(max(pf(1u), 1.0));
    let a = grain1(g, s);
    let r = grain1(g + vec2<f32>(71.0, 3.0), s + 1u);
    let b = grain1(g + vec2<f32>(13.0, 57.0), s + 2u);
    let n = mix3(vec3<f32>(a), vec3<f32>(r, a, b), pf(2u));
    return vec4<f32>(max(c.xyz + n * (k * c.w), vec3<f32>(0.0)), c.w);
}

// A grain between its places, smooth (`f6`) or each its own: about -1 to 1.
fn grain1(g: vec2<f32>, s: u32) -> f32 {
    let b = floor(g);
    let x = i32(b.x);
    let y = i32(b.y);
    let hard = rnd(x, y, s) + rnd(x, y, s + 7919u) - 1.0;
    if pf(1u) <= 1.0 {
        return hard;
    }
    let t = g - b;
    let a = rnd(x, y, s) + rnd(x, y, s + 7919u) - 1.0;
    let c = rnd(x + 1, y, s) + rnd(x + 1, y, s + 7919u) - 1.0;
    let d = rnd(x, y + 1, s) + rnd(x, y + 1, s + 7919u) - 1.0;
    let e = rnd(x + 1, y + 1, s) + rnd(x + 1, y + 1, s + 7919u) - 1.0;
    let v = vec2<f32>(fade(t.x), fade(t.y));
    let soft = mix1(mix1(a, c, v.x), mix1(d, e, v.x), v.y) * 1.5;
    return mix1(hard, soft, pf(6u));
}

// A lens's shade. f0-1 its middle, f2 where it starts, f3 how soft, f4 how
// dark, f5: its roundness (0 the frame's shape, 1 round), f6-8 its color,
// f9: how much the highlights show through; i.x 1: inside out.
fn k_vignette(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    let v = pos(p) - pf2(0u);
    let round = length(v);
    let box = length(vec2<f32>(v.x / max(P.shape, 1e-3), v.y)) * 1.2;
    let r = mix1(box, round, pf(5u));
    var k = ss(pf(2u), pf(2u) + max(pf(3u), 1e-3), r) * pf(4u);
    if P.i.x == 1 {
        k = (1.0 - ss(pf(2u), pf(2u) + max(pf(3u), 1e-3), r)) * pf(4u);
    }
    let l = luma(straight(c));
    k = k * (1.0 - pf(9u) * ss(0.6, 1.0, l));
    return vec4<f32>(mix3(c.xyz, pf3(6u) * c.w, clamp(k, 0.0, 1.0)), c.w);
}

// Raised (the second picture: its light, blurred). f0: the light's turn,
// f1: its height, f2: how deep, f3: how much of the picture's color (0 a
// gray plate), f4: the shine, f5: how tight, f6-8 its color; i.y: 0 an
// emboss, 1 metal, 2 glass (f9: how it bends, f10: the colors' spread);
// i.x: the edges.
fn k_emboss(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let l = luma(at1(p.x - 1, p.y).xyz);
    let r = luma(at1(p.x + 1, p.y).xyz);
    let u = luma(at1(p.x, p.y - 1).xyz);
    let d = luma(at1(p.x, p.y + 1).xyz);
    let k = pf(2u) * 0.5 * f32(P.h) * 0.06;
    let g = vec2<f32>(l - r, u - d) * k;
    let n = normalize(vec3<f32>(g, 1.0));
    let lamp = normalize(vec3<f32>(cos(pf(0u)), sin(pf(0u)), max(pf(1u), 0.05)));
    let flat_by = max(lamp.z, 0.05);
    let diff = dot(n, lamp) / flat_by;
    let h = normalize(lamp + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pw(max(dot(n, h), 0.0), pf(5u)) * pf(4u);
    var base = at0(p.x, p.y);
    if P.i.y == 2 {
        let o = n.xy * pf(9u);
        let a = pic0(q - o * (1.0 + pf(10u)), P.i.x);
        let b = pic0(q - o, P.i.x);
        let c = pic0(q - o * (1.0 - pf(10u)), P.i.x);
        base = vec4<f32>(a.x, b.y, c.z, max(max(a.w, b.w), c.w));
    }
    var col = mix3(vec3<f32>(0.5 * base.w), base.xyz, pf(3u));
    if P.i.y == 1 {
        // A room's light in the metal.
        let m = 0.5 + 0.5 * sin(n.x * 5.0 + n.y * 8.0 + luma(base.xyz) * 2.5);
        col = mix3(vec3<f32>(m * base.w), base.xyz * (0.35 + m), pf(3u));
    }
    let o = col * max(diff, 0.0) + pf3(6u) * (spec * base.w);
    return vec4<f32>(o, base.w);
}

// The colors' fine detail alone (the second picture blurred less, the
// third more). f0: how much, f1: how much of its color, f2: 0 on gray, 1
// on the picture.
fn k_band(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let d = (at1(p.x, p.y).xyz - at2(p.x, p.y).xyz) * pf(0u);
    let m = mix3(vec3<f32>(luma(d)), d, pf(1u));
    let under = mix3(vec3<f32>(0.5 * a.w), a.xyz, pf(2u));
    return vec4<f32>(max(under + m, vec3<f32>(0.0)), a.w);
}

// Colors run wild by the light and noise. f0: the noise's size, f1: how
// many times round the rainbow, f2: how far it has gone, f3: how vivid,
// f4: how much, f5: the light's pull, f6: the turn; i.y: 0 clouds, 1
// stripes.
fn k_psyche(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let s = straight(a);
    let l = luma(s);
    let q = turn(pos(p), -pf(6u)) / max(pf(0u), 1e-3);
    var t = 0.0;
    if P.i.y == 1 {
        t = q.x * 0.5 + wander(q.y * 0.7 + pf(2u) * 0.4, P.seed) * 0.25;
    } else {
        t = fbm(vec3<f32>(q * 0.6, pf(2u) * 0.3), 3, P.seed) * 0.8;
    }
    let h = t * pf(1u) + l * pf(5u) + pf(2u);
    let o = hsv(fr(h), pf(3u), clamp(0.2 + l * 1.1, 0.0, 1.3));
    return back(mix3(s, o, pf(4u)), a.w);
}

// Stripes by the light. f0: how many, f1: how far they have moved, f2: how
// hard, f3-5 and f6-8 their two colors, f9: how much; i.y 1: each color's
// own.
fn k_zebra_light(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let s = straight(a);
    if P.i.y == 1 {
        let o = vec3<f32>(band1(s.x), band1(s.y), band1(s.z));
        return back(mix3(s, o, pf(9u)), a.w);
    }
    return back(mix3(s, mix3(pf3(3u), pf3(6u), band1(luma(s))), pf(9u)), a.w);
}

fn band1(v: f32) -> f32 {
    let t = 0.5 + 0.5 * cos((v * pf(0u) - pf(1u)) * TAU);
    let w = max(0.5 - pf(2u) * 0.5, 0.01);
    return ss(0.5 - w, 0.5 + w, t);
}

// A pencil: (the second picture: blurred) the light that differs from what
// is around it, drawn with strokes. f0: strength, f1: the strokes' size,
// f2: their turn, f3-5 the pencil, f6-8 the paper, f9: the picture's
// color, f10: the paper's grain.
fn k_pencil(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let s = straight(a);
    let l = luma(a.xyz);
    let b = luma(at1(p.x, p.y).xyz);
    // The light over what is around it, as if its negative were laid on.
    let dodge = clamp(l / max(b, 0.03), 0.0, 1.0);
    var dark = pw(1.0 - dodge, 0.8) * pf(0u) * 2.2 + (1.0 - clamp(l, 0.0, 1.0)) * 0.35 * pf(0u);
    let q = pos(p);
    let v = turn(q, -pf(2u)) / max(pf(1u), 1e-4);
    let h = noise3(vec3<f32>(v.x * 0.12, v.y, 0.0), P.seed) * 0.7 + noise3(vec3<f32>(v.x * 0.35, v.y * 2.7, 4.0), P.seed + 1u) * 0.5;
    let grain = noise3(vec3<f32>(q / max(pf(1u), 1e-4) * 1.7, 9.0), P.seed + 2u);
    dark = clamp(dark * (1.0 + h * 0.9) - grain * 0.22 * pf(10u) * (1.0 - dark), 0.0, 1.0);
    let ink = mix3(pf3(3u), s * 0.85, pf(9u));
    let paper = pf3(6u) * (1.0 + grain * 0.05 * pf(10u));
    return back(mix3(paper, ink, dark), a.w);
}

// Watercolor: (the second picture: paint, the third: blurred) pools of
// pigment with dark rims on wet paper. f0: the rims, f1: how the pigment
// varies, f2: the paper's grain, f3: how thin, f4: size.
fn k_wash(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let s = straight(at1(p.x, p.y));
    let soft = straight(at2(p.x, p.y));
    let e = length(s - soft);
    let q = pos(p);
    let z = 1.0 / max(pf(4u), 1e-3);
    let n = fbm(vec3<f32>(q * z, 0.0), 4, P.seed);
    let g = fbm(vec3<f32>(q * z * 14.0, 3.0), 2, P.seed + 1u);
    // Pigment takes light away: more of it, more.
    let thick = 1.0 + n * pf(1u) * 0.9 + ss(0.04, 0.22, e) * pf(0u) * 0.8 + g * pf(2u) * 0.25;
    let o = pw3(max(s, vec3<f32>(0.004)), max(thick, 0.2));
    return back(mix3(o, vec3<f32>(1.0), pf(3u) * 0.5 * (1.0 - luma(o) * 0.4)), a.w);
}

// A wall of lamps. f0: their size, f1: how round
// and small, f2: their glow, f3-5: the wall, f6: how many steps of light
// (0 smooth).
fn k_lamps(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let t = max(pf(0u), px() * 2.0);
    let c = floor(q / t) + vec2<f32>(0.5);
    let o = q / t - c;
    var sum = vec4<f32>(0.0);
    for (var j = 0; j < 3; j++) {
        for (var i = 0; i < 3; i++) {
            sum += pic0((c + (vec2<f32>(f32(i), f32(j)) - vec2<f32>(1.0)) * 0.28) * t, 1);
        }
    }
    sum = sum * (1.0 / 9.0);
    var col = sum.xyz;
    if pf(6u) > 1.0 {
        col = floor(col * pf(6u) + vec3<f32>(0.5)) / pf(6u);
    }
    let r = length(o) * 2.0;
    let w = px() / t * 2.0;
    let disc = 1.0 - ss(pf(1u) - w, pf(1u) + w, r);
    let glow = exp(-r * r * 2.5) * pf(2u);
    let l = col * (disc * (1.15 - 0.4 * r) + glow);
    return vec4<f32>(pf3(3u) * sum.w * (1.0 - disc) + l, sum.w);
}

// Color fringes of cheap printing: each ink moved (f0-1, f2-3, f4-5), f6
// the paper shows through, f7-9 the paper.
fn k_misprint(p: vec2<i32>) -> vec4<f32> {
    let q = pos(p);
    let a = pic0(q + pf2(0u), 1);
    let b = pic0(q + pf2(2u), 1);
    let c = pic0(q + pf2(4u), 1);
    let f = full_pixel(p);
    let n = noise3(vec3<f32>(q * 90.0, 0.0), P.seed) * 0.5 + (rnd(f.x, f.y, P.seed) - 0.5) * 0.5 * specks(1.0);
    let ink = vec3<f32>(a.x, b.y, c.z);
    let o = mix3(ink, vec3<f32>(1.0), clamp((n * 0.5 + 0.12) * pf(6u), 0.0, 1.0)) * pf3(7u);
    return vec4<f32>(o * max(a.w, 1e-6), max(max(a.w, b.w), c.w));
}
