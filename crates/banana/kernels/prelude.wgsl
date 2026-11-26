// Banana Split: what every kernel shares. Pictures are RGBA pixels,
// premultiplied, row after row from the top. Each kernel is a function
// k_… from a pixel to its color: the card runs it as it is, the processor
// as the Rust gen/gen.rs writes from it (but for at… and bil…,
// src/cpu.rs has those by hand).

struct Params {
    // The size written.
    w: u32,
    h: u32,
    // The sizes of the pictures read.
    w0: u32,
    h0: u32,
    w1: u32,
    h1: u32,
    w2: u32,
    h2: u32,
    // Pixels drawn for each of the full frame's.
    scale: f32,
    // The frame's width over its height.
    shape: f32,
    time: f32,
    seed: u32,
    i: vec4<i32>,
    f: array<vec4<f32>, 12>,
}

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> src0: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> src1: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> src2: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> dst: array<vec4<f32>>;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;

// Pixel (x, y) of a picture, its edges held.
fn at0(x: i32, y: i32) -> vec4<f32> {
    let cx = u32(clamp(x, 0, i32(P.w0) - 1));
    let cy = u32(clamp(y, 0, i32(P.h0) - 1));
    return src0[cy * P.w0 + cx];
}

fn at1(x: i32, y: i32) -> vec4<f32> {
    let cx = u32(clamp(x, 0, i32(P.w1) - 1));
    let cy = u32(clamp(y, 0, i32(P.h1) - 1));
    return src1[cy * P.w1 + cx];
}

fn at2(x: i32, y: i32) -> vec4<f32> {
    let cx = u32(clamp(x, 0, i32(P.w2) - 1));
    let cy = u32(clamp(y, 0, i32(P.h2) - 1));
    return src2[cy * P.w2 + cx];
}

// A picture between its pixels (their middles at whole numbers), its edges
// held.
fn bil0(v: vec2<f32>) -> vec4<f32> {
    let c = clamp(v, vec2<f32>(-4.0), vec2<f32>(f32(P.w0) + 4.0, f32(P.h0) + 4.0));
    let b = floor(c);
    let t = c - b;
    let x = i32(b.x);
    let y = i32(b.y);
    let p00 = at0(x, y);
    let p01 = at0(x, y + 1);
    let low = p00 + (at0(x + 1, y) - p00) * t.x;
    let high = p01 + (at0(x + 1, y + 1) - p01) * t.x;
    return low + (high - low) * t.y;
}

fn bil1(v: vec2<f32>) -> vec4<f32> {
    let c = clamp(v, vec2<f32>(-4.0), vec2<f32>(f32(P.w1) + 4.0, f32(P.h1) + 4.0));
    let b = floor(c);
    let t = c - b;
    let x = i32(b.x);
    let y = i32(b.y);
    let p00 = at1(x, y);
    let p01 = at1(x, y + 1);
    let low = p00 + (at1(x + 1, y) - p00) * t.x;
    let high = p01 + (at1(x + 1, y + 1) - p01) * t.x;
    return low + (high - low) * t.y;
}

fn bil2(v: vec2<f32>) -> vec4<f32> {
    let c = clamp(v, vec2<f32>(-4.0), vec2<f32>(f32(P.w2) + 4.0, f32(P.h2) + 4.0));
    let b = floor(c);
    let t = c - b;
    let x = i32(b.x);
    let y = i32(b.y);
    let p00 = at2(x, y);
    let p01 = at2(x, y + 1);
    let low = p00 + (at2(x + 1, y) - p00) * t.x;
    let high = p01 + (at2(x + 1, y + 1) - p01) * t.x;
    return low + (high - low) * t.y;
}

