// Kernels that read one image (`src`, `sw` × `sh`) and write another
// (`dst`, `w` × `h`).

@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;

fn out_index(id: vec3<u32>) -> i32 {
    if id.x >= P.w || id.y >= P.h {
        return -1;
    }
    return i32(id.y * P.w + id.x);
}

// The source placed at (`i.x`, `i.y`) without resampling; transparent
// around it.
@compute @workgroup_size(16, 16)
fn copy_into(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x) - P.i.x;
    let y = i32(id.y) - P.i.y;
    if x >= 0 && y >= 0 && x < i32(P.sw) && y < i32(P.sh) {
        dst[o] = src[u32(y) * P.sw + u32(x)];
    } else {
        dst[o] = vec4<f32>(0.0);
    }
}

// Source pixel (x, y); outside the image or the crop (`f[0..4]`) it is
// transparent.
fn texel(x: i32, y: i32) -> vec4<f32> {
    let xf = f32(x) + 0.5;
    let yf = f32(y) + 0.5;
    if x < 0 || y < 0 || x >= i32(P.sw) || y >= i32(P.sh) || xf < pf(0u) || xf > pf(2u) || yf < pf(1u) || yf > pf(3u) {
        return vec4<f32>(0.0);
    }
    return src[u32(y) * P.sw + u32(x)];
}

fn bilinear(u: f32, v: f32) -> vec4<f32> {
    let x0 = floor(u);
    let y0 = floor(v);
    let fx = u - x0;
    let fy = v - y0;
    let xi = i32(x0);
    let yi = i32(y0);
    let a = texel(xi, yi);
    let b = texel(xi + 1, yi);
    let c = texel(xi, yi + 1);
    let d = texel(xi + 1, yi + 1);
    let top = vec4<f32>(a.x + (b.x - a.x) * fx, a.y + (b.y - a.y) * fx, a.z + (b.z - a.z) * fx, a.w + (b.w - a.w) * fx);
    let bottom = vec4<f32>(c.x + (d.x - c.x) * fx, c.y + (d.y - c.y) * fx, c.z + (d.z - c.z) * fx, c.w + (d.w - c.w) * fx);
    return vec4<f32>(
        top.x + (bottom.x - top.x) * fy,
        top.y + (bottom.y - top.y) * fy,
        top.z + (bottom.z - top.z) * fy,
        top.w + (bottom.w - top.w) * fy,
    );
}

fn catmull_rom(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return vec4<f32>(
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    );
}

fn bicubic(u: f32, v: f32) -> vec4<f32> {
    let x0 = floor(u);
    let y0 = floor(v);
    let wx = catmull_rom(u - x0);
    let wy = catmull_rom(v - y0);
    let xi = i32(x0);
    let yi = i32(y0);
    var o = vec4<f32>(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let p = texel(xi - 1 + i, yi - 1 + j);
            let k = wx[i] * wy[j];
            o = vec4<f32>(o.x + p.x * k, o.y + p.y * k, o.z + p.z * k, o.w + p.w * k);
        }
    }
    // The negative lobes may overshoot: keep a valid premultiplied pixel.
    return vec4<f32>(max(o.x, 0.0), max(o.y, 0.0), max(o.z, 0.0), clamp(o.w, 0.0, 1.0));
}

// The source framed (position, scale, rotation, flips, crop) and
// resampled: `j.x` 0 nearest, 1 bilinear, 2 bicubic.
@compute @workgroup_size(16, 16)
fn place(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let center_x = pf(4u);
    let center_y = pf(5u);
    let sin_a = pf(6u);
    let cos_a = pf(7u);
    let to_src_x = pf(8u);
    let to_src_y = pf(9u);
    let flip_x = pf(10u);
    let flip_y = pf(11u);
    let sw = f32(P.sw);
    let sh = f32(P.sh);
    let dx = f32(id.x) + 0.5 - center_x;
    let dy = f32(id.y) + 0.5 - center_y;
    // Undo the rotation.
    let rx = dx * cos_a - dy * sin_a;
    let ry = dx * sin_a + dy * cos_a;
    let u = flip_x * rx * to_src_x + sw / 2.0 - 0.5;
    let v = flip_y * ry * to_src_y + sh / 2.0 - 0.5;
    if u < -1.0 || v < -1.0 || u > sw || v > sh {
        dst[o] = vec4<f32>(0.0);
        return;
    }
    switch P.j.x {
        case 0u: {
            dst[o] = texel(i32(rround(u)), i32(rround(v)));
        }
        case 2u: {
            dst[o] = bicubic(u, v);
        }
        default: {
            dst[o] = bilinear(u, v);
        }
    }
}

