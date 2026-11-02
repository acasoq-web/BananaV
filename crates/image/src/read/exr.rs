//! OpenEXR (single part, scanline or tiled — the full-resolution level):
//! no compression, RLE, ZIPS, ZIP, PIZ and PXR24; half, float and uint
//! channels.
//! R, G, B and A (or a layer's, or Y for gray) come out as linear light
//! with premultiplied alpha, framed by the display window.

use super::{Picture, PictureData, ReadError, Result, corrupt, unsupported};

#[derive(Clone, Debug)]
struct Channel {
    name: String,
    /// 0 uint, 1 half, 2 float.
    kind: u32,
    xs: i32,
    ys: i32,
}

impl Channel {
    fn bytes(&self) -> usize {
        if self.kind == 1 { 2 } else { 4 }
    }
}

fn i32_at(d: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn bad() -> ReadError {
    ReadError::Corrupt("damaged OpenEXR file".into())
}

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let flags = i32_at(d, 4).ok_or_else(bad)?;
    if flags & 0x800 != 0 {
        return unsupported("deep OpenEXR");
    }
    if flags & 0x1000 != 0 {
        return unsupported("multi-part OpenEXR");
    }
    let tiled_flag = flags & 0x200 != 0;
    let mut pos = 8;
    let mut channels = Vec::new();
    let mut compression = 0u8;
    let mut data_window = None;
    let mut display_window = None;
    let mut tiles = None;
    // Attributes: name, type, size, value; an empty name ends them.
    loop {
        let name_end = pos
            + d.get(pos..)
                .ok_or_else(bad)?
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(bad)?;
        if name_end == pos {
            pos += 1;
            break;
        }
        let name = String::from_utf8_lossy(&d[pos..name_end]).to_string();
        let kind_start = name_end + 1;
        let kind_end = kind_start
            + d.get(kind_start..)
                .ok_or_else(bad)?
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(bad)?;
        let kind = String::from_utf8_lossy(&d[kind_start..kind_end]).to_string();
        let size = i32_at(d, kind_end + 1).ok_or_else(bad)? as usize;
        let value = d.get(kind_end + 5..kind_end + 5 + size).ok_or_else(bad)?;
        match (name.as_str(), kind.as_str()) {
            ("channels", "chlist") => {
                let mut i = 0;
                while i < value.len() && value[i] != 0 {
                    let end = i + value[i..].iter().position(|&b| b == 0).ok_or_else(bad)?;
                    let cname = String::from_utf8_lossy(&value[i..end]).to_string();
                    let k = i32_at(value, end + 1).ok_or_else(bad)? as u32;
                    let xs = i32_at(value, end + 9).ok_or_else(bad)?;
                    let ys = i32_at(value, end + 13).ok_or_else(bad)?;
                    channels.push(Channel {
                        name: cname,
                        kind: k,
                        xs,
                        ys,
                    });
                    i = end + 17;
                }
            }
            ("compression", _) => compression = *value.first().ok_or_else(bad)?,
            ("dataWindow", "box2i") => data_window = Some(box2i(value).ok_or_else(bad)?),
            ("displayWindow", "box2i") => display_window = Some(box2i(value).ok_or_else(bad)?),
            ("tiles", "tiledesc") => {
                let tx = i32_at(value, 0).ok_or_else(bad)?;
                let ty = i32_at(value, 4).ok_or_else(bad)?;
                tiles = Some((tx.max(1) as usize, ty.max(1) as usize));
            }
            _ => {}
        }
        pos = kind_end + 5 + size;
    }
    let (dx0, dy0, dx1, dy1) = data_window.ok_or_else(bad)?;
    let (wx0, wy0, wx1, wy1) = display_window.unwrap_or((dx0, dy0, dx1, dy1));
    let (dw, dh) = ((dx1 - dx0 + 1) as usize, (dy1 - dy0 + 1) as usize);
    if dx1 < dx0 || dy1 < dy0 || dw > 1 << 16 || dh > 1 << 16 {
        return corrupt("OpenEXR data window");
    }
    // A display window much larger than the data is not shown whole.
    let (mut ow, mut oh) = ((wx1 - wx0 + 1) as i64, (wy1 - wy0 + 1) as i64);
    let (mut ox0, mut oy0) = (wx0, wy0);
    if ow <= 0 || oh <= 0 || ow > 1 << 15 || oh > 1 << 15 {
        (ow, oh, ox0, oy0) = (dw as i64, dh as i64, dx0, dy0);
    }
    let lines_per_block = match compression {
        0..=2 => 1,
        3 => 16,
        4 => 32,
        5 => 16,
        6 | 7 => return unsupported("OpenEXR with B44 compression"),
        8 | 9 => return unsupported("OpenEXR with DWA compression"),
        c => return unsupported(format!("OpenEXR compression {c}")),
    };
    // The channels shown: R, G, B, A; or a layer's (the first one with an
    // R); or Y for gray.
    let pick = |suffix: &str| -> Option<usize> {
        channels.iter().position(|c| c.name == suffix).or_else(|| {
            let layer = channels
                .iter()
                .find_map(|c| c.name.strip_suffix(".R").map(str::to_string))?;
            channels
                .iter()
                .position(|c| c.name == format!("{layer}.{suffix}"))
        })
    };
    let (r, g, b) = (pick("R"), pick("G"), pick("B"));
    let a = pick("A");
    let y = channels.iter().position(|c| c.name == "Y");
    let rgb = match (r, g, b, y) {
        (Some(r), Some(g), Some(b), _) => [r, g, b],
        (_, _, _, Some(y)) => [y, y, y],
        _ => return unsupported("OpenEXR without R, G, B or Y channels"),
    };
    for &c in rgb.iter().chain(a.iter()) {
        if channels[c].xs != 1 || channels[c].ys != 1 {
            return unsupported("OpenEXR with subsampled channels");
        }
    }
    let row_bytes: usize = channels.iter().map(|c| c.bytes() * dw).sum();
    // Every channel's samples of the data window, as f32.
    let mut planes = vec![vec![0f32; dw * dh]; channels.len()];
    let mut put_lines = |raw: &[u8], first: usize, x0: usize, width: usize, lines: usize| {
        let row = row_bytes / dw * width;
        for l in 0..lines {
            let yy = first + l;
            if yy >= dh {
                break;
            }
            let Some(line) = raw.get(l * row..(l + 1) * row) else {
                break;
            };
            let mut at = 0;
            for (ci, c) in channels.iter().enumerate() {
                let n = c.bytes();
                for x in 0..width {
                    let s = &line[at + x * n..at + x * n + n];
                    let v = match c.kind {
                        1 => half_to_f32(u16::from_le_bytes([s[0], s[1]])),
                        2 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                        _ => u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32,
                    };
                    if x0 + x < dw {
                        planes[ci][yy * dw + x0 + x] = v;
                    }
                }
                at += n * width;
            }
        }
    };
    let table = pos;
    match (tiled_flag || tiles.is_some(), tiles) {
        (true, Some((tw, th))) => {
            let (across, down) = (dw.div_ceil(tw), dh.div_ceil(th));
            for i in 0..across * down {
                let Some(off) = d.get(table + i * 8..table + i * 8 + 8) else {
                    break;
                };
                let off = u64::from_le_bytes(off.try_into().unwrap_or([0; 8])) as usize;
                let (Some(tx), Some(ty), Some(size)) =
                    (i32_at(d, off), i32_at(d, off + 4), i32_at(d, off + 16))
                else {
                    continue;
                };
                let (tx, ty) = (tx.max(0) as usize, ty.max(0) as usize);
                let x0 = tx * tw;
                let y0 = ty * th;
                let width = tw.min(dw.saturating_sub(x0));
                let lines = th.min(dh.saturating_sub(y0));
                let Some(packed) = d.get(off + 20..off + 20 + size.max(0) as usize) else {
                    continue;
                };
                let expect = row_bytes / dw * width * lines;
                if let Some(raw) = decompress(compression, packed, expect, &channels, width, lines)
                {
                    put_lines(&raw, y0, x0, width, lines);
                }
            }
        }
        (true, None) => return corrupt("tiled OpenEXR without tile size"),
        _ => {
            let blocks = dh.div_ceil(lines_per_block);
            for i in 0..blocks {
                let Some(off) = d.get(table + i * 8..table + i * 8 + 8) else {
                    break;
                };
                let off = u64::from_le_bytes(off.try_into().unwrap_or([0; 8])) as usize;
                let (Some(yy), Some(size)) = (i32_at(d, off), i32_at(d, off + 4)) else {
                    continue;
                };
                let first = (yy - dy0).max(0) as usize;
                let lines = lines_per_block.min(dh.saturating_sub(first));
                let Some(packed) = d.get(off + 8..off + 8 + size.max(0) as usize) else {
                    continue;
                };
                if let Some(raw) =
                    decompress(compression, packed, row_bytes * lines, &channels, dw, lines)
                {
                    put_lines(&raw, first, 0, dw, lines);
                }
            }
        }
    }
    // Framed by the display window; outside the data, transparent.
    let (ow, oh) = (ow as usize, oh as usize);
    let mut out = vec![0f32; ow * oh * 4];
    for yy in 0..dh {
        let oy = dy0 + yy as i32 - oy0;
        if oy < 0 || oy as usize >= oh {
            continue;
        }
        for xx in 0..dw {
            let ox = dx0 + xx as i32 - ox0;
            if ox < 0 || ox as usize >= ow {
                continue;
            }
            let i = yy * dw + xx;
            let o = (oy as usize * ow + ox as usize) * 4;
            out[o] = planes[rgb[0]][i];
            out[o + 1] = planes[rgb[1]][i];
            out[o + 2] = planes[rgb[2]][i];
            out[o + 3] = a.map_or(1.0, |a| planes[a][i]);
        }
    }
    Ok(Picture {
        width: ow as u32,
        height: oh as u32,
        data: PictureData::RgbaF32(out),
        linear: true,
        premultiplied: true,
    })
}

