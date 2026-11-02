//! TIFF 6.0 (the first page): strips or tiles, chunky or planar; no
//! compression, LZW, Deflate or PackBits, with the horizontal and the
//! floating-point predictors; gray, RGB, palette and CMYK of 1 to 16 bits,
//! 32-bit floats; associated or unassociated alpha; the orientation tag.

use super::{Picture, PictureData, ReadError, Result, corrupt, unsupported};

struct Reader<'a> {
    d: &'a [u8],
    le: bool,
}

impl Reader<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.d.get(at..at + 2)?.try_into().ok()?;
        Some(if self.le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.d.get(at..at + 4)?.try_into().ok()?;
        Some(if self.le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }
}

/// A tag's values (as u32).
fn values(r: &Reader<'_>, entry: usize) -> Vec<u32> {
    let (Some(kind), Some(count)) = (r.u16(entry + 2), r.u32(entry + 4)) else {
        return Vec::new();
    };
    let size = match kind {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => return Vec::new(),
    };
    let count = count as usize;
    if count > 1 << 24 {
        return Vec::new();
    }
    let at = if size * count <= 4 {
        entry + 8
    } else {
        r.u32(entry + 8).unwrap_or(0) as usize
    };
    (0..count)
        .filter_map(|i| match size {
            1 => r.d.get(at + i).map(|&b| b as u32),
            2 => r.u16(at + i * 2).map(u32::from),
            4 => r.u32(at + i * 4),
            // Rationals: the numerator is enough here.
            _ => r.u32(at + i * 8),
        })
        .collect()
}

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let le = d.starts_with(b"II");
    let r = Reader { d, le };
    if r.u16(2) == Some(43) {
        return unsupported("BigTIFF");
    }
    let ifd = r.u32(4).ok_or_else(bad)? as usize;
    let count = r.u16(ifd).ok_or_else(bad)? as usize;
    let mut tag = std::collections::HashMap::new();
    for i in 0..count {
        let e = ifd + 2 + i * 12;
        if let Some(t) = r.u16(e) {
            tag.insert(t, values(&r, e));
        }
    }
    let one = |t: u16, default: u32| {
        tag.get(&t)
            .and_then(|v| v.first().copied())
            .unwrap_or(default)
    };
    let width = one(256, 0) as usize;
    let height = one(257, 0) as usize;
    if width == 0 || height == 0 || width > 1 << 16 || height > 1 << 16 {
        return corrupt("TIFF size");
    }
    let spp = one(277, 1) as usize;
    let bits = one(258, 1) as usize;
    let compression = one(259, 1);
    let photometric = one(262, if spp >= 3 { 2 } else { 1 });
    let planar = one(284, 1) == 2;
    let predictor = one(317, 1);
    let format = one(339, 1);
    let orientation = one(274, 1) as u16;
    // 1: associated (premultiplied) alpha, 2: unassociated.
    let extra = one(338, 0);
    if !matches!(compression, 1 | 5 | 8 | 32946 | 32773) {
        return unsupported(match compression {
            7 | 6 => "TIFF with JPEG inside".to_string(),
            c => format!("TIFF compression {c}"),
        });
    }
    if !matches!(bits, 1 | 2 | 4 | 8 | 16 | 32) || (bits == 32 && format != 3) {
        return unsupported(format!("TIFF of {bits} bits"));
    }
    if format == 3 && bits != 32 {
        return unsupported("TIFF of 16 or 64-bit floats");
    }
    if !matches!(photometric, 0..=3 | 5 | 6) {
        return unsupported(format!("TIFF color space {photometric}"));
    }
    // Y′CbCr (photometric 6): blocks of h×v luma samples and their two
    // chroma ones, made RGB as they are read.
    let ycbcr = (photometric == 6).then(|| {
        let sub = tag.get(&530).cloned().unwrap_or_default();
        let (sh, sv) = (
            sub.first().copied().unwrap_or(2).clamp(1, 4) as usize,
            sub.get(1).copied().unwrap_or(2).clamp(1, 4) as usize,
        );
        // Rationals keep only their numerators here: read the coefficients
        // as the usual ones.
        let refbw = tag.get(&532).cloned();
        YCbCr {
            sh,
            sv,
            full: refbw.is_none_or(|v| v.first() == Some(&0)),
        }
    });
    if ycbcr.is_some() && (planar || bits != 8 || spp != 3) {
        return unsupported("this kind of Y′CbCr TIFF");
    }
    // Strips are tiles as wide as the image.
    let tiled = tag.contains_key(&322);
    let (tw, th) = if tiled {
        (one(322, 0) as usize, one(323, 0) as usize)
    } else {
        (width, (one(278, height as u32) as usize).min(height).max(1))
    };
    if tw == 0 || th == 0 {
        return corrupt("TIFF tile size");
    }
    let offsets = tag
        .get(&if tiled { 324 } else { 273 })
        .cloned()
        .unwrap_or_default();
    let counts = tag
        .get(&if tiled { 325 } else { 279 })
        .cloned()
        .unwrap_or_default();
    let across = width.div_ceil(tw);
    let down = height.div_ceil(th);
    let planes = if planar { spp } else { 1 };
    let per_pixel = if planar { 1 } else { spp };
    let row_bytes = (tw * per_pixel * bits).div_ceil(8);
    // Samples as f32 in 0–1 (floats as they are), all planes interleaved.
    let mut samples = vec![0f32; width * height * spp];
    for plane in 0..planes {
        for ty in 0..down {
            for tx in 0..across {
                let i = plane * across * down + ty * across + tx;
                let (Some(&off), Some(&len)) = (offsets.get(i), counts.get(i)) else {
                    continue;
                };
                let Some(raw) = d.get(off as usize..off as usize + len as usize) else {
                    continue;
                };
                let rows = th;
                let mut data = match compression {
                    1 => raw.to_vec(),
                    5 => lzw(raw, row_bytes * rows * 2),
                    32773 => packbits(raw, row_bytes * rows * 2),
                    _ => bv_deflate::zlib_decompress(raw).unwrap_or_default(),
                };
                let need = match &ycbcr {
                    Some(y) => y.bytes(tw, rows),
                    None => row_bytes * rows,
                };
                data.resize(need, 0);
                if let Some(y) = &ycbcr {
                    y.unpack(
                        &data,
                        &mut samples,
                        (width, height),
                        (tx * tw, ty * th, tw, rows),
                    );
                    continue;
                }
                match predictor {
                    2 => horizontal(&mut data, row_bytes, per_pixel, bits, le),
                    3 => data = float_predictor(&data, tw, per_pixel, rows),
                    _ => {}
                }
                unpack(
                    &data,
                    &mut samples,
                    (width, height, spp),
                    (tx * tw, ty * th, tw, rows),
                    (plane, per_pixel, bits, format, le && predictor != 3),
                    row_bytes,
                );
            }
        }
    }
    let colormap = tag.get(&320).cloned().unwrap_or_default();
    // Y′CbCr came out as RGB.
    let photometric = if photometric == 6 { 2 } else { photometric };
    let alpha_at = match photometric {
        2 if spp >= 4 => Some(3),
        5 if spp >= 5 => Some(4),
        0 | 1 if spp >= 2 => Some(1),
        _ => None,
    };
    let premultiplied = extra == 1 && alpha_at.is_some();
    let mut rgba = vec![0f32; width * height * 4];
    for p in 0..width * height {
        let s = &samples[p * spp..p * spp + spp];
        let a = alpha_at.map_or(1.0, |i| s[i]);
        let [r, g, b] = match photometric {
            0 => [1.0 - s[0]; 3],
            1 => [s[0]; 3],
            3 => {
                let n = 1usize << bits;
                let idx = (s[0] * (n - 1) as f32).round() as usize;
                let at = |k: usize| {
                    colormap
                        .get(k * n + idx)
                        .map_or(0.0, |&v| v as f32 / 65535.0)
                };
                [at(0), at(1), at(2)]
            }
            5 => {
                let k = 1.0 - s.get(3).copied().unwrap_or(0.0);
                [(1.0 - s[0]) * k, (1.0 - s[1]) * k, (1.0 - s[2]) * k]
            }
            _ => [
                s[0],
                s.get(1).copied().unwrap_or(s[0]),
                s.get(2).copied().unwrap_or(s[0]),
            ],
        };
        rgba[p * 4..p * 4 + 4].copy_from_slice(&[r, g, b, a]);
    }
    let float = format == 3;
    let data = if float {
        PictureData::RgbaF32(rgba)
    } else if bits == 16 {
        PictureData::Rgba16(
            rgba.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
                .collect(),
        )
    } else {
        PictureData::Rgba8(
            rgba.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        )
    };
    Ok(Picture {
        width: width as u32,
        height: height as u32,
        data,
        // Floating-point TIFFs hold linear light (as HDR tools write them).
        linear: float,
        premultiplied,
    }
    .orient(orientation))
}

