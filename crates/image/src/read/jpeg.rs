//! JPEG (ITU-T T.81) with Huffman coding: baseline, extended and
//! progressive; gray, YCbCr, RGB, CMYK and YCCK (Adobe); any sampling
//! factors and restart intervals; the EXIF orientation applied. Damaged or
//! cut files show what was read (the rest gray), as cameras' files can be.

use super::{Picture, ReadError, Result, corrupt, unsupported};

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// A Huffman table: a 9-bit lookup for short codes, the canonical limits
/// for the rest.
#[derive(Clone)]
struct Huffman {
    /// (symbol, length) for every 9-bit prefix; length 0 when longer.
    fast: Vec<(u8, u8)>,
    /// Largest code of each length (−1 when none), and where its symbols
    /// start in `values`.
    max_code: [i32; 18],
    offset: [i32; 17],
    values: Vec<u8>,
}

const FAST_BITS: u32 = 9;

impl Huffman {
    fn new(counts: &[u8; 16], values: &[u8]) -> Result<Huffman> {
        let mut fast = vec![(0u8, 0u8); 1 << FAST_BITS];
        let mut max_code = [-1i32; 18];
        let mut offset = [0i32; 17];
        let mut code = 0i32;
        let mut k = 0usize;
        for len in 1..=16usize {
            let n = counts[len - 1] as usize;
            offset[len] = k as i32 - code;
            for _ in 0..n {
                let Some(&v) = values.get(k) else {
                    return corrupt("JPEG Huffman table");
                };
                if len as u32 <= FAST_BITS {
                    let shift = FAST_BITS - len as u32;
                    let first = (code as usize) << shift;
                    for e in &mut fast[first..first + (1 << shift)] {
                        *e = (v, len as u8);
                    }
                }
                code += 1;
                k += 1;
            }
            if n > 0 {
                max_code[len] = code - 1;
            }
            code <<= 1;
        }
        max_code[17] = i32::MAX;
        Ok(Huffman {
            fast,
            max_code,
            offset,
            values: values.to_vec(),
        })
    }
}

/// Entropy-coded data, read MSB first past stuffed zeros; a marker ends it.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    count: u32,
    /// A marker was met: zeros from then on.
    marker: bool,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8], pos: usize) -> Bits<'a> {
        Bits {
            d,
            pos,
            acc: 0,
            count: 0,
            marker: false,
        }
    }

    fn fill(&mut self) {
        while self.count <= 56 {
            let mut byte = 0u8;
            if !self.marker {
                match self.d.get(self.pos) {
                    Some(&0xff) => match self.d.get(self.pos + 1) {
                        Some(0) => {
                            byte = 0xff;
                            self.pos += 2;
                        }
                        // Fill bytes before a marker.
                        Some(0xff) => {
                            self.pos += 1;
                            continue;
                        }
                        _ => self.marker = true,
                    },
                    Some(&b) => {
                        byte = b;
                        self.pos += 1;
                    }
                    None => self.marker = true,
                }
            }
            self.acc |= (byte as u64) << (56 - self.count);
            self.count += 8;
        }
    }

    fn peek(&mut self, n: u32) -> u32 {
        if self.count < n {
            self.fill();
        }
        (self.acc >> (64 - n)) as u32
    }

    fn skip(&mut self, n: u32) {
        self.acc <<= n;
        self.count -= n;
    }

    fn bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.peek(n);
        self.skip(n);
        v
    }

    fn bit(&mut self) -> u32 {
        self.bits(1)
    }

    fn symbol(&mut self, h: &Huffman) -> Result<u8> {
        let p = self.peek(16);
        let (v, len) = h.fast[(p >> (16 - FAST_BITS)) as usize];
        if len > 0 {
            self.skip(len as u32);
            return Ok(v);
        }
        for len in FAST_BITS as usize + 1..=16 {
            let code = (p >> (16 - len)) as i32;
            if code <= h.max_code[len] {
                self.skip(len as u32);
                let i = (code + h.offset[len]) as usize;
                return h.values.get(i).copied().ok_or_else(bad_data);
            }
        }
        Err(bad_data())
    }

    /// A value of `n` bits with its sign as T.81's EXTEND says.
    fn signed(&mut self, n: u32) -> i32 {
        if n == 0 {
            return 0;
        }
        let v = self.bits(n) as i32;
        if v < 1 << (n - 1) {
            v - (1 << n) + 1
        } else {
            v
        }
    }

    /// Past a restart marker: the bits left are dropped.
    fn restart(&mut self) {
        self.acc = 0;
        self.count = 0;
        self.marker = false;
        // To the RSTn marker and past it.
        while self.pos + 1 < self.d.len() {
            if self.d[self.pos] == 0xff && (0xd0..=0xd7).contains(&self.d[self.pos + 1]) {
                self.pos += 2;
                return;
            }
            if self.d[self.pos] == 0xff && self.d[self.pos + 1] != 0 && self.d[self.pos + 1] != 0xff
            {
                // Another marker: no restart here (a damaged file).
                return;
            }
            self.pos += 1;
        }
    }
}

