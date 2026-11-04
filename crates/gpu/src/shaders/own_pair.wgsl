// BananaV's own effects that read one buffer and write another
// (`bv-effects`'s plug-ins, the same math as their renders): Color Curves'
// tables, the blurred alphas Layer Dimensionality works from, Pixelate's
// cells, LUT Filter's look-up tables, White Balance's levels, Picture In
// Picture's picture, Unsharp Mask's blurred one and Timecode's writing.

// A Color Curves table's value for `x` (0–1): `c` 0 red, 1 green, 2 blue,
// in its entries' x, y and z; straight lines between its `sw` entries.
fn curve_entry(k: u32, c: u32) -> f32 {
    let e = src[k];
    if c == 0u {
        return e.x;
    }
    if c == 1u {
        return e.y;
    }
    return e.z;
}

fn curve_value(x: f32, c: u32) -> f32 {
    let last = P.sw - 1u;
    let f = clamp(x, 0.0, 1.0) * f32(last);
    let k = min(u32(f), last - 1u);
    let t = f - f32(k);
    let a = curve_entry(k, c);
    return a + (curve_entry(k + 1u, c) - a) * t;
}

// Color Curves: each straight channel of `dst` (`w` × `h`, changed in
// place) through its curve's table in `src` (`sw` entries).
@compute @workgroup_size(16, 16)
fn curves(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let a = px.w;
    if a <= 0.0 {
        dst[o] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    dst[o] = vec4<f32>(
        curve_value(px.x / a, 0u) * a,
        curve_value(px.y / a, 1u) * a,
        curve_value(px.z / a, 2u) * a,
        a,
    );
}

// Layer Dimensionality: the picture's alpha (`src`, `sw` × `sh`) in each
// channel of a field `w` × `h` around it, `i.x` pixels of nothing on every
// side.
@compute @workgroup_size(16, 16)
fn field_alpha(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x) - P.i.x;
    let y = i32(id.y) - P.i.x;
    var a = 0.0;
    if x >= 0 && y >= 0 && x < i32(P.sw) && y < i32(P.sh) {
        a = src[u32(y) * P.sw + u32(x)].w;
    }
    dst[o] = vec4<f32>(a);
}

// A field's pixel `k` of a line (`n` long, from `base`, `step` apart):
// nothing beyond its ends.
fn field_at(base: u32, step: u32, k: i32, n: i32) -> vec4<f32> {
    if k < 0 || k >= n {
        return vec4<f32>(0.0);
    }
    return src[base + u32(k) * step];
}

// A box filter along a line of the field, each channel with its own radius
// (`i`; 0: as it is) and weight (`f` 0–3), as Layer Dimensionality's
// blur on the processor: a running sum, nothing beyond the ends.
fn field_box(base: u32, step: u32, n: i32) {
    let r = P.i;
    let norm = vec4<f32>(pf(0u), pf(1u), pf(2u), pf(3u));
    let most = max(max(r.x, r.y), max(r.z, r.w));
    var sum = vec4<f32>(0.0);
    for (var k = 0; k <= min(most, n - 1); k++) {
        let v = src[base + u32(k) * step];
        if k <= r.x {
            sum.x += v.x;
        }
        if k <= r.y {
            sum.y += v.y;
        }
        if k <= r.z {
            sum.z += v.z;
        }
        if k <= r.w {
            sum.w += v.w;
        }
    }
    for (var x = 0; x < n; x++) {
        let here = src[base + u32(x) * step];
        var out = sum * norm;
        // A channel without a blur stays as it is.
        if r.x == 0 {
            out.x = here.x;
        }
        if r.y == 0 {
            out.y = here.y;
        }
        if r.z == 0 {
            out.z = here.z;
        }
        if r.w == 0 {
            out.w = here.w;
        }
        dst[base + u32(x) * step] = out;
        var add = vec4<f32>(0.0);
        add.x = field_at(base, step, x + r.x + 1, n).x;
        add.y = field_at(base, step, x + r.y + 1, n).y;
        add.z = field_at(base, step, x + r.z + 1, n).z;
        add.w = field_at(base, step, x + r.w + 1, n).w;
        var sub = vec4<f32>(0.0);
        if x >= r.x {
            sub.x = field_at(base, step, x - r.x, n).x;
        }
        if x >= r.y {
            sub.y = field_at(base, step, x - r.y, n).y;
        }
        if x >= r.z {
            sub.z = field_at(base, step, x - r.z, n).z;
        }
        if x >= r.w {
            sub.w = field_at(base, step, x - r.w, n).w;
        }
        sum += add;
        sum -= sub;
    }
}

// The field's rows (one invocation a row: `w` × `h`, `src` and `dst` the
// same size).
@compute @workgroup_size(64)
fn field_box_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.h {
        return;
    }
    field_box(id.x * P.w, 1u, i32(P.w));
}

// Its columns (one invocation a column).
@compute @workgroup_size(64)
fn field_box_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w {
        return;
    }
    field_box(id.x, P.w, i32(P.h));
}

// Pixelate (`bv-effects`' `Cells`, VEGAS's): along a line whose cells are
// `step` 16,384ths of a pixel apart, cell `c`'s first pixel, and the cell
// of `n` pixel `d` is in (the last running to the line's end).
fn pixel_first(c: u32, step: u32) -> u32 {
    return (c * step) >> 14u;
}

fn pixel_cell(d: u32, step: u32, n: u32) -> u32 {
    return min(((d + 1u) * 16384u - 1u) / step, n - 1u);
}

// Pixelate's first step: each row's average over each cell across — the
// step's whole pixels from its first (`src` the picture, `sw` × `sh`;
// `dst` a pixel a cell across and a row, `w` × `h`; i.x the step across).
@compute @workgroup_size(16, 16)
fn pixel_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let step = u32(P.i.x);
    let x0 = pixel_first(id.x, step);
    let n = max(step >> 14u, 1u);
    var acc = vec4<f32>(0.0);
    for (var x = x0; x < x0 + n; x++) {
        acc += src[id.y * P.sw + x];
    }
    dst[o] = acc / f32(n);
}

// Its second: each cell's average, from its rows' — up from the bottom
// row, the step's whole rows from its first (`src` the first step's, `sw`
// × `sh`; `dst` a pixel a cell, `w` × `h`, its rows from the top; i.y the
// step up).
@compute @workgroup_size(16, 16)
fn pixel_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let step = u32(P.i.y);
    let y0 = pixel_first(P.h - 1u - id.y, step);
    let n = max(step >> 14u, 1u);
    var acc = vec4<f32>(0.0);
    for (var y = y0; y < y0 + n; y++) {
        acc += src[(P.sh - 1u - y) * P.sw + id.x];
    }
    dst[o] = acc / f32(n);
}

// Its last: each pixel its cell's average (`src` the cells, `sw` × `sh`,
// rows from the top; `dst` the picture, `w` × `h`; i.x, i.y the steps).
@compute @workgroup_size(16, 16)
fn pixel_fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let cx = pixel_cell(id.x, u32(P.i.x), P.sw);
    let cy = P.sh - 1u - pixel_cell(P.h - 1u - id.y, u32(P.i.y), P.sh);
    dst[o] = src[cy * P.sw + cx];
}

