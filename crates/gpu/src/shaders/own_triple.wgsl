// Layer Dimensionality on the card (`bv-effects`'s plug-in, the same math
// as its render), in two kernels: the picture's surface, then what is under
// it. `ta`: the picture (`w` × `h`, rows from the top); `tb`: its alpha
// blurred four ways (`sw` × `sh`, `i.x` pixels around the picture) — x for
// the shadow, y the glow around the edge, z the glow inside it, w the
// bevel. The plug-in counts rows up: one up is a row less here.
//
// i: the field's margin (x), the shadow's move in pixels across (y) and
// up (z); j: the shadow's type (x: 1 dropped, 2 inside), the glow's (y: 1
// outer, 2 inner, 3 edge, 4 dual), embossed (z). f: the shadow's opacity
// (0), the glow's color (1–3), the picture's and the ground's rise (4, 5),
// the light's direction (6–8), ambience (9), intensity (10), shininess
// (11), the bevel's steepness (12), the picture's opacity (13), how much
// of the shine (14) and of the ground's shading (15) show.

fn layer_field(x: i32, y: i32) -> vec4<f32> {
    let fx = x + P.i.x;
    let fy = y + P.i.x;
    if fx < 0 || fy < 0 || fx >= i32(P.sw) || fy >= i32(P.sh) {
        return vec4<f32>(0.0);
    }
    return tb[u32(fy) * P.sw + u32(fx)];
}

// The blurred alpha where the shadow comes from, for pixel (x, y).
fn layer_shadow(x: i32, y: i32) -> f32 {
    return layer_field(x - P.i.y, y + P.i.z).x;
}

fn layer_bevel(x: i32, y: i32) -> f32 {
    return layer_field(x, y).w;
}

// The light on the bevel at (x, y), its surface rising `rise` with the
// blurred alpha: how much brighter than flat (x) and its shine (y).
fn layer_shade(rise: f32, x: i32, y: i32) -> vec2<f32> {
    let steep = pf(12u);
    let nx = -(rise * layer_bevel(x + 1, y) - rise * layer_bevel(x - 1, y)) / 2.0 * steep;
    let ny = -(rise * layer_bevel(x, y - 1) - rise * layer_bevel(x, y + 1)) / 2.0 * steep;
    let n = vec3<f32>(nx, ny, 1.0) / length(vec3<f32>(nx, ny, 1.0));
    let l = vec3<f32>(pf(6u), pf(7u), pf(8u));
    let ambience = pf(9u);
    let intensity = pf(10u);
    let flat = max(ambience + intensity * l.z, 0.05);
    let lit = ambience + intensity * dot(n, l);
    let k = clamp(lit / flat, 0.0, 3.0);
    let half = vec3<f32>(l.x, l.y, l.z + 1.0);
    let hl = max(length(half), 1e-9);
    let e = 4.0 + 60.0 * pf(11u);
    let sn = max(dot(n, half) / hl, 0.0);
    let sf = max(half.z / hl, 0.0);
    var spec_n = 0.0;
    if sn > 0.0 {
        spec_n = pow(sn, e);
    }
    var spec_f = 0.0;
    if sf > 0.0 {
        spec_f = pow(sf, e);
    }
    let shine = pf(14u) * pf(11u) * max(spec_n - spec_f, 0.0);
    return vec2<f32>(k, shine);
}

fn layer_index(id: vec3<u32>) -> i32 {
    if id.x >= P.w || id.y >= P.h {
        return -1;
    }
    return i32(id.y * P.w + id.x);
}

// The picture's surface: embossed, an inner shadow, a glow inside its
// edge, then its opacity.
@compute @workgroup_size(16, 16)
fn layer_surface(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = layer_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var px = ta[o];
    let a = px.w;
    let rise = pf(4u);
    if P.j.z != 0u && rise != 0.0 && a > 0.0 {
        let s = layer_shade(rise, x, y);
        px = vec4<f32>(
            max(px.x * s.x + s.y * a, 0.0),
            max(px.y * s.x + s.y * a, 0.0),
            max(px.z * s.x + s.y * a, 0.0),
            a,
        );
    }
    if P.j.x == 2u {
        let k = clamp((1.0 - layer_shadow(x, y)) * pf(0u), 0.0, 1.0);
        px = vec4<f32>(px.x * (1.0 - k), px.y * (1.0 - k), px.z * (1.0 - k), a);
    }
    if P.j.y == 2u || P.j.y == 4u {
        let k = clamp(1.0 - layer_field(x, y).z, 0.0, 1.0);
        px = vec4<f32>(
            px.x + (pf(1u) * a - px.x) * k,
            px.y + (pf(2u) * a - px.y) * k,
            px.z + (pf(3u) * a - px.z) * k,
            a,
        );
    }
    dst[o] = px * pf(13u);
}

// Under the picture (`dst`, its surface, changed in place): the ground
// embossed, a dropped shadow, the glow around, the picture over them.
@compute @workgroup_size(16, 16)
fn layer_under(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = layer_index(id);
    if o < 0 {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let a = ta[o].w;
    var under = vec4<f32>(0.0);
    let ground = pf(5u);
    if P.j.z != 0u && ground != 0.0 && a < 1.0 {
        let s = layer_shade(-ground, x, y);
        let open = 1.0 - a;
        let dark = clamp(1.0 - s.x, 0.0, 1.0) * pf(15u) * open;
        let light = min((max(s.x - 1.0, 0.0) + s.y) * pf(15u), 1.0) * open;
        under = vec4<f32>(light, light, light, light + dark * (1.0 - light));
    }
    if P.j.x == 1u {
        let k = layer_shadow(x, y) * pf(0u);
        under = vec4<f32>(
            under.x * (1.0 - k),
            under.y * (1.0 - k),
            under.z * (1.0 - k),
            k + under.w * (1.0 - k),
        );
    }
    if P.j.y == 1u || P.j.y == 3u || P.j.y == 4u {
        let k = min(layer_field(x, y).y, 1.0);
        under = vec4<f32>(
            pf(1u) * k + under.x * (1.0 - k),
            pf(2u) * k + under.y * (1.0 - k),
            pf(3u) * k + under.z * (1.0 - k),
            k + under.w * (1.0 - k),
        );
    }
    let fg = dst[o];
    dst[o] = fg + under * (1.0 - fg.w);
}

// Black Bar Fill: the foreground (`ta`) over its shadow — its alpha
// blurred (`tb`, `sw` × `sh`, `i.x` pixels around the frame), black at f 0
// — over the background (`dst`, written in its place).
@compute @workgroup_size(16, 16)
fn fill_combine(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let m = u32(P.i.x);
    let fg = ta[o];
    let s = tb[(id.y + m) * P.sw + id.x + m].w * pf(0u);
    let bg = dst[o];
    let under = vec4<f32>(bg.xyz * (1.0 - s), bg.w * (1.0 - s) + s);
    dst[o] = fg + under * (1.0 - fg.w);
}

// Bézier Masking's Mask FX: the effects before it (`dst`) over the
// chain's input (`tb`) as much as the masks keep (`ta`'s, worked out by
// `bezier_masking`).
@compute @workgroup_size(16, 16)
fn bezier_mask_fx(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = id.y * P.w + id.x;
    let u = tb[i];
    dst[i] = u + (dst[i] - u) * ta[i].x;
}

// Median and Min and Max (`bv-effects`' `ranked`, the same math): a group
// of threads a row (rows up: the group's number), the area's 8-bit
// histograms (red, green, blue, alpha) and their total weight in the
// group's memory, slid along the row as the processor's; each pixel the
// floor of the mean over its rank's 16384ths of the weights. `ta` is the
// picture; `tb` the table — j.x entries (dx, dy, weight) of the area at
// the row's first pixel, then j.y (dy, dx, change) a pixel to the right;
// i.x the rank in 16384ths.
var<workgroup> rank_hist: array<atomic<u32>, 1024>;
var<workgroup> rank_total: atomic<u32>;
var<workgroup> rank_values: array<f32, 4>;

fn rank_level(c: f32) -> u32 {
    return u32(rround(clamp(c, 0.0, 1.0) * 255.0));
}

// Pixel (x, y)'s levels weighed by `weight` (a change below 0 wraps).
fn rank_weigh(x: i32, y: i32, weight: u32) {
    if x < 0 || x >= i32(P.w) || y < 0 || y >= i32(P.h) {
        return;
    }
    let c = ta[u32(i32(P.h) - 1 - y) * P.w + u32(x)];
    atomicAdd(&rank_hist[rank_level(c.x)], weight);
    atomicAdd(&rank_hist[256u + rank_level(c.y)], weight);
    atomicAdd(&rank_hist[512u + rank_level(c.z)], weight);
    atomicAdd(&rank_hist[768u + rank_level(c.w)], weight);
    atomicAdd(&rank_total, weight);
}

// The mean value of the weights from `lo` to `hi` of histogram `base`.
fn rank_value(base: u32, lo: u32, hi: u32) -> f32 {
    var before = 0u;
    var sum = 0u;
    for (var v = 0u; v < 256u; v++) {
        let n = atomicLoad(&rank_hist[base + v]);
        if n == 0u {
            continue;
        }
        let after = before + n;
        let top = min(after, hi);
        let bottom = max(before, lo);
        if top > bottom {
            sum = sum + (top - bottom) * v;
        }
        if after >= hi {
            break;
        }
        before = after;
    }
    return f32(min(sum / 16384u, 255u)) / 255.0;
}

@compute @workgroup_size(256)
fn kit_median(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) t: u32,
) {
    let j = i32(group.x);
    for (var k = t; k < 1024u; k += 256u) {
        atomicStore(&rank_hist[k], 0u);
    }
    if t == 0u {
        atomicStore(&rank_total, 0u);
    }
    workgroupBarrier();
    for (var k = t; k < P.j.x; k += 256u) {
        let e = tb[k];
        rank_weigh(i32(e.x), j + i32(e.y), u32(i32(e.z)));
    }
    workgroupBarrier();
    let rank = u32(P.i.x);
    let row = u32(i32(P.h) - 1 - j) * P.w;
    for (var x = 0; x < i32(P.w); x++) {
        if t < 4u {
            let over = atomicLoad(&rank_total) - 16384u;
            let lo = (over / 16384u) * rank + ((over % 16384u) * rank) / 16384u;
            rank_values[t] = rank_value(t * 256u, lo, lo + 16384u);
        }
        workgroupBarrier();
        if t == 0u {
            dst[row + u32(x)] = vec4<f32>(rank_values[0], rank_values[1], rank_values[2], rank_values[3]);
        }
        if x + 1 < i32(P.w) {
            for (var k = t; k < P.j.y; k += 256u) {
                let e = tb[P.j.x + k];
                rank_weigh(x + i32(e.y), j + i32(e.x), u32(i32(e.z)));
            }
        }
        workgroupBarrier();
    }
}

// The spatial effects' convolution (`bv-effects`' `spatial::convolve`):
// `ta` through the taps in `tb`'s x (j.x of them, centered) along each
// row, the edge pixels held.
@compute @workgroup_size(16, 16)
fn kit_conv_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let n = i32(P.j.x);
    let r = n / 2;
    let base = id.y * P.w;
    var sum = vec4<f32>(0.0);
    for (var t = 0; t < n; t++) {
        let x = clamp(i32(id.x) + t - r, 0, i32(P.w) - 1);
        sum = sum + tb[t].x * ta[base + u32(x)];
    }
    dst[base + id.x] = sum;
}