fn bad_data() -> ReadError {
    ReadError::Corrupt("bad JPEG data".into())
}

#[derive(Clone)]
struct Component {
    id: u8,
    h: usize,
    v: usize,
    quant: usize,
    /// Blocks across and down, padded to whole MCUs.
    bw: usize,
    bh: usize,
    /// Coefficients of every block, in zigzag order.
    coefs: Vec<i16>,
    dc_table: usize,
    ac_table: usize,
    pred: i32,
}

struct Frame {
    width: usize,
    height: usize,
    progressive: bool,
    comps: Vec<Component>,
    hmax: usize,
    vmax: usize,
    mcus_x: usize,
    mcus_y: usize,
}

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let mut pos = 2usize;
    let mut quant = [[1u16; 64]; 4];
    let mut dc: [Option<Huffman>; 4] = [None, None, None, None];
    let mut ac: [Option<Huffman>; 4] = [None, None, None, None];
    let mut frame: Option<Frame> = None;
    let mut restart_interval = 0usize;
    let mut adobe: Option<u8> = None;
    let mut jfif = false;
    let mut orientation = 1u16;
    let mut eob_run = 0u32;
    loop {
        // The next marker (fill bytes and stray data skipped).
        while pos < d.len() && d[pos] != 0xff {
            pos += 1;
        }
        while pos < d.len() && d[pos] == 0xff {
            pos += 1;
        }
        let Some(&marker) = d.get(pos) else { break };
        pos += 1;
        if marker == 0xd9 {
            break;
        }
        if (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let len = u16::from_be_bytes([
            *d.get(pos).ok_or_else(bad_data)?,
            *d.get(pos + 1).ok_or_else(bad_data)?,
        ]) as usize;
        let seg = d
            .get(pos + 2..pos + len)
            .unwrap_or(&d[(pos + 2).min(d.len())..]);
        match marker {
            0xdb => {
                // Quantization tables (8 or 16 bits).
                let mut i = 0;
                while i < seg.len() {
                    let (prec, id) = (seg[i] >> 4, (seg[i] & 3) as usize);
                    i += 1;
                    for (k, q) in quant[id].iter_mut().enumerate() {
                        *q = if prec == 0 {
                            *seg.get(i + k).unwrap_or(&1) as u16
                        } else {
                            u16::from_be_bytes([
                                *seg.get(i + 2 * k).unwrap_or(&0),
                                *seg.get(i + 2 * k + 1).unwrap_or(&1),
                            ])
                        };
                    }
                    i += if prec == 0 { 64 } else { 128 };
                }
            }
            0xc4 => {
                let mut i = 0;
                while i + 17 <= seg.len() {
                    let (class, id) = (seg[i] >> 4, (seg[i] & 3) as usize);
                    let counts: [u8; 16] = seg[i + 1..i + 17].try_into().unwrap_or([0; 16]);
                    let n: usize = counts.iter().map(|&c| c as usize).sum();
                    let values = seg.get(i + 17..i + 17 + n).ok_or_else(bad_data)?;
                    let table = Huffman::new(&counts, values)?;
                    if class == 0 {
                        dc[id] = Some(table);
                    } else {
                        ac[id] = Some(table);
                    }
                    i += 17 + n;
                }
            }
            0xc0..=0xc2 => {
                if seg.len() < 6 {
                    return corrupt("JPEG frame header");
                }
                if seg[0] != 8 {
                    return unsupported(format!("JPEG of {} bits", seg[0]));
                }
                let height = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                let width = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                let n = seg[5] as usize;
                if width == 0 || height == 0 {
                    return unsupported("JPEG whose height comes later (DNL)");
                }
                let mut comps = Vec::new();
                for c in 0..n {
                    let at = 6 + c * 3;
                    let p = seg.get(at..at + 3).ok_or_else(bad_data)?;
                    let (h, v) = ((p[1] >> 4) as usize, (p[1] & 15) as usize);
                    if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                        return corrupt("JPEG sampling factors");
                    }
                    comps.push(Component {
                        id: p[0],
                        h,
                        v,
                        quant: (p[2] & 3) as usize,
                        bw: 0,
                        bh: 0,
                        coefs: Vec::new(),
                        dc_table: 0,
                        ac_table: 0,
                        pred: 0,
                    });
                }
                if !matches!(n, 1 | 3 | 4) {
                    return unsupported(format!("JPEG of {n} components"));
                }
                let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
                let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
                let mcus_x = width.div_ceil(8 * hmax);
                let mcus_y = height.div_ceil(8 * vmax);
                for c in &mut comps {
                    c.bw = mcus_x * c.h;
                    c.bh = mcus_y * c.v;
                    c.coefs = vec![0; c.bw * c.bh * 64];
                }
                frame = Some(Frame {
                    width,
                    height,
                    progressive: marker == 0xc2,
                    comps,
                    hmax,
                    vmax,
                    mcus_x,
                    mcus_y,
                });
            }
            0xc3 | 0xc5..=0xc7 => return unsupported("lossless or hierarchical JPEG"),
            0xc9..=0xcb | 0xcd..=0xcf => return unsupported("JPEG with arithmetic coding"),
            0xdd => {
                restart_interval = seg
                    .get(..2)
                    .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]) as usize);
            }
            0xe0 => jfif |= seg.starts_with(b"JFIF\0"),
            0xe1 => {
                if seg.starts_with(b"Exif\0\0") {
                    orientation = super::tiff_orientation(&seg[6..]).unwrap_or(1);
                }
            }
            0xee => {
                if seg.starts_with(b"Adobe") && seg.len() >= 12 {
                    adobe = Some(seg[11]);
                }
            }
            0xda => {
                let f = frame.as_mut().ok_or_else(bad_data)?;
                let n = *seg.first().ok_or_else(bad_data)? as usize;
                if !(1..=4).contains(&n) || seg.len() < 1 + n * 2 + 3 {
                    return corrupt("JPEG scan header");
                }
                let mut scan = Vec::new();
                for k in 0..n {
                    let (id, tables) = (seg[1 + k * 2], seg[2 + k * 2]);
                    let c = f
                        .comps
                        .iter()
                        .position(|c| c.id == id)
                        .ok_or_else(bad_data)?;
                    f.comps[c].dc_table = (tables >> 4) as usize & 3;
                    f.comps[c].ac_table = (tables & 15) as usize & 3;
                    scan.push(c);
                }
                let p = 1 + n * 2;
                let (ss, se) = (seg[p] as usize, seg[p + 1] as usize);
                let (ah, al) = ((seg[p + 2] >> 4) as u32, (seg[p + 2] & 15) as u32);
                let start = pos + len;
                let end = scan_data(
                    d,
                    start,
                    f,
                    &scan,
                    (ss, se.min(63), ah, al),
                    &dc,
                    &ac,
                    restart_interval,
                    &mut eob_run,
                );
                pos = end;
                continue;
            }
            _ => {}
        }
        pos += len;
    }
    let f = frame.ok_or_else(|| ReadError::Corrupt("JPEG without a frame".into()))?;
    let planes = idct_planes(&f, &quant);
    let rgba = to_rgba(&f, &planes, adobe, jfif);
    Ok(Picture::rgba8(f.width as u32, f.height as u32, rgba).orient(orientation))
}