// LUT Filter: a table's 1D shaper (`j.x` entries of `src`, over f 0–2 to
// f 3–5) and its lattice (`j.y`³ entries after them, red changing
// fastest, over f 6–8 to f 9–11), as `bv-lut` applies them; `j.z` reads
// it by tetrahedra, `j.w` holds the colors within 0–1 before and after
// (Color Grading's LUTs).
fn lut_unit(v: vec3<f32>, lo: vec3<f32>, hi: vec3<f32>) -> vec3<f32> {
    let t = (v - lo) / max(hi - lo, vec3<f32>(1e-12));
    return clamp(t, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn lut_shaper(c: vec3<f32>) -> vec3<f32> {
    let n = P.j.x;
    let lo = vec3<f32>(pf(0u), pf(1u), pf(2u));
    let hi = vec3<f32>(pf(3u), pf(4u), pf(5u));
    let t = lut_unit(c, lo, hi) * f32(n - 1u);
    let ir = min(u32(t.x), n - 2u);
    let ig = min(u32(t.y), n - 2u);
    let ib = min(u32(t.z), n - 2u);
    return vec3<f32>(
        src[ir].x + (src[ir + 1u].x - src[ir].x) * (t.x - f32(ir)),
        src[ig].y + (src[ig + 1u].y - src[ig].y) * (t.y - f32(ig)),
        src[ib].z + (src[ib + 1u].z - src[ib].z) * (t.z - f32(ib)),
    );
}

fn lut_at(r: u32, g: u32, b: u32) -> vec3<f32> {
    let n = P.j.y;
    return src[P.j.x + r + n * (g + n * b)].xyz;
}

fn lut_cube(c: vec3<f32>) -> vec3<f32> {
    let n = P.j.y;
    let lo = vec3<f32>(pf(6u), pf(7u), pf(8u));
    let hi = vec3<f32>(pf(9u), pf(10u), pf(11u));
    let t = lut_unit(c, lo, hi) * f32(n - 1u);
    let i = min(vec3<u32>(t), vec3<u32>(n - 2u));
    let f = t - vec3<f32>(i);
    let p000 = lut_at(i.x, i.y, i.z);
    let p111 = lut_at(i.x + 1u, i.y + 1u, i.z + 1u);
    if P.j.z == 0u {
        // Trilinear: along red, then green, then blue.
        let c00 = p000 + (lut_at(i.x + 1u, i.y, i.z) - p000) * f.x;
        let p010 = lut_at(i.x, i.y + 1u, i.z);
        let c10 = p010 + (lut_at(i.x + 1u, i.y + 1u, i.z) - p010) * f.x;
        let p001 = lut_at(i.x, i.y, i.z + 1u);
        let c01 = p001 + (lut_at(i.x + 1u, i.y, i.z + 1u) - p001) * f.x;
        let p011 = lut_at(i.x, i.y + 1u, i.z + 1u);
        let c11 = p011 + (p111 - p011) * f.x;
        let c0 = c00 + (c10 - c00) * f.y;
        let c1 = c01 + (c11 - c01) * f.y;
        return c0 + (c1 - c0) * f.z;
    }
    // Tetrahedral: the tetrahedron around the point, its corners weighed.
    var w = vec4<f32>(0.0);
    var q0 = vec3<f32>(0.0);
    var q1 = vec3<f32>(0.0);
    if f.x > f.y {
        if f.y > f.z {
            w = vec4<f32>(1.0 - f.x, f.x - f.y, f.y - f.z, f.z);
            q0 = lut_at(i.x + 1u, i.y, i.z);
            q1 = lut_at(i.x + 1u, i.y + 1u, i.z);
        } else if f.x > f.z {
            w = vec4<f32>(1.0 - f.x, f.x - f.z, f.z - f.y, f.y);
            q0 = lut_at(i.x + 1u, i.y, i.z);
            q1 = lut_at(i.x + 1u, i.y, i.z + 1u);
        } else {
            w = vec4<f32>(1.0 - f.z, f.z - f.x, f.x - f.y, f.y);
            q0 = lut_at(i.x, i.y, i.z + 1u);
            q1 = lut_at(i.x + 1u, i.y, i.z + 1u);
        }
    } else if f.z > f.y {
        w = vec4<f32>(1.0 - f.z, f.z - f.y, f.y - f.x, f.x);
        q0 = lut_at(i.x, i.y, i.z + 1u);
        q1 = lut_at(i.x, i.y + 1u, i.z + 1u);
    } else if f.z > f.x {
        w = vec4<f32>(1.0 - f.y, f.y - f.z, f.z - f.x, f.x);
        q0 = lut_at(i.x, i.y + 1u, i.z);
        q1 = lut_at(i.x, i.y + 1u, i.z + 1u);
    } else {
        w = vec4<f32>(1.0 - f.y, f.y - f.x, f.x - f.z, f.z);
        q0 = lut_at(i.x, i.y + 1u, i.z);
        q1 = lut_at(i.x + 1u, i.y + 1u, i.z);
    }
    return w.x * p000 + w.y * q0 + w.z * q1 + w.w * p111;
}

// LUT Filter on `dst` (`w` × `h`, changed in place): the straight colors
// through the table, mixed with the picture by f 12.
@compute @workgroup_size(16, 16)
fn lut_filter(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let a = px.w;
    if a <= 0.0 {
        dst[o] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    var c = px.xyz / a;
    if P.j.w != 0u {
        c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    var v = c;
    if P.j.x > 0u {
        v = lut_shaper(v);
    }
    if P.j.y > 0u {
        v = lut_cube(v);
    }
    var out = c + (v - c) * pf(12u);
    if P.j.w != 0u {
        out = clamp(out, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    dst[o] = vec4<f32>(out * a, a);
}

// White Balance's color (f 0–8 a matrix's rows, f 9–11 an offset),
// clipped to 0–1: what Equalize Histogram works on.
fn balanced(c: vec3<f32>) -> vec3<f32> {
    let r = pf(0u) * c.x + pf(1u) * c.y + pf(2u) * c.z + pf(9u);
    let g = pf(3u) * c.x + pf(4u) * c.y + pf(5u) * c.z + pf(10u);
    let b = pf(6u) * c.x + pf(7u) * c.y + pf(8u) * c.z + pf(11u);
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Equalize Histogram's first step: each row's lowest (x 0) or highest
// (x 1) white-balanced color, over its pixels that show (`src` the
// picture, `sw` × `sh`; `dst` 2 × `h`).
@compute @workgroup_size(16, 16)
fn equalize_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    var lo = vec3<f32>(1e30);
    var hi = vec3<f32>(-1e30);
    for (var x = 0u; x < P.sw; x++) {
        let px = src[id.y * P.sw + x];
        if px.w > 0.0 {
            let c = balanced(px.xyz / px.w);
            lo = min(lo, c);
            hi = max(hi, c);
        }
    }
    if id.x == 0u {
        dst[o] = vec4<f32>(lo, 1.0);
    } else {
        dst[o] = vec4<f32>(hi, 1.0);
    }
}

// Its second: the picture's, from its rows' (`src` 2 × `sh`, `dst` 2 × 1).
@compute @workgroup_size(16, 16)
fn equalize_total(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    var lo = vec3<f32>(1e30);
    var hi = vec3<f32>(-1e30);
    for (var y = 0u; y < P.sh; y++) {
        lo = min(lo, src[y * 2u].xyz);
        hi = max(hi, src[y * 2u + 1u].xyz);
    }
    if id.x == 0u {
        dst[o] = vec4<f32>(lo, 1.0);
    } else {
        dst[o] = vec4<f32>(hi, 1.0);
    }
}

// Its last: each pixel white-balanced, each channel stretched from the
// picture's lowest to its highest (one all one value stays), red and green
// held under blue (`src` the levels, 2 × 1; `dst` the picture, `w` × `h`).
fn stretched(v: f32, lo: f32, hi: f32) -> f32 {
    if hi - lo > 1e-6 {
        return clamp((v - lo) / (hi - lo), 0.0, 1.0);
    }
    return v;
}

@compute @workgroup_size(16, 16)
fn equalize_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let a = px.w;
    if a <= 0.0 {
        dst[o] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = balanced(px.xyz / a);
    let lo = src[0].xyz;
    let hi = src[1].xyz;
    let b = stretched(c.z, lo.z, hi.z);
    let r = min(stretched(c.x, lo.x, hi.x), b);
    let g = min(stretched(c.y, lo.y, hi.y), b);
    dst[o] = vec4<f32>(r * a, g * a, b * a, a);
}

// Picture In Picture's pixel of the picture (`src`, `sw` × `sh`, row 0 at
// the top) at `x`, `y`: nothing outside it.
fn pip_pixel(x: i32, y: i32) -> vec4<f32> {
    if x < 0 || y < 0 || x >= i32(P.sw) || y >= i32(P.sh) {
        return vec4<f32>(0.0);
    }
    return src[u32(y) * P.sw + u32(x)];
}

// Its color at `p` (pixels, their middles at .5), from its four nearest
// pixels.
fn pip_sample(p: vec2<f32>) -> vec4<f32> {
    let q = p - vec2<f32>(0.5);
    let f = floor(q);
    let t = q - f;
    let x = i32(f.x);
    let y = i32(f.y);
    let top = mix(pip_pixel(x, y), pip_pixel(x + 1, y), t.x);
    let bottom = mix(pip_pixel(x, y + 1), pip_pixel(x + 1, y + 1), t.x);
    return mix(top, bottom, t.y);
}

// Picture In Picture: each pixel of `dst` (`w` × `h`) the picture's where
// the inverse map (f 0–8, rows; the frame and the picture 0–1 across and
// up) puts it, `j.x` × `j.x` samples averaged, times the opacity (f 9).
@compute @workgroup_size(16, 16)
fn picture_in_picture(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let n = max(P.j.x, 1u);
    var acc = vec4<f32>(0.0);
    for (var j = 0u; j < n; j++) {
        for (var i = 0u; i < n; i++) {
            let ux = (f32(id.x) + (f32(i) + 0.5) / f32(n)) / f32(P.w);
            let uy = 1.0 - (f32(id.y) + (f32(j) + 0.5) / f32(n)) / f32(P.h);
            let w = pf(6u) * ux + pf(7u) * uy + pf(8u);
            if w > 1e-12 {
                let mx = (pf(0u) * ux + pf(1u) * uy + pf(2u)) / w;
                let my = (pf(3u) * ux + pf(4u) * uy + pf(5u)) / w;
                acc += pip_sample(vec2<f32>(mx * f32(P.sw), (1.0 - my) * f32(P.sh)));
            }
        }
    }
    dst[o] = acc / f32(n * n) * pf(9u);
}

// Unsharp Mask: each channel of `dst` (changed in place), alpha too, as
// the premultiplied values stand, moved away from `src`'s, the picture
// blurred, by the amount (f 0) times their difference — unless the result
// is within the threshold (f 1) of the blurred one, which keeps the
// pixel's; unclipped.
@compute @workgroup_size(16, 16)
fn unsharp_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let d = src[o];
    let s = (px - d) * pf(0u) + px;
    dst[o] = select(s, px, abs(s - d) <= vec4<f32>(pf(1u)));
}

// Timecode: `src` (`sw` x `sh`, premultiplied, the processor's drawing of
// the text and its box) over `dst`, its top left at `i.x`, `i.y`.
@compute @workgroup_size(16, 16)
fn overlay(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let sx = i32(id.x) - P.i.x;
    let sy = i32(id.y) - P.i.y;
    if sx < 0 || sy < 0 || sx >= i32(P.sw) || sy >= i32(P.sh) {
        return;
    }
    dst[o] = over(src[u32(sy) * P.sw + u32(sx)], dst[o]);
}

// Color Grading (`grading.rs`' `Grade::apply`, between its LUTs, which
// are passes of their own) on `dst` (`w` × `h`, changed in place): the
// straight colors, held within 0–1 first. `src` holds the settings, a
// row each, then the tables, an entry a row (x):
// 0 switches: White Balance, Levels, the RL wheels by channel, by Y;
// 1 switches: the Color Curves by channel, by Y, Lift; the HSL Curves'
//   count (w);
// 2 Saturation (1: none), Exposure's scale, Brightness, whether the
//   panel's Exposure is on;
// 3 Contrast's scale and center, the panel's Exposure's scale before its
//   table, the HSL's hue;
// 4 the HSL's saturation and luminance, whether each CDL is on;
// 5–7 White Balance's matrix, a row each; 8 Levels (in min, in max, out
// min, out max); 9 Lift; 10–12 the first CDL's slope, offset and power,
// 13–15 the second's;
// 16–21 the HSL Curves in their order: which (x: Hue vs Hue … Luminance
//   vs Saturation), where its table starts (y) and its length (z);
// 22–30 where each table starts (x) and its length (y): the RL wheels'
//   red, green, blue and Y, the Color Curves' red, green, blue and Y, the
//   panel's Exposure's.

const GRADE_WHITE_L: f32 = 95.580666;

fn grade_hold(c: vec3<f32>) -> vec3<f32> {
    return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
}

// A table (`n` entries from `at`) at `v`: straight between its entries,
// `v` held within 0–1; no table changes nothing.
fn grade_lookup(at: u32, n: u32, v: f32) -> f32 {
    if n == 0u {
        return v;
    }
    if !(v > 0.0) {
        return src[at].x;
    }
    if v >= 1.0 {
        return src[at + n - 1u].x;
    }
    let p = v * f32(n - 1u);
    let i = u32(floor(p));
    let f = p - f32(i);
    if i + 1u < n {
        let a = src[at + i].x;
        return a + (src[at + i + 1u].x - a) * f;
    }
    return src[at + i].x;
}

// Table `k` of rows 22–30 at `v`.
fn grade_table(k: u32, v: f32) -> f32 {
    let d = src[22u + k];
    return grade_lookup(u32(d.x), u32(d.y), v);
}

fn grade_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let lo = min(min(c.x, c.y), c.z);
    let hi = max(max(c.x, c.y), c.z);
    let l = (lo + hi) * 0.5;
    if !(l > 0.0) {
        return vec3<f32>(0.0, 0.0, l);
    }
    let d = hi - lo;
    if !(d > 0.0) {
        return vec3<f32>(0.0, d, l);
    }
    var s = d / (lo + hi);
    if l > 0.5 {
        s = d / ((2.0 - hi) - lo);
    }
    let dr = (hi - c.x) / d;
    let dg = (hi - c.y) / d;
    let db = (hi - c.z) / d;
    var h = (3.0 - db) / 6.0;
    if c.x == hi {
        if c.y == lo {
            h = (db + 5.0) / 6.0;
        } else {
            h = (1.0 - dg) / 6.0;
        }
    } else if c.y != hi {
        if c.x == lo {
            h = (dg + 3.0) / 6.0;
        } else {
            h = (5.0 - dr) / 6.0;
        }
    } else if c.z == lo {
        h = (dr + 1.0) / 6.0;
    }
    return vec3<f32>(h, s, l);
}

fn grade_from_hsl(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x - floor(hsl.x);
    let s = hsl.y;
    let l = hsl.z;
    var q = (s + 1.0) * l;
    if l > 0.5 {
        q = (l + s) - l * s;
    }
    if q <= 0.0 {
        return vec3<f32>(0.0);
    }
    let i = trunc(h * 6.0);
    let p = (l + l) - q;
    let f = (h * 6.0 - i) * (q - p);
    let down = q - f;
    let up = f + p;
    let k = u32(i);
    if k == 1u {
        return vec3<f32>(down, q, p);
    }
    if k == 2u {
        return vec3<f32>(p, q, up);
    }
    if k == 3u {
        return vec3<f32>(p, down, q);
    }
    if k == 4u {
        return vec3<f32>(up, p, q);
    }
    if k == 5u {
        return vec3<f32>(q, p, down);
    }
    if k == 0u || k == 6u {
        return vec3<f32>(q, up, p);
    }
    return vec3<f32>(0.0);
}

fn grade_to_hsv(c: vec3<f32>) -> vec3<f32> {
    var p = vec4<f32>(c.y, c.z, 0.0, -1.0 / 3.0);
    if c.y < c.z {
        p = vec4<f32>(c.z, c.y, -1.0, 2.0 / 3.0);
    }
    var q = vec4<f32>(p.x, p.y, p.w, c.x);
    if p.x < c.x {
        q = vec4<f32>(c.x, p.y, p.z, p.x);
    }
    let d = q.x - min(q.w, q.y);
    let e = 1e-10;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn grade_from_hsv(c: vec3<f32>) -> vec3<f32> {
    let f = vec3<f32>(c.x) + vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let x = abs((f - floor(f)) * 6.0 - vec3<f32>(3.0)) - vec3<f32>(1.0);
    let k = clamp(x, vec3<f32>(0.0), vec3<f32>(1.0)) - vec3<f32>(1.0);
    return c.z * (vec3<f32>(1.0) + k * c.y);
}

fn grade_linear(c: f32) -> f32 {
    if c > 0.04045 {
        return pow(abs((c + 0.055) / 1.055), 2.4);
    }
    return c / 12.92;
}

fn grade_lab_f(t: f32) -> f32 {
    if t > 0.008856 {
        return pow(abs(t), 0.3333);
    }
    return 7.787 * t + 16.0 / 116.0;
}

fn grade_to_lab(c: vec3<f32>) -> vec3<f32> {
    let r = grade_linear(c.x);
    let g = grade_linear(c.y);
    let b = grade_linear(c.z);
    let fx = grade_lab_f((r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047);
    let fy = grade_lab_f(r * 0.2126 + g * 0.7152 + b * 0.0722);
    let fz = grade_lab_f((r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883);
    return vec3<f32>(116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz));
}

fn grade_lab_inverse(t: f32) -> f32 {
    let t3 = t * t * t;
    if t3 > 0.008856 {
        return t3;
    }
    return (t - 16.0 / 116.0) / 7.787;
}

fn grade_encode(t: f32) -> f32 {
    if t > 0.0031308 {
        return 1.055 * pow(abs(t), 1.0 / 2.4) - 0.055;
    }
    return 12.92 * t;
}

fn grade_from_lab(lab: vec3<f32>) -> vec3<f32> {
    let fy = (lab.x + 16.0) / 116.0;
    let fx = lab.y / 500.0 + fy;
    let fz = fy - lab.z / 200.0;
    let x = grade_lab_inverse(fx) * 0.95047;
    let y = grade_lab_inverse(fy);
    let z = grade_lab_inverse(fz) * 1.08883;
    return vec3<f32>(
        grade_encode(x * 3.2406 + y * -1.5372 + z * -0.4986),
        grade_encode(x * -0.9689 + y * 1.8758 + z * 0.0415),
        grade_encode(x * 0.0557 + y * -0.204 + z * 1.057),
    );
}

// One HSL Curve (`kind`, its table `n` entries from `at`) on a color: its
// key from the color as it is.
fn grade_hsl_curve(kind: u32, at: u32, n: u32, c: vec3<f32>) -> vec3<f32> {
    if kind == 0u {
        let hsv = grade_to_hsv(c);
        var h = hsv.x - (grade_lookup(at, n, hsv.x) - 0.5);
        if h < 0.0 {
            h += 1.0;
        }
        if h > 1.0 {
            h -= 1.0;
        }
        return grade_from_hsv(vec3<f32>(h, hsv.y, hsv.z));
    }
    let hsl = grade_to_hsl(c);
    if kind == 1u || kind == 3u || kind == 5u {
        var key = hsl.x;
        if kind == 3u {
            key = hsl.y;
        } else if kind == 5u {
            key = grade_to_lab(c).x / GRADE_WHITE_L;
        }
        let z = grade_lookup(at, n, key);
        return grade_from_hsl(vec3<f32>(hsl.x, clamp(hsl.y * 2.0 * z, 0.0, 1.0), hsl.z));
    }
    let lab = grade_to_lab(c);
    var scale = 0.0;
    if kind == 2u {
        let w = grade_lookup(at, n, hsl.x);
        var weight = 0.0;
        if lab.x > 60.0 {
            weight = (-1.0 / 35.6) * lab.x + 2.68;
        } else if lab.x > 30.0 {
            weight = lab.x / 30.0 - 1.0;
        }
        var chroma = min(hsl.y, 2.0 - 2.0 * hsl.z) - 1.0;
        if hsl.z <= 0.5 {
            chroma = min(hsl.y, 2.0 * hsl.z) - 1.0;
        }
        weight *= 1.75 / (1.0 + 18.0 * chroma * chroma);
        scale = 1.0 - (1.0 - (2.25 * w - 0.125)) * weight;
    } else {
        scale = 2.0 * grade_lookup(at, n, hsl.y);
    }
    return grade_from_lab(vec3<f32>(clamp(lab.x * scale, 0.0, GRADE_WHITE_L), lab.y, lab.z));
}

fn grade_cdl_one(v: f32, slope: f32, offset: f32, power: f32) -> f32 {
    let x = v * slope + offset;
    if x > 0.0 {
        return pow(x, power);
    }
    return x;
}

// The CDL whose slope, offset and power are rows `row` to `row` + 2.
fn grade_cdl(row: u32, c: vec3<f32>) -> vec3<f32> {
    let s = src[row].xyz;
    let o = src[row + 1u].xyz;
    let p = src[row + 2u].xyz;
    return vec3<f32>(
        grade_cdl_one(c.x, s.x, o.x, p.x),
        grade_cdl_one(c.y, s.y, o.y, p.y),
        grade_cdl_one(c.z, s.z, o.z, p.z),
    );
}

fn grade_color(color: vec3<f32>) -> vec3<f32> {
    var c = grade_hold(color);
    let s0 = src[0u];
    let s1 = src[1u];
    let s2 = src[2u];
    let s3 = src[3u];
    let s4 = src[4u];
    // The HSL Curves, each keyed on what the one before made.
    let curves = u32(s1.w);
    for (var k = 0u; k < curves; k++) {
        let d = src[16u + k];
        c = grade_hold(grade_hsl_curve(u32(d.x), u32(d.y), u32(d.z), c));
    }
    // The first part: White Balance, Levels, the RL wheels, the Color
    // Curves.
    if s0.x != 0.0 {
        let lin = pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.4));
        let m = vec3<f32>(dot(src[5u].xyz, lin), dot(src[6u].xyz, lin), dot(src[7u].xyz, lin));
        c = pow(max(m, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4));
    }
    if s0.y != 0.0 {
        let l = src[8u];
        c = vec3<f32>(l.z) + (c - vec3<f32>(l.x)) * ((l.w - l.z) / (l.y - l.x));
    }
    if s0.z != 0.0 {
        c = vec3<f32>(grade_table(0u, c.x), grade_table(1u, c.y), grade_table(2u, c.z));
    }
    if s0.w != 0.0 {
        c = vec3<f32>(grade_table(3u, c.x), grade_table(3u, c.y), grade_table(3u, c.z));
    }
    if s1.x != 0.0 {
        c = vec3<f32>(grade_table(4u, c.x), grade_table(5u, c.y), grade_table(6u, c.z));
    }
    if s1.y != 0.0 {
        c = vec3<f32>(grade_table(7u, c.x), grade_table(7u, c.y), grade_table(7u, c.z));
    }
    c = grade_hold(c);
    // The second: Lift, then Gain, Offset and Gamma by channel, then by Y.
    if s1.z != 0.0 {
        let l = src[9u].xyz;
        c = l + c * (vec3<f32>(1.0) - l);
    }
    if s4.z != 0.0 {
        c = grade_cdl(10u, c);
    }
    if s4.w != 0.0 {
        c = grade_cdl(13u, c);
    }
    c = grade_hold(c);
    // The third, and the HSL after it.
    if s2.x != 1.0 {
        let y = dot(vec3<f32>(0.2126, 0.7152, 0.0722), c);
        c = vec3<f32>(y) + s2.x * (c - vec3<f32>(y));
    }
    c = c * s2.y + vec3<f32>(s2.z);
    if s3.x != 0.0 {
        c = c + s3.x * (c - vec3<f32>(s3.y));
    }
    if s2.w != 0.0 {
        let e = c * s3.z;
        c = vec3<f32>(grade_table(8u, e.x), grade_table(8u, e.y), grade_table(8u, e.z));
    }
    let hsl = grade_to_hsl(c);
    c = grade_from_hsl(vec3<f32>(
        hsl.x + s3.w,
        clamp(hsl.y * s4.x, 0.0, 1.0),
        clamp(hsl.z * s4.y, 0.0, 1.0),
    ));
    return grade_hold(c);
}

@compute @workgroup_size(16, 16)
fn grade(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let a = px.w;
    if a <= 0.0 {
        dst[o] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    dst[o] = vec4<f32>(grade_color(px.xyz / a) * a, a);
}

// Bézier Masking (`bv-effects`'s plug-in, the same math as its render):
// how much of each pixel stays, by the masks in `src` — entry m (j.x of
// them) their first point's index (x), their points (y), their weight (z)
// and where their edge's softening starts (w, pixels inside the outline),
// entry 5 + m where it ends (x); then the points (x, y: canonical units
// from the frame's lower left). i.x: whether any mask is on; j.y Invert,
// j.z Mask FX; j.w: the amount alone, in every channel (Mask FX's first
// step), else `dst` times it. f: Blend (0), the render scale (1).
fn mask_cover(p: vec2<f32>, m: u32, scale: f32) -> f32 {
    let head = src[m];
    let base = u32(head.x);
    let n = u32(head.y);
    var inside = false;
    var near = 3.0e38;
    for (var k = 0u; k < n; k++) {
        let a = src[base + k].xy * scale - 0.5;
        let b = src[base + (k + 1u) % n].xy * scale - 0.5;
        // Crossings of the row's middle left of the pixel (even-odd).
        if (a.y <= p.y) != (b.y <= p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x <= p.x {
                inside = !inside;
            }
        }
        let e = b - a;
        let length2 = dot(e, e);
        var t = 0.0;
        if length2 > 0.0 {
            t = clamp(dot(p - a, e) / length2, 0.0, 1.0);
        }
        near = min(near, length(p - a - e * t));
    }
    var d = -near;
    if inside {
        d = near;
    }
    return smoothstep(head.w, src[5u + m].x, d);
}

@compute @workgroup_size(16, 16)
fn bezier_masking(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let blend = pf(0u);
    var k: f32;
    if P.i.x == 0 {
        // No mask: Blend of the picture (with Mask FX, 1 - Blend of the
        // effects).
        k = blend;
        if P.j.z != 0u {
            k = 1.0 - blend;
        }
    } else {
        // The plug-in's pixels: their middles at whole numbers, rows up.
        let p = vec2<f32>(f32(id.x), f32(P.h - 1u - id.y));
        var m = 0.0;
        for (var n = 0u; n < P.j.x; n++) {
            m = min(m + mask_cover(p, n, pf(1u)) * src[n].z, 1.0);
        }
        if P.j.y != 0u {
            k = (1.0 - m) + m * blend;
        } else if P.j.z != 0u {
            k = m * (1.0 - blend);
        } else {
            k = m + (1.0 - m) * blend;
        }
    }
    if P.j.w != 0u {
        dst[o] = vec4<f32>(k);
    } else {
        dst[o] = dst[o] * k;
    }
}

// BananaV's table effects that draw each pixel from elsewhere in the
// picture (`bv-effects`' `filters.rs`, the same math): `src` is the
// picture, `dst` as big. Their pixels are the plug-ins': rows going up
// (y 0 the bottom row), the frame's u from the left and v from the top.

// `Buf::at`: pixel (x, y), rows up, its edges held.
fn kit_at(x: i32, y: i32) -> vec4<f32> {
    let cx = clamp(x, 0, i32(P.sw) - 1);
    let cy = clamp(y, 0, i32(P.sh) - 1);
    return src[u32(i32(P.sh) - 1 - cy) * P.sw + u32(cx)];
}

// `Buf::bilinear`: between the four pixels around (x, y), rows up.
fn kit_bilinear(x: f32, y: f32) -> vec4<f32> {
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let xi = i32(x0);
    let yi = i32(y0);
    let a = kit_at(xi, yi);
    let b = kit_at(xi + 1, yi);
    let c = kit_at(xi, yi + 1);
    let d = kit_at(xi + 1, yi + 1);
    let low = a + (b - a) * fx;
    let high = c + (d - c) * fx;
    return low + (high - low) * fy;
}

// `Buf::sample`: the picture at the frame's u, v; nothing outside it.
fn kit_sample(u: f32, v: f32) -> vec4<f32> {
    if u < 0.0 || u > 1.0 || v < 0.0 || v > 1.0 {
        return vec4<f32>(0.0);
    }
    return kit_bilinear(u * f32(P.sw) - 0.5, (1.0 - v) * f32(P.sh) - 0.5);
}

// `Grid::uv` of pixel `id` (rows from the top here).
fn kit_uv(id: vec3<u32>) -> vec2<f32> {
    return vec2<f32>((f32(id.x) + 0.5) / f32(P.w), (f32(id.y) + 0.5) / f32(P.h));
}

// Spherize: f the Amount (0), the area's width and height (1, 2) and its
// center (3, 4: u, v from the top), in the frame's pixels as VEGAS rounds
// the middle.
@compute @workgroup_size(16, 16)
fn kit_spherize(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let w = f32(P.w);
    let ht = f32(P.h);
    let xc = trunc((2.0 * pf(3u) - 0.5) * w);
    let yc = trunc((2.0 * (1.0 - pf(4u)) - 0.5) * ht);
    let kx = 2.0 / (pf(1u) + 1e-4) / w;
    let ky = 2.0 / (pf(2u) + 1e-4) / ht;
    let x = f32(id.x);
    let y = f32(P.h - 1u - id.y);
    let dx = x - xc;
    let dy = y - yc;
    let r2 = (dx * kx) * (dx * kx) + (dy * ky) * (dy * ky);
    if r2 >= 1.0 {
        dst[o] = kit_at(i32(id.x), i32(P.h - 1u - id.y));
        return;
    }
    let f = 1.0 - cos(sqrt(r2) * 1.5707964) * pf(0u) * 0.8;
    dst[o] = kit_bilinear(xc + dx * f, yc + dy * f);
}

// Lens Correction (`bv-effects`' `lens_correction`, the same sums in
// single precision): f k1, k2 and k3 (0–2), what the PanoTools polynomial
// is multiplied by (3: 1 / Scale), the center in parts of the picture (4,
// 5) and in its pixels (6, 7), 2 / min(w, h) (8), 1 / w and 1 / h (9, 10)
// and the polynomial's constant term (11) — `lens_correction_numbers`,
// worked out as the processor does, so nothing is divided here —; j.x 1
// for the PanoTools model, else the radial one. Each pixel reads the
// picture where the model sends it, between the four pixels around
// weighed in 1024ths — right of the last column the next row's first
// pixel —, nothing from 0.4 of a pixel before the first column or row and
// 0.6 before the end.

// Pixel `k` of the picture counted row after row from the bottom.
fn lens_pixel(k: u32) -> vec4<f32> {
    return src[(P.h - 1u - k / P.w) * P.w + k % P.w];
}

@compute @workgroup_size(16, 16)
fn kit_lens_correction(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = f32(id.x);
    let y = f32(P.h - 1u - id.y);
    let k1 = pf(0u);
    let k2 = pf(1u);
    let k3 = pf(2u);
    var at = vec2<f32>(0.0);
    if P.j.x == 1u {
        let s = pf(8u);
        let cx = pf(6u);
        let cy = pf(7u);
        let dx = x - cx;
        let dy = y - cy;
        let r = sqrt(dx * dx + dy * dy);
        let f = (((r * s * s * s * k1 + s * s * k2) * r + k3 * s) * r + pf(11u)) * pf(3u);
        at = vec2<f32>((f * dx + cx) * pf(9u), (f * dy + cy) * pf(10u));
    } else {
        let cx = pf(4u);
        let cy = pf(5u);
        let v = y * pf(10u) - cy;
        let u = x * pf(9u) - cx;
        let r2 = u * u + v * v;
        let f = ((r2 * k3 + k2) * r2 + k1) * r2 + 1.0;
        at = vec2<f32>(f * u + cx, f * v + cy);
    }
    let sx = f32(P.w - 1u) * at.x;
    let sy = f32(P.h - 1u) * at.y;
    if !(sx > -0.4 && sx < f32(P.w) - 0.6 && sy > -0.4 && sy < f32(P.h) - 0.6) {
        dst[o] = vec4<f32>(0.0);
        return;
    }
    let ix = u32(max(sx, 0.0));
    let iy = u32(max(sy, 0.0));
    let fx = max(sx - f32(ix), 0.0);
    let fy = max(sy - f32(iy), 0.0);
    let n = P.w * P.h;
    let k = iy * P.w + ix;
    let p00 = lens_pixel(k);
    var p10 = p00;
    if k + 1u < n {
        p10 = lens_pixel(k + 1u);
    }
    var p01 = p00;
    if k + P.w < n {
        p01 = lens_pixel(k + P.w);
    }
    var p11 = p01;
    if k + P.w + 1u < n {
        p11 = lens_pixel(k + P.w + 1u);
    }
    let w00 = f32(i32((1.0 - fx) * (1.0 - fy) * 1024.0));
    let w10 = f32(i32((1.0 - fy) * fx * 1024.0));
    let w01 = f32(i32((1.0 - fx) * fy * 1024.0));
    let w11 = f32(i32(fy * fx * 1024.0));
    dst[o] = (p10 * w10 + w00 * p00 + p01 * w01 + p11 * w11) * 0.0009765625;
}

// Video Stabilization: each pixel (rows up) reads the picture where the
// frame's correction maps it — `x + f0 x + f1 y + f2 + f3` across and
// `y + f4 x + f5 y + f6 + f7` up, the whole pixels f2 and f6 apart from the
// fractions so that single precision keeps them —, between its four
// pixels weighed in truncated 1024ths. `j.x` the edge filling: 0
// transparent (nothing from 0.4 of a pixel before the first column or row
// or 0.6 before the end, the last column's and row's neighbours their
// own), 1 mirrored, 2 repeated, 3 held.
fn stab_pixel(x: i32, y: i32) -> vec4<f32> {
    return src[u32(i32(P.h) - 1 - y) * P.w + u32(x)];
}

fn stab_fold(i: i32, n: i32, mode: u32) -> i32 {
    if mode == 1u {
        let m = ((i % (2 * n)) + 2 * n) % (2 * n);
        if m >= n {
            return 2 * n - 1 - m;
        }
        return m;
    }
    if mode == 2u {
        return ((i % n) + n) % n;
    }
    return clamp(i, 0, n - 1);
}

@compute @workgroup_size(16, 16)
fn kit_stabilize(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let w = i32(P.w);
    let h = i32(P.h);
    let x = f32(id.x);
    let y = f32(h - 1 - i32(id.y));
    let dx = pf(0u) * x + pf(1u) * y + pf(3u);
    let dy = pf(4u) * x + pf(5u) * y + pf(7u);
    let fdx = floor(dx);
    let fdy = floor(dy);
    var ix = i32(id.x) + i32(pf(2u)) + i32(fdx);
    var iy = h - 1 - i32(id.y) + i32(pf(6u)) + i32(fdy);
    var fx = dx - fdx;
    var fy = dy - fdy;
    let mode = P.j.x;
    var x1 = ix + 1;
    var y1 = iy + 1;
    if mode == 0u {
        let sx = f32(ix) + fx;
        let sy = f32(iy) + fy;
        if !(sx > -0.4 && sx < f32(w) - 0.6 && sy > -0.4 && sy < f32(h) - 0.6) {
            dst[o] = vec4<f32>(0.0);
            return;
        }
        // Just before the first column or row: that pixel itself.
        if ix < 0 {
            ix = 0;
            fx = 0.0;
        }
        if iy < 0 {
            iy = 0;
            fy = 0.0;
        }
        x1 = min(ix + 1, w - 1);
        y1 = min(iy + 1, h - 1);
    } else {
        x1 = stab_fold(x1, w, mode);
        y1 = stab_fold(y1, h, mode);
        ix = stab_fold(ix, w, mode);
        iy = stab_fold(iy, h, mode);
    }
    let p00 = stab_pixel(ix, iy);
    let p10 = stab_pixel(x1, iy);
    let p01 = stab_pixel(ix, y1);
    var p11 = p01;
    if mode != 0u || (x1 != ix && y1 != iy) {
        p11 = stab_pixel(x1, y1);
    }
    let w00 = f32(i32((1.0 - fx) * (1.0 - fy) * 1024.0));
    let w10 = f32(i32(fx * (1.0 - fy) * 1024.0));
    let w01 = f32(i32(fy * (1.0 - fx) * 1024.0));
    let w11 = f32(i32(fy * fx * 1024.0));
    dst[o] = (p00 * w00 + p10 * w10 + p01 * w01 + p11 * w11) * 0.0009765625;
}

// Video Stabilization's motion blur: each pixel the mean of the `2 i.x +
// 1` of its row around it, the row's ends mirrored without repeating the
// end.
@compute @workgroup_size(16, 16)
fn kit_stabilize_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let r = P.i.x;
    let w = i32(P.w);
    let row = id.y * P.w;
    var s = vec4<f32>(0.0);
    for (var k = -r; k <= r; k = k + 1) {
        var i = i32(id.x) + k;
        if i < 0 {
            i = -i;
        }
        if i > w - 1 {
            i = 2 * (w - 1) - i;
        }
        i = clamp(i, 0, w - 1);
        s = s + src[row + u32(i)];
    }
    dst[o] = s / f32(2 * r + 1);
}

// `t` mirrored (1) or repeated (2) past 0-1; otherwise -1 when outside.
fn kit_wrap(t: f32, mode: u32) -> f32 {
    if mode == 1u {
        let m = t - floor(t / 2.0) * 2.0;
        if m > 1.0 {
            return 2.0 - m;
        }
        return m;
    }
    if mode == 2u {
        return t - floor(t);
    }
    if t < 0.0 || t > 1.0 {
        return -1.0;
    }
    return t;
}

// Offset and Wrap: the picture scaled down Count times (f 2) about the
// middle and moved by Offset (0, 1: frames; up above 0), wrapped as j.x
// (across) and j.y (up) say.
@compute @workgroup_size(16, 16)
fn kit_offset_wrap(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let uv = kit_uv(id);
    let su = kit_wrap(0.5 + (uv.x - 0.5) * pf(2u) - pf(0u), P.j.x);
    let sv = kit_wrap(0.5 + (uv.y - 0.5) * pf(2u) + pf(1u), P.j.y);
    if su < 0.0 || sv < 0.0 {
        dst[o] = vec4<f32>(0.0);
        return;
    }
    dst[o] = kit_sample(su, sv);
}

// Tiny Planet: the picture as the whole sphere seen in a stereographic
// projection about a latitude (its sine and cosine, f 0 and 1, from the
// bottom pole) and Longitude (2); Z (3) frame widths reach the
// projection's unit.
@compute @workgroup_size(16, 16)
fn kit_tiny_planet(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let w = i32(P.sw);
    let h = i32(P.sh);
    let unit = 2.0 * f32(w) / pf(3u);
    let sin0 = pf(0u);
    let cos0 = pf(1u);
    let dx = f32(i32(id.x) - w / 2);
    let dy = f32(i32(P.h - 1u - id.y) - h / 2);
    let r = sqrt(dx * dx + dy * dy);
    let t = r / unit;
    let sin_c = 2.0 * t / (1.0 + t * t);
    let cos_c = (1.0 - t * t) / (1.0 + t * t);
    var row = 0.0;
    var along = 0.0;
    if r > 0.0 {
        let up = sin_c * dy * cos0 / r + cos_c * sin0;
        let east = sin_c * dx / r;
        let north = (cos0 * r * cos_c - sin0 * dy * sin_c) / r;
        let phi = atan2(up, sqrt(east * east + north * north));
        along = atan2(sin_c * dx, cos0 * r * cos_c - sin0 * dy * sin_c);
        row = f32(h - 1) * (phi + 1.5707964) / 3.1415927;
    }
    along = along + pf(2u) + 3.1415927;
    if along < 0.0 {
        along = along + 6.2831855;
    } else if along > 6.2831855 {
        along = along - 6.2831855;
    }
    let column = f32(w - 1) * along / 6.2831855;
    dst[o] = kit_bilinear(clamp(column, 0.0, f32(w - 1)), clamp(row, 0.0, f32(h - 1)));
}

// Deform (`bv-effects`' `deformed`, VEGAS's, rows counted from the top as
// the card's are): out of the deformed picture's box (i: first column and
// row in, last out) the background (f 14-17); in it each pixel, placed
// from the box's origin (f 0, 1), is taken back through the shears (f 4,
// 5: across per row, up per column), the squeezes across (6, 7: the top's
// half width, its step per row) and up (8, 9) and the columns' (12, 13)
// and rows' (10, 11) curves — those j.x's bits 1, 2, 4, 16, 8 turn on —
// about the middle (f 2, 3), then read between its four nearest pixels,
// the background past the picture's edges.
fn deform_texel(x: i32, t: i32) -> vec4<f32> {
    if x < 0 || t < 0 || x >= i32(P.sw) || t >= i32(P.sh) {
        return vec4<f32>(pf(14u), pf(15u), pf(16u), pf(17u));
    }
    return src[u32(t) * P.sw + u32(x)];
}

@compute @workgroup_size(16, 16)
fn kit_deform(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let bg = vec4<f32>(pf(14u), pf(15u), pf(16u), pf(17u));
    let column = i32(id.x);
    let row = i32(id.y);
    if column < P.i.x || row < P.i.y || column >= P.i.z || row >= P.i.w {
        dst[o] = bg;
        return;
    }
    let w = f32(P.sw);
    let h = f32(P.sh);
    let hw = pf(2u);
    let hh = pf(3u);
    var x = f32(column - P.i.x) + pf(0u);
    var y = f32(row - P.i.y) + pf(1u);
    let on = P.j.x;
    if (on & 1u) != 0u {
        let t = x * pf(5u);
        x = x - y * pf(4u);
        y = y - t;
    }
    if (on & 2u) != 0u {
        let d = y * pf(7u) + pf(6u);
        if d == 0.0 {
            dst[o] = bg;
            return;
        }
        x = ((x - hw) * hw) / d + hw;
    }
    if (on & 4u) != 0u {
        let d = x * pf(9u) + pf(8u);
        if d == 0.0 {
            dst[o] = bg;
            return;
        }
        y = ((y - hh) * hh) / d + hh;
    }
    if (on & 16u) != 0u {
        let b = (y - hh) * (y - hh) / (hh * hh);
        let l = b * pf(12u) - pf(12u);
        let r = (b * pf(13u) - pf(13u)) + w;
        if l == r {
            dst[o] = bg;
            return;
        }
        x = ((x - l) / (r - l)) * w;
    }
    if (on & 8u) != 0u {
        let a = (x - hw) * (x - hw) / (hw * hw);
        let t = a * pf(10u) - pf(10u);
        let b = (a * pf(11u) - pf(11u)) + h;
        if t == b {
            dst[o] = bg;
            return;
        }
        y = ((y - t) / (b - t)) * h;
    }
    // A point a hair before the first column or row counts as in.
    if x < -0.001 || x >= w || y < -0.001 || y >= h {
        dst[o] = bg;
        return;
    }
    let ix = i32(floor(x));
    let it = i32(floor(y));
    let fx = x - floor(x);
    let ft = y - floor(y);
    let a = deform_texel(ix, it);
    let b = deform_texel(ix + 1, it);
    let c = deform_texel(ix, it + 1);
    let d = deform_texel(ix + 1, it + 1);
    dst[o] = (c * (1.0 - fx) + d * fx) * ft + (a * (1.0 - fx) + b * fx) * (1.0 - ft);
}

// Convolution Kernel: the 3 x 3 weights (f 0-8, row 1 above the pixel),
// then the offset (9); j.x: the alpha summed too, else kept.
@compute @workgroup_size(16, 16)
fn kit_convolution(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(P.h - 1u - id.y);
    var acc = kit_at(x - 1, y + 1) * pf(0u) + kit_at(x, y + 1) * pf(1u) + kit_at(x + 1, y + 1) * pf(2u);
    acc = acc + kit_at(x - 1, y) * pf(3u) + kit_at(x, y) * pf(4u) + kit_at(x + 1, y) * pf(5u);
    acc = acc + kit_at(x - 1, y - 1) * pf(6u) + kit_at(x, y - 1) * pf(7u) + kit_at(x + 1, y - 1) * pf(8u);
    var alpha = kit_at(x, y).w;
    if P.j.x != 0u {
        alpha = clamp(acc.w, 0.0, 1.0);
    }
    let offset = pf(9u);
    dst[o] = vec4<f32>(alpha * (acc.x + offset), alpha * (acc.y + offset), alpha * (acc.z + offset), alpha);
}

// Quick Blur: the picture blurred by a 3 x 3 box (edges held), mixed in by
// Blend amount (f 0).
@compute @workgroup_size(16, 16)
fn kit_quick_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(P.h - 1u - id.y);
    var sum = vec4<f32>(0.0);
    for (var j = -1; j <= 1; j++) {
        sum = sum + (kit_at(x - 1, y + j) + kit_at(x, y + j) + kit_at(x + 1, y + j)) / 3.0;
    }
    let a = kit_at(x, y);
    dst[o] = a + (sum / 3.0 - a) * pf(0u);
}

// Height Map's smoothing (`compositors::box_rows`): a box of radius j.x
// across each row (an invocation a row) of fixed-point heights in x —
// exact integers in single precision, summed as integers so that the
// card adds them as the processor does —, the edge values held, each
// sum rounded back to the heights' units.
@compute @workgroup_size(64)
fn kit_box_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = id.x;
    if y >= P.h {
        return;
    }
    let w = i32(P.w);
    let r = i32(P.j.x);
    let n = 2 * r + 1;
    let base = y * P.w;
    var acc = 0;
    for (var i = -r; i <= r; i++) {
        acc += i32(src[base + u32(clamp(i, 0, w - 1))].x);
    }
    for (var x = 0; x < w; x++) {
        dst[base + u32(x)] = vec4<f32>(f32((acc + r) / n), 0.0, 0.0, 0.0);
        let add = i32(src[base + u32(clamp(x + r + 1, 0, w - 1))].x);
        let sub = i32(src[base + u32(clamp(x - r, 0, w - 1))].x);
        acc += add - sub;
    }
}

// The same down each column (an invocation a column), from the top row.
@compute @workgroup_size(64)
fn kit_box_columns(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x;
    if x >= P.w {
        return;
    }
    let h = i32(P.h);
    let r = i32(P.j.x);
    let n = 2 * r + 1;
    var acc = 0;
    for (var i = -r; i <= r; i++) {
        acc += i32(src[u32(clamp(i, 0, h - 1)) * P.w + x].x);
    }
    for (var y = 0; y < h; y++) {
        dst[u32(y) * P.w + x] = vec4<f32>(f32((acc + r) / n), 0.0, 0.0, 0.0);
        let add = i32(src[u32(clamp(y + r + 1, 0, h - 1)) * P.w + x].x);
        let sub = i32(src[u32(clamp(y - r, 0, h - 1)) * P.w + x].x);
        acc += add - sub;
    }
}

// Fill Light's light `src` (blurred) added to the picture `dst`, f 0 of
// it, the straight colors held under white, the alpha kept.
@compute @workgroup_size(16, 16)
fn kit_fill_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = id.y * P.w + id.x;
    let p = dst[i];
    var c = vec3<f32>(0.0);
    if p.w > 0.0 {
        c = p.xyz / p.w;
    }
    let lit = min(c + pf(0u) * src[i].xyz, vec3<f32>(1.0));
    dst[i] = vec4<f32>(lit * p.w, p.w);
}

// Bump Map (the filter): the light at Source (f 0-2), its axis (3-5), the
// surface's rise per step of the bump channel over the frame's longer
// side (6), the spotlight's cone and spread (7, 8), Intensity, Shininess,
// Ambience (9-11), the bump channel as a mask (12-15); j.x the light's
// type (0 directional, 1 spot, 2 omni).
fn bump_at(x: i32, y: i32) -> f32 {
    return dot(kit_at(x, y), vec4<f32>(pf(12u), pf(13u), pf(14u), pf(15u)));
}

@compute @workgroup_size(16, 16)
fn kit_bump_map(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(P.h - 1u - id.y);
    // Rows go up: +1 is the row above.
    var gx = 0.0;
    var gy = 0.0;
    for (var j = -1; j <= 1; j++) {
        gx = gx + bump_at(x - 1, y + j) - bump_at(x + 1, y + j);
        gy = gy + bump_at(x + j, y - 1) - bump_at(x + j, y + 1);
    }
    let c = kit_at(x, y);
    dst[o] = vec4<f32>(c.xyz * bump_light(gx, gy, x, y), c.w);
}

// Chroma Keyer's mask: each pixel's distance from the key (f 0-2, its hue,
// saturation and lightness 3-5) — the RGB distance times sqrt 3 and the
// HSL one, halved — through the thresholds (6, 7; 8 the scale between
// them), no more than the pixel's alpha, in every channel of `dst`.
fn key_hsl(c: vec3<f32>) -> vec3<f32> {
    let hi = max(max(c.x, c.y), c.z);
    let lo = min(min(c.x, c.y), c.z);
    let l = (hi + lo) * 0.5;
    let spread = hi - lo;
    if l <= 0.0 || spread <= 0.0 {
        return vec3<f32>(0.0, 0.0, l);
    }
    var s = spread / (hi + lo);
    if l > 0.5 {
        s = spread / (2.0 - hi - lo);
    }
    let dr = (hi - c.x) / spread;
    let dg = (hi - c.y) / spread;
    let db = (hi - c.z) / spread;
    var h = 5.0 - dr;
    if c.x == hi {
        if c.y == lo {
            h = 5.0 + db;
        } else {
            h = 1.0 - dg;
        }
    } else if c.y == hi {
        if c.z == lo {
            h = 1.0 + dr;
        } else {
            h = 3.0 - db;
        }
    } else if c.x == lo {
        h = 3.0 + dg;
    }
    return vec3<f32>(h / 6.0, s, l);
}

@compute @workgroup_size(16, 16)
fn kit_key_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let c = src[o];
    let hsl = key_hsl(c.xyz);
    var dh = abs(hsl.x - pf(3u));
    if dh > 0.5 {
        dh = 1.0 - dh;
    }
    let key = vec3<f32>(pf(0u), pf(1u), pf(2u));
    let far = length(key - c.xyz);
    let d = (far * 1.7320508 + (2.0 * dh + abs(hsl.y - pf(4u)) + abs(hsl.z - pf(5u))) / 3.0) * 0.5;
    var m = clamp((d - pf(6u)) * pf(8u), 0.0, 1.0);
    if d < pf(6u) {
        m = 0.0;
    } else if d > pf(7u) - 1e-6 {
        m = 1.0;
    }
    dst[o] = vec4<f32>(min(m, c.w));
}

