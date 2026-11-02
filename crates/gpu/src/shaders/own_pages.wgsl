// Page Peel, Page Roll and Page Loop (`bv-effects`' `transitions/pages.rs`,
// the same math in f32): each pixel's place along the direction (u) and
// across it (v) is mapped onto the page, layer after layer — each read
// between pixels where it lands on the picture, lit and laid over by its
// share —; a pixel whose layers read past the picture's pixels, or end
// otherwise than its right, upper and upper right neighbours', is the mean
// of four samples instead, each layer read at its nearest pixel. Rows count
// from the bottom (`j`), as VEGAS's.
//
// f: the pixels' height in their widths (0) and its inverse (1), the
// direction's sine (2) and cosine (3), the light (4–6), the perspective
// (7), the curl's numbers (8–15). i: the curl (x: 0 Peel, 1 Roll, 2 Loop),
// its layers (y), the page's opacity (z) and what is left of it (w) in
// 255ths; j: the loop not over yet (x).

const PG_PI: f32 = 3.1415927;
const PG_HALF_PI: f32 = 1.5707964;
const PG_QUARTER_PI: f32 = 0.7853982;
const PG_SLOPE: f32 = 254.64790;
const PG_SHARE: f32 = 0.003921569;

struct PgHit {
    // The page's place along and across the direction.
    u: f32,
    v: f32,
    light: u32,
    alpha: u32,
    color: vec3<f32>,
    // Whether the layer is drawn here; whether its place is a number.
    hit: bool,
    ok: bool,
}

fn pg_none() -> PgHit {
    return PgHit(0.0, 0.0, 0u, 0u, vec3<f32>(0.0), false, false);
}

fn pg_hit(u: f32, v: f32, light: u32, alpha: u32, color: vec3<f32>, ok: bool) -> PgHit {
    return PgHit(u, v, light, alpha, color, true, ok);
}

fn pg_light() -> vec3<f32> {
    return vec3<f32>(pf(4u), pf(5u), pf(6u));
}

// A float's whole part as a byte, as the processor's `as i32 as u8`.
fn pg_byte(x: f32) -> u32 {
    return u32(i32(x)) & 255u;
}

// A layer's share from `x` (a part of π in 255ths) and the page's opacity.
fn pg_share(x: f32) -> u32 {
    return (pg_byte(x * 0.31830987) + u32(P.i.z)) & 255u;
}

fn pg_asin_poly(z: f32) -> f32 {
    return (((0.042163199 * z + 0.024181311) * z + 0.045470026) * z + 0.074953003) * z + 0.16666752;
}

// The arcsine to a float's precision (the card's own is rougher), where
// it is a number.
fn pg_asin(t0: f32) -> f32 {
    let t = clamp(t0, -1.0, 1.0);
    let a = abs(t);
    var r: f32;
    if a > 0.5 {
        let z = 0.5 * (1.0 - a);
        let s = sqrt(z);
        r = PG_HALF_PI - 2.0 * (s + s * z * pg_asin_poly(z));
    } else {
        let z = a * a;
        r = a + a * z * pg_asin_poly(z);
    }
    if t < 0.0 {
        return -r;
    }
    return r;
}

fn pg_peel(u0: f32, v: f32, layer: i32) -> PgHit {
    let slide = pf(8u);
    let fold = pf(9u);
    let r = pf(10u);
    let inv_r = pf(11u);
    let back = pf(12u);
    let mirror = pf(13u);
    let end = pf(14u);
    let persp = pf(7u);
    let u = u0 - slide;
    if u < fold {
        if layer == 0 {
            return pg_hit(u, v, 0u, 255u, pg_light(), true);
        }
        let f = 1.0 - (persp + persp);
        return pg_hit((mirror - u) * f, f * v, 0u, u32(P.i.z), pg_light(), true);
    }
    if end <= u {
        return pg_none();
    }
    let t = (u - fold) * inv_r;
    let ok = abs(t) <= 1.0;
    let x = pg_asin(t);
    let c = cos(x);
    let rest = f32(P.i.w);
    if layer == 0 {
        let f = (c - 1.0) * persp + 1.0;
        return pg_hit((r * x + fold) * f, f * v, 0u, pg_share((PG_PI - x) * rest), pg_light(), ok);
    }
    let f = 1.0 - (c + 1.0) * persp;
    let lit = (200u - pg_byte(abs(x - PG_QUARTER_PI) * PG_SLOPE)) & 255u;
    return pg_hit((back - r * x) * f, f * v, lit, pg_share(rest * x), pg_light(), ok);
}

