// BananaV's own effects that change each pixel where it is (`bv-effects`'s
// plug-ins, the same math as their renders): Crop's window, the Color
// Corrector, Cookie Cutter, White Balance's matrix, HSL Adjust, Saturation
// Adjust, Border, Add Noise. Their places are the plug-ins' own: canonical
// units from the frame's lower left, the picture at render scale.

// Pixel `x` of row `y` (from the top) in the plug-ins' coordinates, at
// render scale `scale`.
fn own_place(x: u32, y: u32, scale: f32) -> vec2<f32> {
    return vec2<f32>((f32(x) + 0.5) / scale, (f32(P.h - 1u - y) + 0.5) / scale);
}

// Crop: the picture times the opacity where its window covers a pixel's
// corner, as VEGAS's, nothing elsewhere. f: the pivot (0, 1), the sides
// from it unturned (lo 2, 3; hi 4, 5), the turn's cosine and sine (6, 7),
// the opacity (8), the render scale (9).
@compute @workgroup_size(16, 16)
fn crop_window(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let corner = vec2<f32>(f32(id.x), f32(P.h - 1u - id.y)) / pf(9u);
    let d = corner - vec2<f32>(pf(0u), pf(1u));
    let c = pf(6u);
    let s = pf(7u);
    let q = vec2<f32>(d.x * c + d.y * s, -d.x * s + d.y * c);
    let inside = q.x >= pf(2u) && q.x <= pf(4u) && q.y >= pf(3u) && q.y <= pf(5u);
    img[i] = select(vec4<f32>(0.0), img[i] * pf(8u), inside);
}

// The Color Corrector's Gamma on a luma `y`: raised to `p` in BT.709's
// encoding of it, as if it were light (`color_corrector::gamma_curve`).
fn cc_gamma(y: f32, p: f32) -> f32 {
    if p == 1.0 {
        return y;
    }
    if y <= 0.0 {
        return 0.0;
    }
    return bt709_decode(pow(bt709_encode(y), p));
}

// The Color Corrector, on the straight color (BT.601's Y′UV): each wheel's
// shift to U and V by its weight — whole at black, 0.5 and white, half a
// third of the way from black or white and a fifth from 0.5,
// 2^-(distance / half)² —, the saturation, Gamma's curve (`cc_gamma`),
// gain and offset, unclipped. f: the shifts (Low 0, 1; Mid 2, 3;
// High 4, 5), saturation (6), power (7), gain (8), offset (9).
@compute @workgroup_size(16, 16)
fn color_corrector(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = vec3<f32>(px.x / a, px.y / a, px.z / a);
    let y = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
    var u = 0.492111 * (c.z - y);
    var v = 0.877283 * (c.x - y);
    let yc = clamp(y, 0.0, 1.0);
    let dm = yc - 0.5;
    let dh = 1.0 - yc;
    let w0 = exp2(-9.0 * yc * yc);
    let w1 = exp2(-25.0 * dm * dm);
    let w2 = exp2(-9.0 * dh * dh);
    u += w0 * pf(0u) + w1 * pf(2u) + w2 * pf(4u);
    v += w0 * pf(1u) + w1 * pf(3u) + w2 * pf(5u);
    u *= pf(6u);
    v *= pf(6u);
    let l = pf(8u) * cc_gamma(y, pf(7u)) + pf(9u);
    let r = l + v / 0.877283;
    let b = l + u / 0.492111;
    let g = (l - 0.299 * r - 0.114 * b) / 0.587;
    img[i] = vec4<f32>(vec3<f32>(r, g, b) * a, a);
}

// Cookie Cutter's shapes: how far `p` (from the shape's middle, canonical
// units) is inside, negative outside.

fn cookie_box(half: vec2<f32>, p: vec2<f32>) -> f32 {
    let q = abs(p) - half;
    let outside = length(max(q, vec2<f32>(0.0)));
    return -(outside + min(max(q.x, q.y), 0.0));
}

fn cookie_ellipse(a: f32, b: f32, p: vec2<f32>) -> f32 {
    let x = p.x / max(a, 1e-9);
    let y = p.y / max(b, 1e-9);
    let k = sqrt(x * x + y * y);
    return (1.0 - k) * min(a, b);
}

// An edge of a polygon (from `vi` to `vj`, its previous point): the
// squared distance to it.
fn cookie_edge(p: vec2<f32>, vi: vec2<f32>, vj: vec2<f32>) -> f32 {
    let e = vj - vi;
    let w = p - vi;
    let t = clamp(dot(w, e) / max(dot(e, e), 1e-12), 0.0, 1.0);
    let b = w - e * t;
    return dot(b, b);
}

// Whether the edge from `vi` to `vj` crosses the line through `p` across
// (the even-odd rule's test).
fn cookie_crossed(p: vec2<f32>, vi: vec2<f32>, vj: vec2<f32>) -> bool {
    let e = vj - vi;
    let w = p - vi;
    let c0 = p.y >= vi.y;
    let c1 = p.y < vj.y;
    let c2 = e.x * w.y > e.y * w.x;
    return (c0 && c1 && c2) || (!c0 && !c1 && !c2);
}

fn cookie_triangle(p: vec2<f32>, r: f32) -> f32 {
    let s3 = sqrt(3.0);
    let a = vec2<f32>(0.0, 2.0 * r);
    let b = vec2<f32>(-s3 * r, -r);
    let c = vec2<f32>(s3 * r, -r);
    var d = dot(p - a, p - a);
    d = min(d, cookie_edge(p, a, c));
    d = min(d, cookie_edge(p, b, a));
    d = min(d, cookie_edge(p, c, b));
    var inside = false;
    if cookie_crossed(p, a, c) {
        inside = !inside;
    }
    if cookie_crossed(p, b, a) {
        inside = !inside;
    }
    if cookie_crossed(p, c, b) {
        inside = !inside;
    }
    if inside {
        return sqrt(d);
    }
    return -sqrt(d);
}

fn cookie_arrowhead(p: vec2<f32>, r: f32) -> f32 {
    let s3 = sqrt(3.0);
    let a = vec2<f32>(0.0, 2.0 * r);
    let b = vec2<f32>(-s3 * r, -r);
    let c = vec2<f32>(0.0, -0.25 * r);
    let e = vec2<f32>(s3 * r, -r);
    var d = dot(p - a, p - a);
    d = min(d, cookie_edge(p, a, e));
    d = min(d, cookie_edge(p, b, a));
    d = min(d, cookie_edge(p, c, b));
    d = min(d, cookie_edge(p, e, c));
    var inside = false;
    if cookie_crossed(p, a, e) {
        inside = !inside;
    }
    if cookie_crossed(p, b, a) {
        inside = !inside;
    }
    if cookie_crossed(p, c, b) {
        inside = !inside;
    }
    if cookie_crossed(p, e, c) {
        inside = !inside;
    }
    if inside {
        return sqrt(d);
    }
    return -sqrt(d);
}