// The same down each column: tap t reads the row t - r above in the
// plug-ins' count (rows up), the edge rows held.
@compute @workgroup_size(16, 16)
fn kit_conv_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let n = i32(P.j.x);
    let r = n / 2;
    var sum = vec4<f32>(0.0);
    for (var t = 0; t < n; t++) {
        let y = clamp(i32(id.y) - t + r, 0, i32(P.h) - 1);
        sum = sum + tb[t].x * ta[u32(y) * P.w + id.x];
    }
    dst[id.y * P.w + id.x] = sum;
}

// The compositors (`bv-effects`' `compositors.rs`; Bump Map's in
// `looks.rs`): `ta` the picture drawn, `tb` what guides it — the map, or
// Height Map's smoothed heights in x —, both `w` × `h`, rows from the top;
// the plug-ins count rows up.
fn guide_picture(x: i32, y: i32) -> vec4<f32> {
    let cx = clamp(x, 0, i32(P.w) - 1);
    let cy = clamp(y, 0, i32(P.h) - 1);
    return ta[u32(i32(P.h) - 1 - cy) * P.w + u32(cx)];
}

fn guide_map(x: i32, y: i32) -> vec4<f32> {
    let cx = clamp(x, 0, i32(P.w) - 1);
    let cy = clamp(y, 0, i32(P.h) - 1);
    return tb[u32(i32(P.h) - 1 - cy) * P.w + u32(cx)];
}

// The picture's pixel xi, yi (rows up): nothing outside it, or (`wrap`)
// the picture tiled.
fn guide_texel(xi: i32, yi: i32, wrap: bool) -> vec4<f32> {
    let w = i32(P.w);
    let h = i32(P.h);
    var x = xi;
    var y = yi;
    if wrap {
        x = ((x % w) + w) % w;
        y = ((y % h) + h) % h;
    } else if x < 0 || x >= w || y < 0 || y >= h {
        return vec4<f32>(0.0);
    }
    return ta[u32(h - 1 - y) * P.w + u32(x)];
}

// `compositors::sample`: the picture at x, y (rows up) between its four
// pixels, VEGAS's weights in its order.
fn guide_sample(x: f32, y: f32, wrap: bool) -> vec4<f32> {
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let xi = i32(x0);
    let yi = i32(y0);
    let a = guide_texel(xi, yi, wrap);
    let b = guide_texel(xi + 1, yi, wrap);
    let c = guide_texel(xi, yi + 1, wrap);
    let d = guide_texel(xi + 1, yi + 1, wrap);
    return b * ((1.0 - fy) * fx) + a * ((1.0 - fx) * (1.0 - fy)) + c * ((1.0 - fx) * fy) + d * (fy * fx);
}

// The map's channel c (0 red, 1 green, 2 blue, 3 alpha) at x, y (rows
// up), edges held, in 8-bit steps.
fn guide_byte(x: i32, y: i32, c: u32) -> i32 {
    let m = guide_map(x, y);
    var v = m.w;
    if c == 0u {
        v = m.x;
    }
    if c == 1u {
        v = m.y;
    }
    if c == 2u {
        v = m.z;
    }
    return i32(step8(v));
}

// A map value c by its alpha a, in 0-1 (none where a is 0).
fn guide_unit(c: i32, a: i32) -> f32 {
    if a == 0 {
        return 0.0;
    }
    return f32(c) * 0.003921569 * (1.0 / (f32(a) * 0.003921569));
}

// Displacement Map's move along channel c at map pixel x, y
// (`compositors::displacement_at`): the pixel twice and its diagonal
// neighbours less the middle gray, averaged where their squared
// differences stay under 101 steps, else the pixel alone; straight
// values unless the alpha scales them (j.z).
fn guide_move(x: i32, y: i32, c: u32) -> f32 {
    let a0 = guide_byte(x, y, 3u);
    let a1 = guide_byte(x - 1, y - 1, 3u);
    let a2 = guide_byte(x + 1, y - 1, 3u);
    let a3 = guide_byte(x - 1, y + 1, 3u);
    let a4 = guide_byte(x + 1, y + 1, 3u);
    let alphas = 2 * a0 + a1 + a2 + a3 + a4;
    let c0 = guide_byte(x, y, c);
    let n1 = guide_byte(x - 1, y - 1, c);
    let n2 = guide_byte(x + 1, y - 1, c);
    let n3 = guide_byte(x - 1, y + 1, c);
    let n4 = guide_byte(x + 1, y + 1, c);
    if P.j.z != 0u || alphas >= 1530 {
        let spread = (n1 - c0) * (n1 - c0) + (n2 - c0) * (n2 - c0) + (n3 - c0) * (n3 - c0)
            + (n4 - c0) * (n4 - c0);
        var v = f32(c0) - f32(a0) * 0.5019608;
        if spread < 101 {
            let sum = 2 * c0 + n1 + n2 + n3 + n4;
            if alphas == 1530 {
                v = f32(sum - 768);
            } else {
                v = f32(sum) - f32(alphas) * 0.5019608;
            }
            v = v * 0.16666667;
        }
        return v * 0.0078125;
    }
    let k0 = guide_unit(c0, a0);
    let k1 = guide_unit(n1, a1);
    let k2 = guide_unit(n2, a2);
    let k3 = guide_unit(n3, a3);
    let k4 = guide_unit(n4, a4);
    let spread = (k1 - k0) * (k1 - k0) + (k2 - k0) * (k2 - k0) + (k3 - k0) * (k3 - k0)
        + (k4 - k0) * (k4 - k0);
    var v = k0 - 0.5019608;
    if spread <= 0.00153787 {
        v = (k0 + k0 + k1 + k2 + k3 + k4 - 3.011765) * 0.16666667;
    }
    return v * 1.9921875;
}

// Displacement Map: each pixel shows the picture as far across and up as
// the map's channels say (j.x, j.y: 0 red, 1 green, 2 blue) times the
// scales' reach in pixels (f 0, 1); j.w Wrap pixels around.
@compute @workgroup_size(16, 16)
fn kit_displace(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let x = i32(id.x);
    let y = i32(P.h - 1u - id.y);
    let dx = guide_move(x, y, P.j.x) * pf(0u);
    let dy = guide_move(x, y, P.j.y) * pf(1u);
    dst[id.y * P.w + id.x] = guide_sample(f32(x) + dx, f32(y) + dy, P.j.w != 0u);
}

// Height Map's heights from the map `ta` (`compositors::height_fixed`):
// its channel (j.x: 0 the three's sum, 1 red, 2 green, 3 blue — a byte
// counting three) in 8-bit steps, straight unless the alpha scales it
// (j.z), inverted with j.y; in fixed point, 765ths of the full height and
// i.x bits more, in x (exact integers).
@compute @workgroup_size(16, 16)
fn kit_heights(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = id.y * P.w + id.x;
    let m = ta[i];
    let b = vec4<u32>(step8(m.x), step8(m.y), step8(m.z), step8(m.w));
    var sum = b.x + b.y + b.z;
    var n = 3u;
    if P.j.x == 1u {
        sum = b.x;
        n = 1u;
    }
    if P.j.x == 2u {
        sum = b.y;
        n = 1u;
    }
    if P.j.x == 3u {
        sum = b.z;
        n = 1u;
    }
    let bits = u32(P.i.x);
    let full = 765u << bits;
    var h = 0u;
    if P.j.z != 0u || b.w == 255u {
        h = (sum * (3u / n)) << bits;
    } else if b.w != 0u {
        let d = n * b.w;
        let q = min(sum * 765u, d * 765u);
        h = ((q / d) << bits) + (((q % d) << bits) + d / 2u) / d;
    }
    if P.j.y != 0u {
        h = full - min(h, full);
    }
    dst[i] = vec4<f32>(f32(h), 0.0, 0.0, 0.0);
}

// Height Map (`compositors::height`): the smoothed heights `tb` (x, in
// fixed point) times a unit's height in pixels (f 0) shape a surface;
// each pixel shows the picture moved along its slopes by its depth there
// — its height times Height scale (f 1) plus Elevation's depth (f 2); j.w
// Wrap pixels around. The slopes' differences are taken in fixed point,
// before any product: a fused product and difference would leave a last
// bit where the surface is flat, which the depth magnifies.
fn guide_height(x: i32, y: i32) -> f32 {
    return guide_map(x, y).x;
}

@compute @workgroup_size(16, 16)
fn kit_height_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let x = i32(id.x);
    let y = i32(P.h - 1u - id.y);
    let depth = guide_height(x, y) * pf(0u) * pf(1u) + pf(2u);
    var dx = 0.0;
    var dy = 0.0;
    if depth > 0.0 {
        let across = guide_height(x + 1, y) - guide_height(x - 1, y);
        let up = guide_height(x, y + 1) - guide_height(x, y - 1);
        dx = across * pf(0u) * 0.5 * depth;
        dy = up * pf(0u) * 0.5 * depth;
    }
    dst[id.y * P.w + id.x] = guide_sample(f32(x) + dx, f32(y) + dy, P.j.w != 0u);
}

// Bump Map as a compositor: the picture lit by the map's bumps — the bump
// channel's weights f 12-15, the light `bump_light`'s.
fn guide_bump(x: i32, y: i32) -> f32 {
    return dot(guide_map(x, y), vec4<f32>(pf(12u), pf(13u), pf(14u), pf(15u)));
}

@compute @workgroup_size(16, 16)
fn kit_bump_with(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let x = i32(id.x);
    let y = i32(P.h - 1u - id.y);
    // Rows go up: +1 is the row above.
    var gx = 0.0;
    var gy = 0.0;
    for (var j = -1; j <= 1; j++) {
        gx = gx + guide_bump(x - 1, y + j) - guide_bump(x + 1, y + j);
        gy = gy + guide_bump(x + j, y - 1) - guide_bump(x + j, y + 1);
    }
    let c = guide_picture(x, y);
    dst[id.y * P.w + id.x] = vec4<f32>(c.xyz * bump_light(gx, gy, x, y), c.w);
}

// Wave (`bv-effects`' `filters.rs`, VEGAS's, rows counted from the top as
// VEGAS counts them, as the card's are): `tb` holds each row's move
// across, then each column's move down, in 16,384ths of a pixel (in two
// exact parts: 4,096s and the rest); each pixel of the picture `ta` reads
// it there, held within the picture less its last row and column, between
// pixels.
fn wave_entry(k: u32) -> i32 {
    let e = tb[k];
    return i32(e.x) * 4096 + i32(e.y);
}

fn wave_texel(x: i32, t: i32) -> vec4<f32> {
    return ta[u32(t) * P.w + u32(x)];
}

@compute @workgroup_size(16, 16)
fn kit_wave(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let w = i32(P.w);
    let h = i32(P.h);
    let x = i32(id.x);
    let t = i32(id.y);
    let sx = clamp(x * 16384 + wave_entry(id.y), 0, (w - 2) * 16384);
    let st = clamp(t * 16384 + wave_entry(P.h + id.x), 0, (h - 2) * 16384);
    let ix = sx >> 14u;
    let it = st >> 14u;
    let fx = f32(sx & 16383) / 16384.0;
    let ft = f32(st & 16383) / 16384.0;
    let a = wave_texel(ix, it);
    let b = wave_texel(ix + 1, it);
    let c = wave_texel(ix, it + 1);
    let d = wave_texel(ix + 1, it + 1);
    dst[id.y * P.w + id.x] = a * ((1.0 - fx) * (1.0 - ft)) + b * (fx * (1.0 - ft)) + c * ((1.0 - fx) * ft) + d * (fx * ft);
}