/// Decodes one scan's data; returns where it ended.
#[allow(clippy::too_many_arguments)]
fn scan_data(
    d: &[u8],
    start: usize,
    f: &mut Frame,
    scan: &[usize],
    (ss, se, ah, al): (usize, usize, u32, u32),
    dc: &[Option<Huffman>; 4],
    ac: &[Option<Huffman>; 4],
    restart_interval: usize,
    eob_run: &mut u32,
) -> usize {
    let mut bits = Bits::new(d, start);
    for &c in scan {
        f.comps[c].pred = 0;
    }
    *eob_run = 0;
    // A scan of one component goes block by block (its own size); a scan
    // of several goes MCU by MCU.
    let single = scan.len() == 1;
    let units = if single {
        let c = &f.comps[scan[0]];
        // Only the blocks inside the image (not the MCU padding).
        let bw = (f.width * c.h).div_ceil(8 * f.hmax);
        let bh = (f.height * c.v).div_ceil(8 * f.vmax);
        bw * bh
    } else {
        f.mcus_x * f.mcus_y
    };
    let mut todo = restart_interval;
    for unit in 0..units {
        if restart_interval > 0 {
            if todo == 0 {
                bits.restart();
                for &c in scan {
                    f.comps[c].pred = 0;
                }
                *eob_run = 0;
                todo = restart_interval;
            }
            todo -= 1;
        }
        let ok = if single {
            let ci = scan[0];
            let bw = (f.width * f.comps[ci].h).div_ceil(8 * f.hmax);
            let (bx, by) = (unit % bw, unit / bw);
            block(&mut bits, f, ci, bx, by, (ss, se, ah, al), dc, ac, eob_run)
        } else {
            let (mx, my) = (unit % f.mcus_x, unit / f.mcus_x);
            let mut ok = true;
            for &ci in scan {
                let (h, v) = (f.comps[ci].h, f.comps[ci].v);
                for y in 0..v {
                    for x in 0..h {
                        ok &= block(
                            &mut bits,
                            f,
                            ci,
                            mx * h + x,
                            my * v + y,
                            (ss, se, ah, al),
                            dc,
                            ac,
                            eob_run,
                        );
                    }
                }
            }
            ok
        };
        if !ok {
            break;
        }
    }
    // Past the scan's data, to the next marker.
    let mut p = bits.pos.min(d.len());
    while p + 1 < d.len() {
        if d[p] == 0xff && d[p + 1] != 0 && !(0xd0..=0xd7).contains(&d[p + 1]) {
            break;
        }
        p += 1;
    }
    p
}