// `p` turned `quarters` a quarter clockwise.
fn cookie_turned(p: vec2<f32>, quarters: u32) -> vec2<f32> {
    let q = quarters % 4u;
    if q == 1u {
        return vec2<f32>(-p.y, p.x);
    }
    if q == 2u {
        return vec2<f32>(-p.x, -p.y);
    }
    if q == 3u {
        return vec2<f32>(p.y, -p.x);
    }
    return p;
}

// Shape `j.x` (turned `j.y`): 0 circle, 1 oval, 2 oval on its side, 3
// rectangle, 4 diamond, 5 square, 6 triangle, 7 arrowhead; `size` the
// setting, `frame` the frame's size.
fn cookie_inside(p: vec2<f32>, size: f32, frame: vec2<f32>) -> f32 {
    let w = frame.x;
    let h = frame.y;
    let r = size * length(frame);
    let shape = P.j.x;
    if shape == 1u {
        return cookie_ellipse(size * w * w / h, size * w, p);
    }
    if shape == 2u {
        return cookie_ellipse(size * w, size * w * w / h, p);
    }
    if shape == 3u {
        return cookie_box(vec2<f32>(size * w, size * h), p);
    }
    if shape == 4u {
        let k = r * sqrt(3.14159265358979) / 2.0 * sqrt(2.0);
        return (k - abs(p.x) - abs(p.y)) / sqrt(2.0);
    }
    if shape == 5u {
        let half = r * sqrt(3.14159265358979) / 2.0;
        return cookie_box(vec2<f32>(half, half), p);
    }
    if shape == 6u {
        return cookie_triangle(cookie_turned(p, P.j.y), r);
    }
    if shape == 7u {
        return cookie_arrowhead(cookie_turned(p, P.j.y), r);
    }
    return r - length(p);
}

fn cookie_smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Cookie Cutter: the picture kept in its shape (or cut out of it), the
// border's color along the cut. f: the color (0–2), feather (3), border
// (4), size (5), center (6, 7), the frame's size (8, 9), the render scale
// (10); j: the shape (x), its quarters (y), cut away (z); i: the repeats
// across (x) and up (y).
@compute @workgroup_size(16, 16)
fn cookie_cutter(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let scale = pf(10u);
    let frame = vec2<f32>(pf(8u), pf(9u));
    let at_frame = own_place(id.x, id.y, scale) / frame;
    let n = vec2<f32>(f32(P.i.x), f32(P.i.y));
    let cells = at_frame * n;
    let cell = cells - floor(cells);
    let p = (cell - vec2<f32>(pf(6u), pf(7u))) * frame;
    let d = cookie_inside(p, pf(5u), frame);
    let soft = max(pf(3u) * frame.x / 3.0, (1.0 / scale) * max(n.x, n.y));
    let border = pf(4u) * frame.x / 2.0;
    let shape = cookie_smooth(-soft / 2.0, soft / 2.0, d);
    let core = cookie_smooth(-soft / 2.0, soft / 2.0, d - border);
    let rim = shape - core;
    var picture = core;
    if P.j.z != 0u {
        picture = 1.0 - shape;
    }
    let px = img[i];
    img[i] = vec4<f32>(
        px.x * picture + pf(0u) * rim,
        px.y * picture + pf(1u) * rim,
        px.z * picture + pf(2u) * rim,
        px.w * picture + rim,
    );
}

// White Balance: the straight colors through a matrix (f 0–8, its rows)
// plus an offset (f 9–11), negatives clipped, alpha kept.
@compute @workgroup_size(16, 16)
fn color_matrix(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = px.xyz / a;
    let r = max(pf(0u) * c.x + pf(1u) * c.y + pf(2u) * c.z + pf(9u), 0.0);
    let g = max(pf(3u) * c.x + pf(4u) * c.y + pf(5u) * c.z + pf(10u), 0.0);
    let b = max(pf(6u) * c.x + pf(7u) * c.y + pf(8u) * c.z + pf(11u), 0.0);
    img[i] = vec4<f32>(r * a, g * a, b * a, a);
}

// HSL Adjust's color of a hue's part `t` (HSL's: `p` and `q` its low and
// high).
fn hsl_channel(t0: f32, p: f32, q: f32) -> f32 {
    let t = t0 - floor(t0);
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    return p;
}

// HSL Adjust: each straight color's hue turned by f 0 (a whole turn is 1),
// its saturation scaled by f 1 (no ceiling), its lightness by f 2 (held
// within 0–1), the result unclipped; alpha kept.
@compute @workgroup_size(16, 16)
fn hsl_adjust(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = clamp(px.xyz / a, vec3<f32>(0.0), vec3<f32>(1.0));
    let mx = max(max(c.x, c.y), c.z);
    let mn = min(min(c.x, c.y), c.z);
    var l = (mx + mn) * 0.5;
    let d = mx - mn;
    var s = 0.0;
    var h = 0.0;
    if d > 1e-7 {
        if l <= 0.5 {
            s = d / (mx + mn);
        } else {
            s = d / (2.0 - mx - mn);
        }
        if mx == c.x {
            let k = (c.y - c.z) / d;
            h = k - 6.0 * floor(k / 6.0);
        } else if mx == c.y {
            h = (c.z - c.x) / d + 2.0;
        } else {
            h = (c.x - c.y) / d + 4.0;
        }
        h = h / 6.0;
    }
    h = h + pf(0u);
    h = h - floor(h);
    s = s * pf(1u);
    l = clamp(l * pf(2u), 0.0, 1.0);
    var q = l + s - l * s;
    if l < 0.5 {
        q = l * (1.0 + s);
    }
    let p = 2.0 * l - q;
    let r = hsl_channel(h + 1.0 / 3.0, p, q);
    let g = hsl_channel(h, p, q);
    let b = hsl_channel(h - 1.0 / 3.0, p, q);
    img[i] = vec4<f32>(r * a, g * a, b * a, a);
}

// Color Corrector (Secondary)'s limits: 1 inside lo–hi, falling in a
// straight line to 0 over `soft` (Smooth) beyond (at once without it).
fn secondary_ramp(x: f32, lo: f32, hi: f32, soft: f32) -> f32 {
    let d = max(lo - x, x - hi);
    if d <= 0.0 {
        return 1.0;
    }
    if soft <= 0.0 {
        return 0.0;
    }
    return max(1.0 - d / soft, 0.0);
}

