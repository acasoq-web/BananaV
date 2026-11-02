//! OpenEXR (scanline images): linear light with premultiplied alpha, in
//! half (16-bit) or full (32-bit) floats, uncompressed or ZIP.

use bv_deflate::zlib_compress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExrCompression {
    None,
    /// zlib over blocks of 16 scanlines (lossless).
    Zip,
}

/// IEEE 754 half from single precision, rounded to nearest even.
pub fn f32_to_half(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xFF) as i32;
    let mant = x & 0x007F_FFFF;
    if exp == 0xFF {
        // Infinity or NaN.
        return sign | 0x7C00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7C00;
    }
    if e <= 0 {
        // Subnormal half (or zero).
        if e < -10 {
            return sign;
        }
        let m = mant | 0x0080_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rest = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let round = rest > mid || (rest == mid && half & 1 == 1);
        return sign | (half + round as u32) as u16;
    }
    let half = ((e as u32) << 10) | (mant >> 13);
    let rest = mant & 0x1FFF;
    let round = rest > 0x1000 || (rest == 0x1000 && half & 1 == 1);
    // A carry into the exponent is still the right value (up to infinity).
    sign | (half + round as u32) as u16
}

fn attr(h: &mut Vec<u8>, name: &str, kind: &str, value: &[u8]) {
    h.extend_from_slice(name.as_bytes());
    h.push(0);
    h.extend_from_slice(kind.as_bytes());
    h.push(0);
    h.extend_from_slice(&(value.len() as i32).to_le_bytes());
    h.extend_from_slice(value);
}

/// OpenEXR's ZIP: bytes split into even and odd halves, delta-coded, then
/// zlib; raw when that is not smaller.
fn zip(raw: &[u8]) -> Vec<u8> {
    let n = raw.len();
    let mut t = vec![0u8; n];
    let half = n.div_ceil(2);
    for (i, b) in raw.iter().enumerate() {
        if i % 2 == 0 {
            t[i / 2] = *b;
        } else {
            t[half + i / 2] = *b;
        }
    }
    let mut prev = t.first().copied().unwrap_or(0) as i32;
    for b in t.iter_mut().skip(1) {
        let cur = *b as i32;
        *b = (cur - prev + 128 + 256) as u8;
        prev = cur;
    }
    let z = zlib_compress(&t, 6);
    if z.len() < n { z } else { raw.to_vec() }
}

/// An EXR of `width`×`height` RGBA pixels (linear, premultiplied); with
/// `alpha` false the A channel is left out.
pub fn exr(
    width: u32,
    height: u32,
    pixels: &[[f32; 4]],
    alpha: bool,
    half: bool,
    compression: ExrCompression,
) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    // Channels in alphabetical order, as the file stores them.
    let channels: &[(u8, usize)] = if alpha {
        &[(b'A', 3), (b'B', 2), (b'G', 1), (b'R', 0)]
    } else {
        &[(b'B', 2), (b'G', 1), (b'R', 0)]
    };
    let mut out = vec![0x76, 0x2F, 0x31, 0x01, 2, 0, 0, 0];
    let mut chlist = Vec::new();
    for (name, _) in channels {
        chlist.push(*name);
        chlist.push(0);
        chlist.extend_from_slice(&(if half { 1i32 } else { 2 }).to_le_bytes());
        chlist.extend_from_slice(&[0, 0, 0, 0]); // pLinear, reserved
        chlist.extend_from_slice(&1i32.to_le_bytes());
        chlist.extend_from_slice(&1i32.to_le_bytes());
    }
    chlist.push(0);
    attr(&mut out, "channels", "chlist", &chlist);
    let lines_per_block = match compression {
        ExrCompression::None => 1,
        ExrCompression::Zip => 16,
    };
    attr(
        &mut out,
        "compression",
        "compression",
        &[if compression == ExrCompression::Zip {
            3
        } else {
            0
        }],
    );
    let mut window = Vec::new();
    for v in [0i32, 0, w as i32 - 1, h as i32 - 1] {
        window.extend_from_slice(&v.to_le_bytes());
    }
    attr(&mut out, "dataWindow", "box2i", &window);
    attr(&mut out, "displayWindow", "box2i", &window);
    attr(&mut out, "lineOrder", "lineOrder", &[0]);
    attr(&mut out, "pixelAspectRatio", "float", &1f32.to_le_bytes());
    attr(&mut out, "screenWindowCenter", "v2f", &[0; 8]);
    attr(&mut out, "screenWindowWidth", "float", &1f32.to_le_bytes());
    out.push(0);

    let blocks = h.div_ceil(lines_per_block);
    let table = out.len();
    out.resize(table + blocks * 8, 0);
    let sample = if half { 2 } else { 4 };
    for b in 0..blocks {
        let y0 = b * lines_per_block;
        let y1 = (y0 + lines_per_block).min(h);
        let mut raw = Vec::with_capacity((y1 - y0) * w * channels.len() * sample);
        for y in y0..y1 {
            for (_, c) in channels {
                for x in 0..w {
                    let v = pixels[y * w + x][*c];
                    if half {
                        raw.extend_from_slice(&f32_to_half(v).to_le_bytes());
                    } else {
                        raw.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
        }
        let data = match compression {
            ExrCompression::None => raw,
            ExrCompression::Zip => zip(&raw),
        };
        let at = out.len() as u64;
        out[table + b * 8..table + b * 8 + 8].copy_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&(y0 as i32).to_le_bytes());
        out.extend_from_slice(&(data.len() as i32).to_le_bytes());
        out.extend_from_slice(&data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_to_nearest() {
        assert_eq!(f32_to_half(0.0), 0x0000);
        assert_eq!(f32_to_half(1.0), 0x3C00);
        assert_eq!(f32_to_half(-2.0), 0xC000);
        assert_eq!(f32_to_half(0.5), 0x3800);
        assert_eq!(f32_to_half(65504.0), 0x7BFF);
        assert_eq!(f32_to_half(1e6), 0x7C00);
        // Smallest subnormal half.
        assert_eq!(f32_to_half(5.960_464_5e-8), 0x0001);
        assert_eq!(f32_to_half(0.333_333_34), 0x3555);
    }

    #[test]
    fn header_starts_with_the_magic() {
        let px = vec![[0.5f32, 0.25, 0.125, 1.0]; 4];
        let e = exr(2, 2, &px, true, true, ExrCompression::Zip);
        assert_eq!(&e[..4], &[0x76, 0x2F, 0x31, 0x01]);
        let raw = exr(2, 2, &px, false, false, ExrCompression::None);
        // Two lines, each: y, size, then B, G, R rows of two floats.
        assert!(raw.len() > 2 * (8 + 3 * 2 * 4));
    }
}