fn pg_roll(u0: f32, v: f32, layer: i32) -> PgHit {
    let slide = pf(8u);
    let fold = pf(9u);
    let r = pf(10u);
    let inv_r = pf(11u);
    let back = pf(12u);
    let r9 = pf(13u);
    let inv_r9 = pf(14u);
    let end = pf(15u);
    let persp = pf(7u);
    let rest = f32(P.i.w);
    let u = u0 - slide;
    if layer == 0 {
        if u < fold {
            return pg_hit(u, v, 0u, 255u, pg_light(), true);
        }
        if end <= u {
            return pg_none();
        }
        let t = (u - fold) * inv_r;
        let x = pg_asin(t);
        let f = (cos(x) - 1.0) * persp + 1.0;
        return pg_hit((x * r + fold) * f, f * v, 0u, pg_share(rest * (PG_PI - x)), pg_light(), abs(t) <= 1.0);
    }
    if layer == 1 {
        if u < fold - r || fold + r9 <= u {
            return pg_none();
        }
        var t: f32;
        var x: f32;
        var along: f32;
        if fold <= u {
            t = (u - fold) * inv_r9;
            x = pg_asin(t);
            along = r * x + fold + r * 6.2831855;
        } else {
            t = (fold - u) * inv_r;
            x = pg_asin(t);
            along = (r * PG_PI + back) - r * x;
        }
        let f = (cos(x) - 1.0) * persp + 1.0;
        return pg_hit(along * f, f * v, 0u, pg_share((PG_PI - x) * rest), pg_light(), abs(t) <= 1.0);
    }
    var lo = fold - r;
    var hi = end;
    var radius = r;
    var inv = inv_r;
    if layer == 2 {
        lo = fold - r9;
        hi = r9 + fold;
        radius = r9;
        inv = inv_r9;
    }
    if u < lo || hi <= u {
        return pg_none();
    }
    var t: f32;
    var x: f32;
    var along: f32;
    var dim: u32;
    var color = pg_light();
    if fold <= u {
        t = (u - fold) * inv;
        x = pg_asin(t);
        dim = pg_byte(abs(x - PG_QUARTER_PI) * PG_SLOPE);
        if layer == 2 {
            along = fold - r9 * x;
        } else {
            along = back - r * x;
        }
    } else {
        t = (fold - u) * inv;
        x = pg_asin(t);
        dim = pg_byte((PG_HALF_PI - x) * PG_SLOPE * 0.5);
        color = vec3<f32>(0.0);
        if layer == 2 {
            along = r9 * x + fold;
        } else {
            along = r * x + back;
        }
    }
    let lit = (200u - dim) & 255u;
    let alpha = pg_share(rest * x);
    if layer == 2 {
        let f = 1.0 - (cos(x) + 1.0) * persp;
        return pg_hit((along + r9 * 9.424778) * f, f * v, lit, alpha, color, abs(t) <= 1.0);
    }
    let s = 1.0 - (cos(x) + 1.0) * persp;
    return pg_hit(along * s, s * v, lit, alpha, color, abs(t) <= 1.0);
}

// The paper's glow at angle `x` of the loop, 0–1.
fn pg_glow(x: f32) -> f32 {
    return 1.0 - abs((x - PG_QUARTER_PI) * 1.2732395);
}

