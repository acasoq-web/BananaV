// The frames between two pictures (`bv-effects`' `optical_flow`'s
// `made_between`, Slow Motion's and Warp Flow's): the motion back from the
// second picture (`tb`) to the first (`ta`) read between the flow's cells
// (`tt`: i.x × i.y of them, rows up, each f 1 of the pictures' pixels, the
// move's two floats as bits), the first read f 0 of the move along, the
// second the rest of it back — both between pixels, their edges held —,
// mixed, or the nearer one with j.x; where one is transparent, the other.

fn fb_cell(x: i32, y: i32) -> vec2<f32> {
    let cx = clamp(x, 0, P.i.x - 1);
    let cy = clamp(y, 0, P.i.y - 1);
    let c = tt[cy * P.i.x + cx];
    return vec2<f32>(bitcast<f32>(c.x), bitcast<f32>(c.y));
}

// The move at the pictures' pixel (x, y), rows up, in their pixels.
fn fb_move(x: f32, y: f32) -> vec2<f32> {
    let k = pf(1u);
    let fx = (x + 0.5) / k - 0.5;
    let fy = (y + 0.5) / k - 0.5;
    let x0 = floor(fx);
    let y0 = floor(fy);
    let ax = fx - x0;
    let ay = fy - y0;
    let i = i32(x0);
    let j = i32(y0);
    let top = fb_cell(i, j) * (1.0 - ax) + fb_cell(i + 1, j) * ax;
    let bottom = fb_cell(i, j + 1) * (1.0 - ax) + fb_cell(i + 1, j + 1) * ax;
    return (top * (1.0 - ay) + bottom * ay) * k;
}

// A picture's pixel, rows up, its edges held.
fn fb_at(second: bool, x: i32, y: i32) -> vec4<f32> {
    let cx = clamp(x, 0, i32(P.w) - 1);
    let cy = clamp(y, 0, i32(P.h) - 1);
    let o = u32(i32(P.h) - 1 - cy) * P.w + u32(cx);
    if second {
        return tb[o];
    }
    return ta[o];
}

fn fb_bilinear(second: bool, x: f32, y: f32) -> vec4<f32> {
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let i = i32(x0);
    let j = i32(y0);
    let low = mix(fb_at(second, i, j), fb_at(second, i + 1, j), fx);
    let high = mix(fb_at(second, i, j + 1), fb_at(second, i + 1, j + 1), fx);
    return mix(low, high, fy);
}

@compute @workgroup_size(16, 16)
fn kit_flow_between(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let x = f32(id.x);
    let y = f32(P.h - 1u - id.y);
    let t = pf(0u);
    let m = fb_move(x, y);
    let ca = fb_bilinear(false, x + t * m.x, y + t * m.y);
    let cb = fb_bilinear(true, x - (1.0 - t) * m.x, y - (1.0 - t) * m.y);
    if ca.w <= 1e-6 {
        dst[o] = cb;
    } else if cb.w <= 1e-6 {
        dst[o] = ca;
    } else if P.j.x != 0u {
        dst[o] = select(cb, ca, t <= 0.5);
    } else {
        dst[o] = ca + (cb - ca) * t;
    }
}
