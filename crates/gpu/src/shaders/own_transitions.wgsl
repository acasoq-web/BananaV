// BananaV's own transitions on the card worked out pixel by pixel
// (`bv-effects`' `transitions/pixel.rs`, the same math in f32): `ta` the
// picture going out, `tb` the one coming in, `dst` what shows, all `w` ×
// `h` (rows from the top).

fn tr_clamp01(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

// Linear Wipe — f: the direction (0, 1), the frame's corners' lowest value
// along it (2) and one over their span (3), the feather (4), the progress
// (5).
@compute @workgroup_size(16, 16)
fn tr_linear_wipe(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let d = (pf(0u) * f32(id.x) + pf(1u) * f32(id.y) - pf(2u)) * pf(3u);
    let feather = pf(4u);
    let e = feather * 0.25;
    let at = (e + 1.0 + e) * pf(5u) - e;
    var k = 0.0;
    if d > at + e {
        k = 1.0;
    } else if d >= at - e {
        var inv = 0.0;
        if feather > 0.0 {
            inv = 2.0 / feather;
        }
        let t = ((d - at) + e) * inv;
        k = (3.0 - (t + t)) * t * t;
    }
    let b = tb[o];
    dst[o] = (ta[o] - b) * k + b;
}

// Clock Wipe — f: the pixels' shape across (0) and down (1), the feather
// in degrees (2), the progress (3), half the width and half the height in
// whole pixels (4, 5); i.x: counterclockwise.
@compute @workgroup_size(16, 16)
fn tr_clock_wipe(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let phi = pf(2u) * 0.017453292;
    let full = phi + 6.2831855;
    let p = pf(3u);
    let edge = full * p;
    let dy = ((pf(5u) + 0.5) - f32(id.y)) * pf(1u);
    var dx = (f32(id.x) - pf(4u)) * pf(0u);
    if P.i.x != 0 {
        dx = (pf(4u) - f32(id.x)) * pf(0u);
    }
    var th = atan2(dx, dy);
    if th < 0.0 {
        th += 6.2831855;
    }
    var k = 0.0;
    if edge > th {
        let r = max(sqrt(dx * dx + dy * dy), 2.0);
        let w = max(asin(1.4142135 / r), phi);
        let start = min(edge - w, full);
        if p >= 1.0 || th <= start {
            k = 1.0;
        } else {
            let s = 1.0 - (th - start) / w;
            k = (3.0 - (s + s)) * s * s;
        }
    }
    let a = ta[o];
    dst[o] = (tb[o] - a) * k + a;
}

// Iris's shapes: how far out shape `shape` reaches to x, y (pixels from
// its center, y up); `a` the cell's height over its width.
fn tr_iris_norm(shape: i32, x: f32, y: f32, a: f32) -> f32 {
    let r3 = 0.57735026;
    let r3x2 = 1.1547005;
    let s3 = 1.7320508;
    let two3 = 0.6666667;
    let one3 = 0.33333334;
    let long = 1.118034;
    let short = 0.559017;
    if shape == 1 {
        return sqrt((x * a) * (x * a) + y * y);
    }
    if shape == 2 {
        return sqrt(x * x + (y * a) * (y * a));
    }
    if shape == 3 {
        return max(abs(x) * a, abs(y));
    }
    if shape == 4 {
        return abs(y) + abs(x);
    }
    if shape == 5 {
        return max(abs(x), abs(y));
    }
    if shape == 6 {
        let t = x * r3;
        if x <= 0.0 && t < y {
            return abs((two3 - y) * -r3 - (0.0 - x) * -1.0);
        }
        if y <= t && y < -t {
            return abs((-one3 - y) * r3x2);
        }
        return abs((-one3 - y) * -r3 - (r3 - x));
    }
    if shape == 7 {
        let t = -(x * s3);
        if y <= t || y < 0.0 {
            if y <= t && x * s3 < y {
                return abs((-one3 - x) * r3x2);
            }
            return abs((-r3 - y) - (-one3 - x) * r3);
        }
        return abs(y - (two3 - x) * r3);
    }
    if shape == 8 {
        if 0.0 <= x && y < x * r3 {
            return abs((-two3 - y) * r3 - (0.0 - x));
        }
        if x < 0.0 && y <= -(x * r3) {
            return abs((one3 - y) * r3 - (-r3 - x) * -1.0);
        }
        return abs((one3 - y) * -r3x2);
    }
    if shape == 9 {
        if 0.0 < y && x * s3 <= y {
            return abs((r3 - y) * -1.0 - (one3 - x) * -r3);
        }
        if y <= 0.0 && y < -(x * s3) {
            return abs((-r3 - y) * -1.0 - (one3 - x) * r3);
        }
        return abs((one3 - x) * r3x2);
    }
    if shape == 10 {
        if 0.0 <= x && y < -x {
            return abs((-0.4 - y) * 0.8 - (0.0 - x) * -0.4) * long;
        }
        if 0.0 < x && -x <= y {
            return abs((-0.8 - y) * -0.8 - (0.8 - x) * 1.6) * short;
        }
        if x <= 0.0 && x < y {
            return abs((0.8 - y) * -0.8 - (0.0 - x) * -1.6) * short;
        }
        return abs((-0.8 - y) * 0.8 - (-0.8 - x) * 0.4) * long;
    }
    if shape == 11 {
        if 0.0 <= y && -x < y {
            return abs((0.0 - y) * -1.6 - (0.8 - x) * 0.8) * short;
        }
        if 0.0 < y && y <= -x {
            return abs((0.8 - y) * 0.4 - (-0.8 - x) * -0.8) * long;
        }
        if 0.0 < y || y <= x {
            return abs((-0.8 - y) * 1.6 - (-0.8 - x) * 0.8) * short;
        }
        return abs((0.0 - y) * -0.4 - (-0.4 - x) * -0.8) * long;
    }
    if shape == 12 {
        if 0.0 < x && x <= y {
            return abs((0.8 - y) * -0.8 - (0.8 - x) * -0.4) * long;
        }
        if 0.0 < x {
            return abs((-0.8 - y) * 0.8 - (0.0 - x) * 1.6) * short;
        }
        if -x < y {
            return abs((0.4 - y) * -0.8 - (0.0 - x) * 0.4) * long;
        }
        if 0.0 <= x {
            return abs((-0.8 - y) * 0.8 - (0.0 - x) * 1.6) * short;
        }
        return abs((0.8 - y) * 0.8 - (-0.8 - x) * -1.6) * short;
    }
    if shape == 13 {
        if 0.0 <= y && y < x {
            return abs((0.0 - y) * 0.4 - (0.4 - x) * 0.8) * long;
        }
        if 0.0 < y && x <= y {
            return abs((0.8 - y) * -1.6 - (0.8 - x) * -0.8) * short;
        }
        if y <= 0.0 && y < -x {
            return abs((0.0 - y) * 1.6 - (-0.8 - x) * -0.8) * short;
        }
        return abs((-0.8 - y) * -0.4 - (0.8 - x) * 0.8) * long;
    }
    return sqrt(x * x + y * y);
}

// The cell of pixel coordinate `p` along an axis (-1 before the first
// shape's center), `first` the first center, `size` a cell.
fn tr_iris_cell(p: f32, first: f32, size: f32, count: i32) -> i32 {
    if p >= first {
        return min(i32(p - first) / max(i32(size), 1), count - 1);
    }
    return -1;
}

// A pixel's share for a band of `color` between the picture inside and the
// one outside: x the inner's share, y the outer's, z the color's, w which
// alpha (0 the inner's, 1 the outer's, 2 the color's).
fn tr_band(inner: vec4<f32>, outer: vec4<f32>, color: vec3<f32>, k: vec4<f32>) -> vec4<f32> {
    let c = vec4<f32>(color, 1.0);
    var out = k.x * inner + k.y * outer + k.z * c;
    if k.w < 0.5 {
        out.w = inner.w;
    } else if k.w < 1.5 {
        out.w = outer.w;
    } else {
        out.w = 1.0;
    }
    return out;
}

// Iris — f: the progress (0), the border (1) and feather (2) as parts of
// the reach, one over the largest reach on the frame's edge (3), the
// pixels' shape (4, 5), the cell's aspect (6), a cell's size (7, 8), the
// first center (9, 10), the border's color (11–13); i: the shape, the
// repeats across and down, Iris out.
@compute @workgroup_size(16, 16)
fn tr_iris(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let px = f32(id.x);
    let py = f32(P.h - 1u - id.y);
    let ix = tr_iris_cell(px, pf(9u), pf(7u), P.i.y);
    let iy = tr_iris_cell(py, pf(10u), pf(8u), P.i.z);
    var best = 3.4e38;
    for (var j = max(iy - 1, 0); j <= min(iy + 2, P.i.z - 1); j++) {
        for (var i = max(ix - 1, 0); i <= min(ix + 2, P.i.y - 1); i++) {
            let cx = pf(9u) + f32(i) * pf(7u);
            let cy = pf(10u) + f32(j) * pf(8u);
            best = min(best, tr_iris_norm(P.i.x, (px - cx) * pf(4u), (py - cy) * pf(5u), pf(6u)));
        }
    }
    let v = best * pf(3u);
    var at = pf(0u);
    var inner = tb[o];
    var outer = ta[o];
    if P.i.w == 0 {
        at = 1.0 - pf(0u);
        inner = ta[o];
        outer = tb[o];
    }
    let border = pf(1u);
    let feather = pf(2u);
    var k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    if border >= 1e-6 {
        let lo = min(max(at - border, 0.0), at);
        let hi = max(min(at + border, 1.0), at);
        let lo2 = min(max(lo - feather, 0.0), lo);
        let hi2 = max(min(hi + feather, 1.0), hi);
        if v < lo2 {
            k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
        } else if v > hi2 {
            k = vec4<f32>(0.0, 1.0, 0.0, 1.0);
        } else if lo < v {
            if v < hi {
                k = vec4<f32>(0.0, 0.0, 1.0, 2.0);
            } else {
                let t = tr_clamp01((v - hi) * (1.0 / (hi2 - hi)));
                k = vec4<f32>(0.0, t, 1.0 - t, 1.0);
            }
        } else {
            let t = tr_clamp01((1.0 / (lo - lo2)) * (v - lo2));
            k = vec4<f32>(1.0 - t, 0.0, t, 0.0);
        }
    } else if feather >= 1e-6 {
        let lo = min(max(at - feather, 0.0), at);
        let hi = max(min(feather + at, 1.0), at);
        if v < lo {
            k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
        } else if v > hi {
            k = vec4<f32>(0.0, 1.0, 0.0, 1.0);
        } else {
            let t = tr_clamp01((1.0 / (hi - lo)) * (v - lo));
            var which = 0.0;
            if t >= 0.5 {
                which = 1.0;
            }
            k = vec4<f32>(1.0 - t, t, 0.0, which);
        }
    } else if at <= v {
        k = vec4<f32>(0.0, 1.0, 0.0, 1.0);
    }
    dst[o] = tr_band(inner, outer, vec3<f32>(pf(11u), pf(12u), pf(13u)), k);
}

// Star Wipe's reach to pixel x, y (from the bottom), at least 1 — f: the
// center (3, 4), the angle's cosine and sine (5, 6), an arm's sector (7),
// the slope (8), the pixels' shape (12, 13).
fn tr_star_norm(x: f32, y: f32) -> f32 {
    if abs(x - pf(3u)) < 1e-6 && abs(y - pf(4u)) < 1e-6 {
        return 1.0;
    }
    let dx = (x - pf(3u)) * pf(12u);
    let dy = (y - pf(4u)) * pf(13u);
    let u = dx * pf(5u) + dy * pf(6u);
    let v = dy * pf(5u) - dx * pf(6u);
    var phi = atan2(v, u);
    if phi < 0.0 {
        phi += 6.2831855;
    }
    let sector = pf(7u);
    let half = sector * 0.5;
    var arm = 0.0;
    if half < phi && phi < 6.2831855 - half {
        arm = f32(i32((half + phi) / sector)) * sector;
    }
    let along = abs(sin(arm) * v + cos(arm) * u);
    let across = abs(cos(arm) * v - sin(arm) * u);
    let n = across - along * pf(8u);
    if n >= 1.0 {
        return n;
    }
    return 1.0;
}

// The star's wave at x, y — f: cycles over the largest reach (9); i.x:
// Tooth, i.y: Reverse; `mirrored`: whole cycles taken off as VEGAS's
// mirrored maps take them.
fn tr_star_wave(x: f32, y: f32, mirrored: bool) -> f32 {
    var v = pf(9u) * tr_star_norm(x, y);
    if mirrored {
        if v > 1.0 {
            v -= f32(i32(v));
        }
    } else {
        for (var k = 0; k < 8 && v > 1.0; k++) {
            v -= 1.0;
        }
    }
    if P.i.x != 0 {
        if v >= 0.5 {
            v = 1.0 - v;
        }
        v += v;
    }
    if P.i.y != 0 {
        v = 1.0 - v;
    }
    return tr_clamp01(v);
}

// Star Wipe — f: the progress (0), the border's (1) and feather's (2)
// half widths in the map's units, the star (3–9, 12, 13), the mirrored
// maps' lowest value (10) and stretch (11), the border's color (14–16);
// i: Tooth, Reverse, the mirrors (bit 1 horizontal, 2 vertical), a flat
// map.
@compute @workgroup_size(16, 16)
fn tr_star_wipe(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let w = i32(P.w);
    let h = i32(P.h);
    var x = i32(id.x);
    var y = h - 1 - i32(id.y);
    var m = 1.0;
    if P.i.w == 0 {
        if P.i.z == 0 {
            m = tr_star_wave(f32(x), f32(y), false);
        } else {
            var gap = false;
            if (P.i.z & 1) != 0 && y < h / 2 {
                y = 2 * (h / 2) - 1 - y;
            }
            if (P.i.z & 2) != 0 && x >= w / 2 {
                if w % 2 == 1 && x == w / 2 {
                    gap = true;
                }
                x = w - 1 - x;
            }
            m = 0.0;
            if !gap {
                m = (tr_star_wave(f32(x), f32(y), true) - pf(10u)) * pf(11u);
            }
        }
    }
    let p = pf(0u);
    let border = pf(1u);
    let feather = pf(2u);
    // The second picture inside, the first outside.
    var k = vec4<f32>(0.0, 1.0, 0.0, 1.0);
    if border >= 1e-6 && feather >= 1e-6 {
        let lo = p - border;
        let hi = p + border;
        let lo2 = lo - feather;
        let hi2 = hi + feather;
        if m < lo2 {
            k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
        } else if m > hi2 {
            k = vec4<f32>(0.0, 1.0, 0.0, 1.0);
        } else if lo < m {
            if m < hi {
                k = vec4<f32>(0.0, 0.0, 1.0, 2.0);
            } else {
                let t = tr_clamp01((m - hi) * (1.0 / (hi2 - hi)));
                k = vec4<f32>(0.0, t, 1.0 - t, 1.0);
            }
        } else {
            let t = tr_clamp01((m - lo2) * (1.0 / (lo - lo2)));
            k = vec4<f32>(1.0 - t, 0.0, t, 0.0);
        }
    } else if border >= 1e-6 {
        if m < p - border {
            k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
        } else if m <= p + border {
            k = vec4<f32>(0.0, 0.0, 1.0, 2.0);
        }
    } else if feather >= 1e-6 {
        let lo = p - feather;
        let hi = feather + p;
        if m < lo {
            k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
        } else if m <= hi {
            let t = tr_clamp01(1.0 - (m - lo) * (1.0 / (hi - lo)));
            var which = 0.0;
            if t < 0.5 {
                which = 1.0;
            }
            k = vec4<f32>(t, 1.0 - t, 0.0, which);
        }
    } else if m < p {
        k = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    dst[o] = tr_band(tb[o], ta[o], vec3<f32>(pf(14u), pf(15u), pf(16u)), k);
}

// Flash — `ta` the picture lighting, `tb` its light (blurred, or itself);
// f: the white (0), the light's weight (1), the tint (2–4).
@compute @workgroup_size(16, 16)
fn tr_flash(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let base = ta[o];
    let light = tb[o];
    let white = pf(0u);
    let y = (light.x * 0.299 + light.y * 0.587 + light.z * 0.114) * pf(1u);
    let a = y + white + base.w;
    dst[o] = vec4<f32>(
        y * pf(2u) + white + base.x,
        y * pf(3u) + white + base.y,
        y * pf(4u) + white + base.z,
        min(a, 1.0),
    );
}

// Dissolve — f: the progress (0), the bleed speeds (1–4), the morph speeds
// (5–8), the color faded through (9–12); i.x: the type.
@compute @workgroup_size(16, 16)
fn tr_dissolve(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let a = ta[o];
    let b = tb[o];
    let p = pf(0u);
    let alpha = a.w + (b.w - a.w) * p;
    let t = P.i.x;
    var out = vec4<f32>(0.0);
    if t == 0 {
        if p < 0.5 {
            out = (p + p) * b + a;
        } else {
            let k = 1.0 - ((p - 0.5) + (p - 0.5));
            out = k * a + b;
        }
        out.w = tr_clamp01(out.w);
    } else if t == 1 {
        if p < 0.5 {
            out = vec4<f32>(abs(a.xyz - (p + p) * b.xyz), alpha);
        } else {
            let k = 1.0 - ((p - 0.5) + (p - 0.5));
            out = vec4<f32>(abs(b.xyz - k * a.xyz), alpha);
        }
    } else if t == 2 {
        out = vec4<f32>(abs((a.xyz + b.xyz) * p - a.xyz), alpha);
    } else if t == 3 {
        let s = vec4<f32>(pf(1u) + 1.0, pf(2u) + 1.0, pf(2u) + 1.0, pf(4u) + 1.0);
        let most = max(max(1.0 / s.x, 1.0 / s.y), 1.0 / s.w);
        let q = most * p;
        let r = most - q;
        let ka = clamp(s * q, vec4<f32>(0.0), vec4<f32>(1.0));
        let kb = clamp(s * r, vec4<f32>(0.0), vec4<f32>(1.0));
        out = max(a - ka, vec4<f32>(0.0)) + max(b - kb, vec4<f32>(0.0));
        out.w = tr_clamp01(out.w);
    } else if t == 4 {
        let s = vec4<f32>(pf(5u), pf(6u), pf(7u), pf(8u)) + vec4<f32>(1.0);
        let most = max(max(1.0 / s.x, 1.0 / s.y), max(1.0 / s.z, 1.0 / s.w));
        let step = clamp(s * most * p, vec4<f32>(0.0), vec4<f32>(1.0));
        let down = max(a - step, b);
        let up = min(a + step, b);
        out = select(up, down, a >= b);
    } else if t == 5 {
        out = a;
        if (a.y + a.x + a.z) / 3.0 <= p {
            out = b;
        }
    } else if t == 6 {
        out = b;
        if (b.y + b.x + b.z) / 3.0 < 1.0 - p {
            out = a;
        }
    } else if t == 7 {
        let g = (a.x + a.y + a.z + b.x + b.y + b.z) / 6.0;
        if p < 0.5 {
            out = vec4<f32>(a.xyz + (vec3<f32>(g) - a.xyz) * (p + p), alpha);
        } else {
            out = vec4<f32>(vec3<f32>(g) + (b.xyz - vec3<f32>(g)) * ((p - 0.5) + (p - 0.5)), alpha);
        }
    } else if t == 8 {
        let c = vec4<f32>(pf(9u) * pf(12u), pf(10u) * pf(12u), pf(11u) * pf(12u), pf(12u));
        if p < 0.5 {
            out = a + (c - a) * (p + p);
        } else {
            out = c + (b - c) * tr_clamp01((p - 0.5) + (p - 0.5));
        }
    } else {
        let k = vec3<f32>(
            tr_clamp01((p - 0.0) * 3.0),
            tr_clamp01((p - 1.0 / 3.0) * 3.0),
            tr_clamp01((p - 2.0 / 3.0) * 3.0),
        );
        out = vec4<f32>(a.xyz + (b.xyz - a.xyz) * k, alpha);
    }
    dst[o] = out;
}

// A picture between pixels, x from the left and y from the bottom, the
// last column and row held (Cross Effect's zoom).
fn tr_cross_texel(second: bool, i: i32, j: i32) -> vec4<f32> {
    let x = u32(clamp(i, 0, i32(P.w) - 1));
    let y = P.h - 1u - u32(clamp(j, 0, i32(P.h) - 1));
    if second {
        return tb[y * P.w + x];
    }
    return ta[y * P.w + x];
}

fn tr_cross_zoomed(second: bool, x: f32, y: f32) -> vec4<f32> {
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let i = i32(x0);
    let j = i32(y0);
    let a = tr_cross_texel(second, i, j);
    let b = tr_cross_texel(second, i + 1, j);
    let c = tr_cross_texel(second, i, j + 1);
    let d = tr_cross_texel(second, i + 1, j + 1);
    let top = (b - a) * fx + a;
    let bottom = c + (d - c) * fx;
    return (bottom - top) * fy + top;
}

// Cross Effect — `ta`, `tb` the pictures as the effect left them (or as
// they are); f: the fade (0), one over each picture's zoom (1, 2), the
// zooms' centers (3, 4 and 5, 6; pixels, y up); i.x: the pictures zoomed
// (bit 1 the first, 2 the second).
@compute @workgroup_size(16, 16)
fn tr_cross_effect(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    var a = ta[o];
    var b = tb[o];
    let x = f32(id.x);
    let y = f32(P.h - 1u - id.y);
    if (P.i.x & 1) != 0 {
        a = tr_cross_zoomed(false, (x - pf(3u)) * pf(1u) + pf(3u), (y - pf(4u)) * pf(1u) + pf(4u));
    }
    if (P.i.x & 2) != 0 {
        b = tr_cross_zoomed(true, (x - pf(5u)) * pf(2u) + pf(5u), (y - pf(6u)) * pf(2u) + pf(6u));
    }
    dst[o] = (b - a) * pf(0u) + a;
}

// Cross Effect's cells. The table (`tb`): each cell column's first pixel
// and width, each cell row's (rows from the bottom), each pixel's column
// of cells, each row's (from the bottom); i: the cells across and down.

// Each row's sums over each cell column: `ta` the picture (`sw` wide),
// `dst` the sums (the cells across × the rows).
@compute @workgroup_size(16, 16)
fn tr_cell_rows(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let cell = tb[id.x];
    let start = u32(cell.x);
    let width = u32(cell.y);
    var acc = vec4<f32>(0.0);
    for (var k = 0u; k < width; k++) {
        acc += ta[id.y * P.sw + start + k];
    }
    dst[o] = acc;
}

// Each cell's average from the rows' sums (`ta`: the cells across × `sh`
// rows from the top).
@compute @workgroup_size(16, 16)
fn tr_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let nx = u32(P.i.x);
    let column = tb[id.x];
    // Cells are counted from the bottom, as their rows.
    let j = P.h - 1u - id.y;
    let row = tb[nx + j];
    let start = u32(row.x);
    let height = u32(row.y);
    var acc = vec4<f32>(0.0);
    for (var k = 0u; k < height; k++) {
        let y = P.sh - 1u - (start + k);
        acc += ta[y * nx + id.x];
    }
    dst[o] = acc / column.y / row.y;
}

// Each pixel its cell's average (`ta`: the cells, rows from the top).
@compute @workgroup_size(16, 16)
fn tr_cell_fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = index(id);
    if o < 0 {
        return;
    }
    let nx = u32(P.i.x);
    let ny = u32(P.i.y);
    let i = u32(tb[nx + ny + id.x].x);
    let j = u32(tb[nx + ny + P.w + (P.h - 1u - id.y)].x);
    dst[o] = ta[(ny - 1u - j) * nx + i];
}