// Chroma Keyer's picture through its mask (`src`'s x): blue or green spill
// held under the next channel (j.x 2 or 1, the key's lead), the colors
// times the mask, or the mask shown white (j.y).
@compute @workgroup_size(16, 16)
fn kit_key_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let m = src[o].x;
    var c = dst[o];
    if m > 0.0 {
        if P.j.x == 2u && c.y < c.z {
            c.z = c.y;
        } else if P.j.x == 1u && c.x < c.y {
            c.y = c.x;
        }
    }
    if P.j.y != 0u {
        dst[o] = vec4<f32>(m);
    } else {
        dst[o] = vec4<f32>(c.xyz * m, m);
    }
}

// Chroma Blur's first step: the picture's U and V (BT.601's), in x and y.
@compute @workgroup_size(16, 16)
fn kit_chroma_uv(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let c = src[o];
    dst[o] = vec4<f32>(
        c.x * -0.14713 - c.y * 0.28886 + c.z * 0.436,
        c.x * 0.615 - c.y * 0.51499 - c.z * 0.10001,
        0.0,
        0.0,
    );
}

// Chroma Blur's last step: the picture's Y with the blurred U and V
// (`src`'s x and y), within 0-1, its alpha kept.
@compute @workgroup_size(16, 16)
fn kit_chroma_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let c = dst[o];
    let uv = src[o];
    let y = c.x * 0.299 + c.y * 0.587 + c.z * 0.114;
    dst[o] = vec4<f32>(
        clamp(y + uv.y * 1.13983, 0.0, 1.0),
        clamp(y - uv.x * 0.39465 - uv.y * 0.5806, 0.0, 1.0),
        clamp(y + uv.x * 2.03211, 0.0, 1.0),
        c.w,
    );
}