// Half the size, each pixel the average of 2 × 2.
@compute @workgroup_size(16, 16)
fn half_size(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let xs = vec2<u32>(min(2u * id.x, P.sw - 1u), min(2u * id.x + 1u, P.sw - 1u));
    let ys = vec2<u32>(min(2u * id.y, P.sh - 1u), min(2u * id.y + 1u, P.sh - 1u));
    var acc = vec4<f32>(0.0);
    for (var j = 0u; j < 2u; j++) {
        for (var i = 0u; i < 2u; i++) {
            let p = src[ys[j] * P.sw + xs[i]];
            acc = vec4<f32>(acc.x + p.x * 0.25, acc.y + p.y * 0.25, acc.z + p.z * 0.25, acc.w + p.w * 0.25);
        }
    }
    dst[o] = acc;
}

// `dst` = `src` × `f[0]` over `dst`.
@compute @workgroup_size(16, 16)
fn composite(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let k = pf(0u);
    let l = src[o];
    dst[o] = over(vec4<f32>(l.x * k, l.y * k, l.z * k, l.w * k), dst[o]);
}

// Output rotation: `j.x` 1 clockwise, 2 counterclockwise, 3 half a turn.
@compute @workgroup_size(16, 16)
fn rotate(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    var sx: u32;
    var sy: u32;
    switch P.j.x {
        case 1u: {
            sx = id.y;
            sy = P.sh - 1u - id.x;
        }
        case 2u: {
            sx = P.sw - 1u - id.y;
            sy = id.x;
        }
        default: {
            sx = P.sw - 1u - id.x;
            sy = P.sh - 1u - id.y;
        }
    }
    dst[o] = src[sy * P.sw + sx];
}

// The compositing modes, as the model's `CompositeMode::blend` (`j.x`:
// the mode's index in `CompositeMode::ALL`): `src` at its level `f[0]`
// combined with `dst`, premultiplied colors.
fn straight(c: f32, a: f32) -> f32 {
    if a > 0.0 {
        return c / a;
    }
    return 0.0;
}

// A separable mode's color of `cb` (below) and `cs` (above), straight.
fn mix_mode(mode: u32, cb: f32, cs: f32) -> f32 {
    if mode == 5u {
        return cb + cs - cb * cs;
    }
    if mode == 6u || mode == 7u {
        // Overlay is Hard Light with the two swapped.
        var b = cb;
        var s = cs;
        if mode == 6u {
            b = cs;
            s = cb;
        }
        if s <= 0.5 {
            return b * 2.0 * s;
        }
        let t = 2.0 * s - 1.0;
        return b + t - b * t;
    }
    if mode == 8u {
        if cb <= 0.0 {
            return 0.0;
        }
        if cs >= 1.0 {
            return 1.0;
        }
        return min(cb / (1.0 - cs), 1.0);
    }
    if mode == 9u {
        if cb >= 1.0 {
            return 1.0;
        }
        if cs <= 0.0 {
            return 0.0;
        }
        return 1.0 - min((1.0 - cb) / cs, 1.0);
    }
    if mode == 10u {
        return min(cb, cs);
    }
    if mode == 11u {
        return max(cb, cs);
    }
    if mode == 12u {
        return abs(cb - cs);
    }
    if mode == 13u {
        return (cb - cs) * (cb - cs);
    }
    return cs;
}

