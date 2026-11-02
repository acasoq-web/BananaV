
// Pictures convolved through the frequency domain (`bv-effects`' `fft`,
// Defocus's iris): grids `w` × `h` of two complex numbers a value — red
// and green, blue and alpha —, transformed by Stockham's radix-2 steps.

// A ramp of Starburst's soft edges: 1 within `a` of the middle, gone
// `b` past it.
fn fft_ramp(d: f32, a: f32, b: f32) -> f32 {
    return clamp((a - abs(d)) / (2.0 * b) + 0.5, 0.0, 1.0);
}

// The pixel standing for `i` on a line `n` long: held at the ends, or
// with i.y reflected about the end pixels (… c b | a b c …), as OpenCV's
// filters go on.
fn fft_edge(i: i32, n: i32) -> i32 {
    if P.i.y == 0 || n == 1 {
        return clamp(i, 0, n - 1);
    }
    let period = 2 * n - 2;
    var m = i % period;
    if m < 0 {
        m = m + period;
    }
    return select(period - m, m, m < n);
}

// A pixel of the picture in `src` (`sw` × `sh`, rows from the top), past
// its edges as `fft_edge` says; with j.w times the soft edges' mask
// (middle f3, f4, half sizes f5, f6, ramps' half width f7).
fn fft_at(x: i32, y: i32) -> vec4<f32> {
    let cx = fft_edge(x, i32(P.sw));
    let cy = fft_edge(y, i32(P.sh));
    var c = src[u32(cy) * P.sw + u32(cx)];
    if P.j.w != 0u {
        let m = fft_ramp(f32(cx) - pf(3u), pf(5u), pf(7u))
            * fft_ramp(f32(cy) - pf(4u), pf(6u), pf(7u));
        c = c * m;
    }
    return c;
}

// The picture on the grid, i.x in from its corner, its edges held; with
// j.y each pixel the binomial 3 × 3 of its neighbours first, with j.z each
// channel under f2 gone (Starburst's lights), with j.x the colors past f0
// brightened by f1 for each unit past it (the bloom).
@compute @workgroup_size(16, 16)
fn fft_image(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x) - P.i.x;
    let y = i32(id.y) - P.i.x;
    var c = fft_at(x, y);
    if P.j.y != 0u {
        let corners = fft_at(x - 1, y - 1) + fft_at(x + 1, y - 1) + fft_at(x - 1, y + 1)
            + fft_at(x + 1, y + 1);
        let sides = fft_at(x, y - 1) + fft_at(x - 1, y) + fft_at(x + 1, y) + fft_at(x, y + 1);
        c = (corners + 2.0 * sides + 4.0 * c) / 16.0;
    }
    if P.j.z != 0u {
        c = select(c, vec4<f32>(0.0), c < vec4<f32>(pf(2u)));
    }
    if P.j.x != 0u {
        let t = vec3<f32>(pf(0u));
        let rgb = select(c.xyz, c.xyz + (c.xyz - t) * pf(1u), c.xyz > t);
        c = vec4<f32>(rgb, c.w);
    }
    dst[o] = c;
}

// The kernel's `sw` × `sw` cells (two complex numbers each, rows from the
// top) on the grid, its middle at the grid's corner (i.x its reach).
@compute @workgroup_size(16, 16)
fn fft_kernel(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let r = u32(P.i.x);
    let kx = (id.x + r) % P.w;
    let ky = (id.y + r) % P.h;
    var v = vec4<f32>(0.0);
    if kx < P.sw && ky < P.sw {
        v = src[ky * P.sw + kx];
    }
    dst[o] = v;
}

// One of Stockham's radix-2 steps along the rows (j.x 0) or the columns:
// butterflies of half size i.x, half a line apart, f0 −1 forward and 1
// back; one invocation a butterfly.
@compute @workgroup_size(16, 16)
fn fft_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    var n = P.w;
    var lane = id.y;
    var j = id.x;
    var lanes = P.h;
    var stride = 1u;
    var base = id.y * P.w;
    if P.j.x != 0u {
        n = P.h;
        lane = id.x;
        j = id.y;
        lanes = P.w;
        stride = P.w;
        base = id.x;
    }
    let gap = n / 2u;
    if j >= gap || lane >= lanes {
        return;
    }
    let p = u32(P.i.x);
    let k = j & (p - 1u);
    let angle = pf(0u) * 3.1415927 * f32(k) / f32(p);
    let c = cos(angle);
    let s = sin(angle);
    let a = src[base + j * stride];
    let b = src[base + (j + gap) * stride];
    let wb = vec4<f32>(
        c * b.x - s * b.y,
        c * b.y + s * b.x,
        c * b.z - s * b.w,
        c * b.w + s * b.z,
    );
    let o = (j - k) * 2u + k;
    dst[base + o * stride] = a + wb;
    dst[base + (o + p) * stride] = a - wb;
}

// The picture back from the grid (`sw` wide), i.x in from its corner,
// times f0 (one over the grid's size), each channel held within 0–1.
@compute @workgroup_size(16, 16)
fn fft_crop(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let r = u32(P.i.x);
    let v = src[(id.y + r) * P.sw + id.x + r] * pf(0u);
    dst[o] = clamp(v, vec4<f32>(0.0), vec4<f32>(1.0));
}
