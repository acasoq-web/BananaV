// What the effects are built with: pictures halved, blurred and enlarged
// back, mixed, and the last step of them all (the mix and the mask).

fn k_copy(p: vec2<i32>) -> vec4<f32> {
    return at0(p.x, p.y);
}

// f0-3: a premultiplied color.
fn k_fill(p: vec2<i32>) -> vec4<f32> {
    return vec4<f32>(pf3(0u), pf(3u));
}

// Half as wide, half as tall. i.x: the edges.
fn k_half_x(p: vec2<i32>) -> vec4<f32> {
    let x = p.x * 2;
    let e = P.i.x;
    return (edge0(x - 1, p.y, e) + edge0(x + 2, p.y, e)) * 0.125
        + (edge0(x, p.y, e) + edge0(x + 1, p.y, e)) * 0.375;
}

fn k_half_y(p: vec2<i32>) -> vec4<f32> {
    let y = p.y * 2;
    let e = P.i.x;
    return (edge0(p.x, y - 1, e) + edge0(p.x, y + 2, e)) * 0.125
        + (edge0(p.x, y, e) + edge0(p.x, y + 1, e)) * 0.375;
}

// A Gaussian across. f0: its sigma in pixels; i.x: the edges, i.y: its
// reach.
fn k_gauss_x(p: vec2<i32>) -> vec4<f32> {
    let s = pf(0u);
    var g = 1.0;
    var k = exp(-0.5 / (s * s));
    let kk = k * k;
    var sum = edge0(p.x, p.y, P.i.x);
    var total = 1.0;
    for (var i = 1; i <= P.i.y; i++) {
        g *= k;
        k *= kk;
        sum += (edge0(p.x - i, p.y, P.i.x) + edge0(p.x + i, p.y, P.i.x)) * g;
        total += 2.0 * g;
    }
    return sum * (1.0 / total);
}

fn k_gauss_y(p: vec2<i32>) -> vec4<f32> {
    let s = pf(0u);
    var g = 1.0;
    var k = exp(-0.5 / (s * s));
    let kk = k * k;
    var sum = edge0(p.x, p.y, P.i.x);
    var total = 1.0;
    for (var i = 1; i <= P.i.y; i++) {
        g *= k;
        k *= kk;
        sum += (edge0(p.x, p.y - i, P.i.x) + edge0(p.x, p.y + i, P.i.x)) * g;
        total += 2.0 * g;
    }
    return sum * (1.0 / total);
}

// A cubic B-spline's weights between two pixels.
fn spline(t: f32) -> vec4<f32> {
    let u = 1.0 - t;
    let a = u * u * u / 6.0;
    let b = (3.0 * t * t * t - 6.0 * t * t + 4.0) / 6.0;
    let d = t * t * t / 6.0;
    return vec4<f32>(a, b, 1.0 - a - b - d, d);
}

// A blurred picture enlarged back, smoothly. f0, f1: how many times across
// and down; i.x: the edges.
fn k_up(p: vec2<i32>) -> vec4<f32> {
    let vx = (f32(p.x) + 0.5) / pf(0u) - 0.5;
    let vy = (f32(p.y) + 0.5) / pf(1u) - 0.5;
    let bx = floor(vx);
    let by = floor(vy);
    var wx = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    if pf(0u) > 1.0 {
        wx = spline(vx - bx);
    }
    var wy = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    if pf(1u) > 1.0 {
        wy = spline(vy - by);
    }
    let x = i32(bx);
    let y = i32(by);
    let e = P.i.x;
    let r0 = edge0(x - 1, y - 1, e) * wx.x + edge0(x, y - 1, e) * wx.y + edge0(x + 1, y - 1, e) * wx.z + edge0(x + 2, y - 1, e) * wx.w;
    let r1 = edge0(x - 1, y, e) * wx.x + edge0(x, y, e) * wx.y + edge0(x + 1, y, e) * wx.z + edge0(x + 2, y, e) * wx.w;
    let r2 = edge0(x - 1, y + 1, e) * wx.x + edge0(x, y + 1, e) * wx.y + edge0(x + 1, y + 1, e) * wx.z + edge0(x + 2, y + 1, e) * wx.w;
    let r3 = edge0(x - 1, y + 2, e) * wx.x + edge0(x, y + 2, e) * wx.y + edge0(x + 1, y + 2, e) * wx.z + edge0(x + 2, y + 2, e) * wx.w;
    return r0 * wy.x + r1 * wy.y + r2 * wy.z + r3 * wy.w;
}

// Two pictures mixed. f0: how much of the second.
fn k_mix(p: vec2<i32>) -> vec4<f32> {
    return mix4(at0(p.x, p.y), at1(p.x, p.y), pf(0u));
}