/// The display window's size (or the data window's), from the header.
pub(super) fn dimensions(d: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 8;
    let (mut data, mut display) = (None, None);
    loop {
        let name_end = pos + d.get(pos..)?.iter().position(|&b| b == 0)?;
        if name_end == pos {
            break;
        }
        let kind_end = name_end + 1 + d.get(name_end + 1..)?.iter().position(|&b| b == 0)?;
        let size = i32_at(d, kind_end + 1)? as usize;
        let value = d.get(kind_end + 5..kind_end + 5 + size)?;
        match &d[pos..name_end] {
            b"dataWindow" => data = box2i(value),
            b"displayWindow" => display = box2i(value),
            _ => {}
        }
        pos = kind_end + 5 + size;
    }
    let (x0, y0, x1, y1) = display.or(data)?;
    let (w, h) = ((x1 - x0 + 1) as i64, (y1 - y0 + 1) as i64);
    if w > 0 && h > 0 && w <= 1 << 15 && h <= 1 << 15 {
        return Some((w as u32, h as u32));
    }
    let (x0, y0, x1, y1) = data?;
    Some(((x1 - x0 + 1).max(1) as u32, (y1 - y0 + 1).max(1) as u32))
}

fn box2i(v: &[u8]) -> Option<(i32, i32, i32, i32)> {
    Some((i32_at(v, 0)?, i32_at(v, 4)?, i32_at(v, 8)?, i32_at(v, 12)?))
}

