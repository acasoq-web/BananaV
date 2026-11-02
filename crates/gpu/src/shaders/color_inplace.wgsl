// Color management (ACES, BananaV's OpenColorIO configuration): the
// steps of a `bv-ocio` processor, each its own kernel on the image's
// straight colors, the same math as `Op::apply` in 32 bits. `view_begin`
// takes the colors out of their alpha, `view_end` gives it back, the
// display's code values taken as video's (BT.709's curve) light as
// `bv_render::leave_through_view` does.

// Colors taken as ACES2065-1 into ACES's working space: the matrix (f 0–8,
// its rows) on the premultiplied colors, as `bv_render::enter_chain`.
@compute @workgroup_size(16, 16)
fn color_enter(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let r = pf(0u) * px.x + pf(1u) * px.y + pf(2u) * px.z;
    let g = pf(3u) * px.x + pf(4u) * px.y + pf(5u) * px.z;
    let b = pf(6u) * px.x + pf(7u) * px.y + pf(8u) * px.z;
    img[i] = vec4<f32>(r, g, b, px.w);
}

@compute @workgroup_size(16, 16)
fn view_begin(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    if px.w <= 0.0 {
        img[i] = vec4<f32>(0.0, 0.0, 0.0, px.w);
        return;
    }
    img[i] = vec4<f32>(px.xyz / px.w, px.w);
}

// The colors back into their alpha: with `j.x` 0 the display's code values
// taken as video's light, as `bv_render::leave_through_view` does; with 1
// as they are (a media entering the working space).
@compute @workgroup_size(16, 16)
fn view_end(@builtin(global_invocation_id) id: vec3<u32>) {
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
    if P.j.x != 0u {
        img[i] = vec4<f32>(px.xyz * a, a);
        return;
    }
    img[i] = vec4<f32>(bt709_decode(px.x) * a, bt709_decode(px.y) * a, bt709_decode(px.z) * a, a);
}

// `Op::Matrix`: f 0–8 its rows, f 9–11 its offset.
@compute @workgroup_size(16, 16)
fn view_matrix(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let r = pf(0u) * px.x + pf(1u) * px.y + pf(2u) * px.z + pf(9u);
    let g = pf(3u) * px.x + pf(4u) * px.y + pf(5u) * px.z + pf(10u);
    let b = pf(6u) * px.x + pf(7u) * px.y + pf(8u) * px.z + pf(11u);
    img[i] = vec4<f32>(r, g, b, px.w);
}

// `Op::Range`: x · f 0 + f 1, then at least f 2 (`j.x` bit 0) and at most
// f 3 (bit 1).
fn range1(v: f32) -> f32 {
    var x = v * pf(0u) + pf(1u);
    if (P.j.x & 1u) != 0u {
        x = max(x, pf(2u));
    }
    if (P.j.x & 2u) != 0u {
        x = min(x, pf(3u));
    }
    return x;
}

@compute @workgroup_size(16, 16)
fn view_range(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    img[i] = vec4<f32>(range1(px.x), range1(px.y), range1(px.z), px.w);
}

// `Op::Power`: x^(f 0, f 1, f 2); below 0 (`j.x`) 0, mirrored (1) or as
// it is (2).
fn power1(x: f32, e: f32) -> f32 {
    if x >= 0.0 {
        return pow(x, e);
    }
    if P.j.x == 1u {
        return -pow(-x, e);
    }
    if P.j.x == 2u {
        return x;
    }
    return 0.0;
}

@compute @workgroup_size(16, 16)
fn view_power(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    img[i] = vec4<f32>(power1(px.x, pf(0u)), power1(px.y, pf(1u)), power1(px.z, pf(2u)), px.w);
}