fn secondary_smoothstep(t: f32) -> f32 {
    return t * t * (3.0 - 2.0 * t);
}

// Color Corrector (Secondary), on the straight color (BT.601's Y′UV): the
// mask — luminance's limit (f 0–2: low, high, smooth, 0–1) and
// saturation's (f 3–5) as smoothsteps, the hue's (f 6–8: center, half
// width, smooth, degrees) straight, those j.x turns on (bits 0–2), their
// product squared, inverted by bit 3 — mixes in U and V turned (f 9, 10:
// cosine, sine), moved (f 11, 12) and scaled (f 13), the luma to f 14
// (`cc_gamma`) times f 15 plus f 16; unclipped, the alpha scaled toward f 17. Bit
// 4 shows the mask.
@compute @workgroup_size(16, 16)
fn color_secondary(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = px.xyz / a;
    let y = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
    let u = 0.492111 * (c.z - y);
    let v = 0.877283 * (c.x - y);
    let flags = P.j.x;
    var e = 1.0;
    if (flags & 1u) != 0u {
        e = e * secondary_smoothstep(secondary_ramp(clamp(y, 0.0, 1.0), pf(0u), pf(1u), pf(2u)));
    }
    if (flags & 2u) != 0u {
        let s = min(sqrt(u * u + v * v), 162.0 / 255.0);
        e = e * secondary_smoothstep(secondary_ramp(s, pf(3u), pf(4u), pf(5u)));
    }
    if (flags & 4u) != 0u {
        // A grey's hue is 0° (its rounding's would be anything).
        var h = 0.0;
        if sqrt(u * u + v * v) >= 1e-6 {
            h = degrees(atan2(v, u));
        }
        let w = h - pf(6u) + 180.0;
        let d = abs(w - 360.0 * floor(w / 360.0) - 180.0);
        e = e * secondary_ramp(d, 0.0, pf(7u), pf(8u));
    }
    var m = e * e;
    if (flags & 8u) != 0u {
        m = 1.0 - m;
    }
    if (flags & 16u) != 0u {
        img[i] = vec4<f32>(vec3<f32>(m) * a, a);
        return;
    }
    let u1 = (u * pf(9u) - v * pf(10u) + pf(11u)) * pf(13u);
    let v1 = (u * pf(10u) + v * pf(9u) + pf(12u)) * pf(13u);
    let y1 = pf(15u) * cc_gamma(y, pf(14u)) + pf(16u);
    let yo = y + m * (y1 - y);
    let uo = u + m * (u1 - u);
    let vo = v + m * (v1 - v);
    let r = yo + vo / 0.877283;
    let b = yo + uo / 0.492111;
    let g = (yo - 0.299 * r - 0.114 * b) / 0.587;
    let o = vec3<f32>(r, g, b);
    let alpha = a * (1.0 + m * (pf(17u) - 1.0));
    img[i] = vec4<f32>(o * alpha, alpha);
}

// Color Balance: each straight color moved by f 0–2 (red, green, blue)
// times the weight of its intensity, the channels' mean I: I(2 − I) for
// Highlights (j.x 0), 4I(1 − I) for Midtones (1), 1 − I² for Shadow (2);
// unclipped, alpha kept.
@compute @workgroup_size(16, 16)
fn color_balance(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = px.xyz / a;
    let m = clamp((c.x + c.y + c.z) / 3.0, 0.0, 1.0);
    var w = 4.0 * m * (1.0 - m);
    if P.j.x == 0u {
        w = m * (2.0 - m);
    } else if P.j.x == 2u {
        w = 1.0 - m * m;
    }
    let o = c + vec3<f32>(pf(0u), pf(1u), pf(2u)) * w;
    img[i] = vec4<f32>(o * a, a);
}