// A pixel past the edges as mode says: 0 nothing, 1 held, 2 mirrored.
fn edge0(x: i32, y: i32, mode: i32) -> vec4<f32> {
    let w = i32(P.w0);
    let h = i32(P.h0);
    if mode == 0 && (x < 0 || y < 0 || x >= w || y >= h) {
        return vec4<f32>(0.0);
    }
    if mode == 2 {
        var mx = x;
        var my = y;
        if mx < 0 {
            mx = -1 - mx;
        } else if mx >= w {
            mx = 2 * w - 1 - mx;
        }
        if my < 0 {
            my = -1 - my;
        } else if my >= h {
            my = 2 * h - 1 - my;
        }
        return at0(mx, my);
    }
    return at0(x, y);
}

// The effect's numbers, counted one by one.
fn pf(n: u32) -> f32 {
    return P.f[n / 4u][n % 4u];
}

fn pf2(n: u32) -> vec2<f32> {
    return vec2<f32>(pf(n), pf(n + 1u));
}

fn pf3(n: u32) -> vec3<f32> {
    return vec3<f32>(pf(n), pf(n + 1u), pf(n + 2u));
}

// Places. A pixel's as parts of the frame (from the top left), and as
// frame heights from its middle, x to the right and y down, where circles
// are round: pictures of any size and preview quality agree.
fn uv_of(p: vec2<i32>) -> vec2<f32> {
    return vec2<f32>((f32(p.x) + 0.5) / f32(P.w), (f32(p.y) + 0.5) / f32(P.h));
}

fn pos(p: vec2<i32>) -> vec2<f32> {
    let u = uv_of(p);
    return vec2<f32>((u.x - 0.5) * P.shape, u.y - 0.5);
}

fn uv_at(q: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(q.x / P.shape + 0.5, q.y + 0.5);
}

// A pixel's length in frame heights.
fn px() -> f32 {
    return 1.0 / f32(P.h);
}

// The full frame's pixel (what noise and grain count by, so that a preview
// of fewer pixels shows the same ones).
fn full_pixel(p: vec2<i32>) -> vec2<i32> {
    let s = 1.0 / P.scale;
    return vec2<i32>(i32(floor((f32(p.x) + 0.5) * s)), i32(floor((f32(p.y) + 0.5) * s)));
}

// How strong noise of specks `size` full pixels big is in a pixel drawn:
// a smaller preview's pixels hold many specks, as the frame shrunk does.
fn specks(size: f32) -> f32 {
    return min(1.0, size * P.scale);
}

fn wrap1(v: f32, mode: i32) -> f32 {
    if mode == 2 {
        let t = v - 2.0 * floor(v * 0.5);
        return 1.0 - abs(t - 1.0);
    }
    if mode == 3 {
        return v - floor(v);
    }
    return v;
}

// How much of a place is in the frame: 1 inside, 0 outside, a pixel's ramp
// between.
fn inside(u: vec2<f32>, w: f32, h: f32) -> f32 {
    let d = min(min(u.x, 1.0 - u.x) * w, min(u.y, 1.0 - u.y) * h);
    return clamp(d + 0.5, 0.0, 1.0);
}

// A picture at a place (parts of the frame), past its edges as mode says:
// 0 nothing, 1 the edge held, 2 mirrored, 3 repeated.
fn pic0_uv(u: vec2<f32>, mode: i32) -> vec4<f32> {
    let s = vec2<f32>(f32(P.w0), f32(P.h0));
    let t = vec2<f32>(wrap1(u.x, mode), wrap1(u.y, mode));
    let c = bil0(t * s - vec2<f32>(0.5));
    if mode == 0 {
        return c * inside(u, s.x, s.y);
    }
    return c;
}

fn pic1_uv(u: vec2<f32>, mode: i32) -> vec4<f32> {
    let s = vec2<f32>(f32(P.w1), f32(P.h1));
    let t = vec2<f32>(wrap1(u.x, mode), wrap1(u.y, mode));
    let c = bil1(t * s - vec2<f32>(0.5));
    if mode == 0 {
        return c * inside(u, s.x, s.y);
    }
    return c;
}