// Film Grain (`bv-effects`' `looks::Grain`, VEGAS's, rows from the top):
// `tb` holds the three 64 × 64 grain tables (x), then each column's and
// each row's two table places and how far between them; f: Amount (0),
// Chromaticity (1).
fn grain_value(t: u32, x: u32, y: u32) -> f32 {
    let c = tb[12288u + x];
    let r = tb[12288u + P.w + y];
    let base = t * 4096u;
    let row0 = base + u32(r.x) * 64u;
    let row1 = base + u32(r.y) * 64u;
    let i0 = u32(c.x);
    let i1 = u32(c.y);
    let fx = c.z;
    let fy = r.z;
    let top = fx * tb[row0 + i1].x + (1.0 - fx) * tb[row0 + i0].x;
    let bottom = fx * tb[row1 + i1].x + (1.0 - fx) * tb[row1 + i0].x;
    return top * (1.0 - fy) + bottom * fy;
}

@compute @workgroup_size(16, 16)
fn kit_film_grain(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let c = ta[o];
    let n0 = grain_value(0u, id.x, id.y);
    var n = vec3<f32>(n0);
    let k = pf(1u);
    if k > 0.0 {
        let rest = (1.0 - k) * n0;
        n = vec3<f32>(n0, k * grain_value(1u, id.x, id.y) + rest, k * grain_value(2u, id.x, id.y) + rest);
    }
    let a = pf(0u);
    dst[o] = vec4<f32>((n * 1.3 * c.xyz - c.xyz) * a + c.xyz, c.w);
}

// Light Rays (`bv-effects`' `looks::rays_drawn`, VEGAS's 8-bit math, rows
// from the top): `ta` the picture; `dst` holds 8-bit values while the rays
// are drawn (each pixel reads only those drawn before it). `tb`, a row for each |dy| from the light: the last |dx| the
// bound leaves whole and the first it hides. i: the light's column and
// row, the rays' weight (65,536ths), the threshold (256ths); j: the bound
// (0/1), the noise (255ths), the noise's first number; f: the ellipse's
// reciprocal half axes (0, 1), where its feather starts and that
// feather's reciprocal (2, 3), the source blended back (65,536ths, 4), the
// tint (256ths, 5-7).

fn lr_src(i: u32) -> vec4<u32> {
    let c = ta[i];
    return vec4<u32>(step8(c.x), step8(c.y), step8(c.z), step8(c.w));
}

// 64-bit numbers as (low, high) words.
fn lr_add(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let low = a.x + b.x;
    var carry = 0u;
    if low < a.x {
        carry = 1u;
    }
    return vec2<u32>(low, a.y + b.y + carry);
}

fn lr_mul(a: u32, b: u32) -> vec2<u32> {
    let a0 = a & 0xffffu;
    let a1 = a >> 16u;
    let b0 = b & 0xffffu;
    let b1 = b >> 16u;
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let mid = (p00 >> 16u) + (p01 & 0xffffu) + (p10 & 0xffffu);
    let low = (p00 & 0xffffu) | (mid << 16u);
    return vec2<u32>(low, a1 * b1 + (p01 >> 16u) + (p10 >> 16u) + (mid >> 16u));
}

fn lr_le(a: vec2<u32>, b: vec2<u32>) -> bool {
    return a.y < b.y || (a.y == b.y && a.x <= b.x);
}

// `v` (in 2^-54ths) rounded to a double's 53 bits, ties to even.
fn lr_round(v: vec2<u32>) -> vec2<u32> {
    var top = 0u;
    if v.y != 0u {
        top = 63u - countLeadingZeros(v.y);
    } else if v.x != 0u {
        top = 31u - countLeadingZeros(v.x);
    }
    if top <= 52u {
        return v;
    }
    let shift = top - 52u;
    let unit = 1u << shift;
    let rest = v.x & (unit - 1u);
    let half = unit >> 1u;
    var r = vec2<u32>(v.x & ~(unit - 1u), v.y);
    if rest > half || (rest == half && ((v.x >> shift) & 1u) == 1u) {
        r = lr_add(r, vec2<u32>(unit, 0u));
    }
    return r;
}

// Byte `b` times a third, as a double (in 2^-54ths).
fn lr_third(b: u32) -> vec2<u32> {
    let low = lr_mul(b, 0x55555555u);
    return lr_round(vec2<u32>(low.x, low.y + b * 0x155555u));
}

// Three bytes' thirds summed as VEGAS sums them (doubles), cut to a whole.
fn lr_thirds(a: u32, b: u32, c: u32) -> u32 {
    let s = lr_round(lr_add(lr_third(a), lr_third(b)));
    return lr_round(lr_add(s, lr_third(c))).y >> 22u;
}

// Reading `i` (1-3) toward the light along `d` (dx or dy) of a pixel
// `big` (dx² + dy²) away: the whole pixels it moves and the 256ths past
// them, exactly as VEGAS's doubles give them (0.99 of a pixel at most).
fn lr_step(d: i32, big: u32, i: u32) -> vec2<i32> {
    if d == 0 {
        return vec2<i32>(0, 0);
    }
    let a = u32(abs(d));
    var f = 0u;
    if !lr_le(lr_mul(a * a, 10000u), lr_mul(big, 9801u)) {
        f = select(select(760u, 506u, i == 2u), 253u, i == 1u);
    } else {
        // 256 i a ⁄ √big cut to a whole: the largest g with g² big ≤ n².
        let n = 256u * i * a;
        let nn = lr_mul(n, n);
        f = u32(floor(f32(n) / sqrt(f32(big))));
        if lr_le(lr_mul((f + 1u) * (f + 1u), big), nn) {
            f += 1u;
        } else if !lr_le(lr_mul(f * f, big), nn) {
            f -= 1u;
        }
    }
    if d > 0 {
        return vec2<i32>(i32(f >> 8u), i32(f & 255u));
    }
    return vec2<i32>(-i32(f >> 8u) - 1, i32(255u - (f & 255u)));
}

fn lr_corner(x: i32, y: i32, past: bool) -> vec4<u32> {
    if past {
        return vec4<u32>(0u, 0u, 255u, 0u);
    }
    return vec4<u32>(dst[u32(y * i32(P.w) + x)]);
}

// Reading `i` of pixel x, y of quarter `q`, its own corner `mine`.
fn lr_read(x: i32, y: i32, dx: i32, dy: i32, big: u32, i: u32, q: i32, mine: vec4<u32>) -> vec4<u32> {
    let sx = lr_step(dx, big, i);
    let sy = lr_step(dy, big, i);
    let ix = x + sx.x;
    let iy = y + sy.x;
    let last_x = ix + 1 == i32(P.w);
    let last_y = iy + 1 == i32(P.h);
    var c0 = lr_corner(ix, iy, false);
    var c1 = lr_corner(ix + 1, iy, last_x);
    var c2 = lr_corner(ix, iy + 1, last_y);
    var c3 = lr_corner(ix + 1, iy + 1, last_x || last_y);
    // Within a pixel of itself, the pixel's corner is its source.
    let near_x = sx.x == 0 || (sx.x == -1 && dx < 0);
    let near_y = sy.x == 0 || (sy.x == -1 && dy < 0);
    if near_x && near_y {
        if q == 0 {
            c0 = mine;
        } else if q == 1 {
            c1 = mine;
        } else if q == 2 {
            c2 = mine;
        } else {
            c3 = mine;
        }
    }
    let fx = u32(sx.y);
    let fy = u32(sy.y);
    return ((c2 * (256u - fx) + c3 * fx) * fy + (c1 * fx + c0 * (256u - fx)) * (256u - fy)) >> vec4<u32>(16u);
}

// The noise's number after `n` steps of VEGAS's generator.
fn lr_noise(n: u32) -> u32 {
    var a = 25173u;
    var c = 13849u;
    var ra = 1u;
    var rc = 0u;
    for (var b = 0u; b < 32u; b++) {
        if ((n >> b) & 1u) == 1u {
            ra = (a * ra) & 0xffffu;
            rc = (a * rc + c) & 0xffffu;
        }
        c = (a * c + c) & 0xffffu;
        a = (a * a) & 0xffffu;
    }
    return (ra * P.j.z + rc) & 0xffffu;
}

// Pixel x, y of quarter `q`, the `n`th VEGAS draws.
fn lr_pixel(x: i32, y: i32, q: i32, n: u32) {
    let o = u32(y * i32(P.w) + x);
    let dx = P.i.x - x;
    let dy = P.i.y - y;
    let own = lr_src(o);
    var mine = own;
    if P.j.x != 0u {
        let ax = abs(dx);
        let ay = abs(dy);
        let rows = tb[u32(ay)];
        var fade = 0u;
        if ax <= i32(rows.x) {
            fade = 65536u;
        } else if ax < i32(rows.y) {
            let e = sqrt(f32(ay) * pf(1u) * f32(ay) * pf(1u) + f32(ax) * pf(0u) * f32(ax) * pf(0u));
            fade = u32(clamp(65536 - i32((e - pf(2u)) * pf(3u) * 65536.0), 0, 65536));
        }
        mine = vec4<u32>((own.xyz * fade) >> vec3<u32>(16u), own.w);
    }
    var k = 3;
    if abs(dx) < 3 && abs(dy) < 3 {
        var d = dy;
        if abs(dy) < abs(dx) {
            d = dx;
        }
        k = abs(d);
    }
    var ray = own;
    if k > 0 {
        let big = u32(dx * dx + dy * dy);
        let r1 = lr_read(x, y, dx, dy, big, 1u, q, mine);
        if k == 1 {
            ray = r1;
        } else {
            let r2 = lr_read(x, y, dx, dy, big, 2u, q, mine);
            if k == 2 {
                ray = (r1 + r2) >> vec4<u32>(1u);
            } else {
                let r3 = lr_read(x, y, dx, dy, big, 3u, q, mine);
                ray = vec4<u32>(
                    lr_thirds(r1.x, r2.x, r3.x),
                    lr_thirds(r1.y, r2.y, r3.y),
                    lr_thirds(r1.z, r2.z, r3.z),
                    lr_thirds(r1.w, r2.w, r3.w)
                );
            }
        }
    }
    // A ray darker than the pixel counts less, by Sensitivity.
    let lum_ray = (ray.x + ray.y + ray.z) / 3u;
    let lum = (mine.x + mine.y + mine.z) / 3u;
    var weight = P.i.z;
    if lum_ray < lum {
        weight = (i32(lum_ray) - i32(lum) + 256) * i32(u32(P.i.w * P.i.z) >> 16u);
    }
    var c = vec3<i32>(mine.xyz);
    if P.j.y != 0u && lum > 1u {
        let s = lr_noise(n);
        let g = ((i32((s * lum * 2u) >> 16u) - i32(lum)) * i32(P.j.y)) / 256;
        if g > 0 {
            c = c + (vec3<i32>(255) - c) * g / 256;
        } else {
            c = c + c * g / 256;
        }
    }
    let keep = u32(65536 - weight);
    let wt = u32(weight);
    let rgb = (vec3<u32>(c) * keep + ray.xyz * wt) >> vec3<u32>(16u);
    dst[o] = vec4<f32>(vec4<u32>(rgb, (own.w * keep + ray.w * wt) >> 16u));
}