// Linear Blur's first step: the picture (`src`, `sw` x `sh`, the plug-ins'
// pixels) sheared along the line — row r of `dst` the pixels at w + slope
// x u across the line (w = i.y + r, u = i.x + its column; f 0 the slope;
// j.x: the line goes across, else up).
@compute @workgroup_size(16, 16)
fn kit_shear(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let u = f32(P.i.x + i32(id.x));
    let v = f32(P.i.y + i32(id.y)) + pf(0u) * u;
    if P.j.x != 0u {
        dst[o] = kit_bilinear(u, v);
    } else {
        dst[o] = kit_bilinear(v, u);
    }
}

// The sheared picture's pixel (u, w), its edges held (`src`, the window
// i.x-i.z along, i.y-i.w across).
fn lines_at(u: i32, w: i32) -> vec4<f32> {
    let cu = clamp(u, P.i.x, P.i.z - 1) - P.i.x;
    let cw = clamp(w, P.i.y, P.i.w - 1) - P.i.y;
    return src[u32(cw) * P.sw + u32(cu)];
}

// Linear Blur's last step: each pixel back from the sheared picture, its
// rows blurred (`src`), between the four pixels around it.
@compute @workgroup_size(16, 16)
fn kit_unshear(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = f32(id.x);
    let y = f32(P.h - 1u - id.y);
    var u = x;
    var v = y;
    if P.j.x == 0u {
        u = y;
        v = x;
    }
    let lu = u;
    let lw = v - pf(0u) * u;
    let u0 = floor(lu);
    let w0 = floor(lw);
    let fu = lu - u0;
    let fw = lw - w0;
    let iu = i32(u0);
    let iw = i32(w0);
    let a = lines_at(iu, iw);
    let b = lines_at(iu + 1, iw);
    let c = lines_at(iu, iw + 1);
    let d = lines_at(iu + 1, iw + 1);
    let low = a + (b - a) * fu;
    let high = c + (d - c) * fu;
    dst[o] = low + (high - low) * fw;
}