/// Decodes one block of a scan; false on bad data.
#[allow(clippy::too_many_arguments)]
fn block(
    bits: &mut Bits<'_>,
    f: &mut Frame,
    ci: usize,
    bx: usize,
    by: usize,
    (ss, se, ah, al): (usize, usize, u32, u32),
    dc: &[Option<Huffman>; 4],
    ac: &[Option<Huffman>; 4],
    eob_run: &mut u32,
) -> bool {
    let c = &mut f.comps[ci];
    if bx >= c.bw || by >= c.bh {
        return true;
    }
    let at = (by * c.bw + bx) * 64;
    let coefs = &mut c.coefs[at..at + 64];
    let progressive = f.progressive;
    if !progressive {
        let (Some(dct), Some(act)) = (&dc[c.dc_table], &ac[c.ac_table]) else {
            return false;
        };
        let Ok(t) = bits.symbol(dct) else {
            return false;
        };
        c.pred += bits.signed(t as u32);
        coefs[0] = c.pred as i16;
        let mut k = 1;
        while k < 64 {
            let Ok(rs) = bits.symbol(act) else {
                return false;
            };
            let (r, s) = ((rs >> 4) as usize, (rs & 15) as u32);
            if s == 0 {
                if r == 15 {
                    k += 16;
                    continue;
                }
                break;
            }
            k += r;
            if k > 63 {
                return false;
            }
            coefs[k] = bits.signed(s) as i16;
            k += 1;
        }
        return true;
    }
    if ss == 0 {
        // DC scans (all the components' DC in one).
        if ah == 0 {
            let Some(dct) = &dc[c.dc_table] else {
                return false;
            };
            let Ok(t) = bits.symbol(dct) else {
                return false;
            };
            c.pred += bits.signed(t as u32);
            coefs[0] = (c.pred << al) as i16;
        } else if bits.bit() != 0 {
            coefs[0] |= (1 << al) as i16;
        }
        return true;
    }
    if ah == 0 {
        // AC first scan.
        if *eob_run > 0 {
            *eob_run -= 1;
            return true;
        }
        let Some(act) = &ac[c.ac_table] else {
            return false;
        };
        let mut k = ss;
        while k <= se {
            let Ok(rs) = bits.symbol(act) else {
                return false;
            };
            let (r, s) = ((rs >> 4) as u32, (rs & 15) as u32);
            if s == 0 {
                if r < 15 {
                    *eob_run = (1 << r) - 1 + bits.bits(r);
                    break;
                }
                k += 16;
                continue;
            }
            k += r as usize;
            if k > 63 {
                return false;
            }
            coefs[k] = (bits.signed(s) << al) as i16;
            k += 1;
        }
        return true;
    }
    // AC refinement: the hardest part of T.81 (G.1.2.3).
    let p1 = (1i16) << al;
    let m1 = (-1i16) << al;
    let mut k = ss;
    if *eob_run == 0 {
        let Some(act) = &ac[c.ac_table] else {
            return false;
        };
        while k <= se {
            let Ok(rs) = bits.symbol(act) else {
                return false;
            };
            let (mut r, s) = ((rs >> 4) as i32, (rs & 15) as u32);
            let mut value = 0i16;
            if s == 0 {
                if r < 15 {
                    *eob_run = (1 << r) + bits.bits(r as u32);
                    break;
                }
            } else {
                value = if bits.bit() != 0 { p1 } else { m1 };
            }
            while k <= se {
                let z = k;
                if coefs[z] != 0 {
                    if bits.bit() != 0 && (coefs[z] & p1) == 0 {
                        coefs[z] += if coefs[z] >= 0 { p1 } else { m1 };
                    }
                } else {
                    if r == 0 {
                        if value != 0 {
                            coefs[z] = value;
                        }
                        k += 1;
                        break;
                    }
                    r -= 1;
                }
                k += 1;
            }
        }
    }
    if *eob_run > 0 {
        while k <= se {
            let z = k;
            if coefs[z] != 0 && bits.bit() != 0 && (coefs[z] & p1) == 0 {
                coefs[z] += if coefs[z] >= 0 { p1 } else { m1 };
            }
            k += 1;
        }
        *eob_run -= 1;
    }
    true
}

