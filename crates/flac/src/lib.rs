//! FLAC writing from the format specification (RFC 9639).
//!
//! Blocks of 4096 samples; each channel is predicted by the best of a
//! fixed polynomial or an LPC filter (Levinson–Durbin on a Tukey-windowed
//! autocorrelation, order up to 12), stereo by the best of left/right,
//! left/side, side/right or mid/side, and the residual is Rice-coded in
//! partitions. The STREAMINFO carries the MD5 of the audio.

use std::io::{self, Seek, SeekFrom, Write};

use bv_bitstream::BitWriter;

mod md5;

use md5::Md5;

const BLOCK: usize = 4096;
const MAX_LPC_ORDER: usize = 12;
const MAX_PARTITION_ORDER: u32 = 8;

/// CRC-8 (x^8 + x^2 + x + 1) of a frame header.
fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// CRC-16 (x^16 + x^15 + x^2 + 1) of a frame.
fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// A frame number in FLAC's UTF-8-like code.
fn utf8_number(w: &mut BitWriter, v: u64) {
    if v < 0x80 {
        w.write_bits(v as u32, 8);
        return;
    }
    let mut n = 2;
    while v >= 1u64 << (5 * n + 1) {
        n += 1;
    }
    let lead = (0xFF00u32 >> n) & 0xFF;
    w.write_bits(lead | (v >> (6 * (n - 1))) as u32, 8);
    for i in (0..n - 1).rev() {
        w.write_bits(0x80 | ((v >> (6 * i)) & 0x3F) as u32, 8);
    }
}

/// How a channel's residual is Rice-coded.
#[derive(Clone, Debug, Default)]
struct Rice {
    order: u32,
    params: Vec<u32>,
    /// 5-bit parameters (some above 14).
    wide: bool,
    bits: u64,
}

fn zigzag(r: i64) -> u64 {
    ((r << 1) ^ (r >> 63)) as u64
}

/// The cheapest partitioning of `residual` (whose first `skip` samples of
/// the block are warm-up samples).
fn plan_rice(residual: &[i64], block: usize, skip: usize) -> Rice {
    let mut best: Option<Rice> = None;
    for order in 0..=MAX_PARTITION_ORDER {
        let parts = 1usize << order;
        if !block.is_multiple_of(parts) || block / parts <= skip {
            break;
        }
        let size = block / parts;
        let mut params = Vec::with_capacity(parts);
        let mut bits = 2 + 4u64;
        let mut wide = false;
        let mut start = 0;
        for p in 0..parts {
            let len = if p == 0 { size - skip } else { size };
            let slice = &residual[start..start + len];
            start += len;
            let sum: u64 = slice.iter().map(|r| zigzag(*r)).sum();
            // k from the mean, then the cheaper of its neighbours.
            let mean = sum / len.max(1) as u64;
            let guess = 64 - mean.leading_zeros();
            let cost =
                |k: u32| -> u64 { slice.iter().map(|r| (zigzag(*r) >> k) + 1 + k as u64).sum() };
            let mut k = guess.min(30);
            let mut c = cost(k);
            for cand in [k.saturating_sub(1), (k + 1).min(30)] {
                let cc = cost(cand);
                if cc < c {
                    c = cc;
                    k = cand;
                }
            }
            wide |= k > 14;
            params.push(k);
            bits += c;
        }
        bits += parts as u64 * if wide { 5 } else { 4 };
        if best.as_ref().is_none_or(|b| bits < b.bits) {
            best = Some(Rice {
                order,
                params,
                wide,
                bits,
            });
        }
    }
    best.unwrap_or_default()
}

#[derive(Clone, Debug)]
enum Kind {
    Constant,
    Verbatim,
    Fixed(usize),
    Lpc {
        coefs: Vec<i32>,
        precision: u32,
        shift: u32,
    },
}

/// The chosen coding of one channel of a block.
#[derive(Clone, Debug)]
struct Subframe {
    kind: Kind,
    residual: Vec<i64>,
    rice: Rice,
    bits: u64,
}