// The rays drawn as VEGAS's processor draws them, quarter by quarter from
// the light, by one group of the card's threads: a diagonal of each at a
// time (a pixel reads only nearer ones).
@compute @workgroup_size(16, 16)
fn kit_light_rays(@builtin(local_invocation_index) t: u32) {
    let w = i32(P.w);
    let h = i32(P.h);
    let lx = P.i.x;
    let ly = P.i.y;
    var before = 0u;
    for (var q = 0; q < 4; q++) {
        let right = q == 1 || q == 3;
        let below = q >= 2;
        var columns = lx + 1;
        if right {
            columns = w - lx - 1;
        }
        var rows = ly + 1;
        if below {
            rows = h - ly - 1;
        }
        if columns > 0 && rows > 0 {
            // The light's column on the quarter's first row starts again
            // as the source.
            if t == 0u {
                var first = ly;
                if below {
                    first = ly + 1;
                }
                let i = u32(first * w + lx);
                dst[i] = vec4<f32>(lr_src(i));
            }
            storageBarrier();
            for (var diagonal = 0; diagonal < rows + columns - 1; diagonal++) {
                let lo = max(0, diagonal - (columns - 1));
                let hi = min(diagonal, rows - 1);
                // As many rounds for every thread (the old compiler wants
                // the barrier's flow the same for all).
                let rounds = (hi - lo) / 256 + 1;
                for (var r = 0; r < rounds; r++) {
                    let j = lo + r * 256 + i32(t);
                    if j <= hi {
                        let i = diagonal - j;
                        var y = ly - j;
                        if below {
                            y = ly + 1 + j;
                        }
                        var x = lx - i;
                        if right {
                            x = lx + 1 + i;
                        }
                        lr_pixel(x, y, q, before + u32(j * columns + i) + 1u);
                    }
                }
                storageBarrier();
            }
            before += u32(rows * columns);
        }
    }
}

// The rays' picture tinted, the source blended back, out of 8 bits.
@compute @workgroup_size(16, 16)
fn kit_light_rays_finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    var d = vec4<u32>(dst[o]);
    let blend = u32(pf(4u));
    let tint = vec3<u32>(u32(pf(5u)), u32(pf(6u)), u32(pf(7u)));
    if blend != 0u || tint.x != 0u || tint.y != 0u || tint.z != 0u {
        let s = lr_src(o);
        let b = s.w * blend / 255u;
        let keep = 65536u - b;
        let t = ((d.xyz * tint) >> vec3<u32>(8u)) & vec3<u32>(255u);
        d = vec4<u32>((t * keep + s.xyz * b) >> vec3<u32>(16u), (s.w * b + d.w * keep) >> 16u);
    }
    dst[o] = vec4<f32>(d) / 255.0;
}

// Gradient Map (`bv-effects`' `gradient_map::Gradient`, VEGAS's float
// math): `tb` holds the stops' count (row 0) and places (rows 1 to n),
// then each piece of the gradient (n + 1 of them, the ends' flat): its
// times and each channel's Bézier — alpha, red, green, blue: the start,
// its two handles and the end (five rows a piece). f: Amount (0); j.x:
// Add.
fn gm_bez(r: vec4<f32>, u: f32) -> f32 {
    let ab = (r.y - r.x) * u + r.x;
    let bc = (r.z - r.y) * u + r.y;
    let cd = (r.w - r.z) * u + r.z;
    let abc = (bc - ab) * u + ab;
    let bcd = (cd - bc) * u + bc;
    return (bcd - abc) * u + abc;
}

@compute @workgroup_size(16, 16)
fn kit_gradient_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let c = ta[o];
    let n = u32(tb[0].x);
    if n == 0u {
        dst[o] = c;
        return;
    }
    let y = c.y * 0.587 + c.z * 0.114 + c.x * 0.299;
    // The piece: how many stops are at or before the luma.
    var i = 0u;
    for (var k = 0u; k < n; k++) {
        if tb[1u + k].x <= y {
            i = k + 1u;
        }
    }
    let base = 1u + n + 5u * i;
    let t = tb[base];
    var u = 0.0;
    if t.y - t.x > 0.0 {
        u = (y - t.x) / (t.y - t.x);
    }
    let a = clamp(gm_bez(tb[base + 1u], u), 0.0, 1.0);
    let g = clamp(
        vec3<f32>(gm_bez(tb[base + 2u], u), gm_bez(tb[base + 3u], u), gm_bez(tb[base + 4u], u)),
        vec3<f32>(0.0),
        vec3<f32>(a)
    );
    let k = a * pf(0u);
    var rgb = (1.0 - k) * c.xyz + g * k;
    if P.j.x != 0u {
        rgb = g * k + c.xyz;
    }
    dst[o] = vec4<f32>(rgb, min(k + c.w, 1.0));
}

// Film Effects (`bv-effects`' `looks::Film`, rows from the top): the
// picture shaken by i.xy whole pixels, grayed (i.z), grained (VEGAS's
// float math, the processor's hash), tinted toward the color by Tint,
// its light flickered, then the marks of `tb` laid over (two rows each:
// kind — 0 a speck, 1 a hair, 2 a scratch — and its numbers; i.w how
// many). f: Grain (0), Tint (1), the color (2-4), the flicker (5); j.x
// the frame's random seed.
fn fe_hash(x: u32, y: u32, seed: u32) -> u32 {
    var h = (x * 0x8da6b343u) ^ (y * 0xd8163841u) ^ (seed * 0xcb1ab31fu);
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    return h ^ (h >> 16u);
}

fn fe_cover(a: vec4<f32>, b: vec4<f32>, x: f32, y: f32) -> f32 {
    if a.x < 0.5 {
        let d = length(vec2<f32>(x - a.y, y - a.z));
        return clamp(a.w + 0.5 - d, 0.0, 1.0) * b.w;
    }
    if a.x < 1.5 {
        let dx = x - a.y;
        let dy = y - a.z;
        var t = atan2(dy, dx) - b.x;
        t = t - 6.2831855 * floor(t / 6.2831855);
        if t > b.y {
            return 0.0;
        }
        return clamp(0.75 - abs(length(vec2<f32>(dx, dy)) - a.w), 0.0, 1.0) * b.w;
    }
    if y < a.z || y > b.x {
        return 0.0;
    }
    return clamp(a.w * 0.5 + 0.5 - abs(x - a.y), 0.0, 1.0) * b.w;
}

@compute @workgroup_size(16, 16)
fn kit_film_effects(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let sx = i32(id.x) - P.i.x;
    let sy = i32(id.y) - P.i.y;
    var p = vec4<f32>(0.0);
    if sx >= 0 && sy >= 0 && sx < i32(P.w) && sy < i32(P.h) {
        p = ta[u32(sy) * P.w + u32(sx)];
    }
    if P.i.z != 0 {
        let v = (p.y + p.x + p.z) / 3.0;
        p = vec4<f32>(v, v, v, p.w);
    }
    let grain = pf(0u);
    if grain > 0.0 {
        let h = fe_hash(id.x, id.y, P.j.x);
        let n = f32(i32(h & 0xffffu) - 32768) * 3.0517578e-05 * 0.5 * grain;
        let a = p.w;
        if a <= 1e-6 {
            p = vec4<f32>(0.0);
        } else if a >= 1.0 {
            p = vec4<f32>(p.xyz + vec3<f32>(n), 1.0);
        } else {
            p = vec4<f32>((p.xyz / a + vec3<f32>(n)) * a, a);
        }
    }
    let tint = pf(1u);
    if tint > 0.0 {
        let c = vec3<f32>(pf(2u), pf(3u), pf(4u));
        p = vec4<f32>((c - p.xyz) * tint + p.xyz, p.w);
    }
    let flicker = pf(5u);
    if flicker != 1.0 {
        p = vec4<f32>(p.xyz * flicker, p.w);
    }
    let fx = f32(id.x) + 0.5;
    let fy = f32(id.y) + 0.5;
    for (var k = 0; k < P.i.w; k++) {
        let a = tb[u32(2 * k)];
        let b = tb[u32(2 * k + 1)];
        let t = fe_cover(a, b, fx, fy);
        if t > 0.0 {
            let g = b.z * p.w;
            p = vec4<f32>(p.xyz + (vec3<f32>(g) - p.xyz) * t, p.w);
        }
    }
    dst[o] = p;
}

// TV Simulator (`bv-effects`' `looks::Tv`, rows from the top): each row
// read from the rolled one (i.x rows down), moved across by `tb`'s value
// for it and read between pixels; the grill's stripes and the static mixed
// in (f0), the row's scanline (Interlacing f1) and band (first row and
// height f2, f3), held within 0 and the alpha, then the phosphors'
// powers (f4-f6); the static (f7), the grill's cycle across and the
// scanlines' down in pixels (f8, f9); j.x the frame's seed.

// The scanlines' profile (`looks::SCANLINE`, symmetric about 5).
fn tv_scanline(i: u32) -> f32 {
    var k = i;
    if k > 5u {
        k = 10u - k;
    }
    if k == 0u {
        return 1.0;
    }
    if k == 1u {
        return 0.905;
    }
    if k == 2u {
        return 0.655;
    }
    if k == 3u {
        return 0.345;
    }
    if k == 4u {
        return 0.095;
    }
    return 0.0;
}

// The band's profile (`looks::BAND`).
fn tv_band(j: u32) -> f32 {
    if j == 1u || j == 3u {
        return 1.2;
    }
    if j == 2u {
        return 1.4;
    }
    return 1.0;
}

fn tv_grill(i: u32) -> vec3<f32> {
    if i == 1u {
        return vec3<f32>(0.24, 1.0, 0.24);
    }
    if i == 2u {
        return vec3<f32>(0.24, 0.24, 1.0);
    }
    if i == 3u {
        return vec3<f32>(0.08, 0.08, 0.08);
    }
    return vec3<f32>(1.0, 0.24, 0.24);
}

fn tv_read(x: i32, y: u32) -> vec4<f32> {
    if x < 0 || x >= i32(P.w) {
        return vec4<f32>(0.0);
    }
    return ta[y * P.w + u32(x)];
}

@compute @workgroup_size(16, 16)
fn kit_tv_simulator(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let row = (id.y + u32(P.i.x)) % P.h;
    let sx = f32(id.x) - tb[id.y].x;
    let x0 = floor(sx);
    let t = sx - x0;
    let a0 = tv_read(i32(x0), row);
    let b0 = tv_read(i32(x0) + 1, row);
    let c = a0 + (b0 - a0) * t;
    // The grill's stripe and the static.
    let q = f32(id.x) / pf(8u) * 4.0;
    let qi = floor(q);
    let f = q - qi;
    let i = u32(qi) % 4u;
    var noise = 0.0;
    let snow = pf(7u);
    if snow > 0.0 {
        let h = fe_hash(u32(qi), id.y, P.j.x ^ 0x7e1eu);
        noise = f32(h & 0xffffu) / 65536.0 * 2.0 * snow;
    }
    let w = tv_grill(i) * (1.0 - f) + tv_grill(i + 1u) * f + vec3<f32>(noise);
    // The row's brightness.
    let pos = f32(id.y) / pf(9u) * 10.0;
    let pi = floor(pos);
    let pf_ = pos - pi;
    let li = u32(i32(pi) % 10);
    let line = tv_scanline(li) * (1.0 - pf_) + tv_scanline((li + 1u) % 10u) * pf_;
    let inter = pf(1u);
    var weight = line * inter + (1.0 - inter);
    let at = pf(2u);
    let height = pf(3u);
    let yf = f32(id.y);
    if height > 0.0 && yf >= at && yf <= at + height {
        let k = (yf - at) / height * 4.0;
        let j = min(u32(floor(k)), 3u);
        let g = k - f32(j);
        weight = weight * (tv_band(j) * (1.0 - g) + tv_band(j + 1u) * g);
    }
    let grill = pf(0u);
    let tt = w * 0.25 + 1.9921875 * c.xyz * w;
    var v = clamp((tt * grill + c.xyz * (1.0 - grill)) * weight, vec3<f32>(0.0), vec3<f32>(max(c.w, 0.0)));
    let power = vec3<f32>(pf(4u), pf(5u), pf(6u));
    if power.x < 1.0 {
        v.x = min(pow(v.x, power.x), c.w);
    }
    if power.y < 1.0 {
        v.y = min(pow(v.y, power.y), c.w);
    }
    if power.z < 1.0 {
        v.z = min(pow(v.z, power.z), c.w);
    }
    dst[o] = vec4<f32>(v, c.w);
}

