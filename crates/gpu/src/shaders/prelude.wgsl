// Shared by every kernel: the parameters and the helpers. Images are
// arrays of RGBA pixels in linear light, premultiplied, row after row —
// the processor's `ImageRgbaF32` — and every kernel does the same math as
// its counterpart in `bv-render`.

struct Params {
    // The size written.
    w: u32,
    h: u32,
    // The size read.
    sw: u32,
    sh: u32,
    i: vec4<i32>,
    j: vec4<u32>,
    // Offsets (bytes).
    o: vec4<u32>,
    f: array<vec4<f32>, 6>,
}

@group(0) @binding(0) var<uniform> P: Params;

fn pf(k: u32) -> f32 {
    return P.f[k / 4u][k % 4u];
}

// Rust's `f32::round`: halves away from zero.
fn rround(x: f32) -> f32 {
    if x >= 0.0 {
        return floor(x + 0.5);
    }
    return ceil(x - 0.5);
}

// Flicker Control (`bv-effects`' `flicker_control`): how much of the
// change a color whose largest channel is `m` gets for Type `t` — the
// highlights', midtones' or shadows' share, or all of it.
fn flicker_tone(t: u32, m: f32) -> f32 {
    switch t {
        case 0u: {
            return 1.0 - (1.0 - m) * (1.0 - m);
        }
        case 1u: {
            return 1.0 - (m - 0.5) * (m - 0.5) * 4.0;
        }
        case 2u: {
            return 1.0 - m * m;
        }
        default: {
            return 1.0;
        }
    }
}

fn flicker_largest(c: vec4<f32>) -> f32 {
    return max(max(c.y, c.z), c.x);
}

// Flicker Control's Show mask only: how much `c` became `o` — its
// channels' largest change, a fifth of the range or more white — with
// `c`'s alpha.
fn flicker_mask(c: vec4<f32>, o: vec4<f32>) -> vec4<f32> {
    let d = abs(c.xyz - o.xyz);
    let v = clamp(max(max(d.x, d.y), d.z) * (1.0 / 0.2), 0.0, 1.0);
    return vec4<f32>(v, v, v, c.w);
}

// Rust's `f32::signum` (1 for +0).
fn signum(x: f32) -> f32 {
    if x < 0.0 {
        return -1.0;
    }
    return 1.0;
}

fn luma(c: vec3<f32>) -> f32 {
    return 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
}

fn to_perceptual(l: f32) -> f32 {
    if l <= 0.0 {
        return 0.0;
    }
    return pow(l, 1.0 / 2.2);
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    return pow(v, 2.2);
}

fn perceptual3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(to_perceptual(c.x), to_perceptual(c.y), to_perceptual(c.z));
}

fn linear3(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(to_linear(v.x), to_linear(v.y), to_linear(v.z));
}

fn over(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    let k = 1.0 - src.w;
    return vec4<f32>(src.x + dst.x * k, src.y + dst.y * k, src.z + dst.z * k, src.w + dst.w * k);
}

fn lerp4(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    return vec4<f32>(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
        a.w + (b.w - a.w) * t,
    );
}

// BT.709's OETF and its inverse (video gamma).
fn bt709_encode(l: f32) -> f32 {
    if l < 0.018 {
        return 4.5 * l;
    }
    return 1.099 * pow(l, 0.45) - 0.099;
}

fn bt709_decode(v: f32) -> f32 {
    if v < 0.081 {
        return v / 4.5;
    }
    return pow((v + 0.099) / 1.099, 1.0 / 0.45);
}

// `bv-effects`' `step8`: a value's 8-bit step, rounded, a half step going
// up — nudged a ten-thousandth of a step, so that the card's floats and
// the processor's, a last bit or two apart, round alike.
fn step8(v: f32) -> u32 {
    return u32(clamp(floor(v * 255.0 + 0.5001), 0.0, 255.0));
}