// Saturation Adjust: each straight color scaled about its BT.601 luma by
// 1 + the adjustment for its saturation (its distance from that gray, a
// part of pure green's, 0.927204): f 0 (Amount) at f 1 (Center), fading
// smoothly into f 3 (Low) below and f 4 (High) above, reached f 2
// (Spread) ⁄ 2 away; the result unclipped, alpha kept.
@compute @workgroup_size(16, 16)
fn saturation_adjust(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let c = clamp(px.xyz / a, vec3<f32>(0.0), vec3<f32>(1.0));
    let y = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    let d = c - vec3<f32>(y);
    let s = sqrt(dot(d, d)) / 0.927204;
    let center = pf(1u);
    let half = max(pf(2u) * 0.5, 1e-6);
    let t = clamp(1.0 - abs(s - center) / half, 0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    let side = select(pf(4u), pf(3u), s < center);
    let k = 1.0 + pf(0u) * w + side * (1.0 - w);
    let o = vec3<f32>(y) + k * d;
    img[i] = vec4<f32>(o * a, a);
}

// Border's standard normal distribution: Abramowitz and Stegun's 7.1.26
// erf, as the plug-in's.
fn border_phi(z: f32) -> f32 {
    let x = abs(z) / 1.4142135;
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let poly = t * (0.2548296 + t * (-0.28449672 + t * (1.4214138 + t * (-1.4531521 + t * 1.0614054))));
    let erf = 1.0 - poly * exp(-x * x);
    if z < 0.0 {
        return 0.5 * (1.0 - erf);
    }
    return 0.5 * (1.0 + erf);
}

// Blurred's rectangle along one axis: `w` in from both ends of a line
// `len` long, blurred by `s`, at `t`.
fn border_axis(t: f32, len: f32, w: f32, s: f32) -> f32 {
    if s < 1e-9 {
        if t > w && t < len - w {
            return 1.0;
        }
        return 0.0;
    }
    return max(border_phi((t - w) / s) - border_phi((t - (len - w)) / s), 0.0);
}

// How much of a pixel `size` long around `t` is between `w` from both
// ends of a line `len` long.
fn border_cover(t: f32, size: f32, len: f32, w: f32) -> f32 {
    let lo = max(t - size / 2.0, w);
    let hi = min(t + size / 2.0, len - w);
    return clamp((hi - lo) / size, 0.0, 1.0);
}

// Border: Blurred (j.x 0), Beveled (1) or Solid (2). f: the frame (0, 1),
// the border's width (2), Blurred's sigma (3), the facets' depths across
// and up (4, 5), the facets' multipliers (top 6–8, right 9–11, bottom
// 12–14, left 15–17), Solid's straight color and alpha (18–21), the render
// scale (22).
@compute @workgroup_size(16, 16)
fn border(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let scale = pf(22u);
    let p = own_place(id.x, id.y, scale);
    let fw = pf(0u);
    let fh = pf(1u);
    let w = pf(2u);
    let px = img[i];
    if P.j.x == 0u {
        let k = border_axis(p.x, fw, w, pf(3u)) * border_axis(p.y, fh, w, pf(3u));
        img[i] = px * k;
        return;
    }
    if P.j.x == 1u {
        let across = pf(4u);
        let up = pf(5u);
        if across <= 0.0 || up <= 0.0 {
            return;
        }
        // Each side's distance as a part of its facet's depth: the
        // nearest's facet (the first of equals, as the plug-in's).
        var d = (fh - p.y) / up;
        var m = vec3<f32>(pf(6u), pf(7u), pf(8u));
        let right = (fw - p.x) / across;
        if right < d {
            d = right;
            m = vec3<f32>(pf(9u), pf(10u), pf(11u));
        }
        let bottom = p.y / up;
        if bottom < d {
            d = bottom;
            m = vec3<f32>(pf(12u), pf(13u), pf(14u));
        }
        let left = p.x / across;
        if left < d {
            d = left;
            m = vec3<f32>(pf(15u), pf(16u), pf(17u));
        }
        if d >= 1.0 {
            return;
        }
        let a = px.w;
        img[i] = vec4<f32>(min(px.x * m.x, a), min(px.y * m.y, a), min(px.z * m.z, a), a);
        return;
    }
    let pixel = 1.0 / scale;
    let k = border_cover(p.x, pixel, fw, w) * border_cover(p.y, pixel, fh, w);
    let rim = (1.0 - k) * pf(21u);
    img[i] = vec4<f32>(
        px.x * k + pf(18u) * rim,
        px.y * k + pf(19u) * rim,
        px.z * k + pf(20u) * rim,
        px.w * k + rim,
    );
}

// Add Noise's hash: Chris Wellons's lowbias32, as the plug-in's.
fn noise_hash(v: u32) -> u32 {
    var x = v;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return x;
}

// The noise of pixel `x`, `y` (from the frame's lower left), channel `c`,
// for `seed`: even over -0.5 to 0.5, or (`gaussian`) Gaussian by
// Marsaglia's polar method on whole numbers, as the plug-in's.
fn noise_value(seed: u32, x: u32, y: u32, c: u32, gaussian: bool) -> f32 {
    var h = noise_hash(seed ^ (c * 0x9e3779b9u));
    h = noise_hash(h ^ x);
    h = noise_hash(h ^ (y * 0x85ebca6bu));
    if !gaussian {
        return f32(h >> 8u) / 16777216.0 - 0.5;
    }
    for (var k = 0u; k < 8u; k++) {
        let p = noise_hash(h + k);
        let u = i32(p >> 16u) - 32768;
        let v = i32(p & 0xffffu) - 32768;
        let s = u32(u * u) + u32(v * v);
        if s > 0u && s < 1073741824u {
            let sf = f32(s) / 1073741824.0;
            return f32(u) / 32768.0 * sqrt(-2.0 * log(sf) / sf);
        }
    }
    return 0.0;
}

// `v` folded into 0-1: past 1 back down, below 0 back up.
fn noise_fold(v: f32) -> f32 {
    let t = v - 2.0 * floor(v * 0.5);
    if t > 1.0 {
        return 2.0 - t;
    }
    return t;
}

// Add Noise: each straight color, clipped to 0-1, plus the width (f 0)
// times its pixel's noise for the seed (j.z), one for the three channels
// when monochromatic (j.x), Gaussian (j.y) or even; folded back into 0-1.
@compute @workgroup_size(16, 16)
fn add_noise(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let a = px.w;
    if a <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    let x = id.x;
    let y = P.h - 1u - id.y;
    let seed = P.j.z;
    let gaussian = P.j.y != 0u;
    let width = pf(0u);
    let gray = noise_value(seed, x, y, 0u, gaussian);
    var ng = gray;
    var nb = gray;
    if P.j.x == 0u {
        ng = noise_value(seed, x, y, 1u, gaussian);
        nb = noise_value(seed, x, y, 2u, gaussian);
    }
    let c = clamp(vec3<f32>(px.x / a, px.y / a, px.z / a), vec3<f32>(0.0), vec3<f32>(1.0));
    let o = vec3<f32>(
        noise_fold(c.x + width * gray),
        noise_fold(c.y + width * ng),
        noise_fold(c.z + width * nb),
    );
    img[i] = vec4<f32>(o * a, a);
}

// Color Grading's Apple Log (`color_grading::apple_log`) on `img` (`w` ×
// `h`, changed in place): the straight colors, Apple Log, decoded to
// Rec. 709's linear light — or (j.x) its video —, mixed in by f 0; j.y
// holds the colors within 0–1 before and after.
fn apple_log_linear(x: f32) -> f32 {
    let r0 = -0.05641088;
    let c = 47.28711236;
    let pt = c * (0.01 - r0) * (0.01 - r0);
    if x >= pt {
        return exp2((x - 0.69336945) / 0.08550479) - 0.00964052;
    }
    if x >= 0.0 {
        return sqrt(x / c) + r0;
    }
    return r0;
}

@compute @workgroup_size(16, 16)
fn apple_log(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let px = img[o];
    let a = px.w;
    if a <= 0.0 {
        img[o] = vec4<f32>(0.0, 0.0, 0.0, a);
        return;
    }
    var c = px.xyz / a;
    if P.j.y != 0u {
        c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let l = vec3<f32>(apple_log_linear(c.x), apple_log_linear(c.y), apple_log_linear(c.z));
    var v = vec3<f32>(
        1.660491 * l.x - 0.587641 * l.y - 0.072850 * l.z,
        -0.124551 * l.x + 1.132900 * l.y - 0.008349 * l.z,
        -0.018151 * l.x - 0.100579 * l.y + 1.118730 * l.z,
    );
    if P.j.x != 0u {
        let k = clamp(v, vec3<f32>(0.0), vec3<f32>(1.0));
        v = vec3<f32>(bt709_encode(k.x), bt709_encode(k.y), bt709_encode(k.z));
    }
    var res = c + (v - c) * pf(0u);
    if P.j.y != 0u {
        res = clamp(res, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    img[o] = vec4<f32>(res * a, a);
}

// BananaV's table effects that change each pixel where it is
// (`bv-effects`' `filters.rs` and `looks.rs`, the same math).

// Threshold (VEGAS's): each of red, green and blue in 8-bit steps, full
// (the alpha) from the threshold's step j.x up, else none; the alpha
// kept.
fn threshold_channel(v: f32, a: f32) -> f32 {
    if step8(v) >= P.j.x {
        return a;
    }
    return 0.0;
}

@compute @workgroup_size(16, 16)
fn kit_threshold(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    img[i] = vec4<f32>(
        threshold_channel(c.x, c.w),
        threshold_channel(c.y, c.w),
        threshold_channel(c.z, c.w),
        c.w,
    );
}

// Black Restore: a pixel stays when any of its red, green and blue is past
// f0; else they go to black, its alpha kept.
@compute @workgroup_size(16, 16)
fn kit_black_restore(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    let t = pf(0u);
    if !(c.x > t || c.y > t || c.z > t) {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, c.w);
    }
}

// Channel Blend: each output the pixel's values weighted, plus an offset —
// f, a row of five each (red, green, blue, alpha, offset) for red, green,
// blue and alpha —; the alpha within 0-1, the colors within 0 and it.
@compute @workgroup_size(16, 16)
fn kit_channel_blend(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    let r = pf(0u) * c.x + pf(1u) * c.y + pf(2u) * c.z + pf(3u) * c.w + pf(4u);
    let g = pf(5u) * c.x + pf(6u) * c.y + pf(7u) * c.z + pf(8u) * c.w + pf(9u);
    let b = pf(10u) * c.x + pf(11u) * c.y + pf(12u) * c.z + pf(13u) * c.w + pf(14u);
    let a = clamp(pf(15u) * c.x + pf(16u) * c.y + pf(17u) * c.z + pf(18u) * c.w + pf(19u), 0.0, 1.0);
    img[i] = vec4<f32>(clamp(r, 0.0, a), clamp(g, 0.0, a), clamp(b, 0.0, a), a);
}

// Mask Generator: the mask from the pixel's values as they are — the
// average of red, green and blue (j.x 0) or one channel (1-4 red, green,
// blue, alpha) —, turned around with Invert (j.y), from Low-High in (f0,
// f1; f4 the scale) to 0-1, then to Low-High out (f2, f3), in every
// channel.
@compute @workgroup_size(16, 16)
fn kit_mask_generator(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    var v = (c.x + c.y + c.z) * (1.0 / 3.0);
    if P.j.x == 1u {
        v = c.x;
    } else if P.j.x == 2u {
        v = c.y;
    } else if P.j.x == 3u {
        v = c.z;
    } else if P.j.x == 4u {
        v = c.w;
    }
    if P.j.y != 0u {
        v = 1.0 - v;
    }
    var t = 1.0;
    if v <= pf(0u) {
        t = 0.0;
    } else if v < pf(1u) {
        t = (v - pf(0u)) * pf(4u);
    }
    img[i] = vec4<f32>(t * (pf(3u) - pf(2u)) + pf(2u));
}

// Broadcast Colors: the picture in IRE, its luma, chroma and composite held
// within their limits through cubic knees. f: the setup (0), black's code
// value (1), IRE per code value (2), the luma's Min and Max (3, 4), its
// knees' inner and outer ends (5, 6 near Min; 7, 8 near Max), Chroma's
// Max (9) and knee (10, 11), the composite's knees (12, 13 near Min; 14,
// 15 near Max), Composite's Min and Max (16, 17).
fn bc_knee(start: f32, end: f32, limit: f32, x: f32) -> f32 {
    var c3 = 0.0;
    var c2 = 0.0;
    let d = start - end;
    if d != 0.0 {
        let a = start - limit;
        c3 = (a - d * 0.5) / (d * d * d * -0.5);
        c2 = (a - d / 3.0) / (d * d / 3.0);
    }
    let t = x - end;
    return t * t * t * c3 + t * t * c2 + limit;
}

fn bc_ire(code: f32) -> f32 {
    return (code - pf(1u)) * pf(2u) + pf(0u);
}

fn bc_composite(start: f32, end: f32, x: f32) -> f32 {
    if x < start {
        return 1.0;
    }
    if x >= end {
        return 100.0 / x;
    }
    return bc_knee(start, end, 100.0, x) / x;
}

@compute @workgroup_size(16, 16)
fn kit_broadcast_colors(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    let a = min(c.w, 1.0);
    if a <= 1e-6 {
        img[i] = vec4<f32>(0.0);
        return;
    }
    let k = pf(2u);
    let rgb = c.xyz * 255.0;
    // The luma in IRE, and whether it changed.
    let y = bc_ire(rgb.x) * 0.299 + bc_ire(rgb.y) * 0.587 + bc_ire(rgb.z) * 0.114;
    var yl = y;
    var changed = true;
    if y < pf(6u) {
        yl = pf(3u);
    } else if y < pf(5u) {
        yl = bc_knee(pf(5u), pf(6u), pf(3u), y);
    } else if y < pf(7u) {
        changed = false;
    } else if y < pf(8u) {
        yl = bc_knee(pf(7u), pf(8u), pf(4u), y);
    } else {
        yl = pf(4u);
    }
    var s = rgb;
    if a < 1.0 {
        s = rgb / a;
    }
    var u = -0.147 * s.x - 0.289 * s.y + 0.436 * s.z;
    var v = 0.615 * s.x - 0.515 * s.y - 0.100 * s.z;
    if abs(u) < 1e-6 {
        u = 0.0;
    }
    if abs(v) < 1e-6 {
        v = 0.0;
    }
    let rough = (abs(u) + abs(v)) * k;
    let up = pf(17u) - yl;
    let down = yl - pf(16u);
    let kept = vec4<f32>(c.xyz, a);
    let c_start = pf(10u);
    if !changed && up + 1e-6 > rough && down + 1e-6 > rough && c_start * 0.5 + 1e-6 > rough {
        img[i] = kept;
        return;
    }
    let near_low = down < up;
    var room = up;
    var start = pf(14u);
    if near_low {
        room = down;
        start = pf(12u);
    }
    let ratio = start / 100.0;
    var sq = (u * u + v * v) * k * k;
    var moved = changed;
    if sq > (c_start * 0.5) * (c_start * 0.5) {
        let pp = 2.0 * sqrt(sq);
        var f = 1.0;
        if pp >= pf(11u) {
            f = pf(9u) / pp;
        } else if pp >= c_start {
            f = bc_knee(c_start, pf(11u), pf(9u), pp) / pp;
        }
        moved = true;
        u = u * f;
        v = v * f;
        sq = sq * f * f;
    }
    if sq <= room * room * ratio * ratio {
        if !moved {
            img[i] = kept;
            return;
        }
    } else {
        var r = room / sqrt(sq);
        if r > 1.0 / 1.2 {
            if near_low {
                r = bc_composite(pf(12u), pf(13u), 100.0 / r);
            } else {
                r = bc_composite(pf(14u), pf(15u), 100.0 / r);
            }
        }
        u = u * r;
        v = v * r;
    }
    let yn = ((yl - pf(0u)) / k + pf(1u)) / 255.0;
    u = u / 255.0;
    v = v / 255.0;
    img[i] = vec4<f32>(
        (v * 1.14 + yn) * a,
        (yn - u * 0.394 - v * 0.581) * a,
        (u * 2.032 + yn) * a,
        a,
    );
}

// LAB Adjust: in CIE L*a*b* (D65, the values through sRGB's curve), the
// lightness turned around (j.x) and scaled by f0, a and b moved by f1 and
// f2, then scaled by f3; no alpha comes out empty, alpha past 1 as 1.
fn lab_light(v: f32) -> f32 {
    if v <= 0.04045 {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn lab_value(v: f32) -> f32 {
    if v <= 0.0031308 {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

fn lab_f(t: f32) -> f32 {
    if t > 216.0 / 24389.0 {
        return pow(t, 1.0 / 3.0);
    }
    return (24389.0 / 27.0 * t + 16.0) / 116.0;
}

fn lab_g(t: f32) -> f32 {
    if t > 6.0 / 29.0 {
        return t * t * t;
    }
    return (116.0 * t - 16.0) * 27.0 / 24389.0;
}

@compute @workgroup_size(16, 16)
fn kit_lab_adjust(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    if c.w <= 1e-6 {
        img[i] = vec4<f32>(0.0);
        return;
    }
    let alpha = min(c.w, 1.0);
    let r = lab_light(c.x / alpha);
    let g = lab_light(c.y / alpha);
    let b = lab_light(c.z / alpha);
    let fx = lab_f((0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047);
    let fy = lab_f(0.2126729 * r + 0.7151522 * g + 0.072175 * b);
    let fz = lab_f((0.0193339 * r + 0.119192 * g + 0.9503041 * b) / 1.08883);
    var l = 116.0 * fy - 16.0;
    if P.j.x != 0u {
        l = 100.0 - l;
    }
    l = l * pf(0u);
    let la = (500.0 * (fx - fy) + pf(1u)) * pf(3u);
    let lb = (200.0 * (fy - fz) + pf(2u)) * pf(3u);
    let gy = (l + 16.0) / 116.0;
    let x = lab_g(gy + la / 500.0) * 0.95047;
    let yy = lab_g(gy);
    let z = lab_g(gy - lb / 200.0) * 1.08883;
    let out = vec3<f32>(
        lab_value(3.2404542 * x - 1.5371385 * yy - 0.4985314 * z),
        lab_value(-0.969266 * x + 1.8760108 * yy + 0.041556 * z),
        lab_value(0.0556434 * x - 0.2040259 * yy + 1.0572252 * z),
    );
    img[i] = vec4<f32>(out * alpha, alpha);
}

// Color Match (`bv-effects`' `color_match`, VEGAS's): in CIE L*a*b* (LAB
// Adjust's), a and b — L too with j.x — moved from the target image's
// means (f0–2) by the ratios of the spreads (f3–5) to the source image's
// means (f6–8); f9 of the pixel stays. Transparent pixels go, opaque ones
// stay opaque, the others are matched straight and premultiplied again.
@compute @workgroup_size(16, 16)
fn kit_color_match(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    if c.w <= 1e-6 {
        img[i] = vec4<f32>(0.0);
        return;
    }
    var s = c.xyz;
    var alpha = 1.0;
    if c.w < 1.0 {
        s = c.xyz / c.w;
        alpha = c.w;
    }
    let r = lab_light(s.x);
    let g = lab_light(s.y);
    let b = lab_light(s.z);
    let fx = lab_f((0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047);
    let fy = lab_f(0.2126729 * r + 0.7151522 * g + 0.072175 * b);
    let fz = lab_f((0.0193339 * r + 0.119192 * g + 0.9503041 * b) / 1.08883);
    var l = 116.0 * fy - 16.0;
    if P.j.x != 0u {
        l = pf(3u) * (l - pf(0u)) + pf(6u);
    }
    let la = pf(4u) * (500.0 * (fx - fy) - pf(1u)) + pf(7u);
    let lb = pf(5u) * (200.0 * (fy - fz) - pf(2u)) + pf(8u);
    let gy = (l + 16.0) / 116.0;
    let x = lab_g(gy + la / 500.0) * 0.95047;
    let yy = lab_g(gy);
    let z = lab_g(gy - lb / 200.0) * 1.08883;
    var n = vec3<f32>(
        lab_value(3.2404542 * x - 1.5371385 * yy - 0.4985314 * z),
        lab_value(-0.969266 * x + 1.8760108 * yy + 0.041556 * z),
        lab_value(0.0556434 * x - 0.2040259 * yy + 1.0572252 * z),
    );
    if c.w < 1.0 {
        n = n * c.w;
    }
    img[i] = vec4<f32>((c.xyz - n) * pf(9u) + n, alpha);
}

// Soft Contrast (`bv-effects`' `SoftContrast`): each straight color's luma
// through the curve — Contrast f0, Stretch range f1, the trims f2 and f3 —
// plus the tint's cast (f5–7) by f4 × 0.3 of the luma.
@compute @workgroup_size(16, 16)
fn kit_soft_tone(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    var c = vec3<f32>(0.0);
    if px.w > 0.0 {
        c = px.xyz / px.w;
    }
    let l = 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
    let t = clamp(l, 0.0, 1.0);
    let curve = l + pf(0u) * 0.5 * (t * t * (3.0 - 2.0 * t) - t)
        + pf(1u) * 0.4 * t * t * t
        + pf(2u) * 1.5 * t * (1.0 - t) * (1.0 - t)
        - pf(3u) * 1.5 * t * t * (1.0 - t);
    let k = pf(4u) * 0.3 * t;
    let o = c + vec3<f32>(curve - l) + vec3<f32>(pf(5u), pf(6u), pf(7u)) * k;
    img[i] = vec4<f32>(o * px.w, px.w);
}

// Vignette (`bv-effects`' `vignette`): the color's share at each pixel —
// its corner's distance from Center (f 0, 1: 0–1 of the frame, rows up)
// in pixels times their scale (12, 13), turned (sine and cosine 2, 3),
// widened across (4), from Inner radius (6) of Outer radius (5, of the
// half diagonal) out, to the power f 7, turned around with Invert (j.x),
// times the color's alpha (11) — over the picture's colors (the color
// 8-10, a 32nd of an 8-bit step under), the alpha kept, every channel
// held within 0–1.
@compute @workgroup_size(16, 16)
fn kit_vignette(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let w = f32(P.w);
    let h = f32(P.h);
    let sx = pf(12u);
    let sy = pf(13u);
    let dx = (f32(id.x) - w * pf(0u)) * sx;
    let dy = (f32(P.h - 1u - id.y) - h * pf(1u)) * sy;
    let q = (dx * pf(2u) + dy * pf(3u)) * pf(4u);
    let side = (dx * pf(3u) - dy * pf(2u)) / pf(4u);
    let r = sqrt(q * q + side * side);
    let outer = sqrt(w * sx * w * sx + h * sy * h * sy) * pf(5u) * 0.5;
    let inner = outer * pf(6u);
    var band = outer - inner;
    if band < 1e-6 {
        band = 1.0;
    }
    var m = 0.0;
    if r > inner {
        let t = clamp((r - inner) / band, 0.0, 1.0);
        if t > 0.0 {
            m = min(pow(t, pf(7u)), 1.0);
        }
    }
    if P.j.x != 0u {
        m = 1.0 - m;
    }
    let k = m * pf(11u);
    let p = img[i];
    var c = p.xyz;
    if k > 0.0 {
        let tie = 1.0 / (32.0 * 255.0);
        c = p.xyz * (1.0 - k) + vec3<f32>(pf(8u), pf(9u), pf(10u)) * k - vec3<f32>(tie);
    }
    img[i] = clamp(vec4<f32>(c, p.w), vec4<f32>(0.0), vec4<f32>(1.0));
}

// BananaV's own generators (`bv-effects`' `generators.rs`, the same math),
// each pixel from its place: the frame's size in the picture's pixels (f
// 0, 1) gives the frame's u, v (v from the top) of the pixel's middle.
fn gen_uv(id: vec3<u32>) -> vec2<f32> {
    return vec2<f32>(
        (f32(id.x) + 0.5) / pf(0u),
        1.0 - (f32(P.h - 1u - id.y) + 0.5) / pf(1u),
    );
}

// How far into the other color a tile's edge reaches: 1 at a blend's far
// end, 0.5 on the edge, 0 inside.
fn checker_reach(t: f32, size: f32, b: f32) -> f32 {
    let from_low = t - floor(t / size) * size;
    let near = min(from_low, size - from_low);
    return clamp((b - near) * (0.5 / b), 0.0, 0.5);
}

// Checkerboard: the tiles' width and height (f 2, 3), the blends' half
// widths (4, 5), the grid's move (6, 7), the two colors (8-11, 12-15).
@compute @workgroup_size(16, 16)
fn kit_checkerboard(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let uv = gen_uv(id);
    let x = uv.x * pf(0u) - pf(6u);
    let y = uv.y * pf(1u) - pf(7u);
    let n = floor(x / pf(2u)) + floor(y / pf(3u));
    let c1 = vec4<f32>(pf(8u), pf(9u), pf(10u), pf(11u));
    let c2 = vec4<f32>(pf(12u), pf(13u), pf(14u), pf(15u));
    var own = c1;
    var other = c2;
    if n - floor(n / 2.0) * 2.0 >= 1.0 {
        own = c2;
        other = c1;
    }
    let kx = checker_reach(x, pf(2u), pf(4u));
    let ky = checker_reach(y, pf(3u), pf(5u));
    let row_own = own + (other - own) * ky;
    let row_other = other + (own - other) * ky;
    img[i] = row_own + (row_other - row_own) * kx;
}

fn tp_opaque(r: f32, g: f32, b: f32) -> vec4<f32> {
    return vec4<f32>(r, g, b, 1.0);
}

// Bar `n` of the seven (or EBU's eight): white at `white`, the colors at
// 75 %, then black.
fn tp_bar(n: u32, white: f32) -> vec4<f32> {
    let k = 0.75;
    if n == 0u {
        return tp_opaque(white, white, white);
    }
    if n == 1u {
        return tp_opaque(k, k, 0.0);
    }
    if n == 2u {
        return tp_opaque(0.0, k, k);
    }
    if n == 3u {
        return tp_opaque(0.0, k, 0.0);
    }
    if n == 4u {
        return tp_opaque(k, 0.0, k);
    }
    if n == 5u {
        return tp_opaque(k, 0.0, 0.0);
    }
    if n == 6u {
        return tp_opaque(0.0, 0.0, k);
    }
    return tp_opaque(0.0, 0.0, 0.0);
}

fn tp_color_bars(u: f32, white: f32) -> vec4<f32> {
    var n = 7.0;
    if white >= 1.0 {
        n = 8.0;
    }
    return tp_bar(min(u32(max(u * n, 0.0)), u32(n) - 1u), white);
}

// SMPTE's bottom row: -I, white (or the porch, `porch_on`), +Q, black and
// the PLUGE.
fn tp_pluge(u: f32, porch_on: bool, porch: vec4<f32>) -> vec4<f32> {
    let black = tp_opaque(0.0, 0.0, 0.0);
    var minus_i = tp_opaque(0.0, 0.129, 0.290);
    var plus_q = tp_opaque(0.196, 0.0, 0.416);
    var second = tp_opaque(1.0, 1.0, 1.0);
    if porch_on {
        minus_i = black;
        plus_q = black;
        second = porch;
    }
    let at = u * 7.0;
    if at < 1.25 {
        return minus_i;
    }
    if at < 2.5 {
        return second;
    }
    if at < 3.75 {
        return plus_q;
    }
    if at < 5.0 {
        return black;
    }
    if at < 5.0 + 1.0 / 3.0 {
        return black;
    }
    if at < 5.0 + 2.0 / 3.0 {
        return tp_opaque(0.04, 0.04, 0.04);
    }
    return black;
}

fn tp_smpte(u: f32, v: f32, white: f32) -> vec4<f32> {
    let bar = min(u32(max(u * 7.0, 0.0)), 6u);
    if v < 0.67 {
        return tp_bar(bar, white);
    }
    if v < 0.75 {
        if bar == 0u {
            return tp_bar(6u, white);
        }
        if bar == 2u {
            return tp_bar(4u, white);
        }
        if bar == 4u {
            return tp_bar(2u, white);
        }
        if bar == 6u {
            return tp_bar(0u, white);
        }
        return tp_opaque(0.0, 0.0, 0.0);
    }
    return tp_pluge(u, false, vec4<f32>(0.0));
}

// Test Pattern: pattern j.x (the list's order).
@compute @workgroup_size(16, 16)
fn kit_test_pattern(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let uv = gen_uv(id);
    let u = uv.x;
    let v = uv.y;
    let pattern = P.j.x;
    var c = tp_color_bars(u, 0.75);
    if pattern == 1u {
        c = tp_color_bars(u, 1.0);
    } else if pattern == 2u {
        c = tp_opaque(1.0 - u, u, v);
    } else if pattern == 3u {
        c = tp_pluge(u, false, vec4<f32>(0.0));
    } else if pattern == 4u {
        var t = 0.0;
        if u >= 0.1 && u <= 0.9 && v >= 0.1 && v <= 0.9 {
            t = u;
        }
        c = tp_opaque(t, t, t);
    } else if pattern == 5u {
        c = tp_smpte(u, v, 0.75);
    } else if pattern == 6u {
        c = tp_smpte(u, v, 1.0);
    } else if pattern == 7u {
        c = tp_pluge(u, true, tp_opaque(1.0, 1.0, 1.0));
    } else if pattern == 8u {
        c = tp_pluge(u, true, tp_opaque(0.75, 0.75, 0.0));
    }
    img[i] = c;
}

// Noise Texture (`bv-effects`' `generators::Noise`, VEGAS's layers, styles
// and colors' line): f: the frame's size (0, 1), Frequency, Offset and
// Progress (2-6), the layers' weights (7-11), the line's scale and offset
// (12, 13), Amplitude (14), the two colors premultiplied (15-18, 19-22);
// j.x the style, j.y how many layers.
fn nt_lattice(x: i32, y: i32, z: i32) -> u32 {
    var h = (u32(x) * 0x8da6b343u) ^ (u32(y) * 0xd8163841u) ^ (u32(z) * 0xcb1ab31fu);
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    return (h ^ (h >> 16u)) & 15u;
}

fn nt_gradient(h: u32, x: f32, y: f32, z: f32) -> f32 {
    var u = y;
    if h < 8u {
        u = x;
    }
    var v = z;
    if h < 4u {
        v = y;
    } else if h == 12u || h == 14u {
        v = x;
    }
    if (h & 1u) != 0u {
        u = -u;
    }
    if (h & 2u) != 0u {
        v = -v;
    }
    return u + v;
}

fn nt_corner(xi: i32, yi: i32, zi: i32, i: i32, j: i32, k: i32, f: vec3<f32>) -> f32 {
    let h = nt_lattice(xi + i, yi + j, zi + k);
    return nt_gradient(h, f.x - f32(i), f.y - f32(j), f.z - f32(k));
}

fn nt_noise(p: vec3<f32>) -> f32 {
    let c = floor(p);
    let f = p - c;
    let xi = i32(c.x);
    let yi = i32(c.y);
    let zi = i32(c.z);
    let s = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = nt_corner(xi, yi, zi, 0, 0, 0, f);
    let b = nt_corner(xi, yi, zi, 1, 0, 0, f);
    let cc = nt_corner(xi, yi, zi, 0, 1, 0, f);
    let d = nt_corner(xi, yi, zi, 1, 1, 0, f);
    let e = nt_corner(xi, yi, zi, 0, 0, 1, f);
    let g = nt_corner(xi, yi, zi, 1, 0, 1, f);
    let h = nt_corner(xi, yi, zi, 0, 1, 1, f);
    let k = nt_corner(xi, yi, zi, 1, 1, 1, f);
    let ab = a + (b - a) * s.x;
    let cd = cc + (d - cc) * s.x;
    let eg = e + (g - e) * s.x;
    let hk = h + (k - h) * s.x;
    let near = ab + (cd - ab) * s.y;
    let far = eg + (hk - eg) * s.y;
    return (near + (far - near) * s.z) * 0.70710677;
}

fn nt_weight(k: u32) -> f32 {
    return pf(7u + k);
}

@compute @workgroup_size(16, 16)
fn kit_noise_texture(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let uv = gen_uv(id);
    let fx = pf(2u);
    let fy = pf(3u);
    let x = uv.x * fx + pf(4u);
    let y = uv.y * fy + pf(5u);
    let z = pf(6u);
    let style = P.j.x;
    let turbulent = style == 1u;
    var sum = 0.0;
    var total = 0.0;
    for (var k = 0u; k < P.j.y; k++) {
        let f = f32(1u << k);
        let w = nt_weight(k);
        var n = nt_noise(vec3<f32>(x * f, y * f, z * f)) * w * 1.5151515;
        if turbulent {
            n = abs(n);
        }
        sum += n;
        total += w;
    }
    let n = sum / total;
    var value = n * 0.5 + 0.5;
    if turbulent {
        value = n;
    } else if style >= 2u && style <= 5u {
        // The veins' numbers by style (VEGAS's table).
        var p = array<f32, 5>(0.9, 0.15, 0.4, 1.2, 0.1);
        if style == 3u {
            p = array<f32, 5>(0.05, 0.9, 1.3, 0.0, 0.0);
        } else if style == 4u {
            p = array<f32, 5>(0.6, 0.0, 0.2, 0.5, 0.1);
        } else if style == 5u {
            p = array<f32, 5>(1.0, 0.0, 1.5, 0.5, 0.05);
        }
        var s = p[2] * n + p[0] * fx * uv.x - p[1] * fy * uv.y;
        if p[3] > 0.0 {
            var t = p[3] * fy * uv.y;
            t = t - floor(t);
            s += p[4] * 4.0 * t * (1.0 - t);
        }
        let t = s - floor(s);
        value = 1.0 - (2.0 * t - 1.0) * (2.0 * t - 1.0);
    } else if style == 6u {
        let s = (n + 2.0) * 4.0 * pf(14u);
        let t = s - floor(s);
        if t >= 0.5 {
            value = (4.0 * t - 3.0) * (4.0 * t - 3.0);
        } else {
            value = 2.0 - (4.0 * t - 1.0) * (4.0 * t - 1.0);
        }
    }
    let t = clamp(round((value * pf(12u) + pf(13u)) * 65536.0 + 0.5 + 1.5e-8), 0.0, 65536.0) * 1.5258789e-05;
    let a = vec4<f32>(pf(15u), pf(16u), pf(17u), pf(18u));
    let b = vec4<f32>(pf(19u), pf(20u), pf(21u), pf(22u));
    img[i] = b * t + a * (1.0 - t);
}

// Flicker Control's Addition (`bv-effects`' `flicker_control::add_pixel`):
// the light `f[0]` added as much as Type (`j.x`) gives each pixel's
// largest channel, times its alpha (at most 1), each channel held within
// 0–1; next to nothing see-through, nothing. `j.y`: Show mask only.
@compute @workgroup_size(16, 16)
fn kit_flicker_add(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let c = img[i];
    var a = 1.0;
    if c.w <= 1.0 {
        a = c.w;
    }
    var o = vec4<f32>(0.0);
    if a >= 1.1920929e-7 {
        let d = flicker_tone(P.j.x, flicker_largest(c)) * pf(0u) * a;
        o = vec4<f32>(clamp(c.xyz + vec3<f32>(d), vec3<f32>(0.0), vec3<f32>(1.0)), a);
    }
    if P.j.y != 0u {
        o = flicker_mask(c, o);
    }
    img[i] = o;
}
