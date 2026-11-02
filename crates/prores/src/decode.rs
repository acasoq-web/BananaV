//! ProRes decoding (SMPTE RDD 36): the inverse of the encoder — frame and
//! picture headers, slices of luma and chroma blocks (adaptive DC and
//! run/level codewords, the frame's matrices times the slice's scale, the
//! inverse DCT), lossless alpha, and interlaced frames (two fields).

use bv_bitstream::BitReader;

use crate::{DC_CB, FIRST_DC_CB, LEVEL_CB, RUN_CB, SCAN, qscale};

/// Scan order of interlaced pictures (fields).
const SCAN_INTERLACED: [usize; 64] = [
    0, 8, 1, 9, 16, 24, 17, 25, 2, 10, 3, 11, 18, 26, 19, 27, 32, 40, 33, 34, 41, 48, 56, 49, 42,
    35, 43, 50, 57, 58, 51, 59, 4, 12, 5, 6, 13, 20, 28, 21, 14, 7, 15, 22, 29, 36, 44, 37, 30, 23,
    31, 38, 45, 52, 60, 53, 46, 39, 47, 54, 61, 62, 55, 63,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Unsupported(String),
    Corrupt(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Unsupported(s) => write!(f, "unsupported ProRes: {s}"),
            DecodeError::Corrupt(s) => write!(f, "damaged ProRes frame: {s}"),
        }
    }
}

impl std::error::Error for DecodeError {}

fn corrupt<T>(what: &str) -> Result<T, DecodeError> {
    Err(DecodeError::Corrupt(what.into()))
}

/// A decoded frame: 10-bit Y′CbCr planes (chroma half as wide in 4:2:2),
/// rows top to bottom, and 16-bit alpha when the frame has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// 4:4:4 chroma (else 4:2:2).
    pub chroma444: bool,
    pub y: Vec<u16>,
    pub cb: Vec<u16>,
    pub cr: Vec<u16>,
    pub alpha: Option<Vec<u16>>,
    /// The H.273 codes of the primaries, transfer and matrix (0: unknown).
    pub color: (u8, u8, u8),
}

/// The frame header's facts.
struct Header {
    width: usize,
    height: usize,
    chroma444: bool,
    /// 0 progressive, 1 top field first, 2 bottom field first.
    interlace: u8,
    /// Alpha bits: 0 none, 8 or 16.
    alpha_bits: u32,
    qmat_luma: [u16; 64],
    qmat_chroma: [u16; 64],
    color: (u8, u8, u8),
}

