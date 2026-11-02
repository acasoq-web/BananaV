// Kernels that change an image in place (`w` × `h`).

@group(0) @binding(1) var<storage, read_write> img: array<vec4<f32>>;

fn at(id: vec3<u32>) -> i32 {
    if id.x >= P.w || id.y >= P.h {
        return -1;
    }
    return i32(id.y * P.w + id.x);
}

@compute @workgroup_size(16, 16)
fn fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    img[i] = vec4<f32>(pf(0u), pf(1u), pf(2u), pf(3u));
}

@compute @workgroup_size(16, 16)
fn scale(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let k = pf(0u);
    let px = img[i];
    img[i] = vec4<f32>(px.x * k, px.y * k, px.z * k, px.w * k);
}

// Light → video gamma, on the straight color.
@compute @workgroup_size(16, 16)
fn to_video_gamma(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        return;
    }
    img[i] = vec4<f32>(encoded(px.x, a), encoded(px.y, a), encoded(px.z, a), a);
}

fn encoded(c: f32, a: f32) -> f32 {
    let s = c / a;
    return signum(s) * bt709_encode(abs(s)) * a;
}

fn decoded(c: f32, a: f32) -> f32 {
    let s = c / a;
    return signum(s) * bt709_decode(abs(s)) * a;
}

// What 8-bit formats hold, after each effect: the alpha and the straight
// colors within 0–1.
@compute @workgroup_size(16, 16)
fn clip_range(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = clamp(px.w, 0.0, 1.0);
    img[i] = vec4<f32>(clamp(px.x, 0.0, a), clamp(px.y, 0.0, a), clamp(px.z, 0.0, a), a);
}

// Video gamma → light; `j.x` clips to 0–1 first (8-bit formats).
@compute @workgroup_size(16, 16)
fn from_video_gamma(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    var px = img[i];
    if P.j.x != 0u {
        let a = clamp(px.w, 0.0, 1.0);
        px = vec4<f32>(clamp(px.x, 0.0, a), clamp(px.y, 0.0, a), clamp(px.z, 0.0, a), a);
    }
    let a = px.w;
    if a <= 0.0 {
        img[i] = px;
        return;
    }
    img[i] = vec4<f32>(decoded(px.x, a), decoded(px.y, a), decoded(px.z, a), a);
}

// The effects that work on each pixel's color, one kernel each (simple
// kernels: Direct3D's older shader compiler gives up on large ones).

fn straight(px: vec4<f32>) -> vec3<f32> {
    return vec3<f32>(px.x / px.w, px.y / px.w, px.z / px.w);
}

fn put(i: i32, c: vec3<f32>, a: f32) {
    img[i] = vec4<f32>(c.x * a, c.y * a, c.z * a, a);
}

fn contrast1(x: f32, k: f32, b: f32) -> f32 {
    return (x - 0.5) * k + 0.5 + b;
}

@compute @workgroup_size(16, 16)
fn brightness_contrast(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        return;
    }
    let b = pf(0u);
    let k = pf(1u);
    let v = perceptual3(straight(px));
    put(i, linear3(vec3<f32>(contrast1(v.x, k, b), contrast1(v.y, k, b), contrast1(v.z, k, b))), px.w);
}

fn grade1(v: f32, lift: f32, gamma: f32, gain: f32, pivot: f32, contrast: f32) -> f32 {
    var x = v * (1.0 + gain);
    x += lift * (1.0 - x);
    x = pow(max(x, 0.0), 1.0 / max(1.0 + gamma, 0.05));
    return (x - pivot) * contrast + pivot;
}

@compute @workgroup_size(16, 16)
fn color_grade(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        return;
    }
    let c = straight(px);
    let exposure = pf(0u);
    let contrast = pf(1u);
    let saturation = pf(2u);
    let pivot = pf(3u);
    let lin = vec3<f32>(c.x * exposure * pf(4u), c.y * exposure * pf(5u), c.z * exposure * pf(6u));
    let v = perceptual3(lin);
    let g = vec3<f32>(
        grade1(v.x, pf(7u), pf(10u), pf(13u), pivot, contrast),
        grade1(v.y, pf(8u), pf(11u), pf(14u), pivot, contrast),
        grade1(v.z, pf(9u), pf(12u), pf(15u), pivot, contrast),
    );
    let y = luma(g);
    let s = vec3<f32>(y + (g.x - y) * saturation, y + (g.y - y) * saturation, y + (g.z - y) * saturation);
    put(i, linear3(s), px.w);
}

fn level1(x: f32) -> f32 {
    let ib = pf(0u);
    let range = pf(1u);
    let g = pf(2u);
    let ob = pf(3u);
    let ow = pf(4u);
    return pow(clamp((x - ib) / range, 0.0, 1.0), 1.0 / g) * (ow - ob) + ob;
}

@compute @workgroup_size(16, 16)
fn levels(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        return;
    }
    let v = perceptual3(straight(px));
    put(i, linear3(vec3<f32>(level1(v.x), level1(v.y), level1(v.z))), px.w);
}