fn fixed_residual(x: &[i64], order: usize) -> Vec<i64> {
    (order..x.len())
        .map(|i| match order {
            0 => x[i],
            1 => x[i] - x[i - 1],
            2 => x[i] - 2 * x[i - 1] + x[i - 2],
            3 => x[i] - 3 * x[i - 1] + 3 * x[i - 2] - x[i - 3],
            _ => x[i] - 4 * x[i - 1] + 6 * x[i - 2] - 4 * x[i - 3] + x[i - 4],
        })
        .collect()
}

/// Levinson–Durbin: the LPC coefficients of every order up to `max` and
/// their prediction errors.
fn levinson(r: &[f64], max: usize) -> Vec<(Vec<f64>, f64)> {
    let mut out = Vec::with_capacity(max);
    let mut a: Vec<f64> = Vec::new();
    let mut err = r[0];
    for m in 0..max {
        if err <= 0.0 {
            break;
        }
        let mut acc = r[m + 1];
        for (j, aj) in a.iter().enumerate() {
            acc -= aj * r[m - j];
        }
        let k = acc / err;
        let mut next = vec![0.0; m + 1];
        for j in 0..m {
            next[j] = a[j] - k * a[m - 1 - j];
        }
        next[m] = k;
        a = next;
        err *= 1.0 - k * k;
        out.push((a.clone(), err));
    }
    out
}

/// Quantized coefficients (libFLAC's method: error feedback).
fn quantize(lp: &[f64], precision: u32) -> Option<(Vec<i32>, u32)> {
    let cmax = lp.iter().fold(0.0f64, |m, c| m.max(c.abs()));
    if cmax <= 0.0 || !cmax.is_finite() {
        return None;
    }
    let log2 = cmax.log2().floor() as i32 + 1;
    let shift = (precision as i32 - 1 - log2).clamp(0, 15) as u32;
    let qmax = (1i32 << (precision - 1)) - 1;
    let qmin = -(1i32 << (precision - 1));
    let mut error = 0.0;
    let mut q = Vec::with_capacity(lp.len());
    for c in lp {
        error += c * (1u32 << shift) as f64;
        let v = (error.round() as i64).clamp(qmin as i64, qmax as i64) as i32;
        error -= v as f64;
        q.push(v);
    }
    Some((q, shift))
}

fn lpc_residual(x: &[i64], coefs: &[i32], shift: u32) -> Vec<i64> {
    let order = coefs.len();
    (order..x.len())
        .map(|i| {
            let mut sum = 0i64;
            for (j, c) in coefs.iter().enumerate() {
                sum += *c as i64 * x[i - 1 - j];
            }
            x[i] - (sum >> shift)
        })
        .collect()
}

