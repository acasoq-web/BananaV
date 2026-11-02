// The 3D transitions (`bv-effects`' `transitions/cards.rs`, the same math
// in f32): the cards of the table drawn one after another over what `dst`
// holds — each pixel covered by the exact part of it a card covers, its
// color the average of the card's picture (`ta` the first, `tb` the
// second, rows from the top) over the pixel's footprint, lit as VEGAS
// lights its cards and laid over by its share.
//
// tt[0]: how many cards, what is under them, whether there is a light;
// tt[1]: the light (x, y, z in pixels, strength); tt[2]: the frame's
// size and pixel aspect; from tt[3], 14 rows a card: its picture, corners,
// lit or not and opacity; its normal; its planes (1/w, s/w, t/w at a
// point and their steps); its bounds; its 16 corners (floats as bits).

const CD_ROWS: i32 = 14;

fn cd_f(v: i32) -> f32 {
    return bitcast<f32>(v);
}

fn cd_pixel(second: bool, i: i32, j: i32) -> vec4<f32> {
    let o = u32(j) * P.w + u32(i);
    if second {
        return tb[o];
    }
    return ta[o];
}

fn cd_grow(d: i32) -> i32 {
    let e = max(d, 16384);
    if e > 16384 {
        return e * 3 - 32768;
    }
    return e;
}

fn cd_part(lo: i32, hi: i32, k: i32) -> f32 {
    return f32(max(min(hi, (k + 1) << 14u) - max(lo, k << 14u), 0));
}

// The average over the box from u, v, du × dv (16,384ths of a pixel), as
// VEGAS reads a footprint: at least a pixel, three times wider less two
// pixels when more, within the picture; each pixel by the part of it the
// box covers.
fn cd_average(second: bool, u: i32, v: i32, du: i32, dv: i32) -> vec4<f32> {
    let w = i32(P.w);
    let h = i32(P.h);
    let x1 = cd_grow(du) + u;
    let y1 = v + cd_grow(dv);
    var x0 = 0;
    if u >= 0 {
        x0 = min(u, w * 16384 - 16384);
    }
    var y0 = 0;
    if v >= 0 {
        y0 = min(v, h * 16384 - 16384);
    }
    var xe = x0 + 1;
    if x0 + 1 <= x1 {
        xe = min(x1, w * 16384 - 1);
    }
    var ye = y0 + 1;
    if y0 + 1 <= y1 {
        ye = min(y1, h * 16384 - 1);
    }
    var sum = vec4<f32>(0.0);
    for (var j = y0 >> 14u; j <= (ye - 1) >> 14u; j++) {
        let wy = cd_part(y0, ye, j) * 6.1035156e-05;
        for (var i = x0 >> 14u; i <= (xe - 1) >> 14u; i++) {
            let k = cd_part(x0, xe, i) * 6.1035156e-05 * wy;
            sum = sum + cd_pixel(second, i, j) * k;
        }
    }
    let k = 1.0 / (f32(ye - y0) * 6.1035156e-05 * f32(xe - x0) * 6.1035156e-05);
    return sum * k;
}

// VEGAS's whole 16,384ths of a texture coordinate (halves to even).
fn cd_round(x: f32) -> i32 {
    return i32(round(clamp(x, -1.0e9, 1.0e9)));
}

// The part of pixel x, y (rows from the top) card `base`'s outline covers.
fn cd_cover(base: i32, n: i32, x: f32, y: f32) -> f32 {
    var buf: array<vec2<f32>, 24>;
    var tmp: array<vec2<f32>, 24>;
    for (var k = 0; k < n; k++) {
        let r = tt[base + 6 + k / 2];
        if (k & 1) == 0 {
            buf[k] = vec2<f32>(cd_f(r.x), cd_f(r.y));
        } else {
            buf[k] = vec2<f32>(cd_f(r.z), cd_f(r.w));
        }
    }
    // Which way the corners turn.
    var area = 0.0;
    for (var k = 0; k < n; k++) {
        let a = buf[k];
        let b = buf[(k + 1) % n];
        area += a.x * b.y - b.x * a.y;
    }
    var turn = 1.0;
    if area < 0.0 {
        turn = -1.0;
    }
    // Inside every edge: whole.
    var whole = true;
    for (var c = 0; c < 4; c++) {
        let q = vec2<f32>(x + f32(c & 1), y + f32(c >> 1u));
        for (var k = 0; k < n; k++) {
            let a = buf[k];
            let b = buf[(k + 1) % n];
            if ((b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x)) * turn < 0.0 {
                whole = false;
            }
        }
    }
    if whole {
        return 1.0;
    }
    var m = n;
    for (var e = 0; e < 4; e++) {
        var bound = x;
        var axis = 0;
        var above = true;
        if e == 1 {
            bound = x + 1.0;
            above = false;
        } else if e == 2 {
            bound = y;
            axis = 1;
        } else if e == 3 {
            bound = y + 1.0;
            axis = 1;
            above = false;
        }
        var out = 0;
        for (var k = 0; k < m; k++) {
            let a = buf[k];
            let b = buf[(k + 1) % m];
            var da = a.x - bound;
            var db = b.x - bound;
            if axis == 1 {
                da = a.y - bound;
                db = b.y - bound;
            }
            if !above {
                da = -da;
                db = -db;
            }
            if da >= 0.0 && out < 24 {
                tmp[out] = a;
                out += 1;
            }
            if (da >= 0.0) != (db >= 0.0) && out < 24 {
                let t = da / (da - db);
                tmp[out] = a + (b - a) * t;
                out += 1;
            }
        }
        if out < 3 {
            return 0.0;
        }
        for (var k = 0; k < out; k++) {
            buf[k] = tmp[k];
        }
        m = out;
    }
    var sum = 0.0;
    for (var k = 0; k < m; k++) {
        let a = buf[k];
        let b = buf[(k + 1) % m];
        sum += a.x * b.y - b.x * a.y;
    }
    return min(abs(sum * 0.5), 1.0);
}

