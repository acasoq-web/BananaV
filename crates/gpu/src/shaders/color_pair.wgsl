// ACES 1.3's reference rendering (RRT) and an SDR output transform (ODT)
// on `dst`'s straight colors (`w` × `h`, changed in place): ACES2065-1 →
// CIE XYZ display-linear, as `bv_ocio::aces::SdrOutput::apply` (ported
// from the Academy's CTL; its license in `crates/ocio/licenses/ACES.md`).
// `src` holds `SdrOutput::gpu_constants`: the matrices' rows (AP0 → AP1
// from 0, AP1 → AP0 from 3, the RRT's desaturation from 6, the ODT's from
// 9, AP1 → XYZ from 12, XYZ → AP1 from 15, the output's AP1 → XYZ from
// 18), at 21 black, white, the dim surround's gamma and whether it
// applies, the RRT's tone scale from 22 and the ODT's from 36.

const ACES_HALF_MIN: f32 = 5.9604645e-8;
const ACES_HALF_MAX: f32 = 65504.0;
const LOG10_2: f32 = 0.30102999566;
const LOG2_10: f32 = 3.32192809489;

fn aces_rows(m: u32, c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(src[m].xyz, c), dot(src[m + 1u].xyz, c), dot(src[m + 2u].xyz, c));
}

// A segment of a spline (`Spline::segment`): coefficients from `base`.
fn aces_segment(base: u32, coord: f32) -> f32 {
    let j = u32(coord);
    let t = coord - f32(j);
    let c0 = src[base + j].x;
    let c1 = src[base + j + 1u].x;
    let c2 = src[base + j + 2u].x;
    let a = 0.5 * c0 - c1 + 0.5 * c2;
    let b = c1 - c0;
    let c = 0.5 * (c0 + c1);
    return (a * t + b) * t + c;
}

// `segmented_spline_c5_fwd` / `_c9_fwd`, its constants from `s`: the logs
// of its points, its slopes and knots, then the low and high coefficients.
fn aces_spline(x: f32, s: u32) -> f32 {
    let p = src[s];
    let q = src[s + 1u];
    let knots = q.w;
    let low = s + 2u;
    let high = low + u32(knots) + 2u;
    let logx = log2(max(x, ACES_HALF_MIN)) * LOG10_2;
    var logy = 0.0;
    if logx <= p.x {
        logy = logx * q.y + (p.w - q.y * p.x);
    } else if logx < p.y {
        logy = aces_segment(low, (knots - 1.0) * (logx - p.x) / (p.y - p.x));
    } else if logx < p.z {
        logy = aces_segment(high, (knots - 1.0) * (logx - p.y) / (p.z - p.y));
    } else {
        logy = logx * q.z + (q.x - q.z * p.z);
    }
    return exp2(logy * LOG2_10);
}

fn aces_spline3(c: vec3<f32>, s: u32) -> vec3<f32> {
    return vec3<f32>(aces_spline(c.x, s), aces_spline(c.y, s), aces_spline(c.z, s));
}

fn aces_saturation(c: vec3<f32>) -> f32 {
    let mx = max(c.x, max(c.y, c.z));
    let mn = min(c.x, min(c.y, c.z));
    return (max(mx, 1e-10) - max(mn, 1e-10)) / max(mx, 1e-2);
}

fn aces_yc(c: vec3<f32>) -> f32 {
    let chroma = sqrt(max(c.z * (c.z - c.y) + c.y * (c.y - c.x) + c.x * (c.x - c.z), 0.0));
    return (c.z + c.y + c.x + 1.75 * chroma) / 3.0;
}

fn aces_sigmoid(x: f32) -> f32 {
    let t = max(1.0 - abs(x / 2.0), 0.0);
    return (1.0 + signum(x) * (1.0 - t * t)) / 2.0;
}