fn be16(d: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn be32(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// What a ProRes frame's header says of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    /// 4:4:4 chroma (else 4:2:2).
    pub chroma444: bool,
    /// 0 progressive, 1 top field first, 2 bottom field first.
    pub interlace: u8,
    /// It carries an alpha channel.
    pub alpha: bool,
}

/// The header of one ProRes frame (as stored in a MOV sample), without
/// decoding it; `None` when it is not one.
pub fn frame_info(frame: &[u8]) -> Option<FrameInfo> {
    if frame.get(4..8) != Some(b"icpf") {
        return None;
    }
    let d = &frame[8..];
    let width = be16(d, 8)?;
    let height = be16(d, 10)?;
    let flags = *d.get(12)?;
    let alpha_info = *d.get(17)? & 15;
    (width > 0 && height > 0).then_some(FrameInfo {
        width: width as u32,
        height: height as u32,
        chroma444: flags & 0xc0 == 0xc0,
        interlace: (flags >> 2) & 3,
        alpha: matches!(alpha_info, 1 | 2),
    })
}

/// Decodes one ProRes frame (as stored in a MOV sample).
pub fn decode(frame: &[u8]) -> Result<Decoded, DecodeError> {
    let bad = || DecodeError::Corrupt("frame header".into());
    if frame.get(4..8) != Some(b"icpf") {
        return corrupt("no ProRes frame signature");
    }
    let d = &frame[8..];
    let hdr_size = be16(d, 0).ok_or_else(bad)?;
    let version = be16(d, 2).ok_or_else(bad)?;
    if version > 1 {
        return Err(DecodeError::Unsupported(format!("frame version {version}")));
    }
    let width = be16(d, 8).ok_or_else(bad)?;
    let height = be16(d, 10).ok_or_else(bad)?;
    let flags = *d.get(12).ok_or_else(bad)?;
    let alpha_info = *d.get(17).ok_or_else(bad)? & 15;
    let qflags = *d.get(19).ok_or_else(bad)?;
    if width == 0 || height == 0 {
        return corrupt("frame size");
    }
    let mut qmat_luma = [4u16; 64];
    let mut qmat_chroma = [4u16; 64];
    let mut at = 20;
    if qflags & 2 != 0 {
        for (i, q) in qmat_luma.iter_mut().enumerate() {
            *q = *d.get(at + i).ok_or_else(bad)? as u16;
        }
        at += 64;
    }
    if qflags & 1 != 0 {
        for (i, q) in qmat_chroma.iter_mut().enumerate() {
            *q = *d.get(at + i).ok_or_else(bad)? as u16;
        }
    } else {
        qmat_chroma = qmat_luma;
    }
    let h = Header {
        width,
        height,
        chroma444: flags & 0xc0 == 0xc0,
        interlace: (flags >> 2) & 3,
        alpha_bits: match alpha_info {
            1 => 8,
            2 => 16,
            _ => 0,
        },
        qmat_luma,
        qmat_chroma,
        color: (
            *d.get(14).ok_or_else(bad)?,
            *d.get(15).ok_or_else(bad)?,
            *d.get(16).ok_or_else(bad)?,
        ),
    };
    let cw = if h.chroma444 {
        width
    } else {
        width.div_ceil(2)
    };
    let mut out = Decoded {
        width: width as u32,
        height: height as u32,
        chroma444: h.chroma444,
        y: vec![512; width * height],
        cb: vec![512; cw * height],
        cr: vec![512; cw * height],
        alpha: (h.alpha_bits > 0).then(|| vec![u16::MAX; width * height]),
        color: h.color,
    };
    let mut pic = d.get(hdr_size..).ok_or_else(bad)?;
    let fields = if h.interlace == 0 { 1 } else { 2 };
    for field in 0..fields {
        // The field's first row: the top field takes the even rows.
        let first_row = match (h.interlace, field) {
            (0, _) => 0,
            (1, f) => f,
            (_, f) => 1 - f,
        };
        let used = picture(pic, &h, first_row, fields, &mut out)?;
        pic = pic.get(used..).unwrap_or(&[]);
    }
    Ok(out)
}

/// A slice: where it sits and its bytes.
struct Slice<'a> {
    mb_x: usize,
    mb_y: usize,
    mbs: usize,
    data: &'a [u8],
}

/// Decodes one picture (a whole frame, or a field: every `step`-th row from
/// `first_row`); returns the bytes it took.
fn picture(
    p: &[u8],
    h: &Header,
    first_row: usize,
    step: usize,
    out: &mut Decoded,
) -> Result<usize, DecodeError> {
    let bad = || DecodeError::Corrupt("picture header".into());
    let hdr = (*p.first().ok_or_else(bad)? >> 3) as usize;
    let size = be32(p, 1).ok_or_else(bad)?;
    let log2_mbs = (*p.get(7).ok_or_else(bad)? >> 4) as usize;
    if hdr < 8 || log2_mbs > 3 || size > p.len() {
        return corrupt("picture header");
    }
    let rows = (h.height + step - 1 - first_row) / step;
    let mb_w = h.width.div_ceil(16);
    let mb_h = rows.div_ceil(16);
    // The slice count written is not trusted (QuickTime ignores it).
    let per_row = (mb_w >> log2_mbs) + (mb_w & ((1 << log2_mbs) - 1)).count_ones() as usize;
    let count = per_row * mb_h;
    let index = p.get(hdr..hdr + count * 2).ok_or_else(bad)?;
    let mut data_at = hdr + count * 2;
    let mut slices = Vec::with_capacity(count);
    let (mut x, mut y) = (0usize, 0usize);
    let mut mbs = 1usize << log2_mbs;
    for i in 0..count {
        let len = be16(index, i * 2).ok_or_else(bad)?;
        while mb_w - x < mbs {
            mbs >>= 1;
        }
        let data = p.get(data_at..data_at + len).ok_or_else(bad)?;
        slices.push(Slice {
            mb_x: x,
            mb_y: y,
            mbs,
            data,
        });
        data_at += len;
        x += mbs;
        if x == mb_w {
            x = 0;
            y += 1;
            mbs = 1 << log2_mbs;
        }
    }
    let scan = if h.interlace == 0 {
        &SCAN
    } else {
        &SCAN_INTERLACED
    };
    // Slices decode apart: in parallel, each into its own pixels, then laid
    // into the frame.
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(slices.len().max(1));
    let chunk = slices.len().div_ceil(threads);
    let mut decoded: Vec<Option<SlicePixels>> = vec![None; slices.len()];
    let mut first_error = None;
    std::thread::scope(|s| {
        let handles: Vec<_> = slices
            .chunks(chunk)
            .zip(decoded.chunks_mut(chunk))
            .map(|(part, dst)| {
                s.spawn(move || {
                    let mut err = None;
                    for (slice, d) in part.iter().zip(dst.iter_mut()) {
                        match decode_slice(slice, h, scan) {
                            Ok(px) => *d = Some(px),
                            Err(e) => err = Some(e),
                        }
                    }
                    err
                })
            })
            .collect();
        for handle in handles {
            if let Ok(Some(e)) = handle.join() {
                first_error.get_or_insert(e);
            }
        }
    });
    for (slice, px) in slices.iter().zip(decoded) {
        // A damaged slice stays gray; the rest of the picture shows.
        if let Some(px) = px {
            place(slice, &px, h, first_row, step, rows, out);
        }
    }
    if let Some(e) = first_error
        && slices.is_empty()
    {
        return Err(e);
    }
    Ok(size)
}