fn bad() -> ReadError {
    ReadError::Corrupt("damaged TIFF".into())
}

/// How a Y′CbCr TIFF is laid out.
struct YCbCr {
    /// Luma samples per chroma sample, across and down.
    sh: usize,
    sv: usize,
    /// Full range (TIFF's default reference black and white) or video
    /// range (16–235).
    full: bool,
}

impl YCbCr {
    /// Bytes of a strip or tile of `w` × `rows`.
    fn bytes(&self, w: usize, rows: usize) -> usize {
        w.div_ceil(self.sh) * rows.div_ceil(self.sv) * (self.sh * self.sv + 2)
    }

    /// Makes a strip or tile's blocks RGB samples (0–1) of the image.
    fn unpack(
        &self,
        data: &[u8],
        samples: &mut [f32],
        (width, height): (usize, usize),
        (x0, y0, tw, rows): (usize, usize, usize, usize),
    ) {
        let n = self.sh * self.sv;
        let across = tw.div_ceil(self.sh);
        for by in 0..rows.div_ceil(self.sv) {
            for bx in 0..across {
                let at = (by * across + bx) * (n + 2);
                let Some(block) = data.get(at..at + n + 2) else {
                    return;
                };
                let (cb, cr) = (block[n] as f32 - 128.0, block[n + 1] as f32 - 128.0);
                for j in 0..self.sv {
                    for i in 0..self.sh {
                        let (x, y) = (x0 + bx * self.sh + i, y0 + by * self.sv + j);
                        if x >= width || y >= height || by * self.sv + j >= rows {
                            continue;
                        }
                        let mut l = block[j * self.sh + i] as f32;
                        let (mut b, mut r) = (cb, cr);
                        if !self.full {
                            l = (l - 16.0) * 255.0 / 219.0;
                            b *= 255.0 / 224.0;
                            r *= 255.0 / 224.0;
                        }
                        // BT.601 (TIFF's default coefficients).
                        let rgb = [
                            l + 1.402 * r,
                            l - 0.344_136 * b - 0.714_136 * r,
                            l + 1.772 * b,
                        ];
                        let o = (y * width + x) * 3;
                        for c in 0..3 {
                            samples[o + c] = (rgb[c] / 255.0).clamp(0.0, 1.0);
                        }
                    }
                }
            }
        }
    }
}

