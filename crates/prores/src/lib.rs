//! Apple ProRes encoding and decoding (SMPTE RDD 36, "Apple ProRes
//! Bitstream Syntax and Decoding Process"); [`decode`] reads every profile,
//! alpha and interlaced frames too.
//!
//! A frame is a header and one progressive picture split in slices of up
//! to 8 macroblocks (16×16). Each slice holds its luma and chroma blocks
//! transformed by the 8×8 DCT, quantized by the frame's matrices times the
//! slice's scale, and entropy-coded: DC differences and AC run/level pairs
//! with adaptive Rice/exp-Golomb codewords, coefficients interleaved across
//! the slice's blocks. Alpha (4444) is coded losslessly as run-lengths of
//! differences. Each slice gets the finest scale that fits its share of the
//! profile's data rate.

use bv_bitstream::BitWriter;

mod decode;

pub use decode::{DecodeError, Decoded, FrameInfo, decode, frame_info};

/// The ProRes flavors, by data rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    Proxy,
    Lt,
    Standard,
    Hq,
    P4444,
    P4444Xq,
}

impl Profile {
    /// 4:4:4 chroma (and alpha allowed).
    pub fn is_444(self) -> bool {
        matches!(self, Profile::P4444 | Profile::P4444Xq)
    }

    /// Apple's target data rate for 1920×1080 at 29.97 fps, in Mb/s.
    pub fn megabits_1080p30(self) -> f64 {
        match self {
            Profile::Proxy => 45.0,
            Profile::Lt => 102.0,
            Profile::Standard => 147.0,
            Profile::Hq => 220.0,
            Profile::P4444 => 330.0,
            Profile::P4444Xq => 500.0,
        }
    }

    /// The quantization matrix (natural order): flat for the high-rate
    /// profiles, coarser towards high frequencies for the others.
    fn matrix(self) -> [u8; 64] {
        let slope = match self {
            Profile::Proxy => 1.6,
            Profile::Lt => 1.0,
            Profile::Standard => 0.8,
            _ => 0.0,
        };
        let mut m = [4u8; 64];
        for (i, v) in m.iter_mut().enumerate() {
            let (u, w) = (i % 8, i / 8);
            *v = (4.0 + slope * (u + w) as f64).round() as u8;
        }
        m
    }
}

/// Scan order of interlaced pictures (fields).
const SCAN_INTERLACED: [usize; 64] = [
    0, 8, 1, 9, 16, 24, 17, 25, 2, 10, 3, 11, 18, 26, 19, 27, 32, 40, 33, 34, 41, 48, 56, 49, 42,
    35, 43, 50, 57, 58, 51, 59, 4, 12, 5, 6, 13, 20, 28, 21, 14, 7, 15, 22, 29, 36, 44, 37, 30, 23,
    31, 38, 45, 52, 60, 53, 46, 39, 47, 54, 61, 62, 55, 63,
];