// `Op::Curve`, `j.y` 1 to linear, 0 from it. `j.x` 0: sRGB's (mirrored
// below 0); 1: a monitor curve, f 0 its gamma, f 1 its offset, f 2 where
// its straight part ends and f 3 that part's slope (worked out on the
// processor for the direction); 2: a gamma, f 0 the exponent used, `j.z`
// mirrored below 0.
fn srgb1(x: f32) -> f32 {
    let ax = abs(x);
    if P.j.y == 1u {
        if ax <= 0.04045 {
            return x / 12.92;
        }
        return signum(x) * pow((ax + 0.055) / 1.055, 2.4);
    }
    if ax <= 0.0031308 {
        return x * 12.92;
    }
    return signum(x) * (1.055 * pow(ax, 1.0 / 2.4) - 0.055);
}

fn moncurve1(x: f32) -> f32 {
    let gamma = pf(0u);
    let offs = pf(1u);
    if P.j.y == 1u {
        if x >= pf(2u) {
            return pow((x + offs) / (1.0 + offs), gamma);
        }
        return x * pf(3u);
    }
    if x >= pf(2u) {
        return (1.0 + offs) * pow(x, 1.0 / gamma) - offs;
    }
    return x * pf(3u);
}

fn gamma1(x: f32) -> f32 {
    if P.j.z != 0u {
        return signum(x) * pow(abs(x), pf(0u));
    }
    return pow(max(x, 0.0), pf(0u));
}

fn curve1(x: f32) -> f32 {
    if P.j.x == 0u {
        return srgb1(x);
    }
    if P.j.x == 1u {
        return moncurve1(x);
    }
    return gamma1(x);
}

@compute @workgroup_size(16, 16)
fn view_curve(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    img[i] = vec4<f32>(curve1(px.x), curve1(px.y), curve1(px.z), px.w);
}

// ACES 1.3's Reference Gamut Compression (`bv_ocio::aces::gamut_compress`):
// in AP1 (f 0–8 AP0 → AP1, f 9–17 back), each channel's distance from the
// achromatic compressed beyond its threshold; `j.x` its inverse.
fn gc_compress(dist: f32, lim: f32, thr: f32) -> f32 {
    if dist < thr {
        return dist;
    }
    let pwr = 1.2;
    let scl = (lim - thr) / pow(pow((1.0 - thr) / (lim - thr), -pwr) - 1.0, 1.0 / pwr);
    let nd = (dist - thr) / scl;
    let p = pow(nd, pwr);
    if P.j.x == 0u {
        return thr + scl * nd / pow(1.0 + p, 1.0 / pwr);
    }
    if dist > thr + scl {
        return dist;
    }
    return thr + scl * pow(-(p / (p - 1.0)), 1.0 / pwr);
}

@compute @workgroup_size(16, 16)
fn aces_gamut_compress(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = at(id);
    if i < 0 {
        return;
    }
    let px = img[i];
    let c = px.xyz;
    let lin = vec3<f32>(
        pf(0u) * c.x + pf(1u) * c.y + pf(2u) * c.z,
        pf(3u) * c.x + pf(4u) * c.y + pf(5u) * c.z,
        pf(6u) * c.x + pf(7u) * c.y + pf(8u) * c.z,
    );
    let ach = max(lin.x, max(lin.y, lin.z));
    var d = vec3<f32>(0.0);
    if ach != 0.0 {
        d = (vec3<f32>(ach) - lin) / abs(ach);
    }
    let cd = vec3<f32>(
        gc_compress(d.x, 1.147, 0.815),
        gc_compress(d.y, 1.264, 0.803),
        gc_compress(d.z, 1.312, 0.880),
    );
    let o = vec3<f32>(ach) - cd * abs(ach);
    img[i] = vec4<f32>(
        pf(9u) * o.x + pf(10u) * o.y + pf(11u) * o.z,
        pf(12u) * o.x + pf(13u) * o.y + pf(14u) * o.z,
        pf(15u) * o.x + pf(16u) * o.y + pf(17u) * o.z,
        px.w,
    );
}
