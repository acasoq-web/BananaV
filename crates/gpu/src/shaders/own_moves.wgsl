// BananaV's transitions that move the pictures (`bv-effects`'
// `transitions/moves.rs`: Barn Door, Push, Slide, Swap, Squeeze, Split,
// Zoom and Spiral): each pixel drawn by the frame's steps in order, over
// nothing, from `ta` (the picture going out) and `tb` (the one coming in),
// places in 16,384ths of a pixel, rows from the top. The same math as the
// processor's `Drawing::pixel`, in f32.
//
// tt[0].x: how many steps; then six rows a step (`Drawing::table`). A
// blit: (0, flags — 1 the second picture, 2 averaged, 4 composited —, one
// over its footprint's area and its alpha as floats' bits), its columns'
// pixels (first, end, first's and last's coverage), their source places
// (first, second, step, last), the same for its rows, then the sources'
// last column and row. A border: (1, its edges, size, feather), (how much
// farther, its color as floats' bits), its edges (from, to, at, flags — 1
// down, 2 after). A fill: (2, …), its columns' and rows' coverage, its
// premultiplied color (floats' bits).

const MV_ONE: i32 = 16384;
const MV_RECORD: i32 = 6;

// A picture's pixel, its edges held.
fn mv_texel(second: bool, i: i32, j: i32) -> vec4<f32> {
    let x = u32(clamp(i, 0, i32(P.w) - 1));
    let y = u32(clamp(j, 0, i32(P.h) - 1));
    if second {
        return tb[y * P.w + x];
    }
    return ta[y * P.w + x];
}

// Pixel `k`'s coverage along an axis `a` (`Axis::cover`).
fn mv_cover(a: vec4<i32>, k: i32) -> i32 {
    if k < a.x || k >= a.y {
        return 0;
    }
    if k == a.x {
        return a.z;
    }
    if k == a.y - 1 {
        return a.w;
    }
    return MV_ONE;
}

// Where pixel `k` reads the source (`Axis::at`): `a` the axis's pixels,
// `s` its source places.
fn mv_at(a: vec4<i32>, s: vec4<i32>, k: i32) -> i32 {
    if k == a.x {
        return s.x;
    }
    if k == a.y - 1 && a.y - a.x > 1 {
        return s.w;
    }
    return s.y + (k - a.x - 1) * s.z;
}

// Pixel `k`'s footprint in the source, its start and width
// (`Axis::footprint`).
fn mv_footprint(a: vec4<i32>, s: vec4<i32>, k: i32) -> vec2<i32> {
    if k == a.x {
        return vec2<i32>(s.x, s.y);
    }
    let start = s.x + s.y + (k - a.x - 1) * s.z;
    if k == a.y - 1 && a.y - a.x > 1 {
        return vec2<i32>(start, s.w);
    }
    return vec2<i32>(start, s.z);
}

// A footprint's pixels along an axis: the first, past the last, and how
// much of the first and the last it covers.
struct MvSpan {
    i0: i32,
    i1: i32,
    first: f32,
    last: f32,
}

fn mv_span(s: i32, w: i32) -> MvSpan {
    let i0 = s >> 14u;
    let i1 = (s + w + MV_ONE - 1) >> 14u;
    var first = f32(w) / 16384.0;
    if i1 - i0 > 1 {
        first = f32((i0 + 1) * MV_ONE - s) / 16384.0;
    }
    let last = f32(w + s - (i1 - 1) * MV_ONE) / 16384.0;
    return MvSpan(i0, i1, first, last);
}

// The weighted sum of a footprint (`footprint_sum`).
fn mv_sum(second: bool, fx: vec2<i32>, fy: vec2<i32>) -> vec4<f32> {
    let sx = mv_span(fx.x, fx.y);
    let sy = mv_span(fy.x, fy.y);
    var acc = vec4<f32>(0.0);
    for (var j = sy.i0; j < sy.i1; j++) {
        var wy = 1.0;
        if j == sy.i0 {
            wy = sy.first;
        } else if j == sy.i1 - 1 {
            wy = sy.last;
        }
        for (var i = sx.i0; i < sx.i1; i++) {
            var wx = 1.0;
            if i == sx.i0 {
                wx = sx.first;
            } else if i == sx.i1 - 1 {
                wx = sx.last;
            }
            acc += mv_texel(second, i, j) * (wx * wy);
        }
    }
    return acc;
}