/// Scan order of progressive pictures.
const SCAN: [usize; 64] = [
    0, 1, 8, 9, 2, 3, 10, 11, 16, 17, 24, 25, 18, 19, 26, 27, 4, 5, 12, 20, 13, 6, 7, 14, 21, 28,
    29, 22, 15, 23, 30, 31, 32, 33, 40, 48, 41, 34, 35, 42, 49, 56, 57, 50, 43, 36, 37, 44, 51, 58,
    59, 52, 45, 38, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Codebooks: Rice order in bits 7–5, exp-Golomb order in bits 4–2, the
/// switch between them in bits 1–0.
const FIRST_DC_CB: u8 = 0xB8;
const DC_CB: [u8; 7] = [0x04, 0x28, 0x28, 0x4D, 0x4D, 0x70, 0x70];
const RUN_CB: [u8; 16] = [
    0x06, 0x06, 0x05, 0x05, 0x04, 0x29, 0x29, 0x29, 0x29, 0x28, 0x28, 0x28, 0x28, 0x28, 0x28, 0x4C,
];
const LEVEL_CB: [u8; 10] = [0x04, 0x0A, 0x05, 0x06, 0x04, 0x28, 0x28, 0x28, 0x28, 0x4C];

/// Bits of the adaptive codeword of `val` in `codebook`.
fn codeword_bits(codebook: u8, val: u32) -> u32 {
    let switch_bits = (codebook & 3) as u32 + 1;
    let rice = (codebook >> 5) as u32;
    let exp = ((codebook >> 2) & 7) as u32;
    let switch_val = switch_bits << rice;
    if val >= switch_val {
        let v = val - switch_val + (1 << exp);
        let e = 31 - v.leading_zeros();
        (e - exp + switch_bits) + e + 1
    } else {
        (val >> rice) + 1 + rice
    }
}

fn codeword(w: &mut BitWriter, codebook: u8, val: u32) {
    let switch_bits = (codebook & 3) as u32 + 1;
    let rice = (codebook >> 5) as u32;
    let exp = ((codebook >> 2) & 7) as u32;
    let switch_val = switch_bits << rice;
    if val >= switch_val {
        let v = val - switch_val + (1 << exp);
        let e = 31 - v.leading_zeros();
        for _ in 0..e - exp + switch_bits {
            w.write_bit(false);
        }
        w.write_bits(v, e + 1);
    } else {
        for _ in 0..val >> rice {
            w.write_bit(false);
        }
        w.write_bit(true);
        if rice > 0 {
            w.write_bits(val & ((1 << rice) - 1), rice);
        }
    }
}

fn zigzag(v: i32) -> u32 {
    ((v << 1) ^ (v >> 31)) as u32
}

/// The 8×8 DCT at the scale ProRes codes (four times the orthonormal
/// transform), of samples centred on 512.
struct Dct {
    cos: [[f32; 8]; 8],
}

impl Dct {
    fn new() -> Dct {
        let mut cos = [[0f32; 8]; 8];
        for (u, row) in cos.iter_mut().enumerate() {
            let c = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            for (x, v) in row.iter_mut().enumerate() {
                // Each pass carries a factor 2 of the 4: (C(u)/2)·2.
                *v = c * (((2 * x + 1) * u) as f32 * std::f32::consts::PI / 16.0).cos();
            }
        }
        Dct { cos }
    }

    fn forward(&self, x: &[f32; 64]) -> [f32; 64] {
        let mut rows = [0f32; 64];
        for y in 0..8 {
            for u in 0..8 {
                let mut s = 0.0;
                for k in 0..8 {
                    s += x[y * 8 + k] * self.cos[u][k];
                }
                rows[y * 8 + u] = s * 0.5;
            }
        }
        let mut out = [0f32; 64];
        for v in 0..8 {
            for u in 0..8 {
                let mut s = 0.0;
                for k in 0..8 {
                    s += rows[k * 8 + u] * self.cos[v][k];
                }
                out[v * 8 + u] = s * 0.5;
            }
        }
        // (C(u)C(v)/4)·ΣΣ is the orthonormal DCT; ProRes codes four times it.
        for v in &mut out {
            *v *= 4.0;
        }
        out
    }
}

/// The quantization scale of a quantization index (1–224).
fn qscale(index: u8) -> u32 {
    if index > 128 {
        ((index as u32) - 96) << 2
    } else {
        index as u32
    }
}

/// One frame's planes: 10-bit Y′, Cb and Cr (chroma half as wide in 4:2:2),
/// rows top to bottom; alpha in 16 bits for 4444.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub y: &'a [u16],
    pub cb: &'a [u16],
    pub cr: &'a [u16],
    pub alpha: Option<&'a [u16]>,
}

/// A ProRes encoder for one size and rate.
#[derive(Clone, Debug)]
pub struct Encoder {
    profile: Profile,
    width: usize,
    height: usize,
    alpha: bool,
    matrix: [u8; 64],
    /// Target bytes per frame.
    target: f64,
    /// BT.709 primaries, transfer and matrix (H.273 codes).
    color: (u8, u8, u8),
    /// Interlaced frames: 1 the top field first, 2 the bottom one (0:
    /// progressive).
    interlace: u8,
    /// The coefficients' order: a frame's, or a field's.
    scan: &'static [usize; 64],
}

/// A slice's blocks after the DCT.
struct SliceData {
    planes: Vec<Vec<[f32; 64]>>,
    alpha: Vec<u16>,
}

