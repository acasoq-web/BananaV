// Venetian Blinds (`bv-effects`' `transitions/blinds.rs`): each pixel reads
// the blinds' openness — 8-bit steps, one a pixel along the frame's
// direction — at the table's middle plus its column's and its row's
// offsets (16,384ths, rows from the top), between steps as the processor
// mixes them, and mixes the pictures by it.
//
// tt[0]: the table's length and middle; tt[1 + i]: step i, column i's
// offset, row i's offset.

@compute @workgroup_size(16, 16)
fn tr_venetian_blinds(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let head = tt[0];
    let o = id.y * P.w + id.x;
    let pa = ta[o];
    if head.x < 1 {
        dst[o] = pa;
        return;
    }
    let u = head.y + tt[1 + i32(id.y)].z + tt[1 + i32(id.x)].y;
    let i = clamp(u >> 14u, 0, head.x - 1);
    let a = tt[1 + i].x;
    let b = tt[1 + min(i + 1, head.x - 1)].x;
    var v = (b - a) * (((u & 16383) * 255) >> 14u) + 128;
    v = (((v >> 8u) + v) >> 8u) + a;
    let k = f32(clamp(v, 0, 255)) * 0.003921569;
    dst[o] = (tb[o] - pa) * k + pa;
}
