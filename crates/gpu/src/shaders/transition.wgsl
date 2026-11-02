// Transitions from `a` (going out) to `b` (coming in) at progress
// `f[0]`, as `bv-render`'s: all three images are `w` × `h`.

@group(0) @binding(1) var<storage, read> ta: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> tb: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;

const SOFT: f32 = 0.04;

fn reveal(edge: f32, u: f32) -> f32 {
    return clamp((edge * (1.0 + SOFT) - u) / SOFT, 0.0, 1.0);
}

fn get_b(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(P.w) || y >= i32(P.h) {
        return vec4<f32>(0.0);
    }
    return tb[u32(y) * P.w + u32(x)];
}

fn get_a(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(P.w) || y >= i32(P.h) {
        return vec4<f32>(0.0);
    }
    return ta[u32(y) * P.w + u32(x)];
}

fn index(id: vec3<u32>) -> i32 {
    if id.x >= P.w || id.y >= P.h {
        return -1;
    }
    return i32(id.y * P.w + id.x);
}

@compute @workgroup_size(16, 16)
fn dissolve(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    dst[i] = lerp4(ta[i], tb[i], pf(0u));
}

@compute @workgroup_size(16, 16)
fn fade_through_black(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    let p = pf(0u);
    var s: vec4<f32>;
    var k: f32;
    if p < 0.5 {
        s = ta[i];
        k = 1.0 - 2.0 * p;
    } else {
        s = tb[i];
        k = 2.0 * p - 1.0;
    }
    // The color darkens; the coverage (alpha) stays.
    dst[i] = vec4<f32>(s.x * k, s.y * k, s.z * k, s.w);
}

// `j.x`: 0 left, 1 right, 2 up, 3 down.
@compute @workgroup_size(16, 16)
fn wipe(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    let xn = (f32(id.x) + 0.5) / f32(P.w);
    let yn = (f32(id.y) + 0.5) / f32(P.h);
    var u: f32;
    switch P.j.x {
        case 0u: {
            u = 1.0 - xn;
        }
        case 1u: {
            u = xn;
        }
        case 2u: {
            u = 1.0 - yn;
        }
        default: {
            u = yn;
        }
    }
    dst[i] = lerp4(ta[i], tb[i], reveal(pf(0u), u));
}

// Push (`j.x` = 1) and slide: `b` comes in shifted by (`i.x`, `i.y`),
// from the side `i.z`, `i.w` points away from.
@compute @workgroup_size(16, 16)
fn push_slide(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let shift_x = P.i.x;
    let shift_y = P.i.y;
    let dx = P.i.z;
    let dy = P.i.w;
    let pb = get_b(x - shift_x + dx * i32(P.w), y - shift_y + dy * i32(P.h));
    var pa: vec4<f32>;
    if P.j.x != 0u {
        pa = get_a(x - shift_x, y - shift_y);
    } else {
        pa = get_a(x, y);
    }
    dst[i] = over(pb, pa);
}

// `f[1]`: half the frame's diagonal.
@compute @workgroup_size(16, 16)
fn iris(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    let dx = f32(id.x) + 0.5 - f32(P.w) / 2.0;
    let dy = f32(id.y) + 0.5 - f32(P.h) / 2.0;
    let u = sqrt(dx * dx + dy * dy) / pf(1u);
    dst[i] = lerp4(ta[i], tb[i], reveal(pf(0u), u));
}

@compute @workgroup_size(16, 16)
fn clock(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    let dx = f32(id.x) + 0.5 - f32(P.w) / 2.0;
    let dy = f32(id.y) + 0.5 - f32(P.h) / 2.0;
    // The angle from noon, clockwise, in [0, 1).
    let ang = atan2(dx, -dy);
    var u = (ang / 6.283185307179586) % 1.0;
    if u < 0.0 {
        u += 1.0;
    }
    dst[i] = lerp4(ta[i], tb[i], reveal(pf(0u), u));
}

// `b` grows from the middle while it shows (`f[1]`: its scale).
@compute @workgroup_size(16, 16)
fn zoom(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = index(id);
    if i < 0 {
        return;
    }
    let p = pf(0u);
    let s = pf(1u);
    let wf = f32(P.w);
    let hf = f32(P.h);
    let u = (f32(id.x) + 0.5 - wf / 2.0) / s + wf / 2.0 - 0.5;
    let v = (f32(id.y) + 0.5 - hf / 2.0) / s + hf / 2.0 - 0.5;
    let b = get_b(i32(rround(u)), i32(rround(v)));
    let a = ta[i];
    let k = 1.0 - p * 0.5;
    dst[i] = over(vec4<f32>(b.x * p, b.y * p, b.z * p, b.w * p), vec4<f32>(a.x * k, a.y * k, a.z * k, a.w * k));
}