// Radial Pixelate's first step: the picture (`src`, the plug-ins'
// pixels) laid out by angle across and distance up (`dst`, row j the j-th
// distance from the bottom), each its nearest pixel; f 0, 1 the middle
// (-1 to 1, y up).
fn polar_nearest(v: f32, n: u32) -> u32 {
    return min(u32(max(v, 0.0)), n - 1u);
}

@compute @workgroup_size(16, 16)
fn kit_polar(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let wf = f32(P.w);
    let hf = f32(P.h);
    let a = (2.0 * f32(id.x) / wf - 1.0) * 3.1415927;
    let r = f32(id.y) / hf * 2.828427;
    let x = ((sin(a) * r + pf(0u)) * 0.5 + 0.5) * wf;
    let y = ((cos(a) * r + pf(1u)) * 0.5 + 0.5) * hf;
    let ny = polar_nearest(y, P.h);
    dst[o] = src[(P.h - 1u - ny) * P.w + polar_nearest(x, P.w)];
}

// Radial Pixelate's blocks: each the mean of its part of the laid out
// picture (`src`, `sw` x `sh`; `dst` a pixel a block).
@compute @workgroup_size(16, 16)
fn kit_polar_means(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x0 = id.x * P.sw / P.w;
    let x1 = min(max((id.x + 1u) * P.sw / P.w, x0 + 1u), P.sw);
    let y0 = id.y * P.sh / P.h;
    let y1 = min(max((id.y + 1u) * P.sh / P.h, y0 + 1u), P.sh);
    var sum = vec4<f32>(0.0);
    for (var y = y0; y < y1; y++) {
        for (var x = x0; x < x1; x++) {
            sum = sum + src[y * P.sw + x];
        }
    }
    dst[o] = sum / f32(max((x1 - x0) * (y1 - y0), 1u));
}

