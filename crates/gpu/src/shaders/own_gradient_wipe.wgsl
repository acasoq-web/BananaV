// Gradient Wipe (`bv-effects`' `gradient_wipe.rs`): each pixel's gradient
// step (8-bit, prepared on the processor) gives its share of the first
// picture through the ramp: out = (1 - m) * b + m * a.
//
// tt[0]: the frame's size; tt[1 + s].x: step s's share in 255ths; from
// tt[257]: the steps, four to a word, four words a row, rows from the top.

fn gw_step(o: u32) -> i32 {
    let word = o >> 2u;
    let row = tt[257 + i32(word >> 2u)];
    let c = word & 3u;
    var v = row.x;
    if c == 1u {
        v = row.y;
    } else if c == 2u {
        v = row.z;
    } else if c == 3u {
        v = row.w;
    }
    return (v >> ((o & 3u) * 8u)) & 255;
}

@compute @workgroup_size(16, 16)
fn tr_gradient_wipe(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let m = f32(tt[1 + gw_step(o)].x) * 0.003921569;
    dst[o] = (1.0 - m) * tb[o] + m * ta[o];
}