/// The best coding of `x` at `bps` bits per sample.
fn best_subframe(x: &[i64], bps: u32) -> Subframe {
    let n = x.len();
    let header = 8u64;
    if x.iter().all(|v| *v == x[0]) {
        return Subframe {
            kind: Kind::Constant,
            residual: Vec::new(),
            rice: Rice::default(),
            bits: header + bps as u64,
        };
    }
    let mut best = Subframe {
        kind: Kind::Verbatim,
        residual: Vec::new(),
        rice: Rice::default(),
        bits: header + bps as u64 * n as u64,
    };
    let fits = |r: &[i64]| r.iter().all(|v| v.unsigned_abs() < 1 << 31);
    let consider = |kind: Kind, residual: Vec<i64>, extra: u64, best: &mut Subframe| {
        if !fits(&residual) {
            return;
        }
        let order = n - residual.len();
        let rice = plan_rice(&residual, n, order);
        let bits = header + extra + rice.bits;
        if rice.bits > 0 && bits < best.bits {
            *best = Subframe {
                kind,
                residual,
                rice,
                bits,
            };
        }
    };
    // Fixed: the order with the smallest absolute residual.
    if n > 4 {
        let fixed = (0..=4)
            .map(|o| {
                let r = fixed_residual(x, o);
                let s: u64 = r.iter().map(|v| v.unsigned_abs()).sum();
                (o, r, s)
            })
            .min_by_key(|t| t.2);
        if let Some((o, r, _)) = fixed {
            consider(Kind::Fixed(o), r, o as u64 * bps as u64, &mut best);
        }
    }
    // LPC: the order whose predicted cost is lowest.
    if n > MAX_LPC_ORDER * 2 {
        let windowed: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| {
                // Tukey(0.5).
                let p = 0.25;
                let t = i as f64 / (n - 1) as f64;
                let w = if t < p {
                    0.5 * (1.0 - (std::f64::consts::PI * t / p).cos())
                } else if t > 1.0 - p {
                    0.5 * (1.0 - (std::f64::consts::PI * (1.0 - t) / p).cos())
                } else {
                    1.0
                };
                *v as f64 * w
            })
            .collect();
        let r: Vec<f64> = (0..=MAX_LPC_ORDER)
            .map(|lag| {
                (lag..n)
                    .map(|i| windowed[i] * windowed[i - lag])
                    .sum::<f64>()
            })
            .collect();
        if r[0] > 0.0 {
            let precision = 15u32;
            let orders = levinson(&r, MAX_LPC_ORDER);
            let estimate = |(order, err): (usize, f64)| -> f64 {
                let e = (err / n as f64).max(1e-9);
                0.5 * e.log2().max(0.0) * (n - order) as f64
                    + (order as u32 * (precision + bps)) as f64
            };
            let pick = orders
                .iter()
                .enumerate()
                .map(|(i, (_, err))| (i + 1, *err))
                .min_by(|a, b| estimate(*a).total_cmp(&estimate(*b)));
            if let Some((order, _)) = pick
                && let Some((coefs, shift)) = quantize(&orders[order - 1].0, precision)
            {
                let r = lpc_residual(x, &coefs, shift);
                let extra = order as u64 * bps as u64 + 4 + 5 + order as u64 * precision as u64;
                consider(
                    Kind::Lpc {
                        coefs,
                        precision,
                        shift,
                    },
                    r,
                    extra,
                    &mut best,
                );
            }
        }
    }
    best
}

fn write_signed(w: &mut BitWriter, v: i64, bits: u32) {
    let mask = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    w.write_bits_u64(v as u64 & mask, bits);
}

fn write_subframe(w: &mut BitWriter, x: &[i64], s: &Subframe, bps: u32) {
    w.write_bit(false);
    match &s.kind {
        Kind::Constant => {
            w.write_bits(0, 6);
            w.write_bit(false);
            write_signed(w, x[0], bps);
            return;
        }
        Kind::Verbatim => {
            w.write_bits(1, 6);
            w.write_bit(false);
            for v in x {
                write_signed(w, *v, bps);
            }
            return;
        }
        Kind::Fixed(o) => {
            w.write_bits(0b001000 | *o as u32, 6);
            w.write_bit(false);
            for v in &x[..*o] {
                write_signed(w, *v, bps);
            }
        }
        Kind::Lpc {
            coefs,
            precision,
            shift,
        } => {
            w.write_bits(0b100000 | (coefs.len() as u32 - 1), 6);
            w.write_bit(false);
            for v in &x[..coefs.len()] {
                write_signed(w, *v, bps);
            }
            w.write_bits(precision - 1, 4);
            w.write_bits(*shift, 5);
            for c in coefs {
                write_signed(w, *c as i64, *precision);
            }
        }
    }
    let rice = &s.rice;
    w.write_bits(rice.wide as u32, 2);
    w.write_bits(rice.order, 4);
    let parts = 1usize << rice.order;
    let n = x.len();
    let order = n - s.residual.len();
    let size = n / parts;
    let mut at = 0;
    for (p, k) in rice.params.iter().enumerate() {
        let len = if p == 0 { size - order } else { size };
        w.write_bits(*k, if rice.wide { 5 } else { 4 });
        for r in &s.residual[at..at + len] {
            let u = zigzag(*r);
            let q = u >> k;
            for _ in 0..q {
                w.write_bit(false);
            }
            w.write_bit(true);
            if *k > 0 {
                w.write_bits_u64(u & ((1u64 << k) - 1), *k);
            }
        }
        at += len;
    }
}