// Radial Pixelate's last step: each pixel its block's mean (`src`, `sw`
// x `sh` blocks), found by its angle and distance from the middle.
@compute @workgroup_size(16, 16)
fn kit_polar_back(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let wf = f32(P.w);
    let hf = f32(P.h);
    let dx = 2.0 * f32(id.x) / wf - 1.0 - pf(0u);
    let dy = 2.0 * f32(P.h - 1u - id.y) / hf - 1.0 - pf(1u);
    let r = sqrt(dx * dx + dy * dy) / 2.828427 * hf;
    let a = (atan2(dx, dy) / 3.1415927 * 0.5 + 0.5) * wf;
    let pi = polar_nearest(a, P.w);
    let pj = polar_nearest(r, P.h);
    dst[o] = src[(pj * P.sh / P.h) * P.sw + pi * P.sw / P.w];
}

// Radial Blur, as the processor's: each quarter around the middle (i.x,
// i.y: its pixel, rows up) in turn — below and left, below and right,
// above and left, above and right —, each pixel mixed (f 1 of the way)
// with what is already worked out a step toward the middle — f 0 of its
// distance (j.x: Proportional), else f 2 pixels (Fixed) —, read between
// the four pixels around that point. A pixel needs only nearer ones, so
// the pixels of one diagonal of a quarter are worked out at once, by the
// one group of threads this runs as. `src` is the picture, `dst` its copy
// worked out in place.
fn radial_index(x: i32, y: i32) -> u32 {
    let cx = clamp(x, 0, i32(P.w) - 1);
    let cy = clamp(y, 0, i32(P.h) - 1);
    return u32(i32(P.h) - 1 - cy) * P.w + u32(cx);
}

fn radial_pixel(x: i32, y: i32) {
    let cx = f32(P.i.x);
    let cy = f32(P.i.y);
    let dx = cx - f32(x);
    let dy = cy - f32(y);
    var px = f32(x) + dx * pf(0u);
    var py = f32(y) + dy * pf(0u);
    if P.j.x == 0u {
        let d = sqrt(dx * dx + dy * dy);
        if d > pf(2u) {
            px = f32(x) + dx * pf(2u) / d;
            py = f32(y) + dy * pf(2u) / d;
        } else {
            px = cx;
            py = cy;
        }
    }
    let ix = floor(px);
    let iy = floor(py);
    let tx = px - ix;
    let ty = py - iy;
    let a = dst[radial_index(i32(ix), i32(iy))];
    let b = dst[radial_index(i32(ix) + 1, i32(iy))];
    let c = dst[radial_index(i32(ix), i32(iy) + 1)];
    let d = dst[radial_index(i32(ix) + 1, i32(iy) + 1)];
    let low = a + (b - a) * tx;
    let high = c + (d - c) * tx;
    let i = radial_index(x, y);
    let q = pf(1u);
    dst[i] = (1.0 - q) * src[i] + q * (low + (high - low) * ty);
}

@compute @workgroup_size(16, 16)
fn kit_radial_blur(@builtin(local_invocation_index) t: u32) {
    let w = i32(P.w);
    let h = i32(P.h);
    let cx = P.i.x;
    let cy = P.i.y;
    for (var quarter = 0; quarter < 4; quarter++) {
        let above = quarter >= 2;
        let right = quarter == 1 || quarter == 3;
        var rows = cy + 1;
        if above {
            rows = h - cy - 1;
        }
        var columns = cx + 1;
        if right {
            columns = w - cx - 1;
        }
        if rows > 0 && columns > 0 {
            // Before the last quarter the processor puts back the pixel
            // above the middle, worked out in the one before.
            if quarter == 3 && t == 0u {
                let i = radial_index(cx, cy + 1);
                dst[i] = src[i];
            }
            storageBarrier();
            for (var diagonal = 0; diagonal < rows + columns - 1; diagonal++) {
                let first = max(0, diagonal - (columns - 1));
                let last = min(diagonal, rows - 1);
                for (var ry = first + i32(t); ry <= last; ry += 256) {
                    let rx = diagonal - ry;
                    var y = cy - ry;
                    if above {
                        y = cy + 1 + ry;
                    }
                    var x = cx - rx;
                    if right {
                        x = cx + 1 + rx;
                    }
                    radial_pixel(x, y);
                }
                storageBarrier();
            }
        }
    }
}

// A premultiplied pixel's straight color (none without alpha).
fn kit_straight(p: vec4<f32>) -> vec3<f32> {
    if p.w > 0.0 {
        return p.xyz / p.w;
    }
    return vec3<f32>(0.0);
}

// Gaussian Blur's channels: the blurred picture (`dst`) where the Channels
// group turns a channel on (j: red, green, blue, alpha), the picture
// (`src`) where off, as the premultiplied values stand.
@compute @workgroup_size(16, 16)
fn kit_gauss_pick(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let on = vec4<bool>(P.j.x != 0u, P.j.y != 0u, P.j.z != 0u, P.j.w != 0u);
    dst[o] = select(src[o], dst[o], on);
}

// Sharpen: each straight channel of the picture (`src`) moved f 0 times
// its difference from its neighbours' average (`dst`, then written), a
// 32nd of an 8-bit step under; the alpha kept.
@compute @workgroup_size(16, 16)
fn kit_sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let p = src[o];
    let b = dst[o];
    let a = p.w;
    if a <= 0.0 {
        dst[o] = vec4<f32>(0.0);
        return;
    }
    let c = kit_straight(p);
    var around = c;
    if b.w > 0.0 {
        around = kit_straight(b);
    }
    let tie = 1.0 / (32.0 * 255.0);
    let s = c + pf(0u) * (c - around) - vec3<f32>(tie);
    dst[o] = vec4<f32>(s * a, a);
}

// Glow's brightness: the plain average of the channels as they are.
@compute @workgroup_size(16, 16)
fn kit_glow_bright(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let p = src[o];
    dst[o] = vec4<f32>((p.x + p.y + p.z) / 3.0);
}

// Glow: each pixel toward Color (f 2-4) as far as Intensity (0) times its
// blurred brightness (`src`'s x) less Suppression (1), within 0-1.
@compute @workgroup_size(16, 16)
fn kit_glow_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let k = clamp(pf(0u) * (src[o].x - pf(1u)), 0.0, 1.0);
    if k <= 0.0 {
        return;
    }
    let p = dst[o];
    let to = vec4<f32>(pf(2u), pf(3u), pf(4u), 1.0);
    dst[o] = p + (to - p) * k;
}

// Mirror (`bv-effects`' `mirror::Layout`, VEGAS's floats): a pixel of
// the picture (`src`), columns from the left, rows from the top.
fn mirror_at(c: i32, r: i32) -> vec4<f32> {
    return src[u32(r) * P.w + u32(c)];
}

// Between two rows (or columns, `across`) `t` points to of `n`, the last
// one's neighbour past the picture the background (f 5-8); past the
// picture, the background.
fn mirror_between(t: f32, n: i32, across: bool, keep: i32) -> vec4<f32> {
    let bg = vec4<f32>(pf(5u), pf(6u), pf(7u), pf(8u));
    let i = i32(t);
    let f = t - f32(i);
    if i < 0 || i >= n {
        return bg;
    }
    var here = vec4<f32>(0.0);
    var next = bg;
    if across {
        here = mirror_at(i, keep);
        if i < n - 1 {
            next = mirror_at(i + 1, keep);
        }
    } else {
        here = mirror_at(keep, i);
        if i < n - 1 {
            next = mirror_at(keep, i + 1);
        }
    }
    return f * next + (1.0 - f) * here;
}

fn mirror_read(c: i32, r: i32) -> vec4<f32> {
    if c < 0 || c >= i32(P.w) || r < 0 || r >= i32(P.h) {
        return vec4<f32>(pf(5u), pf(6u), pf(7u), pf(8u));
    }
    return mirror_at(c, r);
}

