// Decoded Y′CbCr planes → linear light, as `bv_color::frame_to_linear_scaled`:
// the planes come one after the other in `planes` (8- or 16-bit samples
// packed in words), reduced `j.x` times on the way (block averages).
//
// Parameters: `sw` × `sh` the picture, `w` × `h` the result;
// `i` = (luma stride, chroma stride, chroma width, chroma height) in
// samples; `j` = (factor, flags, chroma shift x, chroma shift y), flags:
// 1 16-bit samples, 2 GBR (no matrix), 4 other primaries;
// `f[0..4]` luma offset and scale, chroma offset and scale;
// `f[4]`, `f[5]` Kr, Kb; `f[8..17]` the primaries' matrix; `o.xyz` where
// the Y, Cb and Cr planes start (bytes).

@group(0) @binding(1) var<storage, read> planes: array<u32>;
@group(0) @binding(2) var<storage, read> lut: array<f32>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;

const LUT_SIZE: u32 = 4096u;

fn sample(base: u32, stride: u32, x: u32, y: u32) -> u32 {
    let wide = (P.j.y & 1u) != 0u;
    var bps = 1u;
    if wide {
        bps = 2u;
    }
    let b = base + (y * stride + x) * bps;
    let word = planes[b >> 2u];
    let shift = (b & 3u) * 8u;
    if wide {
        return (word >> shift) & 0xffffu;
    }
    return (word >> shift) & 0xffu;
}

fn y_at(x: u32, y: u32) -> u32 {
    return sample(P.o.x, u32(P.i.x), x, y);
}

fn c_at(plane: u32, x: u32, y: u32) -> u32 {
    return sample(P.o[1u + plane], u32(P.i.y), x, y);
}

fn decode(v: f32) -> f32 {
    let x = clamp(v, 0.0, 1.0) * f32(LUT_SIZE);
    let i = min(u32(x), LUT_SIZE - 1u);
    let f = x - f32(i);
    return lut[i] + (lut[i + 1u] - lut[i]) * f;
}

fn convert(yv: f32, cb: f32, cr: f32) -> vec4<f32> {
    let y_off = pf(0u);
    let y_scale = pf(1u);
    let c_off = pf(2u);
    let c_scale = pf(3u);
    var rgb: vec3<f32>;
    if (P.j.y & 2u) != 0u {
        // GBR: Y = G, Cb = B, Cr = R.
        rgb = vec3<f32>((cr - y_off) * y_scale, (yv - y_off) * y_scale, (cb - y_off) * y_scale);
    } else {
        let kr = pf(4u);
        let kb = pf(5u);
        let y = (yv - y_off) * y_scale;
        let u = (cb - c_off) * c_scale;
        let v = (cr - c_off) * c_scale;
        let kg = 1.0 - kr - kb;
        let r = y + 2.0 * (1.0 - kr) * v;
        let b = y + 2.0 * (1.0 - kb) * u;
        let g = (y - kr * r - kb * b) / kg;
        rgb = vec3<f32>(r, g, b);
    }
    var lin = vec3<f32>(decode(rgb.x), decode(rgb.y), decode(rgb.z));
    if (P.j.y & 4u) != 0u {
        lin = vec3<f32>(
            pf(8u) * lin.x + pf(9u) * lin.y + pf(10u) * lin.z,
            pf(11u) * lin.x + pf(12u) * lin.y + pf(13u) * lin.z,
            pf(14u) * lin.x + pf(15u) * lin.y + pf(16u) * lin.z,
        );
    }
    return vec4<f32>(lin, 1.0);
}

// A chroma sample interpolated at full size (co-sited horizontally,
// between the rows vertically in 4:2:0).
fn upsampled(plane: u32, x: u32, y: u32) -> f32 {
    let sx = P.j.z;
    let sy = P.j.w;
    let cw = u32(P.i.z);
    let ch = u32(P.i.w);
    var j0: u32;
    var j1: u32;
    var wy = 0.0;
    if sy == 1u {
        let fy = max(f32(y) * 0.5 - 0.25, 0.0);
        j0 = min(u32(fy), ch - 1u);
        j1 = min(j0 + 1u, ch - 1u);
        wy = fy - f32(j0);
    } else {
        j0 = min(y, ch - 1u);
        j1 = j0;
    }
    var i0: u32;
    var i1: u32;
    var wx = 0.0;
    if sx == 1u {
        i0 = min(x >> 1u, cw - 1u);
        i1 = min(i0 + 1u, cw - 1u);
        if (x & 1u) == 1u {
            wx = 0.5;
        }
    } else {
        i0 = min(x, cw - 1u);
        i1 = i0;
    }
    let a = f32(c_at(plane, i0, j0));
    let b = f32(c_at(plane, i1, j0));
    let c = f32(c_at(plane, i0, j1));
    let d = f32(c_at(plane, i1, j1));
    let top = a + (b - a) * wx;
    let bottom = c + (d - c) * wx;
    return top + (bottom - top) * wy;
}

// The average of a block of a plane (`plane` 3 is luma).
fn average(plane: u32, x0: u32, y0: u32, bw: u32, bh: u32) -> f32 {
    var pw = u32(P.i.z);
    var ph = u32(P.i.w);
    if plane == 3u {
        pw = P.sw;
        ph = P.sh;
    }
    let x1 = max(min(x0 + bw, pw), x0 + 1u);
    let y1 = max(min(y0 + bh, ph), y0 + 1u);
    var acc = 0u;
    for (var y = min(y0, ph - 1u); y < min(y1, ph); y++) {
        for (var x = min(x0, pw - 1u); x < min(x1, pw); x++) {
            if plane == 3u {
                acc += y_at(x, y);
            } else {
                acc += c_at(plane, x, y);
            }
        }
    }
    let n = max((min(x1, pw) - min(x0, pw - 1u)) * (min(y1, ph) - min(y0, ph - 1u)), 1u);
    return f32(acc) / f32(n);
}

@compute @workgroup_size(16, 16)
fn yuv_to_linear(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let k = P.j.x;
    var px: vec4<f32>;
    if k == 1u {
        px = convert(f32(y_at(id.x, id.y)), upsampled(0u, id.x, id.y), upsampled(1u, id.x, id.y));
    } else {
        let x0 = id.x * k;
        let y0 = id.y * k;
        let yv = average(3u, x0, y0, k, k);
        let cx = x0 >> P.j.z;
        let cy = y0 >> P.j.w;
        let cw = max(k >> P.j.z, 1u);
        let ch = max(k >> P.j.w, 1u);
        px = convert(yv, average(0u, cx, cy, cw, ch), average(1u, cx, cy, cw, ch));
    }
    dst[id.y * P.w + id.x] = px;
}