/// Spreads a tile's samples (0–1) into the image.
fn unpack(
    data: &[u8],
    samples: &mut [f32],
    (width, height, spp): (usize, usize, usize),
    (x0, y0, tw, rows): (usize, usize, usize, usize),
    (plane, per_pixel, bits, format, le): (usize, usize, usize, u32, bool),
    row_bytes: usize,
) {
    let max = ((1u64 << bits.min(32)) - 1) as f32;
    for ty in 0..rows {
        let y = y0 + ty;
        if y >= height {
            break;
        }
        let row = &data[ty * row_bytes..(ty + 1) * row_bytes];
        for tx in 0..tw {
            let x = x0 + tx;
            if x >= width {
                break;
            }
            for c in 0..per_pixel {
                let i = tx * per_pixel + c;
                let v = match bits {
                    8 => row[i] as f32 / 255.0,
                    16 => {
                        let b = [row[i * 2], row[i * 2 + 1]];
                        (if le {
                            u16::from_le_bytes(b)
                        } else {
                            u16::from_be_bytes(b)
                        }) as f32
                            / 65535.0
                    }
                    32 => {
                        let b = [row[i * 4], row[i * 4 + 1], row[i * 4 + 2], row[i * 4 + 3]];
                        let u = if le {
                            u32::from_le_bytes(b)
                        } else {
                            u32::from_be_bytes(b)
                        };
                        if format == 3 {
                            f32::from_bits(u)
                        } else {
                            u as f32 / max
                        }
                    }
                    _ => {
                        let bit = i * bits;
                        let byte = row[bit / 8];
                        ((byte >> (8 - bits - bit % 8)) & ((1u16 << bits) - 1) as u8) as f32 / max
                    }
                };
                let channel = if per_pixel == 1 { plane } else { c };
                if channel < spp {
                    samples[(y * width + x) * spp + channel] = v;
                }
            }
        }
    }
}