fn aces_glow(yc: f32, gain: f32, mid: f32) -> f32 {
    if yc <= 2.0 / 3.0 * mid {
        return gain;
    }
    if yc >= 2.0 * mid {
        return 0.0;
    }
    return gain * (mid / yc - 0.5);
}

// `cubic_basis_shaper` (the red modifier's weight), `w` its width.
fn aces_basis(x: f32, w: f32) -> f32 {
    if !(x > -w / 2.0 && x < w / 2.0) {
        return 0.0;
    }
    let coord = (x + w / 2.0) * 4.0 / w;
    let j = u32(coord);
    let t = coord - f32(j);
    var y = 0.0;
    if j == 3u {
        y = (1.0 - t) * (1.0 - t) * (1.0 - t) / 6.0;
    } else if j == 2u {
        y = (3.0 * t * t * t - 6.0 * t * t + 4.0) / 6.0;
    } else if j == 1u {
        y = (-3.0 * t * t * t + 3.0 * t * t + 3.0 * t + 1.0) / 6.0;
    } else if j == 0u {
        y = t * t * t / 6.0;
    }
    return y * 1.5;
}

// The RRT's glow, red modifier, move to AP1 and global desaturation
// (`rrt_sweeteners`).
fn aces_sweeteners(aces: vec3<f32>) -> vec3<f32> {
    let saturation = aces_saturation(aces);
    let s = aces_sigmoid((saturation - 0.4) / 0.2);
    var a = aces * (1.0 + aces_glow(aces_yc(aces), 0.05 * s, 0.08));
    var weight = 0.0;
    if !(a.x == a.y && a.y == a.z) {
        var hue = degrees(atan2(sqrt(3.0) * (a.y - a.z), 2.0 * a.x - a.y - a.z));
        if hue < 0.0 {
            hue = hue + 360.0;
        }
        var centered = hue;
        if centered < -180.0 {
            centered = centered + 360.0;
        } else if centered > 180.0 {
            centered = centered - 360.0;
        }
        weight = aces_basis(centered, 135.0);
    }
    a.x = a.x + weight * saturation * (0.03 - a.x) * (1.0 - 0.82);
    a = max(a, vec3<f32>(0.0));
    let pre = clamp(aces_rows(0u, a), vec3<f32>(0.0), vec3<f32>(ACES_HALF_MAX));
    return aces_rows(6u, pre);
}

// Video's dim surround: the luminance to the power 0.9811, in xyY.
fn aces_dim(lin: vec3<f32>, gamma: f32) -> vec3<f32> {
    let xyz = aces_rows(12u, lin);
    let sum = xyz.x + xyz.y + xyz.z;
    var div = sum;
    if sum == 0.0 {
        div = 1e-10;
    }
    let x = xyz.x / div;
    let y = xyz.y / div;
    var yy = 0.0;
    if xyz.y > 0.0 {
        yy = pow(xyz.y, gamma);
    }
    let back = vec3<f32>(x * yy / max(y, 1e-10), yy, (1.0 - x - y) * yy / max(y, 1e-10));
    return aces_rows(15u, back);
}

// Two kernels, small enough for Direct3D's older shader compiler: the RRT
// (ACES2065-1 → OCES, in AP0)…
@compute @workgroup_size(16, 16)
fn aces_rrt(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let pre = aces_sweeteners(px.xyz);
    dst[o] = vec4<f32>(aces_rows(3u, aces_spline3(pre, 22u)), px.w);
}

// …then the ODT (OCES → CIE XYZ display-linear).
@compute @workgroup_size(16, 16)
fn aces_odt(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = out_index(id);
    if o < 0 {
        return;
    }
    let px = dst[o];
    let k = src[21u];
    let post = aces_spline3(aces_rows(0u, px.xyz), 36u);
    var lin = (post - vec3<f32>(k.x)) / (k.y - k.x);
    if k.w != 0.0 {
        lin = aces_rows(9u, aces_dim(lin, k.z));
    }
    dst[o] = vec4<f32>(aces_rows(18u, lin), px.w);
}