/// A block's bytes as stored uncompressed (`expect` of them).
fn decompress(
    compression: u8,
    packed: &[u8],
    expect: usize,
    channels: &[Channel],
    width: usize,
    lines: usize,
) -> Option<Vec<u8>> {
    if packed.len() == expect {
        // Blocks that did not get smaller are stored as they are.
        return Some(packed.to_vec());
    }
    match compression {
        0 => Some(packed.to_vec()),
        1 => Some(unpredict(&rle(packed, expect))),
        2 | 3 => Some(unpredict(&bv_deflate::zlib_decompress(packed).ok()?)),
        4 => piz(packed, channels, width, lines),
        5 => pxr24(
            &bv_deflate::zlib_decompress(packed).ok()?,
            channels,
            width,
            lines,
        ),
        _ => None,
    }
}

/// PXR24: per line and channel, the samples' bytes in planes (most
/// significant first), each sample stored as the difference from the
/// previous; floats kept to 24 bits.
fn pxr24(t: &[u8], channels: &[Channel], width: usize, lines: usize) -> Option<Vec<u8>> {
    let mut out =
        Vec::with_capacity(channels.iter().map(|c| c.bytes()).sum::<usize>() * width * lines);
    let mut at = 0usize;
    for _ in 0..lines {
        for c in channels {
            let planes = match c.kind {
                1 => 2,
                2 => 3,
                _ => 4,
            };
            let p = t.get(at..at + planes * width)?;
            at += planes * width;
            let mut pixel = 0u32;
            for j in 0..width {
                let diff = match c.kind {
                    1 => (p[j] as u32) << 8 | p[width + j] as u32,
                    2 => {
                        (p[j] as u32) << 24
                            | (p[width + j] as u32) << 16
                            | (p[2 * width + j] as u32) << 8
                    }
                    _ => {
                        (p[j] as u32) << 24
                            | (p[width + j] as u32) << 16
                            | (p[2 * width + j] as u32) << 8
                            | p[3 * width + j] as u32
                    }
                };
                pixel = pixel.wrapping_add(diff);
                if c.kind == 1 {
                    out.extend_from_slice(&(pixel as u16).to_le_bytes());
                } else {
                    out.extend_from_slice(&pixel.to_le_bytes());
                }
            }
        }
    }
    Some(out)
}