/// TIFF's LZW: codes MSB first, 9 to 12 bits, one code early.
fn lzw(data: &[u8], max: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(max);
    let mut table: Vec<(u16, u8, u16)> = Vec::with_capacity(4096); // (prefix, byte, length)
    let reset = |t: &mut Vec<(u16, u8, u16)>| {
        t.clear();
        for i in 0..256u16 {
            t.push((u16::MAX, i as u8, 1));
        }
        t.push((0, 0, 0)); // 256 clear
        t.push((0, 0, 0)); // 257 end
    };
    reset(&mut table);
    let (mut acc, mut nbits) = (0u32, 0u32);
    let mut size = 9u32;
    let mut prev: Option<u16> = None;
    let mut pos = 0;
    let mut stack = Vec::with_capacity(4096);
    let string = |t: &Vec<(u16, u8, u16)>, mut c: u16, stack: &mut Vec<u8>| -> u8 {
        stack.clear();
        loop {
            let (p, b, _) = t[c as usize];
            stack.push(b);
            if p == u16::MAX {
                break;
            }
            c = p;
        }
        *stack.last().unwrap_or(&0)
    };
    while out.len() < max {
        while nbits < size {
            let Some(&b) = data.get(pos) else {
                return out;
            };
            acc = (acc << 8) | b as u32;
            nbits += 8;
            pos += 1;
        }
        let code = ((acc >> (nbits - size)) & ((1 << size) - 1)) as u16;
        nbits -= size;
        match code {
            256 => {
                reset(&mut table);
                size = 9;
                prev = None;
                continue;
            }
            257 => break,
            _ => {}
        }
        let first = if (code as usize) < table.len() {
            let first = string(&table, code, &mut stack);
            out.extend(stack.iter().rev());
            first
        } else if let Some(p) = prev {
            // The KwKwK case: the previous string plus its first byte.
            let first = string(&table, p, &mut stack);
            out.extend(stack.iter().rev());
            out.push(first);
            first
        } else {
            break;
        };
        if let Some(p) = prev
            && table.len() < 4096
        {
            let len = table[p as usize].2 + 1;
            table.push((p, first, len));
        }
        prev = Some(code);
        // "Early change": a code wider as soon as the table is one short.
        size = match table.len() + 1 {
            n if n >= 2048 => 12,
            n if n >= 1024 => 11,
            n if n >= 512 => 10,
            _ => 9,
        };
    }
    out
}

fn packbits(data: &[u8], max: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(max);
    let mut i = 0;
    while i < data.len() && out.len() < max {
        let n = data[i] as i8;
        i += 1;
        if n >= 0 {
            let len = n as usize + 1;
            let end = (i + len).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else if n != -128 {
            let Some(&b) = data.get(i) else { break };
            out.extend(std::iter::repeat_n(b, (1 - n as isize) as usize));
            i += 1;
        }
    }
    out
}

/// Undoes the horizontal predictor: each sample was stored as the
/// difference from the one to its left.
fn horizontal(data: &mut [u8], row_bytes: usize, per_pixel: usize, bits: usize, le: bool) {
    for row in data.chunks_mut(row_bytes) {
        match bits {
            8 => {
                for i in per_pixel..row.len() {
                    row[i] = row[i].wrapping_add(row[i - per_pixel]);
                }
            }
            16 => {
                let n = row.len() / 2;
                let get = |r: &[u8], i: usize| {
                    let b = [r[i * 2], r[i * 2 + 1]];
                    if le {
                        u16::from_le_bytes(b)
                    } else {
                        u16::from_be_bytes(b)
                    }
                };
                for i in per_pixel..n {
                    let v = get(row, i).wrapping_add(get(row, i - per_pixel));
                    let b = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                    row[i * 2..i * 2 + 2].copy_from_slice(&b);
                }
            }
            _ => {}
        }
    }
}

/// Undoes the floating-point predictor (bytes split by significance, then
/// differenced): rows of big-endian floats come back.
fn float_predictor(data: &[u8], tw: usize, per_pixel: usize, rows: usize) -> Vec<u8> {
    let row_bytes = tw * per_pixel * 4;
    let mut out = vec![0u8; row_bytes * rows];
    for (r, row) in data.chunks(row_bytes).enumerate().take(rows) {
        let mut row = row.to_vec();
        for i in 1..row.len() {
            row[i] = row[i].wrapping_add(row[i - 1]);
        }
        let n = tw * per_pixel;
        for i in 0..n {
            for b in 0..4 {
                out[r * row_bytes + i * 4 + b] = row.get(b * n + i).copied().unwrap_or(0);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{Image, Pixels, read::PictureData, tiff};

    #[test]
    fn what_the_writer_wrote_comes_back() {
        let px: Vec<u16> = (0..7 * 5 * 4).map(|i| (i * 1000 % 65536) as u16).collect();
        let img = Image {
            width: 7,
            height: 5,
            pixels: Pixels::Rgba16(&px),
            alpha: true,
        };
        for deflate in [false, true] {
            let p = super::read(&tiff(&img, 16, deflate)).unwrap();
            let PictureData::Rgba16(got) = p.data else {
                panic!("{:?}", p.data)
            };
            assert_eq!(got, px, "deflate {deflate}");
        }
    }

    #[test]
    fn packbits_runs_and_literals() {
        // A run of three, two literal bytes, a no-op, a run of 128.
        let data = [0xFE, 0xAA, 0x01, 1, 2, 0x80, 0x81, 7];
        let mut want = vec![0xAA, 0xAA, 0xAA, 1, 2];
        want.extend([7; 128]);
        assert_eq!(super::packbits(&data, 1000), want);
    }
}