// Bump Map's light (`bv-effects`' `bump_map_with`, the filter's and the
// compositor's) at pixel x, y — rows up — of a picture `w` × `h` whose
// surface leans by the bump channel's 3 × 3 differences `gx`, `gy`: what
// the pixel's color is multiplied by. f: the light's Source (0-2), its
// axis (3-5), the surface's rise per step over the longer side (6), the
// spotlight's cone and spread (7, 8), Intensity, Shininess and Ambience
// (9-11); j.x the light's type (0 directional, 1 spot, 2 omni).
fn bump_light(gx: f32, gy: f32, x: i32, y: i32) -> f32 {
    let w = f32(P.w);
    let h = f32(P.h);
    let longer = max(w, h);
    let lift = pf(6u) * longer;
    let n = normalize(vec3<f32>(gx * lift, gy * lift, 1.0));
    let axis = vec3<f32>(pf(3u), pf(4u), pf(5u));
    var l = axis;
    var reach = 1.0;
    if P.j.x != 0u {
        let step = 2.0 / longer;
        let px = (f32(x) - w * 0.5) * step;
        let py = (f32(y) + 1.0 - h * 0.5) * step;
        let v = vec3<f32>(pf(0u) - px, pf(1u) - py, pf(2u));
        let far = length(v);
        var near = 100.0;
        l = vec3<f32>(0.0, 0.0, 1.0);
        if far > 0.0 {
            l = v / far;
            near = 1.0 / far;
        }
        var beam = 1.0;
        if P.j.x == 1u {
            let c = clamp((dot(axis, l) - pf(7u)) / pf(8u), 0.0, 1.0);
            beam = c * 2.0 * c;
        }
        reach = beam * 2.0 * near;
    }
    let ndl = dot(n, l);
    let shininess = pf(10u);
    let spec = pow(max(2.0 * n.z * ndl - l.z, 0.01), shininess * 8.0 + 4.0);
    return max(
        (spec * shininess * 0.5 + ndl * (0.6 - shininess * 0.2)) * pf(9u) * 8.0 * reach + 2.0 * pf(11u),
        0.0,
    );
}

// The Mask group of Glint and Rays (`bv-effects`' `effect_mask::mask_at`,
// the same floats): at u, v (0–1, rows from the top) of a frame `aspect`
// times as wide as high, the numbers from f[b]: the shape (0 rectangle, 1
// bar, 2 wipe, 3 ellipse), Progress, Softness, Width, Height, Corner
// radius, the middle (2), Falloff, Angle (radians), Strength, Leading and
// Trailing regions.
fn vm_mask(u: f32, v: f32, aspect: f32, b: u32) -> f32 {
    let kind = pf(b);
    let progress = pf(b + 1u);
    let soft = pf(b + 2u);
    let w = pf(b + 3u);
    let h = pf(b + 4u);
    let corner = pf(b + 5u);
    let px = (u - pf(b + 6u)) * aspect;
    let py = v - pf(b + 7u);
    let falloff = pf(b + 8u);
    let c = cos(pf(b + 9u));
    let s = sin(pf(b + 9u));
    let strength = pf(b + 10u);
    let rx = c * px + s * py;
    let ry = c * py - s * px;
    let grow = progress * 2.0;
    var d = 0.0;
    if kind < 0.5 {
        let hx = w * 0.5 * aspect * grow;
        let hy = h * 0.5 * grow;
        let r = corner * min(hx, hy);
        let qx = abs(rx) - (hx - r);
        let qy = abs(ry) - (hy - r);
        d = length(vec2<f32>(max(qx, 0.0), max(qy, 0.0))) + min(max(qx, qy), 0.0) - r;
    } else if kind < 1.5 {
        d = abs(ry) - h * 0.5 * grow;
    } else if kind < 2.5 {
        let reach = (abs(c) * aspect + abs(s)) * 0.5;
        d = rx - (progress * 2.0 - 1.0) * reach;
    } else {
        let hx = max(w * 0.5 * aspect * grow, 1e-6);
        let hy = max(h * 0.5 * grow, 1e-6);
        d = (length(vec2<f32>(rx / hx, ry / hy)) - 1.0) * min(hx, hy);
    }
    let ahead = soft * 0.25 + pf(b + 11u) * 0.5;
    let behind = soft * 0.25 + pf(b + 12u) * 0.5;
    var m = 0.0;
    if ahead + behind > 1e-6 {
        m = clamp((ahead - d) / (ahead + behind), 0.0, 1.0);
    } else if d <= 0.0 {
        m = 1.0;
    }
    if falloff > 0.0 {
        m = m * clamp(-d / (falloff * 0.5), 0.0, 1.0);
    }
    return 1.0 - strength * (1.0 - m);
}

// A hue's color (degrees), full and bright: `bv-effects`' `hue_color`.
fn hue_rgb(deg: f32) -> vec3<f32> {
    let hh = (deg - 360.0 * floor(deg / 360.0)) / 60.0;
    let m2 = hh - 2.0 * floor(hh / 2.0);
    let x = 1.0 - abs(m2 - 1.0);
    if hh < 1.0 {
        return vec3<f32>(1.0, x, 0.0);
    }
    if hh < 2.0 {
        return vec3<f32>(x, 1.0, 0.0);
    }
    if hh < 3.0 {
        return vec3<f32>(0.0, 1.0, x);
    }
    if hh < 4.0 {
        return vec3<f32>(0.0, x, 1.0);
    }
    if hh < 5.0 {
        return vec3<f32>(x, 0.0, 1.0);
    }
    return vec3<f32>(1.0, 0.0, x);
}
