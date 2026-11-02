// Linear light → 8-bit RGBA for the screen, encoded with a transfer curve
// as `bv_color::linear_to_display_rgba8` (its table in `lut`): one word a
// pixel, R in the low byte.
//
// And linear light → Y′CbCr 4:2:0 for the video encoders, as
// `bv_color::linear_to_frame` and `bv_color::pack_420` make it: NV12 (8
// bits) or P010 (10 bits in the high bits of 16-bit words) — the luma
// rows, then the rows of Cb and Cr side by side. Chroma is sited with the
// even columns, filtered [1 2 1] across and averaged over its two rows.

@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> lut: array<f32>;
@group(0) @binding(3) var<storage, read_write> dst: array<u32>;

const LUT_SIZE: u32 = 8192u;

fn encode(l: f32) -> f32 {
    let top = pf(0u);
    let u = sqrt(clamp(l, 0.0, top) / top);
    let x = u * f32(LUT_SIZE);
    let i = min(u32(x), LUT_SIZE - 1u);
    let f = x - f32(i);
    return lut[i] + (lut[i + 1u] - lut[i]) * f;
}

fn q(v: f32) -> u32 {
    return u32(rround(clamp(v, 0.0, 1.0) * 255.0));
}

@compute @workgroup_size(16, 16)
fn encode_rgba8(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = id.y * P.w + id.x;
    let px = src[i];
    dst[i] = q(encode(px.x)) | (q(encode(px.y)) << 8u) | (q(encode(px.z)) << 16u) | (q(px.w) << 24u);
}

// The parameters of the Y′CbCr kernels: f = [top, kr, kb, luma offset,
// luma range, chroma offset, chroma range, largest value]; j.x = words
// from one row of the planes to the next.

// Pixel (x, y), its column kept in the image.
fn at(x: i32, y: u32) -> vec4<f32> {
    let c = u32(clamp(x, 0, i32(P.w) - 1));
    return src[y * P.w + c];
}

// Y′, Cb and Cr of a pixel, as values of the file (not yet rounded).
fn ycbcr(px: vec4<f32>) -> vec3<f32> {
    let r = clamp(encode(px.x), 0.0, 1.0);
    let g = clamp(encode(px.y), 0.0, 1.0);
    let b = clamp(encode(px.z), 0.0, 1.0);
    let kr = pf(1u);
    let kb = pf(2u);
    let y = kr * r + (1.0 - kr - kb) * g + kb * b;
    let cb = (b - y) / (2.0 * (1.0 - kb));
    let cr = (r - y) / (2.0 * (1.0 - kr));
    return vec3<f32>(y * pf(4u) + pf(3u), cb * pf(6u) + pf(5u), cr * pf(6u) + pf(5u));
}

fn quantize(v: f32) -> u32 {
    return u32(clamp(rround(v), 0.0, pf(7u)));
}

// Cb and Cr at even column `c` from the pixels of columns c − 1, c and
// c + 1 of rows `y0` and `y1` ([1 2 1] across, the rows averaged).
fn chroma(l0: vec3<f32>, m0: vec3<f32>, r0: vec3<f32>, l1: vec3<f32>, m1: vec3<f32>, r1: vec3<f32>) -> vec2<f32> {
    let a = 0.25 * l0.yz + 0.5 * m0.yz + 0.25 * r0.yz;
    let b = 0.25 * l1.yz + 0.5 * m1.yz + 0.25 * r1.yz;
    return (a + b) / 2.0;
}

// NV12: an invocation for four columns and two rows (one word of each
// luma row, one of chroma: Cb, Cr, Cb, Cr). Written out, without arrays or
// loops, for Direct3D's old compiler.
@compute @workgroup_size(16, 16)
fn encode_nv12(@builtin(global_invocation_id) id: vec3<u32>) {
    let x0 = id.x * 4u;
    let y0 = id.y * 2u;
    if x0 >= P.w || y0 >= P.h {
        return;
    }
    let y1 = min(y0 + 1u, P.h - 1u);
    let x = i32(x0);
    // Columns x0 − 1 … x0 + 3 of both rows.
    let a0 = ycbcr(at(x - 1, y0));
    let a1 = ycbcr(at(x, y0));
    let a2 = ycbcr(at(x + 1, y0));
    let a3 = ycbcr(at(x + 2, y0));
    let a4 = ycbcr(at(x + 3, y0));
    let b0 = ycbcr(at(x - 1, y1));
    let b1 = ycbcr(at(x, y1));
    let b2 = ycbcr(at(x + 1, y1));
    let b3 = ycbcr(at(x + 2, y1));
    let b4 = ycbcr(at(x + 3, y1));
    // The columns past the image's edge stay 0 (the rows' padding).
    let in1 = x0 + 1u < P.w;
    let in2 = x0 + 2u < P.w;
    let in3 = x0 + 3u < P.w;
    var top = quantize(a1.x);
    var bottom = quantize(b1.x);
    if in1 {
        top = top | (quantize(a2.x) << 8u);
        bottom = bottom | (quantize(b2.x) << 8u);
    }
    if in2 {
        top = top | (quantize(a3.x) << 16u);
        bottom = bottom | (quantize(b3.x) << 16u);
    }
    if in3 {
        top = top | (quantize(a4.x) << 24u);
        bottom = bottom | (quantize(b4.x) << 24u);
    }
    let stride = P.j.x;
    let word = id.x;
    dst[y0 * stride + word] = top;
    if y0 + 1u < P.h {
        dst[(y0 + 1u) * stride + word] = bottom;
    }
    let c0 = chroma(a0, a1, a2, b0, b1, b2);
    var uv = quantize(c0.x) | (quantize(c0.y) << 8u);
    if in2 {
        let c1 = chroma(a2, a3, a4, b2, b3, b4);
        uv = uv | (quantize(c1.x) << 16u) | (quantize(c1.y) << 24u);
    }
    dst[P.h * stride + id.y * stride + word] = uv;
}

// P010: an invocation for two columns and two rows (one word of each
// luma row, one of chroma: Cb, Cr).
@compute @workgroup_size(16, 16)
fn encode_p010(@builtin(global_invocation_id) id: vec3<u32>) {
    let x0 = id.x * 2u;
    let y0 = id.y * 2u;
    if x0 >= P.w || y0 >= P.h {
        return;
    }
    let y1 = min(y0 + 1u, P.h - 1u);
    let x = i32(x0);
    let a0 = ycbcr(at(x - 1, y0));
    let a1 = ycbcr(at(x, y0));
    let a2 = ycbcr(at(x + 1, y0));
    let b0 = ycbcr(at(x - 1, y1));
    let b1 = ycbcr(at(x, y1));
    let b2 = ycbcr(at(x + 1, y1));
    let stride = P.j.x;
    let word = id.x;
    dst[y0 * stride + word] = (quantize(a1.x) << 6u) | (quantize(a2.x) << 22u);
    if y0 + 1u < P.h {
        dst[(y0 + 1u) * stride + word] = (quantize(b1.x) << 6u) | (quantize(b2.x) << 22u);
    }
    let c = chroma(a0, a1, a2, b0, b1, b2);
    dst[P.h * stride + id.y * stride + word] = (quantize(c.x) << 6u) | (quantize(c.y) << 22u);
}