// Page Loop; `last` the layer before (its loop's layers, where it has
// none, are the paper's again).
fn pg_loop(u0: f32, v: f32, layer: i32, last: PgHit) -> PgHit {
    let at = pf(8u);
    let rise = pf(9u);
    let shift = pf(10u);
    let around = pf(11u);
    let alpha = pf(12u);
    let lo = pf(14u);
    let hi = pf(15u);
    let persp = pf(7u);
    let opaque = 255.0 - alpha;
    var u = u0;
    if layer == 0 {
        if at <= u {
            if P.j.x != 0u {
                let t = (u - at) / rise;
                let x = pg_asin(t);
                let f = 1.0 - (1.0 - cos(x)) * persp;
                let a = opaque * x * 0.63661975 + alpha;
                let lit = pg_byte(pg_glow(x) * 200.0 + 0.5);
                let along = ((at + around + rise * x) - shift) * f;
                return pg_hit(along, f * v, lit, pg_byte(a + 0.5), pg_light(), abs(t) <= 1.0);
            }
            u = u + around;
        }
        return pg_hit(u - shift, v, 0u, pg_byte(alpha + 0.5), pg_light(), true);
    }
    if u <= lo || hi <= u {
        return last;
    }
    let r = max(pf(13u), 0.001);
    var t: f32;
    if at <= u {
        t = (u - at) / r;
    } else {
        t = (at - u) / r;
    }
    var x = pg_asin(t);
    let ok = abs(t) <= 1.0;
    let a = pg_byte(opaque * x * 0.63661975 + alpha + 0.5);
    let c = cos(x);
    if at <= u {
        var turn = c + 1.0;
        var lit = 0u;
        if layer == 1 {
            turn = 1.0 - c;
            lit = pg_byte(pg_glow(x) * 200.0);
        } else {
            x = PG_PI - x;
        }
        let f = 1.0 - turn * persp;
        return pg_hit(((r * x + at) - shift) * f, f * v, lit, a, pg_light(), ok);
    }
    if layer == 1 {
        let f = 1.0 - (1.0 - c) * persp;
        return pg_hit((((6.2831855 - x) * r + at) - shift) * f, f * v, 0u, a, pg_light(), ok);
    }
    let f = 1.0 - (c + 1.0) * persp;
    let along = (((x + PG_PI) * r + at) - shift) * f;
    return pg_hit(along, f * v, pg_byte(pg_glow(x) * 200.0 + 0.5), a, pg_light(), ok);
}

fn pg_map(u: f32, v: f32, layer: i32, last: PgHit) -> PgHit {
    if P.i.x == 0 {
        return pg_peel(u, v, layer);
    }
    if P.i.x == 1 {
        return pg_roll(u, v, layer);
    }
    return pg_loop(u, v, layer, last);
}

// The point `x`, `y` (pixels from the frame's middle, y up in pixel
// widths) along and across the direction.
fn pg_turned(x: f32, y: f32) -> vec2<f32> {
    let s = pf(2u);
    let c = pf(3u);
    let ys = y * s;
    let yc = y * c;
    return vec2<f32>(x * c + ys, yc - x * s);
}

// Where a hit lands in the picture: its row (from the bottom), column.
fn pg_landing(h: PgHit) -> vec2<f32> {
    let s = pf(2u);
    let c = pf(3u);
    let row = (h.v * c + h.u * s) * pf(1u) + f32(P.h) * 0.5;
    let col = h.v * -s + h.u * c + f32(P.w) * 0.5;
    return vec2<f32>(row, col);
}

// The page's picture and the one under it at row `r` (from the bottom),
// column `c`.
fn pg_page(r: i32, c: i32) -> vec4<f32> {
    let o = u32(i32(P.h) - 1 - r) * P.w + u32(c);
    if P.i.x == 2 {
        return tb[o];
    }
    return ta[o];
}

fn pg_under(r: i32, c: i32) -> vec4<f32> {
    let o = u32(i32(P.h) - 1 - r) * P.w + u32(c);
    if P.i.x == 2 {
        return ta[o];
    }
    return tb[o];
}

fn pg_lit(c: vec4<f32>, h: PgHit) -> vec4<f32> {
    if h.light == 0u {
        return c;
    }
    let s = f32(h.light) * PG_SHARE;
    return vec4<f32>(
        (h.color.x - c.x) * s + c.x,
        (h.color.y - c.y) * s + c.y,
        (h.color.z - c.z) * s + c.z,
        (1.0 - c.w) * s + c.w
    );
}

struct PgPixel {
    c: vec4<f32>,
    flag: u32,
}