// The second picture's light on the first. f0: how much, f1-3: its color;
// i.x: added or screened.
fn k_add(p: vec2<i32>) -> vec4<f32> {
    return lit(at0(p.x, p.y), at1(p.x, p.y).xyz * pf(0u) * pf3(1u), P.i.x);
}

// Three pictures summed. f0-2: how much of each.
fn k_sum3(p: vec2<i32>) -> vec4<f32> {
    return at0(p.x, p.y) * pf(0u) + at1(p.x, p.y) * pf(1u) + at2(p.x, p.y) * pf(2u);
}

// The picture's bright parts alone, premultiplied. f0: the brightness they
// start at, f1: how softly, f2: their saturation, f3: their gain; i.x 1:
// the picture's dark parts instead (as light).
fn k_bright(p: vec2<i32>) -> vec4<f32> {
    let c = at0(p.x, p.y);
    var rgb = c.xyz;
    if P.i.x == 1 {
        rgb = max(vec3<f32>(c.w) - rgb, vec3<f32>(0.0));
    }
    let v = max(max(rgb.x, rgb.y), rgb.z);
    let k = ss(pf(0u) - pf(1u), pf(0u) + pf(1u) + 1e-4, v);
    let out = max(saturate3(rgb, pf(2u)), vec3<f32>(0.0)) * (k * pf(3u));
    return vec4<f32>(out, c.w * k);
}

// How far inside a shape a place is, in frame heights (negative outside).
// The shape: 1 an ellipse, 2 a rectangle, 3 a diamond, 4 a rounded
// rectangle, `r` its half sizes.
fn shape_depth(v: vec2<f32>, r: vec2<f32>, shape: i32) -> f32 {
    if shape == 1 {
        let n = v / r;
        let l = length(n);
        if l < 1e-5 {
            return min(r.x, r.y);
        }
        return (1.0 - l) / max(length(n / r) / l, 1e-6);
    }
    if shape == 3 {
        return (1.0 - abs(v.x) / r.x - abs(v.y) / r.y) / length(vec2<f32>(1.0 / r.x, 1.0 / r.y));
    }
    var corner = 0.0;
    if shape == 4 {
        corner = min(r.x, r.y) * 0.35;
    }
    let q = abs(v) - r + vec2<f32>(corner);
    return corner - length(max(q, vec2<f32>(0.0))) - min(max(q.x, q.y), 0.0);
}

// The last step: the effect (the second picture) mixed with the picture it
// got, inside its mask. f0: how much of it, f1-2: the mask's middle, f3-4:
// its half sizes, f5: its turn, f6: its feather; i.x: its shape (0 none),
// i.y: inverted, i.z: shown alone.
fn k_finish(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y);
    var k = 1.0;
    if P.i.x > 0 {
        let v = turn(pos(p) - pf2(1u), -pf(5u));
        let d = shape_depth(v, pf2(3u), P.i.x);
        let f = max(pf(6u), px() * 0.5);
        k = ss(-f, f, d);
        if P.i.y == 1 {
            k = 1.0 - k;
        }
        if P.i.z == 1 {
            return vec4<f32>(k, k, k, 1.0);
        }
    }
    return mix4(a, b, k * pf(0u));
}

fn soft_light1(a: f32, b: f32) -> f32 {
    return a + (2.0 * b - 1.0) * (a - a * a);
}

// What a generator made (the second picture) laid on the picture under it.
// f0: its opacity; i.x: 0 over, 1 added, 2 screened, 3 multiplied, 4 soft
// light, 5 alone.
fn k_lay(p: vec2<i32>) -> vec4<f32> {
    let a = at0(p.x, p.y);
    let b = at1(p.x, p.y) * pf(0u);
    let m = P.i.x;
    if m == 1 {
        return vec4<f32>(a.xyz + b.xyz, a.w + b.w * (1.0 - a.w));
    }
    if m == 2 {
        let room = clamp(vec3<f32>(1.0) - a.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(a.xyz + b.xyz * room, a.w + b.w * (1.0 - a.w));
    }
    if m == 3 {
        let c = straight(b);
        return vec4<f32>(a.xyz * mix3(vec3<f32>(1.0), c, b.w), a.w);
    }
    if m == 4 {
        let s = straight(a);
        let c = straight(b);
        let l = vec3<f32>(soft_light1(s.x, c.x), soft_light1(s.y, c.y), soft_light1(s.z, c.z));
        return vec4<f32>(mix3(s, l, b.w) * a.w, a.w);
    }
    if m == 5 {
        return mix4(a, at1(p.x, p.y), pf(0u));
    }
    return vec4<f32>(b.xyz + a.xyz * (1.0 - b.w), b.w + a.w * (1.0 - b.w));
}