@compute @workgroup_size(16, 16)
fn black_and_white(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        return;
    }
    let k = pf(0u);
    let c = straight(px);
    let y = luma(c);
    put(i, vec3<f32>(c.x + (y - c.x) * k, c.y + (y - c.y) * k, c.z + (y - c.z) * k), px.w);
}

@compute @workgroup_size(16, 16)
fn sepia(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        return;
    }
    let k = pf(0u);
    let v = perceptual3(straight(px));
    let s = vec3<f32>(
        0.393 * v.x + 0.769 * v.y + 0.189 * v.z,
        0.349 * v.x + 0.686 * v.y + 0.168 * v.z,
        0.272 * v.x + 0.534 * v.y + 0.131 * v.z,
    );
    put(i, linear3(vec3<f32>(v.x + (s.x - v.x) * k, v.y + (s.y - v.y) * k, v.z + (s.z - v.z) * k)), px.w);
}

fn invert1(x: f32, k: f32) -> f32 {
    return x + ((1.0 - min(x, 1.0)) - x) * k;
}

@compute @workgroup_size(16, 16)
fn invert(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        return;
    }
    let k = pf(0u);
    let v = perceptual3(straight(px));
    put(i, linear3(vec3<f32>(invert1(v.x, k), invert1(v.y, k), invert1(v.z, k))), px.w);
}

@compute @workgroup_size(16, 16)
fn vignette(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let amount = pf(0u);
    let size = pf(1u);
    let soft = pf(2u);
    let w = f32(P.w);
    let h = f32(P.h);
    let dx = (f32(id.x) + 0.5 - w / 2.0) / (w / 2.0);
    let dy = (f32(id.y) + 0.5 - h / 2.0) / (h / 2.0);
    let d = sqrt(dx * dx + dy * dy) / 1.4142135623730951;
    let t = clamp((d - size) / soft, 0.0, 1.0);
    let k = 1.0 - amount * t * t * (3.0 - 2.0 * t);
    let px = img[i];
    img[i] = vec4<f32>(px.x * k, px.y * k, px.z * k, px.w);
}

fn cbcr(c: vec3<f32>) -> vec2<f32> {
    let y = luma(c);
    return vec2<f32>((c.z - y) / 1.8556, (c.x - y) / 1.5748);
}

@compute @workgroup_size(16, 16)
fn chroma_key(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let tol = pf(0u);
    let soft = pf(1u);
    let spill = pf(2u);
    let key = vec2<f32>(pf(3u), pf(4u));
    let key_len = pf(5u);
    let dom = P.j.x;
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        return;
    }
    let lin = vec3<f32>(px.x / a, px.y / a, px.z / a);
    let c = cbcr(perceptual3(lin));
    let d = sqrt((c.x - key.x) * (c.x - key.x) + (c.y - key.y) * (c.y - key.y)) / (2.0 * key_len);
    let alpha = clamp((d - tol) / soft, 0.0, 1.0);
    var o = lin;
    if spill > 0.0 {
        // Takes the key's dominant channel (usually green) down to the
        // others.
        if dom == 0u {
            let others = max(max(0.0, o.y), o.z);
            if o.x > others {
                o.x -= (o.x - others) * spill;
            }
        } else if dom == 1u {
            let others = max(max(0.0, o.x), o.z);
            if o.y > others {
                o.y -= (o.y - others) * spill;
            }
        } else {
            let others = max(max(0.0, o.x), o.y);
            if o.z > others {
                o.z -= (o.z - others) * spill;
            }
        }
    }
    let na = a * alpha;
    img[i] = vec4<f32>(o.x * na, o.y * na, o.z * na, na);
}

// Glow's first step: only what is brighter than the threshold stays.
@compute @workgroup_size(16, 16)
fn glow_bright(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let threshold = pf(0u);
    let px = img[i];
    let y = luma(px.xyz);
    let keep = clamp((to_perceptual(y) - threshold) / max(1.0 - threshold, 1e-3), 0.0, 1.0);
    img[i] = vec4<f32>(px.x * keep, px.y * keep, px.z * keep, px.w);
}

// A 2D shadow's or glow's color where the image shows: f0–f2 the color,
// f3 its opacity (the look's intensity times the color's alpha).
@compute @workgroup_size(16, 16)
fn shade(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let a = img[i].w * pf(3u);
    img[i] = vec4<f32>(pf(0u) * a, pf(1u) * a, pf(2u) * a, a);
}

// Toward a color where the image shows (a track's Fade to Color envelope):
// f0–f2 the color, f3 how far (0–1).
@compute @workgroup_size(16, 16)
fn fade_to_color(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let v = pf(3u);
    let px = img[i];
    let c = vec3<f32>(pf(0u), pf(1u), pf(2u));
    img[i] = vec4<f32>(px.xyz * (1.0 - v) + c * (v * px.w), px.w);
}