// Newsprint (`bv-effects`' `looks::Newsprint`, VEGAS's halftone, rows from
// the top). f: the cell's width and height (0, 1), how many fit across
// and down (2, 3), the grid's shift in cells (4, 5), how much of the
// source comes back (6); i: the averaged picture's columns and rows, the
// rows' parity, the dots' kind (0 black, 1 the cell's color, 2 three
// offset inks). The table: the rims' coverage (rows 0-255), then the
// averaged picture (from row 256).

// Each cell the mean of the pixels it covers (by how much of each).
@compute @workgroup_size(16, 16)
fn kit_newsprint_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let cols = u32(P.i.x);
    let rows = u32(P.i.y);
    if id.x >= cols || id.y >= rows {
        return;
    }
    let sx = f32(P.w) / f32(cols);
    let sy = f32(P.h) / f32(rows);
    let x0 = f32(id.x) * sx;
    let x1 = f32(id.x + 1u) * sx;
    let y0 = f32(id.y) * sy;
    let y1 = f32(id.y + 1u) * sy;
    var sum = vec4<f32>(0.0);
    let ye = min(u32(ceil(y1)), P.h);
    let xe = min(u32(ceil(x1)), P.w);
    for (var y = u32(floor(y0)); y < ye; y++) {
        let wy = max(min(y1, f32(y) + 1.0) - max(y0, f32(y)), 0.0);
        for (var x = u32(floor(x0)); x < xe; x++) {
            let wx = max(min(x1, f32(x) + 1.0) - max(x0, f32(x)), 0.0);
            sum = sum + ta[y * P.w + x] * (wx * wy);
        }
    }
    dst[256u + id.y * cols + id.x] = sum / ((x1 - x0) * (y1 - y0));
}

fn np_cell(i: i32, j: i32) -> vec4<f32> {
    let cols = P.i.x;
    let rows = P.i.y;
    return tb[256 + min(j, rows - 1) * cols + min(i, cols - 1)];
}

// The averaged picture between its cells, held a cell and a half from its
// far edges.
fn np_sample(x: f32, y: f32) -> vec4<f32> {
    let fx = i32(clamp(x * 16384.0, 0.0, max((pf(2u) - 1.5) * 16384.0, 0.0)));
    let fy = i32(clamp(y * 16384.0, 0.0, max((pf(3u) - 1.5) * 16384.0, 0.0)));
    let ix = fx >> 14u;
    let iy = fy >> 14u;
    let tx = f32(fx & 0x3fff) / 16384.0;
    let ty = f32(fy & 0x3fff) / 16384.0;
    let a = np_cell(ix, iy);
    let b = np_cell(ix + 1, iy);
    let c = np_cell(ix, iy + 1);
    let d = np_cell(ix + 1, iy + 1);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    return top + (bottom - top) * ty;
}

// How much of pixel x, y a dot at `at` with radii `r` covers.
fn np_cover(at: vec2<f32>, r: vec2<f32>, x: f32, y: f32) -> f32 {
    if x < round(at.x - r.x - 0.499999985) || x >= round(at.x + r.x + 0.499999985)
        || y < round(at.y - r.y - 0.499999985) || y >= round(at.y + r.y + 0.499999985) {
        return 0.0;
    }
    let inner = max(r.y - 0.7, 0.0);
    let dy = (y - at.y) + 0.5;
    let dx = ((x + 0.5) - at.x) * (r.y / r.x);
    let d2 = dx * dx + dy * dy;
    if d2 < inner * inner {
        return 1.0;
    }
    let d = sqrt(d2);
    if d > r.y + 0.5 {
        return 0.0;
    }
    if d < r.y - 0.5 {
        return 1.0;
    }
    let k = i32(round((d - r.y) * 255.0)) + 128;
    return tb[clamp(k, 0, 255)].x;
}

@compute @workgroup_size(16, 16)
fn kit_newsprint(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let x = f32(id.x);
    let y = f32(id.y);
    let cw = pf(0u);
    let ch = pf(1u);
    let last_j = i32(pf(3u)) + 3;
    let last_i = i32(pf(2u)) + 3;
    var p = vec3<f32>(1.0);
    // The dots that can reach this pixel: a few cells around it.
    let j0 = max(i32(floor((y - ch - 2.0) / ch - pf(5u))), -1);
    let j1 = min(i32(ceil((y + ch + 2.0) / ch - pf(5u))), last_j - 1);
    for (var j = j0; j <= j1; j++) {
        let cy = f32(j) + pf(5u);
        var offset = 0.0;
        if ((j + P.i.z) & 1) != 0 {
            offset = 0.5;
        }
        let i0 = max(i32(floor((x - cw - 2.0) / cw - offset - pf(4u))), -1);
        let i1 = min(i32(ceil((x + cw + 2.0) / cw - offset - pf(4u))), last_i - 1);
        for (var i = i0; i <= i1; i++) {
            let cx = f32(i) + offset + pf(4u);
            let c = np_sample(cx, cy);
            let dark = 1.0 - (c.y * 0.587 + c.z * 0.114 + c.x * 0.299);
            let r = vec2<f32>(cw * 0.5 * dark * 1.25, ch * 0.5 * dark * 1.25);
            if r.x <= 1.0 / 255.0 || r.y <= 1.0 / 255.0 {
                continue;
            }
            let at = vec2<f32>(cx * cw, cy * ch);
            if P.i.w == 0 {
                p = p - vec3<f32>(np_cover(at, r, x, y));
            } else if P.i.w == 1 {
                p = p - (vec3<f32>(1.0) - c.xyz) * np_cover(at, r, x, y);
            } else {
                let q = r * 0.25;
                p.z = p.z - (1.0 - c.z) * np_cover(vec2<f32>(at.x - q.x, at.y - q.y), r, x, y);
                p.y = p.y - (1.0 - c.y) * np_cover(vec2<f32>(at.x + q.x, at.y - q.y), r, x, y);
                p.x = p.x - (1.0 - c.x) * np_cover(vec2<f32>(at.x, at.y + q.y), r, x, y);
            }
        }
    }
    let blend = pf(6u);
    if blend > 0.0 {
        var s = ta[o].xyz;
        if P.i.w == 0 {
            s = vec3<f32>(s.y * 0.587 + s.z * 0.114 + s.x * 0.299);
        }
        p = (s - p) * blend + p;
    }
    dst[o] = vec4<f32>(p, 1.0);
}

// Lens Flare (`bv-effects`' `looks::flare_spots`, rows from the top): the
// table's elements, two rows each — middle and half width and height;
// shape and color —, i.x how many; f0 Blend.
fn lf_ray(k: u32) -> f32 {
    return 0.35 + 0.65 * f32(fe_hash(k, 0x5ca7u, 0x1e75u) & 0xffffu) / 65535.0;
}

fn lf_shape(shape: i32, u: f32, v: f32, d: f32) -> f32 {
    if d >= 1.0 {
        return 0.0;
    }
    let au = abs(u);
    let av = abs(v);
    let hexagon = max(au * 0.8660254 + av * 0.5, av);
    if shape == 0 {
        return 1.0 - clamp((d - 0.8) / 0.2, 0.0, 1.0);
    }
    if shape == 1 {
        return max(1.0 - abs(d - 0.85) / 0.15, 0.0);
    }
    if shape == 2 {
        return (1.0 - d) * (1.0 - d);
    }
    if shape == 3 {
        if hexagon >= 1.0 {
            return 0.0;
        }
        if hexagon < 0.2 {
            return hexagon / 0.2;
        }
        return (1.0 - hexagon) / 0.8;
    }
    if shape == 4 {
        let p = min(hexagon, 1.0);
        return (1.0 - p) * (1.0 - p);
    }
    // The middle: the processor's angle there is 0 (the card's atan2 of
    // nothing is not), its reach whole.
    if d <= 0.0 {
        return 1.0;
    }
    var a = atan2(v, u);
    a = a - 6.2831855 * floor(a / 6.2831855);
    a = a / 6.2831855 * 64.0;
    let kf = floor(a);
    let f = a - kf;
    let k = u32(kf) % 64u;
    let ray = lf_ray(k) * (1.0 - f) + lf_ray((k + 1u) % 64u) * f;
    let core = min(d / 0.3, 1.0);
    let reach = (1.0 - core) + core * ray;
    let r = d / reach;
    if r < 1.0 {
        return (1.0 - r) * (1.0 - r) * (1.0 - r);
    }
    return 0.0;
}

@compute @workgroup_size(16, 16)
fn kit_lens_flare(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let c = ta[o];
    let x = f32(id.x);
    let y = f32(id.y);
    var sum = vec3<f32>(0.0);
    var touched = false;
    for (var k = 0; k < P.i.x; k++) {
        let a = tb[u32(2 * k)];
        let b = tb[u32(2 * k + 1)];
        let u = (a.x - x) / a.z;
        let v = (a.y - y) / a.w;
        if abs(u) > 1.0 || abs(v) > 1.0 {
            continue;
        }
        touched = true;
        let s = lf_shape(i32(b.x), u, v, sqrt(u * u + v * v));
        sum = sum + b.yzw * s;
    }
    if !touched {
        dst[o] = c;
        return;
    }
    let blend = pf(0u);
    let alpha = c.w;
    if alpha <= 1e-6 {
        dst[o] = vec4<f32>(0.0);
    } else if alpha >= 1.0 {
        dst[o] = vec4<f32>((c.xyz + sum) * blend + c.xyz * (1.0 - blend), 1.0);
    } else {
        let s = c.xyz / alpha;
        dst[o] = vec4<f32>(((s + sum) * blend + s * (1.0 - blend)) * alpha, alpha);
    }
}

// The frequency domain's product (`bv-effects`' `fft`): each complex
// number of the grid `ta` times the kernel's in `tb`.
@compute @workgroup_size(16, 16)
fn fft_multiply(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let v = ta[o];
    let q = tb[o];
    dst[o] = vec4<f32>(
        v.x * q.x - v.y * q.y,
        v.x * q.y + v.y * q.x,
        v.z * q.z - v.w * q.w,
        v.z * q.w + v.w * q.z,
    );
}

// The frequency domain's result added to the picture (`bv-effects`'
// Starburst, Glint): the grid `ta` (`sw` wide, i.x in from its corner)
// times f0 (one over its size) and f1 (Strength), over `tb` — or nothing
// with j.x —, each channel held within 0–1; with j.y 1 through the mask
// (f2 on), with 3 the mask alone.
@compute @workgroup_size(16, 16)
fn fft_add(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let r = u32(P.i.x);
    var v = ta[(id.y + r) * P.sw + id.x + r] * pf(0u) * pf(1u);
    if P.j.y == 1u || P.j.y == 3u {
        let u = (f32(id.x) + 0.5) / f32(P.w);
        let vv = (f32(id.y) + 0.5) / f32(P.h);
        let m = vm_mask(u, vv, f32(P.w) / f32(P.h), 2u);
        if P.j.y == 3u {
            dst[o] = vec4<f32>(m, m, m, 1.0);
            return;
        }
        v = v * m;
    }
    var base = tb[o];
    if P.j.x != 0u {
        base = vec4<f32>(0.0);
    }
    dst[o] = clamp(base + v, vec4<f32>(0.0), vec4<f32>(1.0));
}