/// A component's samples, `bw * 8` by `bh * 8`.
struct Plane {
    w: usize,
    data: Vec<u8>,
}

/// Dequantizes and transforms every block, in parallel over rows of blocks.
fn idct_planes(f: &Frame, quant: &[[u16; 64]; 4]) -> Vec<Plane> {
    let cos = cos_table();
    f.comps
        .iter()
        .map(|c| {
            let w = c.bw * 8;
            let mut data = vec![0u8; w * c.bh * 8];
            let q = &quant[c.quant];
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(16));
            let rows_per = c.bh.div_ceil(threads).max(1);
            std::thread::scope(|s| {
                for (chunk, out) in data.chunks_mut(rows_per * 8 * w).enumerate() {
                    let cos = &cos;
                    s.spawn(move || {
                        let mut block = [0f32; 64];
                        let rows = out.len() / (8 * w);
                        for r in 0..rows {
                            let by = chunk * rows_per + r;
                            for bx in 0..c.bw {
                                let at = (by * c.bw + bx) * 64;
                                let coefs = &c.coefs[at..at + 64];
                                for k in 0..64 {
                                    block[ZIGZAG[k]] = coefs[k] as f32 * q[k] as f32;
                                }
                                idct(&mut block, cos);
                                for y in 0..8 {
                                    let row = &mut out[(r * 8 + y) * w + bx * 8..][..8];
                                    for x in 0..8 {
                                        row[x] =
                                            (block[y * 8 + x] + 128.0).round().clamp(0.0, 255.0)
                                                as u8;
                                    }
                                }
                            }
                        }
                    });
                }
            });
            Plane { w, data }
        })
        .collect()
}