/// A FLAC file being written.
#[derive(Debug)]
pub struct FlacWriter<W: Write + Seek> {
    out: W,
    channels: usize,
    bits: u32,
    sample_rate: u32,
    pending: Vec<Vec<i64>>,
    frame: u64,
    total: u64,
    md5: Md5,
    min_frame: u32,
    max_frame: u32,
    bytes: u64,
}

impl<W: Write + Seek> FlacWriter<W> {
    /// `bits`: 16 or 24 per sample. `tags`: Vorbis comments such as
    /// `("TITLE", "…")`.
    pub fn new(
        mut out: W,
        sample_rate: u32,
        channels: u16,
        bits: u16,
        tags: &[(&str, &str)],
    ) -> io::Result<FlacWriter<W>> {
        out.write_all(b"fLaC")?;
        // STREAMINFO, rewritten at the end.
        out.write_all(&[0x00, 0, 0, 34])?;
        out.write_all(&[0u8; 34])?;
        // VORBIS_COMMENT (last block).
        let vendor = b"BananaV";
        let tags: Vec<String> = tags
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let mut vc = Vec::new();
        vc.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        vc.extend_from_slice(vendor);
        vc.extend_from_slice(&(tags.len() as u32).to_le_bytes());
        for t in &tags {
            vc.extend_from_slice(&(t.len() as u32).to_le_bytes());
            vc.extend_from_slice(t.as_bytes());
        }
        let len = vc.len() as u32;
        out.write_all(&[0x80 | 4, (len >> 16) as u8, (len >> 8) as u8, len as u8])?;
        out.write_all(&vc)?;
        let bytes = 4 + 4 + 34 + 4 + vc.len() as u64;
        Ok(FlacWriter {
            out,
            channels: channels as usize,
            bits: bits as u32,
            sample_rate,
            pending: vec![Vec::new(); channels as usize],
            frame: 0,
            total: 0,
            md5: Md5::default(),
            min_frame: u32::MAX,
            max_frame: 0,
            bytes,
        })
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes
    }

    /// Planar float samples (one vector per channel).
    pub fn write_planar(&mut self, channels: &[Vec<f32>]) -> io::Result<()> {
        let max = ((1i64 << (self.bits - 1)) - 1) as f32;
        let n = channels.first().map_or(0, Vec::len);
        let bytes = self.bits as usize / 8;
        let mut raw = Vec::with_capacity(n * self.channels * bytes);
        for i in 0..n {
            for (c, pending) in self.pending.iter_mut().enumerate() {
                let s = channels.get(c).map_or(0.0, |ch| ch[i]).clamp(-1.0, 1.0);
                let v = (s * max).round() as i64;
                pending.push(v);
                raw.extend_from_slice(&v.to_le_bytes()[..bytes]);
            }
        }
        self.md5.update(&raw);
        while self.pending[0].len() >= BLOCK {
            let block: Vec<Vec<i64>> = self
                .pending
                .iter_mut()
                .map(|p| p.drain(..BLOCK).collect())
                .collect();
            self.write_frame(&block)?;
        }
        Ok(())
    }