// f: the line's point (0, 1), a slanted line's slope (2), the pixels'
// scale (3, 4), the background (5-8); j.x the way (0 Top, 1 Bottom, 2
// Left, 3 Right, 4 Slanted), j.y the left part reflecting.
@compute @workgroup_size(16, 16)
fn kit_mirror(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let c = i32(id.x);
    let r = i32(id.y);
    let wi = i32(P.w);
    let hi = i32(P.h);
    let px = pf(0u);
    let py = pf(1u);
    let bg = vec4<f32>(pf(5u), pf(6u), pf(7u), pf(8u));
    let way = P.j.x;
    if way == 0u {
        let kept = min(i32(ceil(py)), hi);
        let end = min(1 - i32(py * -2.0), hi);
        if r < kept {
            dst[o] = mirror_at(c, r);
        } else if r < end {
            dst[o] = mirror_between(py + py - f32(r), hi, false, c);
        } else {
            dst[o] = bg;
        }
        return;
    }
    if way == 1u {
        let start = max(min(i32(py + py - f32(hi)), hi), 0);
        let end = min(i32(ceil(py)), hi - 1);
        if r < start {
            dst[o] = bg;
        } else if r <= end {
            dst[o] = mirror_between(py + py - f32(r), hi, false, c);
        } else {
            dst[o] = mirror_at(c, r);
        }
        return;
    }
    if way == 2u {
        if c < i32(ceil(px)) {
            dst[o] = mirror_at(c, r);
        } else {
            dst[o] = mirror_between(px + px - f32(c), wi, true, r);
        }
        return;
    }
    if way == 3u {
        if c <= i32(ceil(px)) {
            dst[o] = mirror_between(px + px - f32(c), wi, true, r);
        } else {
            dst[o] = mirror_at(c, r);
        }
        return;
    }
    let sx = pf(3u);
    let sy = pf(4u);
    let m = pf(2u);
    let cx = px * (1.0 / sx);
    let cy = py * (1.0 / sy);
    let y = f32(r) * (1.0 / sy);
    let cross = min(i32(((y - cy) / m + cx) * sx) + 1, wi);
    if (c < cross) != (P.j.y != 0u) {
        dst[o] = mirror_at(c, r);
        return;
    }
    let x = f32(c) * (1.0 / sx);
    let foot = (((y - (-1.0 / m) * x) - cy) + m * cx) / (m - -1.0 / m);
    let foot_y = (foot - cx) * m + cy;
    let rx = ((foot + foot) - x) * sx;
    let ry = ((foot_y + foot_y) - y) * sy;
    let ix = i32(floor(rx));
    let iy = i32(floor(ry));
    if ix < -1 || ix >= wi || iy < -1 || iy >= hi {
        dst[o] = bg;
        return;
    }
    let fx = rx - f32(ix);
    let fy = ry - f32(iy);
    let upper = mix(mirror_read(ix, iy), mirror_read(ix + 1, iy), fx);
    let lower = mix(mirror_read(ix, iy + 1), mirror_read(ix + 1, iy + 1), fx);
    dst[o] = mix(upper, lower, fy);
}

// VEGAS's Gaussian blur (`recursive_gauss`): Deriche's filter along a line
// of `n` pixels from `base`, `step` apart, as its two oscillations side by
// side — f 0–7 the first's numbers, 8–15 the second's: its complex pole,
// its weight, and what its causal and anticausal passes settle to per unit
// of a constant line —, each one complex pole at a time (`z = x + p z`),
// which floats keep accurate however wide the blur is; a constant beyond
// the ends.
fn gauss_line(base: u32, step: u32, n: u32) {
    let p = vec2<f32>(pf(0u), pf(1u));
    let q = vec2<f32>(pf(8u), pf(9u));
    let first = src[base];
    var zr = first * pf(4u);
    var zi = first * pf(5u);
    var vr = first * pf(12u);
    var vi = first * pf(13u);
    for (var i = 0u; i < n; i++) {
        let o = base + i * step;
        let x = src[o];
        let zr1 = x + p.x * zr - p.y * zi;
        zi = p.x * zi + p.y * zr;
        zr = zr1;
        let vr1 = x + q.x * vr - q.y * vi;
        vi = q.x * vi + q.y * vr;
        vr = vr1;
        dst[o] = pf(2u) * zr - pf(3u) * zi + pf(10u) * vr - pf(11u) * vi;
    }
    let last = src[base + (n - 1u) * step];
    var wr = last * pf(6u);
    var wi = last * pf(7u);
    var ur = last * pf(14u);
    var ui = last * pf(15u);
    for (var k = 0u; k < n; k++) {
        let o = base + (n - 1u - k) * step;
        let x = src[o];
        dst[o] = dst[o] + pf(2u) * wr - pf(3u) * wi + pf(10u) * ur - pf(11u) * ui;
        // The next pixel back: p (x + w).
        let ar = x + wr;
        let wr1 = p.x * ar - p.y * wi;
        wi = p.x * wi + p.y * ar;
        wr = wr1;
        let br = x + ur;
        let ur1 = q.x * br - q.y * ui;
        ui = q.x * ui + q.y * br;
        ur = ur1;
    }
}

// The filter along each row (an invocation a row).
@compute @workgroup_size(64)
fn kit_gauss_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.h {
        return;
    }
    gauss_line(id.x * P.w, 1u, P.w);
}

// The same down each column (an invocation a column).
@compute @workgroup_size(64)
fn kit_gauss_columns(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w {
        return;
    }
    gauss_line(id.x, P.w, P.h);
}

// Rays (`bv-effects`' `Beams`): each pixel's light — its color by how far
// past Threshold (f10) its brightness, or its alpha with j.y's bit 0, is —
// from the picture smoothed by the binomial 3 × 3 with j.y's bit 1; with
// the mask on Source (j.w 2) times the mask (f11 on).
fn rays_at(x: i32, y: i32) -> vec4<f32> {
    let cx = clamp(x, 0, i32(P.sw) - 1);
    let cy = clamp(y, 0, i32(P.sh) - 1);
    return src[u32(cy) * P.sw + u32(cx)];
}

@compute @workgroup_size(16, 16)
fn kit_rays_lights(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var c = rays_at(x, y);
    if (P.j.y & 2u) != 0u {
        let corners = rays_at(x - 1, y - 1) + rays_at(x + 1, y - 1) + rays_at(x - 1, y + 1)
            + rays_at(x + 1, y + 1);
        let sides = rays_at(x, y - 1) + rays_at(x - 1, y) + rays_at(x + 1, y) + rays_at(x, y + 1);
        c = (corners + 2.0 * sides + 4.0 * c) / 16.0;
    }
    var level = c.w;
    if (P.j.y & 1u) == 0u {
        var s = vec3<f32>(0.0);
        if c.w > 0.0 {
            s = c.xyz / c.w;
        }
        level = luma(s);
    }
    let t = pf(10u);
    let k = clamp((level - t) / max(1.0 - t, 1e-4), 0.0, 1.0);
    var light = c * k;
    if P.j.w == 2u {
        let u = (f32(id.x) + 0.5) / f32(P.w);
        let v = (f32(id.y) + 0.5) / f32(P.h);
        light = light * vm_mask(u, v, f32(P.w) / f32(P.h), 11u);
    }
    dst[o] = light;
}

// Glint (`bv-effects`' `Glint`): each pixel's light — its luma by how far
// past Threshold (f0) its straight luma is, smoothly, times the gain (f1)
// — from the picture smoothed by the binomial 3 × 3 with j.y; with the
// mask on Source (j.w 2) times the mask (f2 on); in both complex lanes.
@compute @workgroup_size(16, 16)
fn kit_glint_lights(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var c = rays_at(x, y);
    if P.j.y != 0u {
        let corners = rays_at(x - 1, y - 1) + rays_at(x + 1, y - 1) + rays_at(x - 1, y + 1)
            + rays_at(x + 1, y + 1);
        let sides = rays_at(x, y - 1) + rays_at(x - 1, y) + rays_at(x + 1, y) + rays_at(x, y + 1);
        c = (corners + 2.0 * sides + 4.0 * c) / 16.0;
    }
    var s = vec3<f32>(0.0);
    if c.w > 0.0 {
        s = c.xyz / c.w;
    }
    let t = pf(0u);
    var k = 0.0;
    if t >= 1.0 {
        k = select(0.0, 1.0, luma(s) >= t);
    } else {
        k = smoothstep(t, 1.0, luma(s));
    }
    var l = luma(c.xyz) * k * pf(1u);
    if P.j.w == 2u {
        let u = (f32(id.x) + 0.5) / f32(P.w);
        let v = (f32(id.y) + 0.5) / f32(P.h);
        l = l * vm_mask(u, v, f32(P.w) / f32(P.h), 2u);
    }
    dst[o] = vec4<f32>(l, 0.0, l, 0.0);
}

// Flicker Control's Histogram smoothing (`flicker_control::match_pixel`):
// a channel `k` of value `v` whose level (it times 1024, cut down) is
// within `i.x`–`i.y` taken to the level the table (`src`, a level a pixel:
// its red, green and blue matches) matches it to.
fn flicker_matched(v: f32, k: u32) -> f32 {
    let x = v * 1024.0;
    if !(x > -1.0 && x < 1024.0) {
        return v;
    }
    let l = i32(x);
    if l < P.i.x || l >= P.i.y {
        return v;
    }
    let t = src[u32(l)];
    var m = t.x;
    if k == 1u {
        m = t.y;
    } else if k == 2u {
        m = t.z;
    }
    return m * (1.0 / 1024.0);
}

// Flicker Control's Histogram smoothing on the picture (`dst`). `j.x`:
// Show mask only.
@compute @workgroup_size(16, 16)
fn kit_flicker_match(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let c = dst[o];
    var r = vec4<f32>(
        flicker_matched(c.x, 0u),
        flicker_matched(c.y, 1u),
        flicker_matched(c.z, 2u),
        c.w,
    );
    if P.j.x != 0u {
        r = flicker_mask(c, r);
    }
    dst[o] = r;
}

// Flicker Control's Temporal blend's end (`flicker_control::blend`): the
// sums (`src`: `w` × `h` sums of the frames' colors and weights, then `w`
// × `h` alphas) as the picture (`dst`) — for Type All (`j.x` 3) as they
// are, else over the weights. `j.y`: Show mask only.
@compute @workgroup_size(16, 16)
fn kit_flicker_finish(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let s = src[o];
    let a = src[u32(o) + P.w * P.h].x;
    var rgb = s.xyz;
    if P.j.x != 3u {
        rgb = s.xyz / s.w;
    }
    var r = vec4<f32>(rgb, a);
    if P.j.y != 0u {
        r = flicker_mask(dst[o], r);
    }
    dst[o] = r;
}

// Denoise (`bv-effects`' `looks::denoise`): the picture's BT.601 planes
// in 8-bit steps (`looks::yuv_of`: Y, U, V, then the alpha) into `dst`,
// the picture (`src`, `sw` × `sh`) reflected `w − sw` over 2 pixels past
// its edges, its end pixels not repeated.
fn dn_reflected(i: i32, n: i32) -> i32 {
    var k = i;
    if k < 0 {
        k = -k;
    } else if k >= n {
        k = 2 * (n - 1) - k;
    }
    return clamp(k, 0, n - 1);
}

@compute @workgroup_size(16, 16)
fn kit_denoise_planes(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let reach = i32(P.w - P.sw) / 2;
    let x = dn_reflected(i32(id.x) - reach, i32(P.sw));
    let y = dn_reflected(i32(id.y) - reach, i32(P.sh));
    let c = src[u32(y) * P.sw + u32(x)];
    let r = c.x * 255.0;
    let g = c.y * 255.0;
    let b = c.z * 255.0;
    dst[o] = vec4<f32>(
        r * 0.299 + g * 0.587 + b * 0.114,
        (g * -0.28886 - r * 0.14713) + b * 0.436,
        (r * 0.615 - g * 0.51499) - b * 0.10001,
        c.w,
    );
}

