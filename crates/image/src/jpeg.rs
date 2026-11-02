//! Baseline JPEG (ITU-T T.81) in a JFIF file: YCbCr, 4:2:0 below quality
//! 90 and 4:4:4 from there, the example tables of Annex K scaled by
//! quality as the IJG library does.

use crate::Image;

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Annex K.1, in natural order.
const LUMA_Q: [u8; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA_Q: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

/// Annex K.3: code counts per length (1–16) and symbols.
const DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALUES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7D];
const AC_LUMA_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
    0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
    0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
    0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];
const AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
const AC_CHROMA_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0,
    0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
    0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
    0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
    0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
    0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];

/// (code, length) of every symbol of a table (Annex C).
fn codes(bits: &[u8; 16], values: &[u8]) -> [(u16, u8); 256] {
    let mut out = [(0u16, 0u8); 256];
    let mut code = 0u16;
    let mut k = 0;
    for (len, &n) in bits.iter().enumerate() {
        for _ in 0..n {
            out[values[k] as usize] = (code, len as u8 + 1);
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    out
}

struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, code: u32, len: u32) {
        self.acc = (self.acc << len) | (code & ((1 << len) - 1));
        self.n += len;
        while self.n >= 8 {
            let b = (self.acc >> (self.n - 8)) as u8;
            self.out.push(b);
            if b == 0xFF {
                self.out.push(0);
            }
            self.n -= 8;
        }
        self.acc &= (1 << self.n) - 1;
    }

    fn flush(&mut self) {
        if self.n > 0 {
            let pad = 8 - self.n;
            self.put((1 << pad) - 1, pad);
        }
    }
}

/// Bits of `v` (its magnitude category) and how they are written.
fn category(v: i32) -> (u32, u32) {
    let size = 32 - v.unsigned_abs().leading_zeros();
    // Negative values: the one's complement of the magnitude.
    let bits = if v < 0 { v - 1 } else { v } as u32 & ((1u64 << size) - 1) as u32;
    (size, bits)
}

struct Coder {
    dc: [(u16, u8); 256],
    ac: [(u16, u8); 256],
    prev: i32,
}

impl Coder {
    fn block(&mut self, out: &mut Bits, q: &[i32; 64]) {
        let (size, bits) = category(q[0] - self.prev);
        self.prev = q[0];
        let (c, l) = self.dc[size as usize];
        out.put(c as u32, l as u32);
        if size > 0 {
            out.put(bits, size);
        }
        let mut run = 0;
        for &v in &q[1..] {
            if v == 0 {
                run += 1;
                continue;
            }
            while run > 15 {
                let (c, l) = self.ac[0xF0];
                out.put(c as u32, l as u32);
                run -= 16;
            }
            let (size, bits) = category(v);
            let (c, l) = self.ac[((run << 4) | size) as usize];
            out.put(c as u32, l as u32);
            out.put(bits, size);
            run = 0;
        }
        if run > 0 {
            let (c, l) = self.ac[0];
            out.put(c as u32, l as u32);
        }
    }
}

/// Forward DCT of an 8×8 block (values centred on 0), quantized, in
/// zigzag order.
fn dct_quantize(block: &[f32; 64], cos: &[[f32; 8]; 8], qt: &[u16; 64]) -> [i32; 64] {
    let mut rows = [0f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            rows[y * 8 + u] = (0..8).map(|x| block[y * 8 + x] * cos[u][x]).sum();
        }
    }
    let mut out = [0i32; 64];
    for (z, &n) in ZIGZAG.iter().enumerate() {
        let (v, u) = (n / 8, n % 8);
        let f: f32 = (0..8).map(|y| rows[y * 8 + u] * cos[v][y]).sum();
        out[z] = (f / qt[n] as f32).round() as i32;
    }
    out
}

fn segment(out: &mut Vec<u8>, marker: u8, body: &[u8]) {
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(body);
}