    fn write_frame(&mut self, x: &[Vec<i64>]) -> io::Result<()> {
        let n = x[0].len();
        let mut w = BitWriter::new();
        w.write_bits(0b11_1111_1111_1110, 14);
        w.write_bit(false);
        w.write_bit(false); // fixed block size
        let code = if n == BLOCK { 12 } else { 7 };
        w.write_bits(code, 4);
        w.write_bits(0, 4); // rate from STREAMINFO

        let bps = self.bits;
        let mut subframes: Vec<(Vec<i64>, Subframe, u32)> = Vec::new();
        let assignment = if self.channels == 2 {
            let (l, r) = (&x[0], &x[1]);
            let side: Vec<i64> = l.iter().zip(r).map(|(a, b)| a - b).collect();
            let mid: Vec<i64> = l.iter().zip(r).map(|(a, b)| (a + b) >> 1).collect();
            let sl = best_subframe(l, bps);
            let sr = best_subframe(r, bps);
            let ss = best_subframe(&side, bps + 1);
            let sm = best_subframe(&mid, bps);
            let options = [
                (1u32, sl.bits + sr.bits),
                (8, sl.bits + ss.bits),
                (9, ss.bits + sr.bits),
                (10, sm.bits + ss.bits),
            ];
            let (a, _) = options.into_iter().min_by_key(|o| o.1).unwrap_or((1, 0));
            match a {
                8 => {
                    subframes.push((l.clone(), sl, bps));
                    subframes.push((side, ss, bps + 1));
                }
                9 => {
                    subframes.push((side, ss, bps + 1));
                    subframes.push((r.clone(), sr, bps));
                }
                10 => {
                    subframes.push((mid, sm, bps));
                    subframes.push((side, ss, bps + 1));
                }
                _ => {
                    subframes.push((l.clone(), sl, bps));
                    subframes.push((r.clone(), sr, bps));
                }
            }
            a
        } else {
            for ch in x {
                subframes.push((ch.clone(), best_subframe(ch, bps), bps));
            }
            self.channels as u32 - 1
        };
        w.write_bits(assignment, 4);
        w.write_bits(0, 3); // sample size from STREAMINFO
        w.write_bit(false);
        utf8_number(&mut w, self.frame);
        if code == 7 {
            w.write_bits(n as u32 - 1, 16);
        }
        let crc = crc8(w.as_bytes());
        w.write_bits(crc as u32, 8);
        for (signal, s, b) in &subframes {
            write_subframe(&mut w, signal, s, *b);
        }
        w.align_zero();
        let crc = crc16(w.as_bytes());
        w.write_bits(crc as u32, 16);
        let frame = w.into_bytes();
        self.out.write_all(&frame)?;
        self.bytes += frame.len() as u64;
        self.min_frame = self.min_frame.min(frame.len() as u32);
        self.max_frame = self.max_frame.max(frame.len() as u32);
        self.frame += 1;
        self.total += n as u64;
        Ok(())
    }

    /// Writes the last (shorter) block and the final STREAMINFO.
    pub fn finish(mut self) -> io::Result<W> {
        if !self.pending[0].is_empty() {
            let block: Vec<Vec<i64>> = std::mem::take(&mut self.pending);
            self.write_frame(&block)?;
        }
        let mut info = BitWriter::new();
        // Fixed-size blocks (the last one may be shorter).
        info.write_bits(BLOCK as u32, 16);
        info.write_bits(BLOCK as u32, 16);
        info.write_bits(
            if self.min_frame == u32::MAX {
                0
            } else {
                self.min_frame
            },
            24,
        );
        info.write_bits(self.max_frame, 24);
        info.write_bits(self.sample_rate, 20);
        info.write_bits(self.channels as u32 - 1, 3);
        info.write_bits(self.bits - 1, 5);
        info.write_bits_u64(self.total, 36);
        let mut bytes = info.into_bytes();
        bytes.extend_from_slice(&self.md5.clone().finish());
        self.out.seek(SeekFrom::Start(8))?;
        self.out.write_all(&bytes)?;
        self.out.seek(SeekFrom::End(0))?;
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_of_the_specification() {
        // "123456789" check values: CRC-8/SMBUS F4, CRC-16/UMTS FEE8.
        assert_eq!(crc8(b"123456789"), 0xF4);
        assert_eq!(crc16(b"123456789"), 0xFEE8);
    }

    #[test]
    fn frame_numbers() {
        let code = |v| {
            let mut w = BitWriter::new();
            utf8_number(&mut w, v);
            w.into_bytes()
        };
        assert_eq!(code(5), vec![5]);
        assert_eq!(code(0x80), vec![0xC2, 0x80]);
        assert_eq!(code(0x7FF), vec![0xDF, 0xBF]);
        assert_eq!(code(0x800), vec![0xE0, 0xA0, 0x80]);
    }

    #[test]
    fn predictors_shrink_a_sine() {
        let x: Vec<i64> = (0..4096)
            .map(|i| (10_000.0 * (i as f64 * 0.05).sin()) as i64)
            .collect();
        let s = best_subframe(&x, 16);
        assert!(matches!(s.kind, Kind::Lpc { .. } | Kind::Fixed(_)));
        assert!(s.bits < 16 * 4096 / 4, "{} bits", s.bits);
    }
}