fn rle(d: &[u8], max: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(max);
    let mut i = 0;
    while i < d.len() && out.len() < max {
        let n = d[i] as i8;
        i += 1;
        if n < 0 {
            let count = (-(n as i32)) as usize;
            let end = (i + count).min(d.len());
            out.extend_from_slice(&d[i..end]);
            i = end;
        } else {
            let Some(&b) = d.get(i) else { break };
            out.extend(std::iter::repeat_n(b, n as usize + 1));
            i += 1;
        }
    }
    out
}

/// Undoes ZIP's and RLE's delta coding and byte split.
fn unpredict(t: &[u8]) -> Vec<u8> {
    let mut t = t.to_vec();
    for i in 1..t.len() {
        t[i] = (t[i - 1] as i32 + t[i] as i32 - 128) as u8;
    }
    let half = t.len().div_ceil(2);
    let mut out = vec![0u8; t.len()];
    for i in 0..t.len() {
        out[i] = if i % 2 == 0 {
            t[i / 2]
        } else {
            t[half + i / 2]
        };
    }
    out
}

/// IEEE 754 half to single precision.
pub(crate) fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 31) as u32;
    let mant = (h & 0x3ff) as u32;
    let bits = match exp {
        0 if mant == 0 => sign,
        0 => {
            // Subnormal: normalized in single precision.
            let mut e = 127 - 15 + 1;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((m & 0x3ff) << 13)
        }
        31 => sign | 0x7f80_0000 | (mant << 13),
        _ => sign | ((exp + 127 - 15) << 23) | (mant << 13),
    };
    f32::from_bits(bits)
}

// ---- PIZ: a range table, Huffman coding and a wavelet. ----

/// A PIZ block: the samples of each channel (as 16-bit units) went through
/// a lookup table, a 2D wavelet and Huffman coding.
fn piz(d: &[u8], channels: &[Channel], width: usize, lines: usize) -> Option<Vec<u8>> {
    let min_nz = u16::from_le_bytes(d.get(..2)?.try_into().ok()?) as usize;
    let max_nz = u16::from_le_bytes(d.get(2..4)?.try_into().ok()?) as usize;
    let mut bitmap = [0u8; 8192];
    let mut pos = 4;
    if min_nz <= max_nz {
        if max_nz >= 8192 {
            return None;
        }
        let n = max_nz - min_nz + 1;
        bitmap[min_nz..=max_nz].copy_from_slice(d.get(pos..pos + n)?);
        pos += n;
    }
    // The table back from the values present (0 always is).
    let mut lut = vec![0u16; 65536];
    let mut k = 0usize;
    for i in 0..65536usize {
        if i == 0 || bitmap[i >> 3] & (1 << (i & 7)) != 0 {
            lut[k] = i as u16;
            k += 1;
        }
    }
    let max_value = (k - 1) as u16;
    let length = i32_at(d, pos)? as usize;
    pos += 4;
    // Each channel's units, planar.
    let sizes: Vec<usize> = channels.iter().map(|c| c.bytes() / 2).collect();
    let total: usize = sizes.iter().map(|s| s * width * lines).sum();
    let mut tmp = huffman_decode(d.get(pos..pos + length)?, total)?;
    let mut start = 0;
    for &size in &sizes {
        for j in 0..size {
            wav2_decode(
                &mut tmp[start + j..],
                width,
                size,
                lines,
                width * size,
                max_value,
            );
        }
        start += width * lines * size;
    }
    for v in &mut tmp {
        *v = lut[*v as usize];
    }
    // Back to lines of channels.
    let mut out = Vec::with_capacity(total * 2);
    let mut ends: Vec<usize> = Vec::new();
    let mut acc = 0;
    for &size in &sizes {
        ends.push(acc);
        acc += width * lines * size;
    }
    for _ in 0..lines {
        for (c, &size) in sizes.iter().enumerate() {
            let n = width * size;
            for v in &tmp[ends[c]..ends[c] + n] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            ends[c] += n;
        }
    }
    Some(out)
}