/// A slice's samples: luma 16 rows × mbs·16, chroma 16 rows × its width,
/// alpha like luma.
#[derive(Clone)]
struct SlicePixels {
    y: Vec<u16>,
    cb: Vec<u16>,
    cr: Vec<u16>,
    alpha: Option<Vec<u16>>,
}

fn decode_slice(s: &Slice<'_>, h: &Header, scan: &[usize; 64]) -> Result<SlicePixels, DecodeError> {
    let d = s.data;
    let bad = || DecodeError::Corrupt("slice header".into());
    let hdr = (*d.first().ok_or_else(bad)? >> 3) as usize;
    let qs = qscale((*d.get(1).ok_or_else(bad)?).clamp(1, 224)) as i32;
    let y_size = be16(d, 2).ok_or_else(bad)?;
    let u_size = be16(d, 4).ok_or_else(bad)?;
    let rest = d.len().saturating_sub(hdr + y_size + u_size);
    let v_size = if hdr > 7 {
        be16(d, 6).ok_or_else(bad)?
    } else {
        rest
    };
    let a_size = d.len().saturating_sub(hdr + y_size + u_size + v_size);
    let mut at = hdr;
    let mut part = |n: usize| -> Result<&[u8], DecodeError> {
        let p = d.get(at..at + n).ok_or_else(bad)?;
        at += n;
        Ok(p)
    };
    let (yd, ud, vd) = (part(y_size)?, part(u_size)?, part(v_size)?);
    let ad = part(a_size)?;
    let mbs = s.mbs;
    let luma_blocks = mbs * 4;
    let chroma_blocks = if h.chroma444 { mbs * 4 } else { mbs * 2 };
    let y_coefs = coefficients(yd, luma_blocks, scan)?;
    let u_coefs = coefficients(ud, chroma_blocks, scan)?;
    let v_coefs = coefficients(vd, chroma_blocks, scan)?;
    let lw = mbs * 16;
    let cw = if h.chroma444 { lw } else { mbs * 8 };
    let mut px = SlicePixels {
        y: vec![0; lw * 16],
        cb: vec![0; cw * 16],
        cr: vec![0; cw * 16],
        alpha: None,
    };
    // Luma: each macroblock's blocks top left, top right, bottom left,
    // bottom right.
    for (b, c) in y_coefs.iter().enumerate() {
        let (mb, k) = (b / 4, b % 4);
        let (bx, by) = (mb * 16 + (k % 2) * 8, (k / 2) * 8);
        put_block(c, &h.qmat_luma, qs, &mut px.y, lw, bx, by);
    }
    for (coefs, plane) in [(&u_coefs, &mut px.cb), (&v_coefs, &mut px.cr)] {
        for (b, c) in coefs.iter().enumerate() {
            let (bx, by) = if h.chroma444 {
                // Column by column: left top, left bottom, right top, right
                // bottom.
                let (mb, k) = (b / 4, b % 4);
                (mb * 16 + (k / 2) * 8, (k % 2) * 8)
            } else {
                let (mb, k) = (b / 2, b % 2);
                (mb * 8, k * 8)
            };
            put_block(c, &h.qmat_chroma, qs, plane, cw, bx, by);
        }
    }
    if h.alpha_bits > 0 && !ad.is_empty() {
        px.alpha = Some(alpha(ad, lw * 16, h.alpha_bits));
    }
    Ok(px)
}