// Pixel `i` of row `j` drawn layer after layer (`colors` false: only how
// its layers end: 127 none, else the last one's number, 128 more when it
// read past the picture's pixels).
fn pg_pixel(i: i32, j: i32, colors: bool) -> PgPixel {
    let w = i32(P.w);
    let h = i32(P.h);
    let y = (f32(j) - f32(P.h) * 0.5) * pf(0u);
    let uv = pg_turned(f32(i) - f32(P.w) * 0.5, y);
    var out = vec4<f32>(0.0);
    if colors {
        out = pg_under(j, i);
    }
    var flag = 127u;
    var last = pg_none();
    for (var layer = 0; layer < P.i.y; layer = layer + 1) {
        let m = pg_map(uv.x, uv.y, layer, last);
        if !m.hit {
            continue;
        }
        last = m;
        let at = pg_landing(m);
        let row = at.x;
        let col = at.y;
        if !m.ok || !(row >= -1.0 && row < f32(h) && col >= -1.0 && col < f32(w)) {
            continue;
        }
        let r0 = i32(floor(row));
        let c0 = i32(floor(col));
        let left = c0 < 0;
        let right = c0 + 1 >= w;
        let low = r0 < 0;
        let high = r0 + 1 >= h;
        flag = u32(layer);
        if left || right || low || high {
            flag = flag + 128u;
        }
        if !colors {
            continue;
        }
        var p00 = out;
        var p01 = out;
        var p10 = out;
        var p11 = out;
        if !(left || low) {
            p00 = pg_page(r0, c0);
        }
        if !(right || low) {
            p01 = pg_page(r0, c0 + 1);
        }
        if !(left || high) {
            p10 = pg_page(r0 + 1, c0);
        }
        if !(right || high) {
            p11 = pg_page(r0 + 1, c0 + 1);
        }
        let wr = 1.0 - (row - f32(r0));
        let wc = 1.0 - (col - f32(c0));
        let fc = 1.0 - wc;
        let fr = 1.0 - wr;
        let c = pg_lit((wc * p00 + fc * p01) * wr + (fc * p11 + wc * p10) * fr, m);
        let s = f32(m.alpha) * PG_SHARE;
        out = (c - out) * s + out;
    }
    return PgPixel(out, flag);
}

// Pixel `i` of row `j` as the mean of four samples a third of a pixel
// apart, each layer read at its nearest pixel.
fn pg_smoothed(i: i32, j: i32) -> vec4<f32> {
    let w = f32(P.w);
    let h = f32(P.h);
    let base = pg_under(j, i);
    var sum = vec4<f32>(0.0);
    let top = f32(j + 1) - 0.1;
    let right = f32(i) + 0.7;
    var y = f32(j + 1) - 0.666;
    loop {
        if !(y < top) {
            break;
        }
        let yy = (y - h * 0.5) * pf(0u);
        var x = f32(i) + 0.333;
        loop {
            if !(x < right) {
                break;
            }
            let uv = pg_turned(x - w * 0.5, yy);
            var c = base;
            var last = pg_none();
            for (var layer = 0; layer < P.i.y; layer = layer + 1) {
                let m = pg_map(uv.x, uv.y, layer, last);
                if !m.hit {
                    continue;
                }
                last = m;
                let at = pg_landing(m);
                if !m.ok || !(at.x >= 0.0 && at.x < h && at.y >= 0.0 && at.y < w) {
                    continue;
                }
                let s = pg_lit(pg_page(i32(at.x), i32(at.y)), m);
                let k = f32(m.alpha) * PG_SHARE;
                c = c + (s - c) * k;
            }
            sum = sum + c;
            x = x + 0.333;
        }
        y = y + 0.333;
    }
    return sum * 0.25;
}

@compute @workgroup_size(16, 16)
fn tr_pages(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let w = i32(P.w);
    let h = i32(P.h);
    let i = i32(id.x);
    let j = h - 1 - i32(id.y);
    let o = id.y * P.w + id.x;
    let own = pg_pixel(i, j, true);
    // The neighbours' ends: the last column's right one is itself, the
    // last row's upper ones its own row's; past the last column the row
    // above reads what the row two below left there (nothing for the
    // first two rows), as VEGAS's.
    let a0 = own.flag;
    var a1 = a0;
    if i + 1 < w {
        a1 = pg_pixel(i + 1, j, false).flag;
    }
    var b0 = a0;
    var b1 = a1;
    if j + 1 < h {
        b0 = pg_pixel(i, j + 1, false).flag;
        if i + 1 < w {
            b1 = pg_pixel(i + 1, j + 1, false).flag;
        } else if j >= 2 {
            b1 = pg_pixel(w - 1, j - 2, false).flag;
        } else {
            b1 = 0u;
        }
    }
    if a0 > 127u || (a0 & a1 & b0 & b1) != (a0 | a1 | b0 | b1) {
        dst[o] = pg_smoothed(i, j);
    } else {
        dst[o] = own.c;
    }
}