// Soft Contrast's Diffusion: the picture `ta` screened by its glow `tb`,
// as much as the glow's luma times f0.
@compute @workgroup_size(16, 16)
fn kit_soft_diffuse(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let c = ta[o];
    let b = tb[o];
    let w = clamp(0.2126 * b.x + 0.7152 * b.y + 0.0722 * b.z, 0.0, 1.0) * pf(0u);
    let lift = max(b.xyz - c.xyz * b.xyz, vec3<f32>(0.0)) * w;
    dst[o] = vec4<f32>(c.xyz + lift, c.w);
}

// Soft Contrast's vignette: f0 Strength, f1 Softness, its size f2, f3 and
// middle f4, f5 (0–1, rows from the top), the corners' radius f6, a
// rectangle with f7, what is outside f8 (1 Blur: `tb`, 2 Black, 3 White,
// 4 Transparent) over the picture `ta`.
@compute @workgroup_size(16, 16)
fn kit_soft_vignette(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let u = (f32(id.x) + 0.5) / f32(P.w);
    let v = (f32(id.y) + 0.5) / f32(P.h);
    let dx = abs((u - pf(4u)) / (pf(2u) / 2.0));
    let dy = abs((v - pf(5u)) / (pf(3u) / 2.0));
    let r = pf(6u);
    var d = length(vec2<f32>(dx, dy));
    if pf(7u) > 0.5 {
        let qx = dx - (1.0 - r);
        let qy = dy - (1.0 - r);
        if qx > 0.0 && qy > 0.0 {
            d = (1.0 - r) + length(vec2<f32>(qx, qy));
        } else {
            d = max(dx, dy);
        }
    }
    let k = smoothstep(1.0 - pf(1u) * 0.5, 1.0 + 1e-9, d) * pf(0u);
    let c = ta[o];
    let e = pf(8u);
    var out = c * (1.0 - k);
    if e < 1.5 {
        out = c + (tb[o] - c) * k;
    } else if e < 2.5 {
        out = vec4<f32>(c.xyz * (1.0 - k), c.w);
    } else if e < 3.5 {
        out = vec4<f32>(c.xyz + (vec3<f32>(c.w) - c.xyz) * k, c.w);
    }
    dst[o] = out;
}

// Rays (`bv-effects`' `Beams`): over the picture `ta`, the rays from the
// lights `tb` — j.x readings toward the light (f0, f1: 0–1, rows from the
// top) over Amount (f2) of the way, farther across with Aspect (f3) —,
// their mean times the gain (f4) and Noise's uneven rays (f6), colored by
// Hue (f7) along Hue sweep (f8) at Saturation (f9), held under Peak (f5);
// through the mask on Effect (j.w 1) or the mask alone (j.w 3), alone
// with Effect only (j.z).
fn rays_light(x: f32, y: f32) -> vec4<f32> {
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = floor(fx);
    let y0 = floor(fy);
    let tx = fx - x0;
    let ty = fy - y0;
    let w = i32(P.w) - 1;
    let h = i32(P.h) - 1;
    let i0 = clamp(i32(x0), 0, w);
    let i1 = clamp(i32(x0) + 1, 0, w);
    let j0 = clamp(i32(y0), 0, h);
    let j1 = clamp(i32(y0) + 1, 0, h);
    let a = tb[u32(j0) * P.w + u32(i0)];
    let b = tb[u32(j0) * P.w + u32(i1)];
    let c = tb[u32(j1) * P.w + u32(i0)];
    let d = tb[u32(j1) * P.w + u32(i1)];
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    return top + (bottom - top) * ty;
}

fn rays_noise(angle: f32, noise: f32) -> f32 {
    if noise <= 0.0 {
        return 1.0;
    }
    let turn = 6.2831855;
    let a = (angle - turn * floor(angle / turn)) / turn * 128.0;
    let kf = floor(a);
    let f = a - kf;
    let k = u32(kf) % 128u;
    let r0 = f32(fe_hash(k, 0x7a1du, 0x3c55u) & 0xffffu) / 65535.0;
    let r1 = f32(fe_hash((k + 1u) % 128u, 0x7a1du, 0x3c55u) & 0xffffu) / 65535.0;
    return 1.0 - noise * (r0 * (1.0 - f) + r1 * f);
}

@compute @workgroup_size(16, 16)
fn kit_rays(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let x = f32(id.x) + 0.5;
    let y = f32(id.y) + 0.5;
    let wf = f32(P.w);
    let hf = f32(P.h);
    let m = vm_mask(x / wf, y / hf, wf / hf, 11u);
    if P.j.w == 3u {
        dst[o] = vec4<f32>(m, m, m, 1.0);
        return;
    }
    let dx = pf(0u) * wf - x;
    let dy = pf(1u) * hf - y;
    let angle = atan2(dy, dx);
    let aspect = pf(3u);
    let along = pf(2u) * min(length(vec2<f32>(aspect * cos(angle), sin(angle))) / max(aspect, 1.0), 1.0);
    let n = max(P.j.x, 1u);
    var sum = vec4<f32>(0.0);
    for (var k = 0u; k < n; k++) {
        let t = along * f32(k) / f32(n);
        sum = sum + rays_light(x + dx * t, y + dy * t);
    }
    let reach = length(vec2<f32>(dx, dy)) / length(vec2<f32>(wf, hf));
    let tint = vec3<f32>(1.0) + (hue_rgb(pf(7u) + pf(8u) * reach) - vec3<f32>(1.0)) * pf(9u);
    let strength = pf(4u) * rays_noise(angle, pf(6u)) / f32(n);
    let peak = pf(5u);
    var r = vec4<f32>(min(sum.xyz * strength * tint, vec3<f32>(peak)), min(sum.w * strength, peak));
    if P.j.w == 1u {
        r = r * m;
    }
    var under = ta[o];
    if P.j.z != 0u {
        under = vec4<f32>(0.0);
    }
    dst[o] = clamp(under + r, vec4<f32>(0.0), vec4<f32>(1.0));
}

// Swirl and Pinch/Punch (`bv-effects`' `lensed`, VEGAS's, rows counted
// from the top): `tb` holds each column's offset from the center (i 0, 1)
// over the area's half width and its square, then each row's over the
// half height; a pixel whose squares add up to 1 or more stays.
fn lens_out(id: vec3<u32>) -> bool {
    return tb[id.x].y + tb[P.w + id.y].y >= 1.0;
}

// The picture's pixel at column x, row t, held in the picture.
fn lens_texel(x: i32, t: i32) -> vec4<f32> {
    let xi = clamp(x, 0, i32(P.w) - 1);
    let ti = clamp(t, 0, i32(P.h) - 1);
    return ta[u32(ti) * P.w + u32(xi)];
}

// Swirl: the area turned about the center by f 2 (15 × Amount) × (1 − r)²
// radians, f 0, 1 its half width and height in pixels; read between the
// four nearest pixels weighted by their alpha.
@compute @workgroup_size(16, 16)
fn kit_swirl(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    if lens_out(id) {
        dst[o] = ta[o];
        return;
    }
    let u = tb[id.x].x;
    let v = tb[P.w + id.y].x;
    let r = sqrt(tb[id.x].y + tb[P.w + id.y].y);
    let turn = (1.0 - r) * pf(2u) * (1.0 - r);
    let cs = cos(turn);
    let sn = sin(turn);
    let t = f32(P.i.y) + (cs * v - sn * u) * pf(1u);
    let x = f32(P.i.x) + (cs * u + sn * v) * pf(0u);
    let it = i32(floor(t));
    let ix = i32(floor(x));
    let ft = t - floor(t);
    let fx = x - floor(x);
    let a = lens_texel(ix, it);
    let b = lens_texel(ix + 1, it);
    let c = lens_texel(ix, it + 1);
    let d = lens_texel(ix + 1, it + 1);
    let wa = (1.0 - fx) * a.w * (1.0 - ft);
    let wb = fx * b.w * (1.0 - ft);
    let wc = (1.0 - fx) * c.w * ft;
    let wd = fx * d.w * ft;
    let alpha = wb + wa + wc + wd;
    if alpha <= 0.0 {
        dst[o] = vec4<f32>(0.0);
        return;
    }
    dst[o] = vec4<f32>(wa * a.xyz + wb * b.xyz + wc * c.xyz + wd * d.xyz, alpha);
}

// Pinch/Punch: each offset from the center times 1 − (cos πr + 1) × f 2
// (Amount) × 0.4, read between the four nearest pixels.
@compute @workgroup_size(16, 16)
fn kit_pinch_punch(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    if lens_out(id) {
        dst[o] = ta[o];
        return;
    }
    let r = sqrt(tb[id.x].y + tb[P.w + id.y].y);
    let k = 1.0 - (cos(r * 3.1415926) + 1.0) * pf(2u) * 0.4;
    let t = f32(i32(id.y) - P.i.y) * k + f32(P.i.y);
    let x = k * f32(i32(id.x) - P.i.x) + f32(P.i.x);
    let it = i32(floor(t));
    let ix = i32(floor(x));
    let ft = t - floor(t);
    let fx = x - floor(x);
    let a = lens_texel(ix, it);
    let b = lens_texel(ix + 1, it);
    let c = lens_texel(ix, it + 1);
    let d = lens_texel(ix + 1, it + 1);
    dst[o] = ((1.0 - fx) * c + fx * d) * ft + ((1.0 - fx) * a + fx * b) * (1.0 - ft);
}

// Fill Light's light (`bv-effects`' `fill_light`): for each pixel of the
// picture `ta`, the entry of the table `tb` (256) its luma picks — its
// straight color in 8-bit steps, their BT.601 luma in whole numbers.
@compute @workgroup_size(16, 16)
fn kit_fill_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = id.y * P.w + id.x;
    let p = ta[i];
    var c = vec3<f32>(0.0);
    if p.w > 0.0 {
        c = p.xyz / p.w;
    }
    let b = vec3<u32>(step8(c.x), step8(c.y), step8(c.z));
    let l = (19595u * b.x + 38469u * b.y + 7472u * b.z + 32768u) >> 16u;
    dst[i] = tb[l];
}

// Motion Blur (`bv-effects`' `motion_blur`): the picture (`dst`) blurred
// (`ta`) where the mask spread by the kernel (`tb`) is not 0 in 8 bits.
@compute @workgroup_size(16, 16)
fn kit_motion_blur_pick(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    if tb[o].x > 0.5 {
        dst[o] = ta[o];
    }
}

// Flicker Control's Temporal blend (`bv-effects`' `flicker_control::blend`),
// a frame of its window at a time: the frame (`ta`) added into the sums
// (`dst`: `w` × `h` sums of the colors and weights, then `w` × `h`
// alphas) by its weight `f[0]` and, but for the frame keeping its alpha
// (`j.y`), by how much Type (`j.x`; 3, all: its weight alone) gives its
// largest channel. With a map (`j.z`, the homography `f[1..10]`, rows
// up): its nearest pixel where the map takes the pixel, within the
// picture; past it, the frame keeping its alpha (`tb`) there.
@compute @workgroup_size(16, 16)
fn kit_flicker_gather(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    var c = ta[o];
    if P.j.z != 0u {
        let x = f32(id.x);
        let y = f32(P.h - 1u - id.y);
        let d = 1.0 / (x * pf(7u) + y * pf(8u) + pf(9u));
        let mx = (y * pf(2u) + x * pf(1u) + pf(3u)) * d;
        let my = (x * pf(4u) + y * pf(5u) + pf(6u)) * d;
        if mx <= -0.4 || f32(P.w) - 0.6 <= mx || my <= -0.4 || f32(P.h) - 0.6 <= my {
            c = tb[o];
        } else {
            let sx = u32(rround(mx));
            let sy = P.h - 1u - u32(rround(my));
            c = ta[sy * P.w + sx];
        }
    }
    let own = P.j.y != 0u;
    var w = pf(0u);
    if P.j.x != 3u && !own {
        w = flicker_tone(P.j.x, flicker_largest(c)) * w;
    }
    let s = dst[o];
    dst[o] = vec4<f32>(s.xyz + c.xyz * w, s.w + w);
    if own {
        dst[o + P.w * P.h] = vec4<f32>(c.w, 0.0, 0.0, 0.0);
    }
}