/// cos((2x+1)uπ/16) · C(u)/2, by [u][x].
fn cos_table() -> [[f32; 8]; 8] {
    let mut t = [[0f32; 8]; 8];
    for (u, row) in t.iter_mut().enumerate() {
        let cu = if u == 0 {
            std::f32::consts::FRAC_1_SQRT_2
        } else {
            1.0
        };
        for (x, v) in row.iter_mut().enumerate() {
            *v = cu / 2.0 * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    t
}

/// The 8×8 inverse DCT, rows then columns (blocks often have only a few
/// coefficients: zero rows are skipped).
fn idct(b: &mut [f32; 64], cos: &[[f32; 8]; 8]) {
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        let row = &b[y * 8..y * 8 + 8];
        if row[1..].iter().all(|&v| v == 0.0) {
            let dc = row[0] * cos[0][0];
            tmp[y * 8..y * 8 + 8].fill(dc);
            continue;
        }
        for x in 0..8 {
            let mut s = 0.0;
            for u in 0..8 {
                s += row[u] * cos[u][x];
            }
            tmp[y * 8 + x] = s;
        }
    }
    for x in 0..8 {
        let mut col = [0f32; 8];
        for v in 0..8 {
            col[v] = tmp[v * 8 + x];
        }
        for y in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                s += col[v] * cos[v][y];
            }
            b[y * 8 + x] = s;
        }
    }
}

/// Row `y` of the image's size from a component's samples, with libjpeg's
/// "fancy" (triangle) upsampling where they are halved: the two nearest
/// samples at 3/4 and 1/4.
fn upsample_row(f: &Frame, c: &Component, p: &Plane, y: usize, out: &mut [f32]) {
    let (sx, sy) = (f.hmax / c.h, f.vmax / c.v);
    let ph = p.data.len() / p.w;
    let row = |yy: isize| -> &[u8] {
        let yy = yy.clamp(0, ph as isize - 1) as usize;
        &p.data[yy * p.w..yy * p.w + p.w]
    };
    let w = out.len();
    if sy > 2 || sx > 2 {
        // Rare factors: the nearest sample.
        let r = row((y / sy) as isize);
        for (x, o) in out.iter_mut().enumerate() {
            *o = r[(x / sx).min(p.w - 1)] as f32;
        }
        return;
    }
    // The vertical blend first (rows at 3/4 and 1/4), then the horizontal.
    let cy = (y / sy) as isize;
    let near = row(cy);
    let far = if sy == 2 {
        row(if y.is_multiple_of(2) { cy - 1 } else { cy + 1 })
    } else {
        near
    };
    let wy = if sy == 2 { 0.75 } else { 1.0 };
    let col = |i: usize| near[i] as f32 * wy + far[i] as f32 * (1.0 - wy);
    if sx == 1 {
        for (x, o) in out.iter_mut().enumerate() {
            *o = col(x);
        }
        return;
    }
    let last = p.w - 1;
    for (x, o) in out.iter_mut().enumerate().take(w) {
        let cx = x / 2;
        let nx = if x % 2 == 0 {
            cx.saturating_sub(1)
        } else {
            (cx + 1).min(last)
        };
        *o = col(cx.min(last)) * 0.75 + col(nx) * 0.25;
    }
}