fn wdec14(l: u16, h: u16) -> (u16, u16) {
    let ls = l as i16 as i32;
    let hi = h as i16 as i32;
    let ai = ls + (hi & 1) + (hi >> 1);
    let a = ai as i16;
    let b = (ai - hi) as i16;
    (a as u16, b as u16)
}

fn wdec16(l: u16, h: u16) -> (u16, u16) {
    let (m, dd) = (l as i32, h as i32);
    let bb = (m - (dd >> 1)) & 0xffff;
    let aa = (dd + bb - 0x8000) & 0xffff;
    (aa as u16, bb as u16)
}

/// OpenEXR's 2D wavelet decoding, in place (`ox` and `oy` are the steps
/// between neighbours across and down).
fn wav2_decode(buf: &mut [u16], nx: usize, ox: usize, ny: usize, oy: usize, mx: u16) {
    let w14 = mx < (1 << 14);
    let dec = |l: u16, h: u16| if w14 { wdec14(l, h) } else { wdec16(l, h) };
    let n = nx.min(ny);
    let mut p = 1usize;
    while p <= n {
        p <<= 1;
    }
    p >>= 1;
    let mut p2 = p;
    p >>= 1;
    while p >= 1 {
        let (oy1, oy2, ox1, ox2) = (oy * p, oy * p2, ox * p, ox * p2);
        let ey = oy * (ny - p2);
        let mut py = 0usize;
        while py <= ey {
            let mut px = py;
            let ex = py + ox * (nx - p2);
            while px <= ex {
                let (p01, p10) = (px + ox1, px + oy1);
                let p11 = p10 + ox1;
                let (i00, i10) = dec(buf[px], buf[p10]);
                let (i01, i11) = dec(buf[p01], buf[p11]);
                let (a, b) = dec(i00, i01);
                buf[px] = a;
                buf[p01] = b;
                let (a, b) = dec(i10, i11);
                buf[p10] = a;
                buf[p11] = b;
                px += ox2;
            }
            if nx & p != 0 {
                let p10 = px + oy1;
                let (i00, b) = dec(buf[px], buf[p10]);
                buf[p10] = b;
                buf[px] = i00;
            }
            py += oy2;
        }
        if ny & p != 0 {
            let mut px = py;
            let ex = py + ox * (nx - p2);
            while px <= ex {
                let p01 = px + ox1;
                let (i00, b) = dec(buf[px], buf[p01]);
                buf[p01] = b;
                buf[px] = i00;
                px += ox2;
            }
        }
        p2 = p;
        p >>= 1;
    }
}