// Mesh Warp (`bv-effects`' `mesh_warp`, the same sums in single
// precision): `ta` the picture (`w` × `h`, rows from the top), `tb` the
// table `mesh_warp_table` works out. j.x 1 (Target): `tb` is the map the
// mesh's triangles drew on a reduced picture (`sw` × `sh`, rows up), each
// pixel's place there its own times f 0 and 1 (the ratios), read between
// the map's four values — past its last value the one before held —; 0
// (Source): `tb` holds each column's cell and share, each row's, then the
// mesh's points (`sw` × `sh`), each pixel its cell's four points mixed.
// The picture is then read there as Lens Correction reads it.

// Pixel `k` of the picture counted row after row from the bottom.
fn mesh_pixel(k: u32) -> vec4<f32> {
    return ta[(P.h - 1u - k / P.w) * P.w + k % P.w];
}

@compute @workgroup_size(16, 16)
fn kit_mesh_warp(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let x = id.x;
    let y = P.h - 1u - id.y;
    var at = vec2<f32>(0.0);
    if P.j.x == 1u {
        let ty = f32(y) * pf(1u);
        let iy = u32(ty);
        let fy = ty - f32(iy);
        let tx = f32(x) * pf(0u);
        let ix = u32(tx);
        let fx = tx - f32(ix);
        let n = P.sw * P.sh;
        let i = P.sw * iy + ix;
        let up = i + P.sw;
        let m00 = tb[i].xy;
        var m10 = m00;
        if i + 1u < n {
            m10 = tb[i + 1u].xy;
        }
        var m01 = m00;
        if up < n {
            m01 = tb[up].xy;
        }
        var m11 = m01;
        if up + 1u < n {
            m11 = tb[up + 1u].xy;
        }
        let gx = 1.0 - fx;
        let gy = 1.0 - fy;
        at = m10 * fx * gy + gx * m00 * gy + m01 * fy * gx + fy * fx * m11;
    } else {
        let col = tb[x];
        let row = tb[P.w + y];
        let c = u32(col.x);
        let r = u32(row.x);
        let fx = col.y;
        let fy = row.y;
        let base = P.w + P.h;
        let p10 = tb[base + (r - 1u) * P.sw + c].xy;
        let p00 = tb[base + (r - 1u) * P.sw + c - 1u].xy;
        let p01 = tb[base + r * P.sw + c - 1u].xy;
        let p11 = tb[base + r * P.sw + c].xy;
        let w1 = (1.0 - fy) * fx;
        let w0 = (1.0 - fy) * (1.0 - fx);
        let w2 = (1.0 - fx) * fy;
        at = p10 * w1 + p00 * w0 + p01 * w2 + p11 * fx * fy;
    }
    let wf = f32(P.w);
    let hf = f32(P.h);
    let sx = f32(P.w - 1u) * at.x;
    let sy = f32(P.h - 1u) * at.y;
    if !(sx > -0.4 && sx < wf - 0.6 && sy > -0.4 && sy < hf - 0.6) {
        dst[o] = vec4<f32>(0.0);
        return;
    }
    let ix = u32(max(sx, 0.0));
    let iy = u32(max(sy, 0.0));
    let fx = max(sx - f32(ix), 0.0);
    let fy = max(sy - f32(iy), 0.0);
    let n = P.w * P.h;
    let k = iy * P.w + ix;
    let p00 = mesh_pixel(k);
    var p10 = p00;
    if k + 1u < n {
        p10 = mesh_pixel(k + 1u);
    }
    var p01 = p00;
    if k + P.w < n {
        p01 = mesh_pixel(k + P.w);
    }
    var p11 = p01;
    if k + P.w + 1u < n {
        p11 = mesh_pixel(k + P.w + 1u);
    }
    let w00 = f32(i32((1.0 - fx) * (1.0 - fy) * 1024.0));
    let w10 = f32(i32((1.0 - fy) * fx * 1024.0));
    let w01 = f32(i32((1.0 - fx) * fy * 1024.0));
    let w11 = f32(i32(fy * fx * 1024.0));
    dst[o] = (p10 * w10 + w00 * p00 + p01 * w01 + p11 * w11) * 0.0009765625;
}

// The 360° effects (360° Video Stabilization, Scene Rotation): each
// pixel's direction on the sphere — its column's longitude and its row's
// latitude in `tb`, their sines and cosines worked out on the processor
// with where their eye's picture starts and how big it is (the columns'
// first, then the rows from the top) — turned by the matrix f0–f8 (by
// rows; x across, y up, z ahead) and read back where it lands,
// bilinearly, the edges held.
fn sphere_pixel(x: i32, y: i32) -> vec4<f32> {
    let xi = clamp(x, 0, i32(P.w) - 1);
    let yi = clamp(y, 0, i32(P.h) - 1);
    return ta[u32(i32(P.h) - 1 - yi) * P.w + u32(xi)];
}

@compute @workgroup_size(16, 16)
fn kit_sphere_turn(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let col = tb[id.x];
    let row = tb[P.w + id.y];
    let d = vec3<f32>(row.y * col.x, row.x, row.y * col.y);
    let r = vec3<f32>(
        pf(0u) * d.x + pf(1u) * d.y + pf(2u) * d.z,
        pf(3u) * d.x + pf(4u) * d.y + pf(5u) * d.z,
        pf(6u) * d.x + pf(7u) * d.y + pf(8u) * d.z,
    );
    var pu = atan2(r.x, r.z) / 6.2831855 + 0.5;
    pu = pu - floor(pu);
    let pv = 0.5 - atan2(r.y, sqrt(r.x * r.x + r.z * r.z)) / 3.1415927;
    // The pixel there, rows up, each middle at whole numbers.
    let px = (col.z + pu * col.w) * f32(P.w) - 0.5;
    let py = (1.0 - (row.z + pv * row.w)) * f32(P.h) - 0.5;
    let x0 = floor(px);
    let y0 = floor(py);
    let fx = px - x0;
    let fy = py - y0;
    let xi = i32(x0);
    let yi = i32(y0);
    let a = sphere_pixel(xi, yi);
    let b = sphere_pixel(xi + 1, yi);
    let c = sphere_pixel(xi, yi + 1);
    let e = sphere_pixel(xi + 1, yi + 1);
    let low = a + (b - a) * fx;
    let high = c + (e - c) * fx;
    dst[o] = low + (high - low) * fy;
}

// Dual Fish Eye Stitching: each pixel's direction on the sphere — its
// column's longitude and its row's latitude in `tb`, their cosines and
// sines worked out on the processor (the columns', then the rows' from
// the top) — read from the lens that sees it, or from both where they
// overlap, mixed in Y′UV, as the plug-in does. The frame read is its top,
// at most half as high as wide. f: the field of view (0), how far along
// the lenses' axis both see (1), the vignetting's intensity and range
// (2, 3), Center and Radius as parts (4, 5), the back lens's turn's cosine
// and sine (6, 7), the front's (8, 9).
fn stitch_pixel(i: i32, j: i32, used: u32) -> vec4<f32> {
    let x = u32(clamp(i, 0, i32(P.w) - 1));
    let y = u32(clamp(j, 0, i32(used) - 1));
    return ta[(used - 1u - y) * P.w + x];
}

// Lens `front`'s point for direction `d` (x along the front lens, y up):
// where to read (x, y, each −1 to 1, y up) and how far out on its circle
// (z).
fn stitch_project(d: vec3<f32>, front: bool) -> vec3<f32> {
    let rho = sqrt(d.z * d.z + d.y * d.y);
    let theta = atan2(rho, d.x);
    let r = (theta + theta) / pf(0u);
    let shift = r * pf(4u);
    let rr = (r - shift) * pf(5u);
    var px = rr;
    var py = 0.0;
    if rho > 0.0 {
        px = d.z / rho * rr;
        py = d.y / rho * rr;
    }
    var ca = pf(6u);
    var sa = pf(7u);
    if front {
        ca = pf(8u);
        sa = pf(9u);
    }
    let x = ca * px - sa * py;
    var u = (x - 1.0) - shift;
    if front {
        u = x + 1.0 + shift;
    }
    return vec3<f32>(u * 0.5, ca * py + sa * px, abs(rr));
}

// A channel raised to the vignetting's power `e`, kept within 0–1.
fn stitch_power(v: f32, e: f32) -> f32 {
    if v > 0.0 {
        return min(pow(v, e), 1.0);
    }
    if e == 0.0 {
        return 1.0;
    }
    return 0.0;
}

// The frame read at `at` (from `stitch_project`), between four pixels,
// the edges held, the vignetting taken out.
fn stitch_read(at: vec3<f32>, used: u32) -> vec3<f32> {
    let u = clamp(at.x, -1.0, 1.0);
    let v = clamp(at.y, -1.0, 1.0);
    let x = f32(P.w - 1u) * (u + 1.0) * 0.5;
    let top = f32(used - 1u);
    let y = top - (top - (v + 1.0) * 0.5 * top);
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let gx = 1.0 - fx;
    let gy = 1.0 - fy;
    let i = i32(x0);
    let j = i32(y0);
    let p00 = stitch_pixel(i, j, used).xyz;
    let p10 = stitch_pixel(i + 1, j, used).xyz;
    let p01 = stitch_pixel(i, j + 1, used).xyz;
    let p11 = stitch_pixel(i + 1, j + 1, used).xyz;
    var c = p00 * gx * gy + p10 * fx * gy + p01 * gx * fy + p11 * fx * fy;
    let intensity = pf(2u);
    let range = pf(3u);
    if intensity > 0.0 && range != 0.0 && 1.0 - range <= at.z {
        let f = (at.z - (1.0 - range)) / range;
        let e = 1.0 - f * intensity * f;
        c = vec3<f32>(stitch_power(c.x, e), stitch_power(c.y, e), stitch_power(c.z, e));
    }
    return c;
}

// Two colors mixed, `a` weighing `wa` and `b` `wb`, in Y′UV.
fn stitch_mixed(a: vec3<f32>, b: vec3<f32>, wa: f32, wb: f32) -> vec3<f32> {
    let ya = a.y * 0.587 + a.x * 0.299 + a.z * 0.114;
    let yb = b.y * 0.587 + b.x * 0.299 + b.z * 0.114;
    let y = yb * wb + ya * wa;
    let vv = ((b.x - yb) / 0.701) * 0.615 * wb + ((a.x - ya) / 0.701) * 0.615 * wa;
    let uu = ((b.z - yb) / 0.886) * 0.436 * wb + ((a.z - ya) / 0.886) * 0.436 * wa;
    let c = vec3<f32>(vv * 1.14 + y, (y - uu * 0.395) - vv * 0.581, uu * 2.033 + y);
    return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
}