impl Encoder {
    pub fn new(profile: Profile, width: u32, height: u32, fps: f64, alpha: bool) -> Encoder {
        let rate = profile.megabits_1080p30() * 1e6;
        let scale = (width as f64 * height as f64) / (1920.0 * 1080.0) * fps / 29.97;
        Encoder {
            profile,
            width: width as usize,
            height: height as usize,
            alpha: alpha && profile.is_444(),
            matrix: profile.matrix(),
            target: rate * scale / fps.max(1.0) / 8.0,
            color: (1, 1, 1),
            interlace: 0,
            scan: &SCAN,
        }
    }

    /// Frames of two fields (`Some(true)`: the top one first; `None`:
    /// progressive), each coded as its own picture.
    pub fn interlaced(mut self, top_first: Option<bool>) -> Encoder {
        self.interlace = match top_first {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        };
        self
    }

    fn mb_size(&self) -> (usize, usize) {
        (self.width.div_ceil(16), self.height.div_ceil(16))
    }

    /// Slices of every macroblock row: (first MB, row, MBs).
    fn slices(&self) -> Vec<(usize, usize, usize)> {
        let (mw, mh) = self.mb_size();
        let mut out = Vec::new();
        for row in 0..mh {
            let mut x = 0;
            let mut mbs = 8;
            while x < mw {
                while mw - x < mbs {
                    mbs >>= 1;
                }
                out.push((x, row, mbs));
                x += mbs;
            }
        }
        out
    }