/// OpenEXR's Huffman coding: a packed table of code lengths, then the
/// codes; the largest symbol repeats the previous value (a run).
fn huffman_decode(d: &[u8], count: usize) -> Option<Vec<u16>> {
    if d.is_empty() {
        return (count == 0).then(Vec::new);
    }
    let u32_at = |at: usize| Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?));
    let im = u32_at(0)? as usize;
    let i_max = u32_at(4)? as usize;
    let nbits = u32_at(12)? as usize;
    const ENCSIZE: usize = 65537;
    if im >= ENCSIZE || i_max >= ENCSIZE {
        return None;
    }
    // Code lengths: 6 bits each; 59–62 a short run of zeros, 63 a long one.
    let mut lengths = vec![0u8; ENCSIZE];
    let mut p = 20usize;
    let (mut c, mut lc) = (0u64, 0u32);
    let mut get = |n: u32, p: &mut usize| -> Option<u64> {
        while lc < n {
            c = (c << 8) | *d.get(*p)? as u64;
            *p += 1;
            lc += 8;
        }
        lc -= n;
        Some((c >> lc) & ((1 << n) - 1))
    };
    let mut i = im;
    while i <= i_max {
        let l = get(6, &mut p)? as usize;
        if l == 63 {
            let run = get(8, &mut p)? as usize + 6;
            if i + run > i_max + 1 {
                return None;
            }
            i += run;
            continue;
        }
        if l >= 59 {
            let run = l - 59 + 2;
            if i + run > i_max + 1 {
                return None;
            }
            i += run;
            continue;
        }
        lengths[i] = l as u8;
        i += 1;
    }
    // Canonical codes, longest first as OpenEXR assigns them.
    let mut n = [0u64; 59];
    for &l in &lengths {
        n[l as usize] += 1;
    }
    let mut code = 0u64;
    for l in (1..=58).rev() {
        let next = (code + n[l]) >> 1;
        n[l] = code;
        code = next;
    }
    let mut codes = vec![0u64; ENCSIZE];
    for s in 0..ENCSIZE {
        let l = lengths[s] as usize;
        if l > 0 {
            codes[s] = n[l];
            n[l] += 1;
        }
    }
    // Decoding table on 14 bits; longer codes listed per prefix.
    const DECBITS: u32 = 14;
    let mut short = vec![(0u32, 0u8); 1 << DECBITS];
    let mut long: Vec<Vec<u32>> = vec![Vec::new(); 1 << DECBITS];
    for s in im..=i_max {
        let l = lengths[s] as u32;
        let cde = codes[s];
        if l == 0 {
            continue;
        }
        if cde >> l != 0 {
            return None;
        }
        if l > DECBITS {
            long[(cde >> (l - DECBITS)) as usize].push(s as u32);
        } else {
            let first = (cde << (DECBITS - l)) as usize;
            for e in &mut short[first..first + (1 << (DECBITS - l))] {
                *e = (s as u32, l as u8);
            }
        }
    }
    let rlc = i_max as u32;
    let data = d.get(p..)?;
    let mut out: Vec<u16> = Vec::with_capacity(count);
    // A symbol: a value, or the run of the previous one (8 more bits).
    let emit = |sym: u32,
                lc: &mut u32,
                data: &[u8],
                q: &mut usize,
                cc: &mut u64,
                out: &mut Vec<u16>|
     -> Option<()> {
        if sym == rlc {
            if *lc < 8 {
                *cc = (*cc << 8) | *data.get(*q)? as u64;
                *q += 1;
                *lc += 8;
            }
            *lc -= 8;
            let run = (*cc >> *lc) as u8 as usize;
            let last = *out.last()?;
            if out.len() + run > count {
                return None;
            }
            out.extend(std::iter::repeat_n(last, run));
        } else {
            if out.len() >= count {
                return None;
            }
            out.push(sym as u16);
        }
        Some(())
    };
    let end = nbits.div_ceil(8).min(data.len());
    let (mut cc, mut lc) = (0u64, 0u32);
    let mut q = 0usize;
    while q < end {
        cc = (cc << 8) | data[q] as u64;
        q += 1;
        lc += 8;
        while lc >= DECBITS {
            let idx = ((cc >> (lc - DECBITS)) & ((1 << DECBITS) - 1)) as usize;
            let (sym, len) = short[idx];
            if len > 0 {
                lc -= len as u32;
                emit(sym, &mut lc, data, &mut q, &mut cc, &mut out)?;
            } else {
                let mut found = false;
                for &s in &long[idx] {
                    let l = lengths[s as usize] as u32;
                    while lc < l && q < end {
                        cc = (cc << 8) | data[q] as u64;
                        q += 1;
                        lc += 8;
                    }
                    if lc >= l && codes[s as usize] == (cc >> (lc - l)) & ((1u64 << l) - 1) {
                        lc -= l;
                        emit(s, &mut lc, data, &mut q, &mut cc, &mut out)?;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return None;
                }
            }
        }
    }
    // The last short codes, in the bits left.
    let i = ((8 - nbits as i64).rem_euclid(8)) as u32;
    cc >>= i;
    lc = lc.saturating_sub(i);
    while lc > 0 {
        let idx = ((cc << (DECBITS - lc.min(DECBITS))) & ((1 << DECBITS) - 1)) as usize;
        let (sym, len) = short[idx];
        if len == 0 || len as u32 > lc {
            break;
        }
        lc -= len as u32;
        emit(sym, &mut lc, data, &mut q, &mut cc, &mut out)?;
    }
    (out.len() == count).then_some(out)
}

#[cfg(test)]
mod tests {
    use crate::read::PictureData;
    use crate::{ExrCompression, exr};

    #[test]
    fn what_the_writer_wrote_comes_back() {
        let px: Vec<[f32; 4]> = (0..9 * 5)
            .map(|i| [i as f32 / 10.0, 0.5, 2.0, (i % 3) as f32 / 2.0])
            .collect();
        for (half, comp) in [
            (true, ExrCompression::Zip),
            (false, ExrCompression::None),
            (false, ExrCompression::Zip),
        ] {
            let file = exr(9, 5, &px, true, half, comp);
            let p = super::read(&file).unwrap();
            let PictureData::RgbaF32(got) = p.data else {
                panic!()
            };
            for (a, b) in got.chunks(4).zip(&px) {
                for c in 0..4 {
                    assert!((a[c] - b[c]).abs() < 0.002, "{a:?} vs {b:?}");
                }
            }
        }
    }
}