// `c` (premultiplied) lit as VEGAS lights the pixel x, y of a card with
// normal `n` at depth `z`.
fn cd_lit(c: vec4<f32>, n: vec3<f32>, lit: bool, x: f32, y: f32, z: f32) -> vec4<f32> {
    let a = c.w;
    var s = vec3<f32>(0.0);
    if a > 1e-6 {
        s = c.xyz / a;
    }
    var sum = s * abs(n.z);
    let frame = tt[2];
    let w = cd_f(frame.x);
    let h = cd_f(frame.y);
    let par = cd_f(frame.z);
    if lit && tt[0].z != 0 {
        let l = tt[1];
        let strength = cd_f(l.w);
        var view = vec3<f32>((w * 0.5 - x) * par, h * 0.5 - y, 0.0 - z);
        var to = vec3<f32>((cd_f(l.x) - x) * par, cd_f(l.y) - y, cd_f(l.z) - z);
        let d2 = (to.y * to.y) / (h * h) + (to.x * to.x) / (w * w) + (to.z * to.z) / (w * w);
        if length(view) > 0.0 {
            view = view / length(view);
        }
        var near = 0.0;
        var spec = 0.0;
        var facing = 0.0;
        if d2 >= 1e-5 {
            to = to / length(to);
            facing = dot(n, to);
            var r = (n + n) * facing - to;
            var lean = dot(view, r);
            if facing < 0.0 {
                r = -r;
                facing = -facing;
                lean = -dot(view, r);
            }
            if lean >= 0.001 {
                let vr = dot(view, r);
                let v2 = vr * vr;
                let v4 = v2 * v2;
                spec = v4 * v4;
            }
            near = 1.0;
            if d2 >= 1.0 {
                near = min(1.0 / d2, 1.0);
            }
        }
        sum = sum + (s * 0.8 * facing + vec3<f32>(spec)) * (near * strength);
    }
    return vec4<f32>(sum * a, a);
}

@compute @workgroup_size(16, 16)
fn tr_cards(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= P.w || id.y >= P.h {
        return;
    }
    let o = id.y * P.w + id.x;
    let x = f32(id.x);
    let y = f32(id.y);
    let tw = f32(i32(P.w));
    let th = f32(i32(P.h));
    var d = dst[o];
    let count = tt[0].x;
    for (var c = 0; c < count; c++) {
        let base = 3 + c * CD_ROWS;
        let head = tt[base];
        let bounds = tt[base + 5];
        if x + 1.0 <= cd_f(bounds.x) || y + 1.0 <= cd_f(bounds.y) || x >= cd_f(bounds.z)
            || y >= cd_f(bounds.w) {
            continue;
        }
        let cov = cd_cover(base, head.y, x, y);
        if cov <= 0.0 {
            continue;
        }
        let second = head.x != 0;
        let lit = head.z != 0;
        let r0 = tt[base + 2];
        let r1 = tt[base + 3];
        let r2 = tt[base + 4];
        let ox = x - cd_f(r0.x);
        let oy = y - cd_f(r0.y);
        let v0 = vec3<f32>(cd_f(r0.z), cd_f(r0.w), cd_f(r1.x));
        let dx = vec3<f32>(cd_f(r1.y), cd_f(r1.z), cd_f(r1.w));
        let dy = vec3<f32>(cd_f(r2.x), cd_f(r2.y), cd_f(r2.z));
        let at = v0 + ox * dx + oy * dy;
        let next = at + dx + dy;
        let wq = 1.0 / at.x;
        let u = cd_round(at.y * wq * 16384.0 * tw);
        let v = cd_round(at.z * wq * 16384.0 * th);
        let wn = 1.0 / next.x;
        let du = abs(cd_round(next.y * wn * 16384.0 * tw) - u);
        let dv = abs(cd_round(next.z * wn * 16384.0 * th) - v);
        var col = cd_average(second, u, v, du, dv);
        var n = vec3<f32>(0.0, 0.0, 1.0);
        if lit {
            let nr = tt[base + 1];
            n = vec3<f32>(cd_f(nr.x), cd_f(nr.y), cd_f(nr.z));
        }
        col = cd_lit(col, n, lit, x, y, tw * wq * tw);
        let k = cov * cd_f(head.w);
        if k >= 1.0 {
            d = col;
        } else {
            d = vec4<f32>(col.xyz * k + d.xyz * (1.0 - k), d.w + (min(col.w, 1.0) - d.w) * k);
        }
    }
    dst[o] = d;
}