/// Lays a slice's samples into the frame (rows of the picture are every
/// `step`-th row of the frame from `first_row`).
fn place(
    s: &Slice<'_>,
    px: &SlicePixels,
    h: &Header,
    first_row: usize,
    step: usize,
    rows: usize,
    out: &mut Decoded,
) {
    let (w, ch_w) = (
        h.width,
        if h.chroma444 {
            h.width
        } else {
            h.width.div_ceil(2)
        },
    );
    let lw = s.mbs * 16;
    let cw = if h.chroma444 { lw } else { s.mbs * 8 };
    let (x0, cx0) = (
        s.mb_x * 16,
        if h.chroma444 { s.mb_x * 16 } else { s.mb_x * 8 },
    );
    for r in 0..16 {
        let pr = s.mb_y * 16 + r;
        if pr >= rows {
            break;
        }
        let fr = first_row + pr * step;
        if fr >= h.height {
            break;
        }
        let n = lw.min(w.saturating_sub(x0));
        out.y[fr * w + x0..fr * w + x0 + n].copy_from_slice(&px.y[r * lw..r * lw + n]);
        let cn = cw.min(ch_w.saturating_sub(cx0));
        out.cb[fr * ch_w + cx0..fr * ch_w + cx0 + cn].copy_from_slice(&px.cb[r * cw..r * cw + cn]);
        out.cr[fr * ch_w + cx0..fr * ch_w + cx0 + cn].copy_from_slice(&px.cr[r * cw..r * cw + cn]);
        if let (Some(a), Some(dst)) = (&px.alpha, &mut out.alpha) {
            dst[fr * w + x0..fr * w + x0 + n].copy_from_slice(&a[r * lw..r * lw + n]);
        }
    }
}

/// Reads an adaptive codeword (Rice, then exp-Golomb past the switch).
fn codeword(r: &mut BitReader<'_>, codebook: u8) -> Option<u32> {
    let switch_bits = (codebook & 3) as u32;
    let rice = (codebook >> 5) as u32;
    let exp = ((codebook >> 2) & 7) as u32;
    // Leading zeros.
    let mut q = 0u32;
    while !r.read_bit().ok()? {
        q += 1;
        if q > 32 {
            return None;
        }
    }
    if q > switch_bits {
        // exp-Golomb: the value's bits after the leading zeros and its
        // leading 1.
        let bits = exp + q - switch_bits - 1;
        let v = (1u64 << bits) | r.read_bits_u64(bits).ok()?;
        Some((v - (1 << exp) + (((switch_bits + 1) as u64) << rice)) as u32)
    } else if rice > 0 {
        Some((q << rice) + r.read_bits(rice).ok()?)
    } else {
        Some(q)
    }
}

/// A plane's quantized coefficients for `blocks` blocks (natural order).
fn coefficients(
    d: &[u8],
    blocks: usize,
    scan: &[usize; 64],
) -> Result<Vec<[i32; 64]>, DecodeError> {
    let mut out = vec![[0i32; 64]; blocks];
    if blocks == 0 || d.is_empty() {
        return Ok(out);
    }
    let mut r = BitReader::new(d);
    let bad = || DecodeError::Corrupt("coefficients".into());
    // DC: the first, then differences with a carried sign.
    let code = codeword(&mut r, FIRST_DC_CB).ok_or_else(bad)?;
    let mut prev = ((code >> 1) as i32) ^ -((code & 1) as i32);
    out[0][0] = prev;
    let mut code = 5u32;
    let mut sign = 0i32;
    for b in out.iter_mut().skip(1) {
        code = codeword(&mut r, DC_CB[(code as usize).min(6)]).ok_or_else(bad)?;
        if code != 0 {
            sign ^= -((code & 1) as i32);
        } else {
            sign = 0;
        }
        prev += (((code as i32) + 1) >> 1 ^ sign) - sign;
        b[0] = prev;
    }
    // AC: run/level pairs, coefficient by coefficient across the blocks.
    let log2 = blocks.trailing_zeros() as usize;
    if !blocks.is_power_of_two() {
        return corrupt("block count");
    }
    let mask = blocks - 1;
    let max = 64 << log2;
    let (mut run, mut level) = (4u32, 2u32);
    let mut pos = mask;
    loop {
        // The rest is padding when only zero bits are left.
        let left = r.bits_left();
        if left == 0 {
            break;
        }
        if left < 32 && r.peek_bits(left as u32).unwrap_or(0) == 0 {
            break;
        }
        run = codeword(&mut r, RUN_CB[(run as usize).min(15)]).ok_or_else(bad)?;
        pos += run as usize + 1;
        if pos >= max {
            return corrupt("coefficient position");
        }
        level = codeword(&mut r, LEVEL_CB[(level as usize).min(9)]).ok_or_else(bad)? + 1;
        let negative = r.read_bit().map_err(|_| bad())?;
        let v = if negative {
            -(level as i32)
        } else {
            level as i32
        };
        out[pos & mask][scan[pos >> log2]] = v;
    }
    Ok(out)
}