fn pic2_uv(u: vec2<f32>, mode: i32) -> vec4<f32> {
    let s = vec2<f32>(f32(P.w2), f32(P.h2));
    let t = vec2<f32>(wrap1(u.x, mode), wrap1(u.y, mode));
    let c = bil2(t * s - vec2<f32>(0.5));
    if mode == 0 {
        return c * inside(u, s.x, s.y);
    }
    return c;
}

// The same at a place in frame heights.
fn pic0(q: vec2<f32>, mode: i32) -> vec4<f32> {
    return pic0_uv(uv_at(q), mode);
}

fn pic1(q: vec2<f32>, mode: i32) -> vec4<f32> {
    return pic1_uv(uv_at(q), mode);
}

fn pic2(q: vec2<f32>, mode: i32) -> vec4<f32> {
    return pic2_uv(uv_at(q), mode);
}

// A line half as wide as `wide`, its edge `soft`, seen from `e` away: one
// thinner than its edge is fainter instead.
fn stroke(e: f32, wide: f32, soft: f32) -> f32 {
    let w = max(wide, max(soft, 1e-5));
    return (1.0 - ss(w - soft, w + max(soft, 1e-5), e)) * max(wide, 0.0) / w;
}

// Edges are looked for `gap` pixels to each side, never so near that
// nothing is seen; a line thinner than that is fainter instead.
fn edge_gap(gap: f32) -> f32 {
    return max(gap, 0.7);
}

fn edge_thin(gap: f32) -> f32 {
    return min(1.0, gap / 0.7);
}

// Small sums.
fn ss(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn sq(x: f32) -> f32 {
    return x * x;
}

fn pw(x: f32, e: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    return pow(x, e);
}

fn pw3(c: vec3<f32>, e: f32) -> vec3<f32> {
    return vec3<f32>(pw(c.x, e), pw(c.y, e), pw(c.z, e));
}

// The rest of a division, never negative.
fn md(x: f32, m: f32) -> f32 {
    return x - m * floor(x / m);
}

fn fr(x: f32) -> f32 {
    return x - floor(x);
}

fn turn(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(v.x * c - v.y * s, v.x * s + v.y * c);
}

// A vector's angle, 0 for none (counted up: y grows down the frame).
fn angle(v: vec2<f32>) -> f32 {
    if abs(v.x) + abs(v.y) < 1e-7 {
        return 0.0;
    }
    return atan2(v.y, v.x);
}

// The distance to a segment.
fn seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = b - a;
    let t = clamp(dot(p - a, d) / max(dot(d, d), 1e-9), 0.0, 1.0);
    return length(p - a - d * t);
}

// Colors.
fn luma(c: vec3<f32>) -> f32 {
    return 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
}

// A premultiplied color's own color.
fn straight(c: vec4<f32>) -> vec3<f32> {
    if c.w <= 1e-6 {
        return vec3<f32>(0.0);
    }
    return c.xyz / c.w;
}

// A straight color premultiplied.
fn back(rgb: vec3<f32>, a: f32) -> vec4<f32> {
    return vec4<f32>(rgb * a, a);
}

// Contrast about a middle gray that bends instead of clipping.
fn contrast3(c: vec3<f32>, k: f32) -> vec3<f32> {
    let d = c - vec3<f32>(0.5);
    let s = d * k;
    // Past the range the line eases off.
    let e = max(abs(s) - vec3<f32>(0.4), vec3<f32>(0.0));
    let soft = s - sign(s) * e + sign(s) * (e / (vec3<f32>(1.0) + e * max(k - 1.0, 0.0) * 2.0));
    return vec3<f32>(0.5) + soft;
}

fn mix1(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}

fn mix3(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    return a + (b - a) * t;
}

fn mix4(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    return a + (b - a) * t;
}

fn saturate3(c: vec3<f32>, s: f32) -> vec3<f32> {
    return mix3(vec3<f32>(luma(c)), c, s);
}