@compute @workgroup_size(16, 16)
fn blend(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let d = dst[o];
    let mode = P.j.x;
    // An adjustment track: `dst` moved `f[0]` of the way to `src`.
    if mode == 100u {
        dst[o] = d + (src[o] - d) * pf(0u);
        return;
    }
    // Pan/Crop masks: `dst` as far as `src`'s alpha keeps it, or what it
    // leaves.
    if mode == 101u {
        dst[o] = d * src[o].w;
        return;
    }
    if mode == 102u {
        dst[o] = d * (1.0 - src[o].w);
        return;
    }
    let s = src[o] * pf(0u);
    var r = s + d * (1.0 - s.w);
    if mode == 2u {
        r = vec4<f32>(s.xyz + d.xyz, min(s.w + d.w, 1.0));
    } else if mode == 3u {
        r = vec4<f32>(max(d.xyz - s.xyz, vec3<f32>(0.0)), d.w);
    } else if mode == 4u {
        r = d * (1.0 - s.w);
    } else if mode == 0u {
        let m = vec3<f32>(1.0 - s.w) + s.xyz;
        let luma = 0.2126 * s.x + 0.7152 * s.y + 0.0722 * s.z;
        r = vec4<f32>(d.xyz * m, d.w * (1.0 - s.w + luma));
    } else if mode >= 5u {
        let cs = vec3<f32>(straight(s.x, s.w), straight(s.y, s.w), straight(s.z, s.w));
        let cb = vec3<f32>(straight(d.x, d.w), straight(d.y, d.w), straight(d.z, d.w));
        let b = vec3<f32>(
            mix_mode(mode, cb.x, cs.x),
            mix_mode(mode, cb.y, cs.y),
            mix_mode(mode, cb.z, cs.z)
        );
        let rgb = s.xyz * (1.0 - d.w) + d.xyz * (1.0 - s.w) + s.w * d.w * b;
        r = vec4<f32>(rgb, s.w + d.w * (1.0 - s.w));
    }
    dst[o] = r;
}

// Reduce Interlace Flicker (an event's switch): each line half itself and
// a quarter of each line next to it, the edge lines held (as `bv_render`'s
// `flicker_filter`).
@compute @workgroup_size(16, 16)
fn flicker_filter(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let up = select(id.y - 1u, 0u, id.y == 0u);
    let down = min(id.y + 1u, P.h - 1u);
    let a = src[up * P.w + id.x];
    let b = src[id.y * P.w + id.x];
    let c = src[down * P.w + id.x];
    dst[o] = 0.25 * a + 0.5 * b + 0.25 * c;
}

// Interpolate (the Deinterlace method): the lines of the field kept (`j.x`:
// 0 the upper one, the even lines; 1 the lower) as they are, the others
// the mean of the lines above and below (as `bv_render`'s
// `interpolate_fields`).
@compute @workgroup_size(16, 16)
fn interpolate_fields(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let b = src[id.y * P.w + id.x];
    if P.h < 2u || (id.y % 2u) == P.j.x {
        dst[o] = b;
        return;
    }
    let up = select(id.y - 1u, 1u, id.y == 0u);
    let down = select(id.y + 1u, id.y - 1u, id.y + 1u >= P.h);
    let mean = 0.5 * src[up * P.w + id.x] + 0.5 * src[down * P.w + id.x];
    if P.j.y == 0u {
        dst[o] = mean;
        return;
    }
    // Smart adaptive: toward the mean by how far the line's luma stands
    // out from it (`f.x`, `f.y`: the thresholds).
    let w = vec3<f32>(0.2126, 0.7152, 0.0722);
    let d = abs(dot(b.xyz, w) - dot(mean.xyz, w));
    let m = clamp((d - P.f[0].x) / (P.f[0].y - P.f[0].x), 0.0, 1.0);
    dst[o] = b + (mean - b) * m;
}

// Mirror: `j.x` horizontally, `j.y` vertically.
@compute @workgroup_size(16, 16)
fn mirror(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    var sx = id.x;
    var sy = id.y;
    if P.j.x != 0u {
        sx = P.w - 1u - id.x;
    }
    if P.j.y != 0u {
        sy = P.h - 1u - id.y;
    }
    dst[o] = src[sy * P.w + sx];
}