@compute @workgroup_size(16, 16)
fn kit_stitch(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let used = min(P.h, P.w / 2u);
    let col = tb[id.x];
    let row = tb[P.w + id.y];
    let d = vec3<f32>(col.x * row.x, row.y, col.y * row.x);
    let t = pf(1u);
    var c = vec3<f32>(0.0);
    if t <= abs(d.x) {
        let front = d.x > -t;
        var e = d;
        if !front {
            e = vec3<f32>(abs(d.x), d.y, -d.z);
        }
        c = stitch_read(stitch_project(e, front), used);
    } else {
        let a = stitch_read(stitch_project(d, true), used);
        let b = stitch_read(stitch_project(vec3<f32>(-d.x, d.y, -d.z), false), used);
        var wa = 0.5;
        if d.x > 0.0 {
            wa = (abs(d.x) / t + 1.0) * 0.5;
        } else if d.x < 0.0 {
            wa = (1.0 - abs(d.x) / t) * 0.5;
        }
        c = stitch_mixed(a, b, wa, 1.0 - wa);
    }
    dst[id.y * P.w + id.x] = vec4<f32>(c, 1.0);
}

// Stereoscopic 3D Adjust: each pixel read where the eye's inverse map puts
// it, in 32nds of a pixel worked out on the processor as OpenCV's warps
// work them out — `j.x` 1: an affine map's columns' whole numbers, then
// its rows' from the top, in `tb` (added, then made 32nds); 0: a
// perspective map's 32nds for each pixel from the top, two pixels an
// entry — between four pixels with OpenCV's weights, nothing past the
// picture (rows up); then the floating window: columns `i.x` to `i.y`
// opaque black, column `i.z` toward it by f0.
fn stereo_pixel(i: i32, j: i32) -> vec4<f32> {
    if i < 0 || j < 0 || i >= i32(P.w) || j >= i32(P.h) {
        return vec4<f32>(0.0);
    }
    return ta[u32(i32(P.h) - 1 - j) * P.w + u32(i)];
}

@compute @workgroup_size(16, 16)
fn kit_stereo_warp(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    var xy = vec2<i32>(0, 0);
    if P.j.x == 1u {
        let c = tb[id.x];
        let r = tb[P.w + id.y];
        xy = vec2<i32>((i32(r.x) + i32(c.x)) >> 5u, (i32(r.y) + i32(c.y)) >> 5u);
    } else {
        let e = tb[o / 2u];
        if (o & 1u) == 0u {
            xy = vec2<i32>(i32(e.x), i32(e.y));
        } else {
            xy = vec2<i32>(i32(e.z), i32(e.w));
        }
    }
    let sx = xy.x >> 5u;
    let sy = xy.y >> 5u;
    let tx = f32(xy.x & 31) / 32.0;
    let ty = f32(xy.y & 31) / 32.0;
    let c = stereo_pixel(sx, sy) * ((1.0 - ty) * (1.0 - tx))
        + stereo_pixel(sx + 1, sy) * ((1.0 - ty) * tx)
        + stereo_pixel(sx, sy + 1) * (ty * (1.0 - tx))
        + stereo_pixel(sx + 1, sy + 1) * (ty * tx);
    let black = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let x = i32(id.x);
    var out = c;
    if x >= P.i.x && x < P.i.y {
        out = black;
    } else if x == P.i.z {
        out = c + (black - c) * pf(0u);
    }
    dst[o] = out;
}

// Smart Zoom's Lanczos, rows first: each pixel the sum of its eight taps,
// their first and weights in `tb` (three entries a column, then three a
// row from the top; the taps' rows counted up), the edges held.
@compute @workgroup_size(16, 16)
fn kit_lanczos_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let e = id.x * 3u;
    let first = i32(tb[e].x);
    let a = tb[e + 1u];
    let b = tb[e + 2u];
    let row = id.y * P.w;
    let last = i32(P.w) - 1;
    var s = ta[row + u32(clamp(first, 0, last))] * a.x;
    s = s + ta[row + u32(clamp(first + 1, 0, last))] * a.y;
    s = s + ta[row + u32(clamp(first + 2, 0, last))] * a.z;
    s = s + ta[row + u32(clamp(first + 3, 0, last))] * a.w;
    s = s + ta[row + u32(clamp(first + 4, 0, last))] * b.x;
    s = s + ta[row + u32(clamp(first + 5, 0, last))] * b.y;
    s = s + ta[row + u32(clamp(first + 6, 0, last))] * b.z;
    s = s + ta[row + u32(clamp(first + 7, 0, last))] * b.w;
    dst[id.y * P.w + id.x] = s;
}

fn lanczos_at(r: i32, x: u32) -> vec4<f32> {
    let j = clamp(r, 0, i32(P.h) - 1);
    return ta[u32(i32(P.h) - 1 - j) * P.w + x];
}

@compute @workgroup_size(16, 16)
fn kit_lanczos_columns(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let e = (P.w + id.y) * 3u;
    let first = i32(tb[e].x);
    let a = tb[e + 1u];
    let b = tb[e + 2u];
    var s = lanczos_at(first, id.x) * a.x;
    s = s + lanczos_at(first + 1, id.x) * a.y;
    s = s + lanczos_at(first + 2, id.x) * a.z;
    s = s + lanczos_at(first + 3, id.x) * a.w;
    s = s + lanczos_at(first + 4, id.x) * b.x;
    s = s + lanczos_at(first + 5, id.x) * b.y;
    s = s + lanczos_at(first + 6, id.x) * b.z;
    s = s + lanczos_at(first + 7, id.x) * b.w;
    dst[id.y * P.w + id.x] = s;
}

// AI Smoothen and AI Sharpen: the guided filter's layer on the small
// copy (`ta`, `P.w` × `P.h`) and what was worked out on it (`tb`): each
// channel's local linear map's `a` and `b` over 3 × 3 (ε 10⁻⁴), two
// entries a pixel.
fn guided_x(i: i32, j: i32) -> vec3<f32> {
    let x = clamp(i, 0, i32(P.w) - 1);
    let y = clamp(j, 0, i32(P.h) - 1);
    return ta[u32(y) * P.w + u32(x)].xyz;
}

fn guided_p(i: i32, j: i32) -> vec3<f32> {
    let x = clamp(i, 0, i32(P.w) - 1);
    let y = clamp(j, 0, i32(P.h) - 1);
    return tb[u32(y) * P.w + u32(x)].xyz;
}

@compute @workgroup_size(16, 16)
fn kit_guided_coef(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let i = i32(id.x);
    let j = i32(id.y);
    var mx = vec3<f32>(0.0);
    var mp = vec3<f32>(0.0);
    var mxx = vec3<f32>(0.0);
    var mxp = vec3<f32>(0.0);
    for (var dj = -1; dj <= 1; dj = dj + 1) {
        for (var di = -1; di <= 1; di = di + 1) {
            let x = guided_x(i + di, j + dj);
            let p = guided_p(i + di, j + dj);
            mx = mx + x;
            mp = mp + p;
            mxx = mxx + x * x;
            mxp = mxp + x * p;
        }
    }
    let k = 1.0 / 9.0;
    mx = mx * k;
    mp = mp * k;
    mxx = mxx * k;
    mxp = mxp * k;
    let v = mxx - mx * mx;
    let c = mxp - mx * mp;
    let a = c / (v + vec3<f32>(0.0001));
    let b = mp - a * mx;
    let o = (id.y * P.w + id.x) * 2u;
    dst[o] = vec4<f32>(a, 0.0);
    dst[o + 1u] = vec4<f32>(b, 0.0);
}

// The full picture (`ta`, `P.w` × `P.h`, rows from the top) through the
// small coefficients (`tb`, `P.sw` × `P.sh`, two entries a pixel) read
// between four values, corners aligned (f2, f3 its steps a framed pixel;
// `i.x` the margin): `a (x + 0.1 f1) f0 + b`, f4 of the way from the
// picture, held within 0 and its alpha.
fn guided_coef(i: i32, j: i32, which: u32) -> vec3<f32> {
    let x = clamp(i, 0, i32(P.sw) - 1);
    let y = clamp(j, 0, i32(P.sh) - 1);
    return tb[(u32(y) * P.sw + u32(x)) * 2u + which].xyz;
}

@compute @workgroup_size(16, 16)
fn kit_guided_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let c = ta[o];
    let m = f32(P.i.x);
    let lx = (f32(id.x) + m) * pf(2u);
    let ly = (f32(id.y) + m) * pf(3u);
    let x0 = clamp(floor(lx), 0.0, f32(P.sw - 1u));
    let y0 = clamp(floor(ly), 0.0, f32(P.sh - 1u));
    let fx = clamp(lx - x0, 0.0, 1.0);
    let fy = clamp(ly - y0, 0.0, 1.0);
    let i = i32(x0);
    let j = i32(y0);
    let a = (guided_coef(i, j, 0u) * (1.0 - fx) + guided_coef(i + 1, j, 0u) * fx) * (1.0 - fy)
        + (guided_coef(i, j + 1, 0u) * (1.0 - fx) + guided_coef(i + 1, j + 1, 0u) * fx) * fy;
    let b = (guided_coef(i, j, 1u) * (1.0 - fx) + guided_coef(i + 1, j, 1u) * fx) * (1.0 - fy)
        + (guided_coef(i, j + 1, 1u) * (1.0 - fx) + guided_coef(i + 1, j + 1, 1u) * fx) * fy;
    let guide = (c.xyz + vec3<f32>(0.1 * pf(1u))) * pf(0u);
    let q = a * guide + b;
    let s = pf(4u);
    let out = clamp((1.0 - s) * c.xyz + s * q, vec3<f32>(0.0), vec3<f32>(max(c.w, 0.0)));
    dst[o] = vec4<f32>(out, c.w);
}

var<workgroup> air_top: array<f32, 256>;
var<workgroup> air_sum: array<vec4<f32>, 256>;

// AI Dehaze's air light, one group: the mean color of the small copy
// (`ta`) where its dark channel (`tb`'s x) is within 1 % of its brightest.
@compute @workgroup_size(256)
fn kit_dehaze_air(@builtin(local_invocation_index) t: u32) {
    let n = P.w * P.h;
    var m = 0.0;
    for (var k = t; k < n; k += 256u) {
        m = max(m, tb[k].x);
    }
    air_top[t] = m;
    workgroupBarrier();
    for (var s = 128u; s > 0u; s = s >> 1u) {
        if t < s {
            air_top[t] = max(air_top[t], air_top[t + s]);
        }
        workgroupBarrier();
    }
    let top = air_top[0] * 0.99;
    var acc = vec4<f32>(0.0);
    for (var k = t; k < n; k += 256u) {
        if tb[k].x >= top {
            acc = acc + vec4<f32>(ta[k].xyz, 1.0);
        }
    }
    air_sum[t] = acc;
    workgroupBarrier();
    for (var s = 128u; s > 0u; s = s >> 1u) {
        if t < s {
            air_sum[t] = air_sum[t] + air_sum[t + s];
        }
        workgroupBarrier();
    }
    if t == 0u {
        let c = air_sum[0];
        dst[0] = vec4<f32>(max(c.xyz / max(c.w, 1.0), vec3<f32>(0.000001)), 1.0);
    }
}

// AI Dehaze's small result: the small copy (`ta`) less the air light
// (`tb[0]`) over the transmission — 1 − 0.95 × the 3 × 3 dark channel of
// the colors over the air light, held at 0.1 — and the air light again.
@compute @workgroup_size(16, 16)
fn kit_dehaze_op(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let air = tb[0].xyz;
    let i = i32(id.x);
    let j = i32(id.y);
    var dark = 3.0e38;
    for (var dj = -1; dj <= 1; dj = dj + 1) {
        for (var di = -1; di <= 1; di = di + 1) {
            let v = guided_x(i + di, j + dj) / air;
            dark = min(dark, min(v.x, min(v.y, v.z)));
        }
    }
    let t = max(1.0 - 0.95 * dark, 0.1);
    let p = guided_x(i, j);
    dst[id.y * P.w + id.x] = vec4<f32>((p - air) / t + air, 0.0);
}