fn hsv(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = vec3<f32>(h + 1.0, h + 2.0 / 3.0, h + 1.0 / 3.0);
    let p = abs((k - floor(k)) * 6.0 - vec3<f32>(3.0));
    let q = clamp(p - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0));
    return (vec3<f32>(1.0) + (q - vec3<f32>(1.0)) * s) * v;
}

// A color's hue, saturation and value.
fn to_hsv(c: vec3<f32>) -> vec3<f32> {
    let hi = max(max(c.x, c.y), c.z);
    let lo = min(min(c.x, c.y), c.z);
    let d = hi - lo;
    if d <= 1e-6 {
        return vec3<f32>(0.0, 0.0, hi);
    }
    var h = 0.0;
    if hi == c.x {
        h = (c.y - c.z) / d;
    } else if hi == c.y {
        h = 2.0 + (c.z - c.x) / d;
    } else {
        h = 4.0 + (c.x - c.y) / d;
    }
    h = h / 6.0;
    return vec3<f32>(h - floor(h), d / max(hi, 1e-6), hi);
}

// A color turned about the grays, its luma kept.
fn hue_turn(c: vec3<f32>, a: f32) -> vec3<f32> {
    let y = luma(c);
    let cb = c.z - y;
    let cr = c.x - y;
    let s = sin(a);
    let k = cos(a);
    let r = y + cb * s + cr * k;
    let b = y + cb * k - cr * s;
    return vec3<f32>(r, (y - 0.2126 * r - 0.0722 * b) / 0.7152, b);
}