// Denoise's non-local means (`looks::non_local_means`) of plane `j.x` of
// the reflected planes (`src`, 13 pixels past the picture each way) into
// that plane of `dst` (`w` × `h`) at strength `f[0]`: each pixel the mean
// of those within 10 of it, each weighted by how alike the 7 × 7 patches
// around the two are — their mean absolute difference rounded, `d`:
// exp(−max(d − 2h², 0)/h²), none under 0.001. A workgroup's 16 × 16
// pixels keep the 42 × 42 around them; each of the 21 × 21 moves' patch
// differences summed across, then down.
var<workgroup> dn_tile: array<f32, 1764>;
var<workgroup> dn_diff: array<f32, 484>;
var<workgroup> dn_rows: array<f32, 352>;

fn dn_plane(v: vec4<f32>, k: u32) -> f32 {
    if k == 0u {
        return v.x;
    }
    if k == 1u {
        return v.y;
    }
    return v.z;
}

@compute @workgroup_size(16, 16)
fn kit_denoise_nlm(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
    @builtin(workgroup_id) wg: vec3<u32>,
) {
    let k = P.j.x;
    let x0 = wg.x * 16u;
    let y0 = wg.y * 16u;
    for (var i = li; i < 1764u; i += 256u) {
        let sx = min(x0 + i % 42u, P.sw - 1u);
        let sy = min(y0 + i / 42u, P.sh - 1u);
        dn_tile[i] = dn_plane(src[sy * P.sw + sx], k);
    }
    workgroupBarrier();
    let lx = li % 16u;
    let ly = li / 16u;
    let h = pf(0u);
    let h2 = h * h;
    var acc = 0.0;
    var total = 0.0;
    for (var dy = 0u; dy < 21u; dy++) {
        for (var dx = 0u; dx < 21u; dx++) {
            for (var i = li; i < 484u; i += 256u) {
                let c = i % 22u;
                let r = i / 22u;
                dn_diff[i] = abs(dn_tile[(r + 10u) * 42u + c + 10u] - dn_tile[(r + dy) * 42u + c + dx]);
            }
            workgroupBarrier();
            for (var i = li; i < 352u; i += 256u) {
                let c = i % 16u;
                let r = i / 16u;
                var s = 0.0;
                for (var j = 0u; j < 7u; j++) {
                    s += dn_diff[r * 22u + c + j];
                }
                dn_rows[i] = s;
            }
            workgroupBarrier();
            var s = 0.0;
            for (var j = 0u; j < 7u; j++) {
                s += dn_rows[(ly + j) * 16u + lx];
            }
            let d = rround(s * (1.0 / 49.0));
            var w = exp(-max(d - 2.0 * h2, 0.0) / h2);
            if w < 0.001 {
                w = 0.0;
            }
            acc += w * dn_tile[(ly + dy + 3u) * 42u + lx + dx + 3u];
            total += w;
            workgroupBarrier();
        }
    }
    if id.x < P.w && id.y < P.h {
        let o = id.y * P.w + id.x;
        var v = dst[o];
        let r = acc / total;
        if k == 0u {
            v.x = r;
        } else if k == 1u {
            v.y = r;
        } else {
            v.z = r;
        }
        dst[o] = v;
    }
}

// Denoise's picture (`dst`) back from its planes (`src`,
// `looks::rgb_of`): BT.601's RGB, each channel within 0–1, the alpha too.
@compute @workgroup_size(16, 16)
fn kit_denoise_rgb(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let p = src[o];
    let r = p.y * 0.0 + p.x + p.z * 1.13983;
    let g = (p.x - p.y * 0.39465) - p.z * 0.5806;
    let b = p.y * 2.03211 + p.x + p.z * 0.0;
    let rgb = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(255.0)) / 255.0;
    dst[o] = vec4<f32>(rgb, clamp(p.w, 0.0, 1.0));
}

// Denoise's Sharpening (`looks::sharpened`): every channel of the picture
// (`src`) through the 3 × 3 kernel −1 −2 −1, −2 c −2, −1 −2 −1 (`f[0]`
// c) over c − 12, its edges held, within 0–1; the colors then at most the
// alpha.
@compute @workgroup_size(16, 16)
fn kit_denoise_sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let xl = u32(max(x - 1, 0));
    let xr = u32(min(x + 1, i32(P.w) - 1));
    let yu = u32(max(y - 1, 0)) * P.w;
    let yd = u32(min(y + 1, i32(P.h) - 1)) * P.w;
    let yc = u32(y) * P.w;
    let xc = u32(x);
    let c = pf(0u);
    let corners = src[yu + xl] + src[yu + xr] + src[yd + xl] + src[yd + xr];
    let sides = src[yu + xc] + src[yc + xl] + src[yc + xr] + src[yd + xc];
    let s = src[yc + xc] * c - corners - sides * 2.0;
    let v = clamp(s / (c - 12.0), vec4<f32>(0.0), vec4<f32>(1.0));
    dst[o] = vec4<f32>(min(v.xyz, vec3<f32>(v.w)), v.w);
}

// Smart Zoom's enhancement: the luma's difference from the binomial 3 × 3
// around it, lifted by f0 where it stands out of f1, the edges held.
fn enhance_luma(x: i32, y: i32) -> f32 {
    let i = clamp(x, 0, i32(P.w) - 1);
    let j = clamp(y, 0, i32(P.h) - 1);
    let c = src[u32(j) * P.w + u32(i)];
    return c.x * 0.299 + c.y * 0.587 + c.z * 0.114;
}

fn enhance_row(x: i32, y: i32) -> f32 {
    return enhance_luma(x - 1, y) + enhance_luma(x, y) * 2.0 + enhance_luma(x + 1, y);
}

@compute @workgroup_size(16, 16)
fn kit_smart_enhance(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let blur = (enhance_row(x, y - 1) + enhance_row(x, y) * 2.0 + enhance_row(x, y + 1)) * 0.0625;
    let d = enhance_luma(x, y) - blur;
    let threshold = pf(1u);
    let t = clamp((abs(d) - threshold) / max(threshold, 0.000001), 0.0, 1.0);
    let lift = d * pf(0u) * (t * t * (3.0 - 2.0 * t));
    let c = src[id.y * P.w + id.x];
    dst[o] = vec4<f32>(c.x + lift, c.y + lift, c.z + lift, c.w);
}

// AI Smoothen and AI Sharpen. The small copy (`P.w` × `P.h`) of the
// picture (`P.sw` × `P.sh`, rows from the top) framed by a margin of
// `i.x` pixels reflected as OpenCV's, taken as `(x + 0.1 f1) f0`, read as
// `cv::resize` reads it (half-pixel centers, between four pixels): f2 and
// f3 the framed size.
fn guided_reflect(i: i32, n: i32) -> i32 {
    var k = i;
    for (var t = 0; t < 8; t = t + 1) {
        if k < 0 {
            k = -k - 1;
        } else if k >= n {
            k = 2 * n - k - 1;
        }
    }
    return clamp(k, 0, n - 1);
}

fn guided_high(x: i32, y: i32) -> vec3<f32> {
    let m = P.i.x;
    let sx = guided_reflect(x - m, i32(P.sw));
    let sy = guided_reflect(y - m, i32(P.sh));
    let c = src[u32(sy) * P.sw + u32(sx)].xyz;
    return (c + vec3<f32>(0.1 * pf(1u))) * pf(0u);
}

fn guided_tap(i: u32, n: u32, size: f32) -> vec2<f32> {
    let f = (f32(i) + 0.5) * (size / f32(n)) - 0.5;
    let s = floor(f);
    if s < 0.0 {
        return vec2<f32>(0.0, 0.0);
    }
    if s >= size - 1.0 {
        return vec2<f32>(size - 1.0, 0.0);
    }
    return vec2<f32>(s, f - s);
}

@compute @workgroup_size(16, 16)
fn kit_guided_low(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let pw = pf(2u);
    let ph = pf(3u);
    let tx = guided_tap(id.x, P.w, pw);
    let ty = guided_tap(id.y, P.h, ph);
    let x0 = i32(tx.x);
    let y0 = i32(ty.x);
    let x1 = min(x0 + 1, i32(pw) - 1);
    let y1 = min(y0 + 1, i32(ph) - 1);
    let fx = tx.y;
    let fy = ty.y;
    let top = guided_high(x0, y0) * (1.0 - fx) + guided_high(x1, y0) * fx;
    let bottom = guided_high(x0, y1) * (1.0 - fx) + guided_high(x1, y1) * fx;
    dst[id.y * P.w + id.x] = vec4<f32>(top * (1.0 - fy) + bottom * fy, 0.0);
}

fn guided_at(i: i32, j: i32) -> vec3<f32> {
    let x = clamp(i, 0, i32(P.w) - 1);
    let y = clamp(j, 0, i32(P.h) - 1);
    return src[u32(y) * P.w + u32(x)].xyz;
}

// What BananaV works out on the small copy: `j.x` 0 a binomial 5 × 5
// (Smoothen), 1 its difference from a binomial 3 × 3 twice over
// (Sharpen); the edges held.
@compute @workgroup_size(16, 16)
fn kit_guided_op(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = i32(id.x);
    let j = i32(id.y);
    var out = vec3<f32>(0.0);
    if P.j.x == 1u {
        let p = guided_at(i, j);
        let r0 = guided_at(i - 1, j - 1) + guided_at(i, j - 1) * 2.0 + guided_at(i + 1, j - 1);
        let r1 = guided_at(i - 1, j) + guided_at(i, j) * 2.0 + guided_at(i + 1, j);
        let r2 = guided_at(i - 1, j + 1) + guided_at(i, j + 1) * 2.0 + guided_at(i + 1, j + 1);
        let blur = (r0 + r1 * 2.0 + r2) * 0.0625;
        out = p + (p - blur) * 2.0;
    } else {
        var acc = vec3<f32>(0.0);
        for (var dj = -2; dj <= 2; dj = dj + 1) {
            let wy = select(select(6.0, 4.0, abs(dj) == 1), 1.0, abs(dj) == 2);
            var row = vec3<f32>(0.0);
            for (var di = -2; di <= 2; di = di + 1) {
                let wx = select(select(6.0, 4.0, abs(di) == 1), 1.0, abs(di) == 2);
                row = row + guided_at(i + di, j + dj) * wx;
            }
            acc = acc + row * wy;
        }
        out = acc * (1.0 / 256.0);
    }
    dst[id.y * P.w + id.x] = vec4<f32>(out, 0.0);
}

fn guided_pair(i: i32, j: i32, which: u32) -> vec3<f32> {
    let x = clamp(i, 0, i32(P.w) - 1);
    let y = clamp(j, 0, i32(P.h) - 1);
    return src[(u32(y) * P.w + u32(x)) * 2u + which].xyz;
}

// The guided filter's coefficients `a` and `b` (two entries a pixel)
// averaged again over 3 × 3, the edges held.
@compute @workgroup_size(16, 16)
fn kit_guided_mean(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = i32(id.x);
    let j = i32(id.y);
    var a = vec3<f32>(0.0);
    var b = vec3<f32>(0.0);
    for (var dj = -1; dj <= 1; dj = dj + 1) {
        for (var di = -1; di <= 1; di = di + 1) {
            a = a + guided_pair(i + di, j + dj, 0u);
            b = b + guided_pair(i + di, j + dj, 1u);
        }
    }
    let o = (id.y * P.w + id.x) * 2u;
    dst[o] = vec4<f32>(a * (1.0 / 9.0), 0.0);
    dst[o + 1u] = vec4<f32>(b * (1.0 / 9.0), 0.0);
}

// AI Dehaze's dark channel of the small copy (`P.w` × `P.h`): the least
// of the colors over 3 × 3, the edges held.
@compute @workgroup_size(16, 16)
fn kit_dehaze_dark(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = i32(id.x);
    let j = i32(id.y);
    var d = 3.0e38;
    for (var dj = -1; dj <= 1; dj = dj + 1) {
        for (var di = -1; di <= 1; di = di + 1) {
            let p = guided_at(i + di, j + dj);
            d = min(d, min(p.x, min(p.y, p.z)));
        }
    }
    dst[id.y * P.w + id.x] = vec4<f32>(d, 0.0, 0.0, 0.0);
}