// The pixel standing for `i` on a line `n` long past its ends: the edge
// pixel held, or (`j.y` 1) the line reflected, its edge pixels repeated.
fn box_edge(i: i32, n: i32) -> i32 {
    if P.j.y == 0u {
        return clamp(i, 0, n - 1);
    }
    var m = i % (2 * n);
    if m < 0 {
        m = m + 2 * n;
    }
    if m >= n {
        m = 2 * n - 1 - m;
    }
    return m;
}

// A box filter of radius `j.x` along each row (one invocation a row),
// running sum as the processor's; past the ends as `box_edge` says.
@compute @workgroup_size(64)
fn box_h(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = id.x;
    if y >= P.h {
        return;
    }
    let w = i32(P.w);
    let r = i32(P.j.x);
    let inv = pf(0u);
    let base = y * P.w;
    var acc = vec4<f32>(0.0);
    for (var i = -r; i <= r; i++) {
        acc += src[base + u32(box_edge(i, w))];
    }
    for (var x = 0; x < w; x++) {
        dst[base + u32(x)] = vec4<f32>(acc.x * inv, acc.y * inv, acc.z * inv, acc.w * inv);
        let add = src[base + u32(box_edge(x + r + 1, w))];
        let sub = src[base + u32(box_edge(x - r, w))];
        acc += add - sub;
    }
}

// The same down each column (one invocation a column).
@compute @workgroup_size(64)
fn box_v(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x;
    if x >= P.w {
        return;
    }
    let h = i32(P.h);
    let r = i32(P.j.x);
    let inv = pf(0u);
    var acc = vec4<f32>(0.0);
    for (var i = -r; i <= r; i++) {
        acc += src[u32(box_edge(i, h)) * P.w + x];
    }
    for (var y = 0; y < h; y++) {
        dst[u32(y) * P.w + x] = vec4<f32>(acc.x * inv, acc.y * inv, acc.z * inv, acc.w * inv);
        let add = src[u32(box_edge(y + r + 1, h)) * P.w + x];
        let sub = src[u32(box_edge(y - r, h)) * P.w + x];
        acc += add - sub;
    }
}

// Pixelate's first step: the average of each `i.x`-pixel block of the
// image (`sw` × `sh`); `dst` holds one pixel a block.
@compute @workgroup_size(16, 16)
fn block_average(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let block = u32(P.i.x);
    let x0 = id.x * block;
    let y0 = id.y * block;
    let x1 = min(x0 + block, P.sw);
    let y1 = min(y0 + block, P.sh);
    var acc = vec4<f32>(0.0);
    for (var y = y0; y < y1; y++) {
        for (var x = x0; x < x1; x++) {
            acc += src[y * P.sw + x];
        }
    }
    let n = f32((y1 - y0) * (x1 - x0));
    dst[o] = vec4<f32>(acc.x / n, acc.y / n, acc.z / n, acc.w / n);
}

// Pixelate's second step: each pixel takes its block's average (`src`
// has one pixel a block, `sw` wide).
@compute @workgroup_size(16, 16)
fn block_fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let block = u32(P.i.x);
    dst[o] = src[(id.y / block) * P.sw + id.x / block];
}

// Sharpen: `dst` + (`dst` − `src`, the blurred image) × amount.
@compute @workgroup_size(16, 16)
fn sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let amount = pf(0u);
    let s = src[o];
    let px = dst[o];
    dst[o] = vec4<f32>(
        max(px.x + (px.x - s.x) * amount, 0.0),
        max(px.y + (px.y - s.y) * amount, 0.0),
        max(px.z + (px.z - s.z) * amount, 0.0),
        px.w,
    );
}

// Glow: `dst` + `src` (the blurred bright parts) × intensity.
@compute @workgroup_size(16, 16)
fn add_glow(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let intensity = pf(0u);
    let g = src[o];
    let px = dst[o];
    dst[o] = vec4<f32>(px.x + g.x * intensity, px.y + g.y * intensity, px.z + g.z * intensity, px.w);
}