// Light laid on a picture: 0 added, 1 screened (never past white). The
// light shows where the picture is see-through too.
fn lit(base: vec4<f32>, light: vec3<f32>, mode: i32) -> vec4<f32> {
    let l = max(light, vec3<f32>(0.0));
    let glow = clamp(max(max(l.x, l.y), l.z), 0.0, 1.0);
    let a = base.w + (1.0 - base.w) * glow;
    if mode == 1 {
        let room = clamp(vec3<f32>(1.0) - base.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(base.xyz + min(l, vec3<f32>(1.0)) * room, a);
    }
    return vec4<f32>(base.xyz + l, a);
}

// Noise, from whole numbers only, so that the card and the processor agree.
fn hash(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

fn hash2(x: i32, y: i32, s: u32) -> u32 {
    return hash((bitcast<u32>(x) * 0x8da6b343u) ^ (bitcast<u32>(y) * 0xd8163841u) ^ (s * 0xcb1ab31fu));
}

fn hash3(x: i32, y: i32, z: i32, s: u32) -> u32 {
    return hash((bitcast<u32>(x) * 0x8da6b343u) ^ (bitcast<u32>(y) * 0xd8163841u) ^ (bitcast<u32>(z) * 0xcb1ab31fu) ^ (s * 0x165667b1u));
}

fn unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

// A number 0 to 1 for a cell.
fn rnd(x: i32, y: i32, s: u32) -> f32 {
    return unit(hash2(x, y, s));
}

fn rnd2(x: i32, y: i32, s: u32) -> vec2<f32> {
    let h = hash2(x, y, s);
    return vec2<f32>(unit(h), unit(hash(h + 0x9e3779b9u)));
}

fn rnd1(n: i32, s: u32) -> f32 {
    return unit(hash2(n, 17, s));
}

fn fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

// A smooth walk through random numbers, -1 to 1.
fn wander(x: f32, s: u32) -> f32 {
    let i = floor(x);
    let t = fade(x - i);
    let a = rnd1(i32(i), s);
    let b = rnd1(i32(i) + 1, s);
    return (a + (b - a) * t) * 2.0 - 1.0;
}

fn grad3(h: u32, x: f32, y: f32, z: f32) -> f32 {
    let k = h >> 28u;
    var u = y;
    if k < 8u {
        u = x;
    }
    var v = z;
    if k < 4u {
        v = y;
    } else if k == 12u || k == 14u {
        v = x;
    }
    if (k & 1u) == 1u {
        u = -u;
    }
    if (k & 2u) == 2u {
        v = -v;
    }
    return u + v;
}

fn grad_row(x: i32, y: i32, z: i32, f: vec3<f32>, s: u32, t: f32) -> f32 {
    let a = grad3(hash3(x, y, z, s), f.x, f.y, f.z);
    let b = grad3(hash3(x + 1, y, z, s), f.x - 1.0, f.y, f.z);
    return a + (b - a) * t;
}

// Gradient noise in space, about -1 to 1.
fn noise3(q: vec3<f32>, s: u32) -> f32 {
    let b = floor(q);
    let f = q - b;
    let x = i32(b.x);
    let y = i32(b.y);
    let z = i32(b.z);
    let tx = fade(f.x);
    let ty = fade(f.y);
    let tz = fade(f.z);
    let a0 = grad_row(x, y, z, f, s, tx);
    let a1 = grad_row(x, y + 1, z, f - vec3<f32>(0.0, 1.0, 0.0), s, tx);
    let b0 = grad_row(x, y, z + 1, f - vec3<f32>(0.0, 0.0, 1.0), s, tx);
    let b1 = grad_row(x, y + 1, z + 1, f - vec3<f32>(0.0, 1.0, 1.0), s, tx);
    let low = a0 + (a1 - a0) * ty;
    let high = b0 + (b1 - b0) * ty;
    return low + (high - low) * tz;
}

// Layers of noise, each finer and fainter: about -1 to 1.
fn fbm(q: vec3<f32>, layers: i32, s: u32) -> f32 {
    var sum = 0.0;
    var w = 0.5;
    var total = 0.0;
    var p = q;
    for (var i = 0; i < layers; i++) {
        sum += noise3(p, s + u32(i) * 131u) * w;
        total += w;
        w *= 0.5;
        p = vec3<f32>(p.x * 2.0 + 7.3, p.y * 2.0 + 3.1, p.z * 1.6 + 1.7);
    }
    return sum / max(total, 1e-6);
}

// The same, creased where the noise passes 0: 0 to 1, ridges high.
fn ridges(q: vec3<f32>, layers: i32, s: u32) -> f32 {
    var sum = 0.0;
    var w = 0.5;
    var total = 0.0;
    var p = q;
    for (var i = 0; i < layers; i++) {
        let n = 1.0 - abs(noise3(p, s + u32(i) * 131u));
        sum += n * n * w;
        total += w;
        w *= 0.5;
        p = vec3<f32>(p.x * 2.0 + 7.3, p.y * 2.0 + 3.1, p.z * 1.6 + 1.7);
    }
    return sum / max(total, 1e-6);
}

// Scattered points that drift with t: the distance to the nearest (x) and
// the second (y), the nearest's own number 0 to 1 (z); jitter how far from
// a grid.
fn cells(q: vec2<f32>, jitter: f32, t: f32, s: u32) -> vec3<f32> {
    // The sixteen nearest cells: a point two cells away never is nearer.
    let b = floor(q - vec2<f32>(0.5));
    var d1 = 64.0;
    var d2 = 64.0;
    var id = 0.0;
    for (var j = -1; j <= 2; j++) {
        for (var i = -1; i <= 2; i++) {
            let cx = i32(b.x) + i;
            let cy = i32(b.y) + j;
            let h = hash2(cx, cy, s);
            let a = unit(h) * TAU;
            let k = 0.6 + unit(hash(h + 0x9e3779b9u)) * 0.8;
            let r = vec2<f32>(unit(hash(h + 0x2545f491u)), unit(hash(h + 0x6c8e9cf5u))) - vec2<f32>(0.5);
            let o = r * 0.72 + vec2<f32>(cos(a + t * k), sin(a * 1.7 + t * k * 1.3)) * 0.18;
            let c = vec2<f32>(f32(cx) + 0.5, f32(cy) + 0.5) + o * jitter;
            let d = length(c - q);
            if d < d1 {
                d2 = d1;
                d1 = d;
                id = unit(hash(h + 0x51ed270bu));
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    return vec3<f32>(d1, d2, id);
}