    fn gather(&self, dct: &Dct, f: &Frame<'_>, mb_x: usize, mb_y: usize, mbs: usize) -> SliceData {
        let (w, h) = (self.width, self.height);
        let cw = if self.profile.is_444() {
            w
        } else {
            w.div_ceil(2)
        };
        let block = |plane: &[u16], pw: usize, x0: usize, y0: usize| -> [f32; 64] {
            let mut b = [0f32; 64];
            for y in 0..8 {
                let row = (y0 + y).min(h - 1) * pw;
                for x in 0..8 {
                    b[y * 8 + x] = plane[row + (x0 + x).min(pw - 1)] as f32 - 512.0;
                }
            }
            dct.forward(&b)
        };
        let py = mb_y * 16;
        let mut luma = Vec::with_capacity(mbs * 4);
        let mut cb = Vec::with_capacity(mbs * 4);
        let mut cr = Vec::with_capacity(mbs * 4);
        for i in 0..mbs {
            let px = (mb_x + i) * 16;
            for (dx, dy) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
                luma.push(block(f.y, w, px + dx, py + dy));
            }
            if self.profile.is_444() {
                // Chroma goes column by column: left top, left bottom,
                // right top, right bottom.
                for (dx, dy) in [(0, 0), (0, 8), (8, 0), (8, 8)] {
                    cb.push(block(f.cb, cw, px + dx, py + dy));
                    cr.push(block(f.cr, cw, px + dx, py + dy));
                }
            } else {
                let cx = (mb_x + i) * 8;
                for dy in [0, 8] {
                    cb.push(block(f.cb, cw, cx, py + dy));
                    cr.push(block(f.cr, cw, cx, py + dy));
                }
            }
        }
        let mut alpha = Vec::new();
        if self.alpha
            && let Some(a) = f.alpha
        {
            let x0 = mb_x * 16;
            alpha.reserve(16 * mbs * 16);
            for y in 0..16 {
                let row = (py + y).min(h - 1) * w;
                for x in 0..mbs * 16 {
                    alpha.push(a[row + (x0 + x).min(w - 1)]);
                }
            }
        }
        SliceData {
            planes: vec![luma, cb, cr],
            alpha,
        }
    }

    /// The quantized coefficients of a plane's blocks.
    fn quantize(&self, blocks: &[[f32; 64]], qs: u32) -> Vec<[i32; 64]> {
        blocks
            .iter()
            .map(|b| {
                let mut q = [0i32; 64];
                for (i, v) in b.iter().enumerate() {
                    let step = (self.matrix[i] as u32 * qs) as f32;
                    let x = v / step;
                    q[i] = if i == 0 {
                        x.round() as i32
                    } else {
                        // A slight dead zone: small coefficients are noise.
                        (x.abs() + 0.38).floor().copysign(x) as i32
                    };
                }
                q
            })
            .collect()
    }

    /// Codes (or, with `out` None, only counts) a plane's coefficients, in
    /// `scan`'s order.
    fn code_plane(
        levels: &[[i32; 64]],
        scan: &[usize; 64],
        mut out: Option<&mut BitWriter>,
    ) -> u64 {
        let mut bits = 0u64;
        let put = |cb: u8, v: u32, out: &mut Option<&mut BitWriter>| -> u64 {
            if let Some(w) = out {
                codeword(w, cb, v);
            }
            codeword_bits(cb, v) as u64
        };
        // DC: the first, then differences with a carried sign.
        let mut prev = levels[0][0];
        bits += put(FIRST_DC_CB, zigzag(prev), &mut out);
        let (mut cb, mut sign) = (5usize, 0i32);
        for b in &levels[1..] {
            let delta = b[0] - prev;
            let new_sign = delta >> 31;
            let code = zigzag((delta ^ sign) - sign);
            bits += put(DC_CB[cb], code, &mut out);
            cb = (code as usize).min(6);
            sign = new_sign;
            prev = b[0];
        }
        // AC: run/level pairs, coefficient by coefficient across blocks.
        let (mut run_cb, mut level_cb) = (RUN_CB[4], LEVEL_CB[2]);
        let mut run = 0u32;
        for &n in &scan[1..] {
            for b in levels {
                let level = b[n];
                if level == 0 {
                    run += 1;
                    continue;
                }
                let mag = level.unsigned_abs();
                bits += put(run_cb, run, &mut out);
                bits += put(level_cb, mag - 1, &mut out);
                bits += 1;
                if let Some(w) = &mut out {
                    w.write_bit(level < 0);
                }
                run_cb = RUN_CB[(run as usize).min(15)];
                level_cb = LEVEL_CB[(mag as usize).min(9)];
                run = 0;
            }
        }
        bits
    }

    /// Lossless alpha: differences and runs of repeats, in 16 bits.
    fn code_alpha(values: &[u16], w: &mut BitWriter) {
        let diff = |w: &mut BitWriter, cur: u16, prev: u16| {
            let mut d = cur.wrapping_sub(prev) as i32;
            if d >= (1 << 16) - 64 {
                d -= 1 << 16;
            }
            if !(-64..=64).contains(&d) || d == 0 {
                w.write_bit(true);
                w.write_bits(d as u32 & 0xFFFF, 16);
            } else {
                w.write_bit(false);
                w.write_bits(d.unsigned_abs() - 1, 6);
                w.write_bit(d < 0);
            }
        };
        // A run: 4 bits, or 4 zero bits and 11 more (so at most 2047).
        let run = |w: &mut BitWriter, n: u32| {
            if n > 0 {
                w.write_bit(false);
                if n < 0x10 {
                    w.write_bits(n, 4);
                } else {
                    w.write_bits(n, 15);
                }
            } else {
                w.write_bit(true);
            }
        };
        let mut prev = 0xFFFFu16;
        diff(w, values[0], prev);
        prev = values[0];
        let mut n = 0u32;
        // After the longest run the next value is a difference (maybe 0).
        let mut must_diff = false;
        for &v in &values[1..] {
            if must_diff {
                diff(w, v, prev);
                prev = v;
                must_diff = false;
            } else if v != prev {
                run(w, n);
                diff(w, v, prev);
                prev = v;
                n = 0;
            } else {
                n += 1;
                if n == 2047 {
                    run(w, n);
                    n = 0;
                    must_diff = true;
                }
            }
        }
        if n > 0 {
            run(w, n);
        }
        w.align_zero();
    }

    /// A slice: header, then each plane byte-aligned.
    fn encode_slice(&self, data: &SliceData, target: f64) -> Vec<u8> {
        // Bytes at a quantization index (header and alignment included).
        let header = if self.alpha { 8 } else { 6 };
        let size_at = |index: u8| -> f64 {
            let qs = qscale(index);
            let mut total = header as f64;
            for p in &data.planes {
                total +=
                    (Self::code_plane(&self.quantize(p, qs), self.scan, None)).div_ceil(8) as f64;
            }
            total
        };
        // The finest index whose slice fits its share.
        let (mut lo, mut hi) = (1u8, 224u8);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if size_at(mid) <= target {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let mut index = lo;
        loop {
            let qs = qscale(index);
            let mut planes = Vec::with_capacity(4);
            for p in &data.planes {
                let mut w = BitWriter::new();
                Self::code_plane(&self.quantize(p, qs), self.scan, Some(&mut w));
                w.align_zero();
                planes.push(w.into_bytes());
            }
            if self.alpha {
                let mut w = BitWriter::new();
                Self::code_alpha(&data.alpha, &mut w);
                planes.push(w.into_bytes());
            }
            // A slice's planes are counted in 16 bits.
            let fits = planes.iter().all(|p| p.len() <= 0xFFFF)
                && planes.iter().map(Vec::len).sum::<usize>() + header <= 0xFFFF;
            if !fits && index < 224 {
                index += 1;
                continue;
            }
            let mut out = Vec::with_capacity(header + planes.iter().map(Vec::len).sum::<usize>());
            out.push((header as u8) << 3);
            out.push(index);
            for p in &planes[..planes.len() - 1] {
                out.extend_from_slice(&(p.len() as u16).to_be_bytes());
            }
            for p in &planes {
                out.extend_from_slice(p);
            }
            return out;
        }
    }

    /// Encodes one frame (interlaced: its fields, each a picture of every
    /// other row, the one shown first first).
    pub fn encode(&self, f: &Frame<'_>) -> Vec<u8> {
        if self.interlace == 0 {
            return self.frame(&[self.picture(f)]);
        }
        let (w, h) = (self.width, self.height);
        let cw = if self.profile.is_444() {
            w
        } else {
            w.div_ceil(2)
        };
        let pictures: Vec<Vec<u8>> = (0..2)
            .map(|field| {
                let first = if self.interlace == 1 {
                    field
                } else {
                    1 - field
                };
                let rows = (h + 1 - first) / 2;
                let pick = |plane: &[u16], pw: usize| -> Vec<u16> {
                    (0..rows)
                        .flat_map(|r| plane[(first + 2 * r) * pw..][..pw].iter().copied())
                        .collect()
                };
                let (y, cb, cr) = (pick(f.y, w), pick(f.cb, cw), pick(f.cr, cw));
                let alpha = f.alpha.map(|a| pick(a, w));
                let half = Encoder {
                    height: rows.max(1),
                    target: self.target / 2.0,
                    scan: &SCAN_INTERLACED,
                    ..self.clone()
                };
                half.picture(&Frame {
                    y: &y,
                    cb: &cb,
                    cr: &cr,
                    alpha: alpha.as_deref(),
                })
            })
            .collect();
        self.frame(&pictures)
    }

    /// One picture: a whole frame, or a field.
    fn picture(&self, f: &Frame<'_>) -> Vec<u8> {
        let slices = self.slices();
        let (mw, mh) = self.mb_size();
        let per_mb = self.target / (mw * mh) as f64;
        let dct = Dct::new();
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(slices.len().max(1));
        let chunk = slices.len().div_ceil(threads);
        let mut coded: Vec<Vec<u8>> = vec![Vec::new(); slices.len()];
        std::thread::scope(|scope| {
            for (part, out) in slices.chunks(chunk).zip(coded.chunks_mut(chunk)) {
                let dct = &dct;
                scope.spawn(move || {
                    for ((x, y, mbs), o) in part.iter().zip(out.iter_mut()) {
                        let data = self.gather(dct, f, *x, *y, *mbs);
                        *o = self.encode_slice(&data, per_mb * *mbs as f64);
                    }
                });
            }
        });

        // Picture: header, slice sizes, slices.
        let mut picture = Vec::new();
        picture.push(8 << 3);
        picture.extend_from_slice(&[0; 4]);
        picture.extend_from_slice(&(slices.len() as u16).to_be_bytes());
        picture.push(3 << 4); // 8 macroblocks per slice
        for s in &coded {
            picture.extend_from_slice(&(s.len() as u16).to_be_bytes());
        }
        for s in &coded {
            picture.extend_from_slice(s);
        }
        let size = picture.len() as u32;
        picture[1..5].copy_from_slice(&size.to_be_bytes());
        picture
    }

    /// A frame of its `pictures`: the frame header first.
    fn frame(&self, pictures: &[Vec<u8>]) -> Vec<u8> {
        // Frame header.
        let mut h = Vec::with_capacity(148);
        h.extend_from_slice(&0u16.to_be_bytes()); // size, below
        h.push(0);
        h.push(if self.profile.is_444() || self.alpha {
            1
        } else {
            0
        });
        h.extend_from_slice(b"bnnv");
        h.extend_from_slice(&(self.width as u16).to_be_bytes());
        h.extend_from_slice(&(self.height as u16).to_be_bytes());
        // Chroma format (2: 4:2:2, 3: 4:4:4), the interlace mode.
        h.push(
            if self.profile.is_444() {
                3 << 6
            } else {
                2 << 6
            } | self.interlace << 2,
        );
        h.push(0); // aspect ratio and frame rate unknown
        h.push(self.color.0);
        h.push(self.color.1);
        h.push(self.color.2);
        h.push(if self.alpha { 2 } else { 0 }); // 16-bit alpha
        h.push(0);
        h.push(0x03); // both matrices follow
        h.extend_from_slice(&self.matrix);
        h.extend_from_slice(&self.matrix);
        let hl = h.len() as u16;
        h[..2].copy_from_slice(&hl.to_be_bytes());

        let total = 8 + h.len() + pictures.iter().map(Vec::len).sum::<usize>();
        let mut frame = Vec::with_capacity(total);
        frame.extend_from_slice(&(total as u32).to_be_bytes());
        frame.extend_from_slice(b"icpf");
        frame.extend_from_slice(&h);
        for p in pictures {
            frame.extend_from_slice(p);
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An interlaced frame (each field its own picture, in the interlaced
    /// scan) decodes back close to what went in, for either field first.
    #[test]
    fn interlaced_frames_round_trip() {
        let (w, h) = (64usize, 48usize);
        let y: Vec<u16> = (0..w * h)
            .map(|i| {
                let (x, r) = (i % w, i / w);
                // A field apart from the other: odd rows lighter.
                (300 + x * 4 + (r % 2) * 200) as u16
            })
            .collect();
        let c = vec![512u16; w.div_ceil(2) * h];
        let frame = Frame {
            y: &y,
            cb: &c,
            cr: &c,
            alpha: None,
        };
        for top in [true, false] {
            let enc =
                Encoder::new(Profile::Hq, w as u32, h as u32, 25.0, false).interlaced(Some(top));
            let data = enc.encode(&frame);
            let back = crate::decode::decode(&data).expect("decodes");
            let worst = back
                .y
                .iter()
                .zip(&y)
                .map(|(a, b)| (*a as i32 - *b as i32).abs())
                .max()
                .unwrap();
            assert!(worst < 24, "top first {top}: {worst}");
        }
    }

    #[test]
    fn codewords_count_what_they_write() {
        for cb in [
            FIRST_DC_CB,
            0x04,
            0x28,
            0x4D,
            0x70,
            0x06,
            0x05,
            0x29,
            0x4C,
            0x0A,
        ] {
            for v in [0u32, 1, 2, 3, 5, 8, 17, 100, 1000, 40_000] {
                let mut w = BitWriter::new();
                codeword(&mut w, cb, v);
                assert_eq!(w.bit_len() as u32, codeword_bits(cb, v), "cb {cb:#x} v {v}");
            }
        }
    }

    #[test]
    fn rice_part_of_a_codeword() {
        // 0x04: Rice order 0, exp-Golomb order 1, switch after 1 bit.
        let mut w = BitWriter::new();
        codeword(&mut w, 0x04, 0);
        assert_eq!(w.bit_len(), 1);
        // Value 1 switches to exp-Golomb: v = 1 - 1 + 2 = 2 → 0 then "10".
        let mut w = BitWriter::new();
        codeword(&mut w, 0x04, 1);
        assert_eq!(w.bit_len(), 3);
    }

    #[test]
    fn scan_is_a_permutation() {
        let mut seen = [false; 64];
        for &s in &SCAN {
            assert!(!seen[s]);
            seen[s] = true;
        }
    }

    #[test]
    fn slices_cover_odd_widths() {
        let e = Encoder::new(Profile::Hq, 720, 480, 30.0, false);
        let s = e.slices();
        // 45 macroblocks a row: 8×5, 4, 1.
        let row: Vec<usize> = s.iter().filter(|t| t.1 == 0).map(|t| t.2).collect();
        assert_eq!(row, vec![8, 8, 8, 8, 8, 4, 1]);
    }
}