/// Dequantizes a block, inverse-transforms it and puts its 10-bit samples
/// at (`x`, `y`) of a plane `stride` wide.
#[allow(clippy::needless_range_loop)]
fn put_block(
    c: &[i32; 64],
    qmat: &[u16; 64],
    qs: i32,
    plane: &mut [u16],
    stride: usize,
    x: usize,
    y: usize,
) {
    let cos = idct_table();
    let mut f = [0f32; 64];
    for i in 0..64 {
        // The encoder codes four times the orthonormal DCT.
        f[i] = (c[i] * qmat[i] as i32 * qs) as f32 * 0.25;
    }
    // Rows, then columns (orthonormal: C(u)/2 each way).
    let mut tmp = [0f32; 64];
    for v in 0..8 {
        let row = &f[v * 8..v * 8 + 8];
        if row[1..].iter().all(|&a| a == 0.0) {
            tmp[v * 8..v * 8 + 8].fill(row[0] * cos[0][0]);
            continue;
        }
        for xx in 0..8 {
            let mut s = 0.0;
            for u in 0..8 {
                s += row[u] * cos[u][xx];
            }
            tmp[v * 8 + xx] = s;
        }
    }
    for xx in 0..8 {
        for yy in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                s += tmp[v * 8 + xx] * cos[v][yy];
            }
            let at = (y + yy) * stride + x + xx;
            if let Some(p) = plane.get_mut(at) {
                *p = (s + 512.0).round().clamp(0.0, 1023.0) as u16;
            }
        }
    }
}

/// C(u)/2 · cos((2x+1)uπ/16), by [u][x].
fn idct_table() -> &'static [[f32; 8]; 8] {
    use std::sync::OnceLock;
    static T: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [[0f32; 8]; 8];
        for (u, row) in t.iter_mut().enumerate() {
            let c = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            for (x, v) in row.iter_mut().enumerate() {
                *v = c / 2.0 * (((2 * x + 1) * u) as f32 * std::f32::consts::PI / 16.0).cos();
            }
        }
        t
    })
}

/// Lossless alpha: differences and runs of repeats, as 16-bit values.
fn alpha(d: &[u8], count: usize, bits: u32) -> Vec<u16> {
    let mut out = Vec::with_capacity(count);
    let mut r = BitReader::new(d);
    let mask = ((1u32 << bits) - 1) as i32;
    let mut val = mask;
    let to16 = |v: i32| -> u16 {
        if bits == 16 {
            v as u16
        } else {
            (v as u16) << 8 | v as u16
        }
    };
    'all: while out.len() < count {
        loop {
            let escape = match r.read_bit() {
                Ok(b) => b,
                Err(_) => break 'all,
            };
            let delta = if escape {
                r.read_bits(bits).unwrap_or(0) as i32
            } else {
                let v = r.read_bits(if bits == 16 { 7 } else { 4 }).unwrap_or(0) as i32;
                let magnitude = (v + 2) >> 1;
                if v & 1 != 0 { -magnitude } else { magnitude }
            };
            val = (val + delta) & mask;
            out.push(to16(val));
            if out.len() >= count {
                break 'all;
            }
            // A 1 next: another value; a 0: a run of repeats.
            match r.read_bit() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(_) => break 'all,
            }
        }
        let mut n = r.read_bits(4).unwrap_or(0) as usize;
        if n == 0 {
            n = r.read_bits(11).unwrap_or(0) as usize;
        }
        let n = n.min(count - out.len());
        out.extend(std::iter::repeat_n(to16(val), n));
    }
    out.resize(count, to16(val));
    out
}