// A blit's step at row `r` on pixel `x`, `y` over `under` (`Drawn::at`).
fn mv_blit(r: i32, x: i32, y: i32, under: vec4<f32>) -> vec4<f32> {
    let head = tt[r];
    let cols = tt[r + 1];
    let cs = tt[r + 2];
    let rows = tt[r + 3];
    let rs = tt[r + 4];
    let limits = tt[r + 5];
    let cx = mv_cover(cols, x);
    let cy = mv_cover(rows, y);
    let composite = (head.y & 4) != 0;
    // A single column is never scaled; composites draw it.
    if cx == 0 || cy == 0 || (!composite && cols.y - cols.x < 2) {
        return under;
    }
    let second = (head.y & 1) != 0;
    if (head.y & 2) != 0 {
        let fx = mv_footprint(cols, cs, x);
        let fy = mv_footprint(rows, rs, y);
        let sum = mv_sum(second, fx, fy);
        if cx == MV_ONE && cy == MV_ONE {
            return sum * bitcast<f32>(head.z);
        }
        let part = f32(fx.y) * f32(fy.y);
        if part < 16384.0 {
            return under;
        }
        let share = f32((cx * cy) >> 14u) / 16384.0;
        let v = sum * (268435456.0 / part);
        return (v - under) * share + under;
    }
    let px = mv_at(cols, cs, x);
    let py = mv_at(rows, rs, y);
    let ix = px >> 14u;
    let iy = py >> 14u;
    let fx = f32(px & 16383) / 16384.0;
    let fy = f32(py & 16383) / 16384.0;
    var nx = ix + 1;
    if ix == limits.x {
        nx = ix;
    }
    var ny = iy + 1;
    if iy == limits.y {
        ny = iy;
    }
    let g = 1.0 - fy;
    let w00 = (1.0 - fx) * g;
    let w10 = g * fx;
    let w01 = (1.0 - fx) * fy;
    let w11 = fy * fx;
    let v = mv_texel(second, ix, iy) * w00 + mv_texel(second, nx, iy) * w10
        + mv_texel(second, ix, ny) * w01 + mv_texel(second, nx, ny) * w11;
    var share = (f32(cx) / 16384.0) * (f32(cy) / 16384.0);
    if composite {
        if cx == MV_ONE && cy == MV_ONE {
            share = bitcast<f32>(head.w);
        } else {
            // The edges' coverage times the alpha in 16,384ths, the row's
            // first.
            let a = i32(bitcast<f32>(head.w) * 16384.0);
            share = f32((((cy * a) >> 14u) * cx) >> 14u) / 16384.0;
        }
    }
    if share == 1.0 {
        return v;
    }
    return (v - under) * share + under;
}

// A fill's step at row `r` (`Step::Fill`): its color laid over by its
// coverage.
fn mv_fill(r: i32, x: i32, y: i32, under: vec4<f32>) -> vec4<f32> {
    let cx = mv_cover(tt[r + 1], x);
    let cy = mv_cover(tt[r + 2], y);
    if cx == 0 || cy == 0 {
        return under;
    }
    var k = 1.0;
    if cx != MV_ONE || cy != MV_ONE {
        k = f32((cy * cx) >> 14u) / 16384.0;
    }
    let c = bitcast<vec4<f32>>(tt[r + 3]);
    return c * k + under * (1.0 - c.w * k);
}

// A border's step at row `r` on pixel `x`, `y` over `under` (`Border::at`).
fn mv_border(r: i32, x: i32, y: i32, under: vec4<f32>) -> vec4<f32> {
    let head = tt[r];
    let n = head.y;
    let b = head.z;
    let f = head.w;
    let most = b + f;
    if most == 0 || n == 0 {
        return under;
    }
    let more = tt[r + 1];
    let px = x * MV_ONE + MV_ONE / 2;
    let py = y * MV_ONE + MV_ONE / 2;
    var near = most;
    var inside = f;
    var hit = false;
    for (var k = 0; k < n; k++) {
        let s = tt[r + 2 + k];
        var along = px;
        var across = py;
        if (s.w & 1) != 0 {
            along = py;
            across = px;
        }
        if s.x <= along && along <= s.y {
            var d = s.z - across;
            if (s.w & 2) != 0 {
                d = -d;
            }
            if d < 0 {
                if -d < inside {
                    inside = -d;
                    hit = true;
                }
            } else if d < near {
                near = d;
                hit = true;
            }
        }
    }
    if !hit {
        // The ends of the edges along x, rounded.
        for (var k = 0; k < n; k++) {
            let s = tt[r + 2 + k];
            if (s.w & 1) != 0 {
                continue;
            }
            var beyond = near;
            if s.x - px >= 0 {
                beyond = s.x - px;
            }
            var past = near;
            if px - s.y >= 0 {
                past = px - s.y;
            }
            let along = min(past, beyond);
            if along >= 0 && along < near {
                var side = s.z - py;
                if (s.w & 2) != 0 {
                    side = py - s.z;
                }
                if side > 0 && side < near {
                    let fa = f32(along);
                    let fs = f32(side);
                    let d = i32(sqrt(fa * fa + fs * fs) + 0.5);
                    if d < near {
                        near = d;
                        hit = true;
                    }
                }
            }
        }
    }
    if !hit {
        return under;
    }
    if more.x != 0 {
        near = min(near + more.x, most);
        inside = min(inside + more.x, f);
    }
    var k = 0.0;
    if near < b {
        k = 1.0;
    } else if f != 0 {
        let t = f32(min(near - b, inside)) / f32(f);
        k = 1.0 - (t * 3.0 * t - (t + t) * t * t);
    }
    let c = vec4<f32>(bitcast<f32>(more.y), bitcast<f32>(more.z), bitcast<f32>(more.w), 1.0);
    return (c - under) * k + under;
}

@compute @workgroup_size(16, 16)
fn tr_moves(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    var acc = vec4<f32>(0.0);
    let n = tt[0].x;
    for (var s = 0; s < n; s++) {
        let r = 1 + s * MV_RECORD;
        let kind = tt[r].x;
        if kind == 0 {
            acc = mv_blit(r, x, y, acc);
        } else if kind == 1 {
            acc = mv_border(r, x, y, acc);
        } else {
            acc = mv_fill(r, x, y, acc);
        }
    }
    dst[id.y * P.w + id.x] = acc;
}