pub fn jpeg(img: &Image<'_>, quality: u8) -> Vec<u8> {
    let q = quality.clamp(1, 100) as u32;
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    let table = |base: &[u8; 64]| {
        let mut t = [0u16; 64];
        for (i, b) in base.iter().enumerate() {
            t[i] = ((*b as u32 * scale + 50) / 100).clamp(1, 255) as u16;
        }
        t
    };
    let (lq, cq) = (table(&LUMA_Q), table(&CHROMA_Q));
    let sub = q < 90;
    let (w, h) = (img.width as usize, img.height as usize);

    // Full-resolution planes.
    let mut y_plane = vec![0f32; w * h];
    let mut cb = vec![0f32; w * h];
    let mut cr = vec![0f32; w * h];
    for i in 0..w * h {
        let (r, g, b) = (
            img.get8(i, 0) as f32,
            img.get8(i, 1) as f32,
            img.get8(i, 2) as f32,
        );
        y_plane[i] = 0.299 * r + 0.587 * g + 0.114 * b;
        cb[i] = -0.168_736 * r - 0.331_264 * g + 0.5 * b + 128.0;
        cr[i] = 0.5 * r - 0.418_688 * g - 0.081_312 * b + 128.0;
    }
    let (cw, ch) = if sub {
        (w.div_ceil(2), h.div_ceil(2))
    } else {
        (w, h)
    };
    let half = |p: &[f32]| -> Vec<f32> {
        let mut out = vec![0f32; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                let at = |xx: usize, yy: usize| p[yy.min(h - 1) * w + xx.min(w - 1)];
                out[y * cw + x] = (at(2 * x, 2 * y)
                    + at(2 * x + 1, 2 * y)
                    + at(2 * x, 2 * y + 1)
                    + at(2 * x + 1, 2 * y + 1))
                    / 4.0;
            }
        }
        out
    };
    if sub {
        cb = half(&cb);
        cr = half(&cr);
    }

    let mut cos = [[0f32; 8]; 8];
    for (u, row) in cos.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let cu = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            *c = 0.5 * cu * (((2 * x + 1) * u) as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    let block_at = |p: &[f32], pw: usize, ph: usize, x0: usize, y0: usize| -> [f32; 64] {
        let mut b = [0f32; 64];
        for y in 0..8 {
            for x in 0..8 {
                b[y * 8 + x] = p[(y0 + y).min(ph - 1) * pw + (x0 + x).min(pw - 1)] - 128.0;
            }
        }
        b
    };

    let mut out = Vec::with_capacity(w * h / 4 + 1024);
    out.extend_from_slice(&[0xFF, 0xD8]);
    segment(
        &mut out,
        0xE0,
        &[b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0],
    );
    for (id, t) in [(0u8, &lq), (1, &cq)] {
        let mut b = vec![id];
        b.extend(ZIGZAG.iter().map(|&n| t[n] as u8));
        segment(&mut out, 0xDB, &b);
    }
    let hv = if sub { 0x22 } else { 0x11 };
    let mut sof = vec![8];
    sof.extend_from_slice(&(h as u16).to_be_bytes());
    sof.extend_from_slice(&(w as u16).to_be_bytes());
    sof.extend_from_slice(&[3, 1, hv, 0, 2, 0x11, 1, 3, 0x11, 1]);
    segment(&mut out, 0xC0, &sof);
    for (class_id, bits, values) in [
        (0x00u8, &DC_LUMA_BITS, &DC_VALUES[..]),
        (0x10, &AC_LUMA_BITS, &AC_LUMA_VALUES[..]),
        (0x01, &DC_CHROMA_BITS, &DC_VALUES[..]),
        (0x11, &AC_CHROMA_BITS, &AC_CHROMA_VALUES[..]),
    ] {
        let mut b = vec![class_id];
        b.extend_from_slice(bits);
        b.extend_from_slice(values);
        segment(&mut out, 0xC4, &b);
    }
    segment(&mut out, 0xDA, &[3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);

    let mut bits = Bits { out, acc: 0, n: 0 };
    let mut luma = Coder {
        dc: codes(&DC_LUMA_BITS, &DC_VALUES),
        ac: codes(&AC_LUMA_BITS, &AC_LUMA_VALUES),
        prev: 0,
    };
    let chroma_codes = (
        codes(&DC_CHROMA_BITS, &DC_VALUES),
        codes(&AC_CHROMA_BITS, &AC_CHROMA_VALUES),
    );
    let mut cbc = Coder {
        dc: chroma_codes.0,
        ac: chroma_codes.1,
        prev: 0,
    };
    let mut crc = Coder {
        dc: chroma_codes.0,
        ac: chroma_codes.1,
        prev: 0,
    };
    let mcu = if sub { 16 } else { 8 };
    for my in 0..h.div_ceil(mcu) {
        for mx in 0..w.div_ceil(mcu) {
            let (x0, y0) = (mx * mcu, my * mcu);
            let luma_blocks: &[(usize, usize)] = if sub {
                &[(0, 0), (8, 0), (0, 8), (8, 8)]
            } else {
                &[(0, 0)]
            };
            for (bx, by) in luma_blocks {
                let b = block_at(&y_plane, w, h, x0 + bx, y0 + by);
                luma.block(&mut bits, &dct_quantize(&b, &cos, &lq));
            }
            let (cx, cy) = if sub { (mx * 8, my * 8) } else { (x0, y0) };
            let b = block_at(&cb, cw, ch, cx, cy);
            cbc.block(&mut bits, &dct_quantize(&b, &cos, &cq));
            let b = block_at(&cr, cw, ch, cx, cy);
            crc.block(&mut bits, &dct_quantize(&b, &cos, &cq));
        }
    }
    bits.flush();
    let mut out = bits.out;
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_tables_give_the_standard_codes() {
        let dc = codes(&DC_LUMA_BITS, &DC_VALUES);
        assert_eq!(dc[0], (0b00, 2));
        assert_eq!(dc[1], (0b010, 3));
        assert_eq!(dc[11], (0b1_1111_1110, 9));
        let ac = codes(&AC_LUMA_BITS, &AC_LUMA_VALUES);
        assert_eq!(ac[0x01], (0b00, 2));
        assert_eq!(ac[0x00], (0b1010, 4)); // EOB
        assert_eq!(ac[0xF0], (0b111_1111_1001, 11)); // ZRL
    }

    #[test]
    fn categories() {
        assert_eq!(category(0), (0, 0));
        assert_eq!(category(1), (1, 1));
        assert_eq!(category(-1), (1, 0));
        assert_eq!(category(-3).0, 2);
        assert_eq!(category(-3).1 & 3, 0);
        assert_eq!(category(255), (8, 255));
    }
}