fn to_rgba(f: &Frame, planes: &[Plane], adobe: Option<u8>, jfif: bool) -> Vec<u8> {
    let (w, h) = (f.width, f.height);
    let mut out = vec![255u8; w * h * 4];
    let n = f.comps.len();
    // Three components are YCbCr unless an Adobe marker says RGB or the
    // components are named R, G, B.
    let rgb = n == 3 && !jfif && (adobe == Some(0) || f.comps.iter().map(|c| c.id).eq(*b"RGB"));
    let ycck = n == 4 && adobe == Some(2);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(16));
    let rows_per = h.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (chunk, out) in out.chunks_mut(rows_per * w * 4).enumerate() {
            s.spawn(move || {
                let rows = out.len() / (w * 4);
                let mut lines = vec![vec![0f32; w]; n];
                for r in 0..rows {
                    let y = chunk * rows_per + r;
                    for (i, line) in lines.iter_mut().enumerate() {
                        upsample_row(f, &f.comps[i], &planes[i], y, line);
                    }
                    for x in 0..w {
                        let v = [
                            lines[0][x],
                            lines.get(1).map_or(0.0, |l| l[x]),
                            lines.get(2).map_or(0.0, |l| l[x]),
                            lines.get(3).map_or(0.0, |l| l[x]),
                        ];
                        let px = &mut out[(r * w + x) * 4..][..4];
                        let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                        match n {
                            1 => {
                                let g = clamp(v[0]);
                                px[..3].copy_from_slice(&[g, g, g]);
                            }
                            3 if rgb => {
                                px[..3].copy_from_slice(&[clamp(v[0]), clamp(v[1]), clamp(v[2])])
                            }
                            3 => {
                                let [r, g, b] = ycc(v[0], v[1], v[2]);
                                px[..3].copy_from_slice(&[clamp(r), clamp(g), clamp(b)]);
                            }
                            _ => {
                                // Adobe's CMYK is stored inverted.
                                let [c, m, yy] = if ycck {
                                    ycc(v[0], v[1], v[2])
                                } else {
                                    [v[0], v[1], v[2]]
                                };
                                let k = v[3] / 255.0;
                                px[..3].copy_from_slice(&[
                                    clamp(c * k),
                                    clamp(m * k),
                                    clamp(yy * k),
                                ]);
                            }
                        }
                    }
                }
            });
        }
    });
    out
}

/// JFIF's YCbCr (BT.601, full range) to RGB.
fn ycc(y: f32, cb: f32, cr: f32) -> [f32; 3] {
    let (cb, cr) = (cb - 128.0, cr - 128.0);
    [
        y + 1.402 * cr,
        y - 0.344_136 * cb - 0.714_136 * cr,
        y + 1.772 * cb,
    ]
}

#[cfg(test)]
mod tests {
    use crate::{Image, Pixels, jpeg, read::PictureData};

    #[test]
    fn what_the_writer_wrote_comes_back_close() {
        let (w, h) = (37usize, 21usize);
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 6) as u8, (y * 11) as u8, ((x + y) * 3) as u8, 255]
            })
            .collect();
        for quality in [75, 95] {
            let img = Image {
                width: w as u32,
                height: h as u32,
                pixels: Pixels::Rgba8(&px),
                alpha: false,
            };
            let p = super::read(&jpeg(&img, quality)).unwrap();
            assert_eq!((p.width, p.height), (w as u32, h as u32));
            let PictureData::Rgba8(got) = p.data else {
                panic!()
            };
            let err: f64 = got
                .iter()
                .zip(&px)
                .map(|(a, b)| (*a as f64 - *b as f64).abs())
                .sum::<f64>()
                / got.len() as f64;
            assert!(err < 4.0, "quality {quality}: mean error {err}");
        }
    }
}
