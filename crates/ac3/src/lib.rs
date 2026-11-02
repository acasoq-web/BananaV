//! Dolby Digital (AC-3) decoding, from ATSC A/52 (sections 5 to 7): every
//! audio coding mode (1/0 to 3/2 and dual mono) with or without the LFE
//! channel; coupling, rematrixing, block switching, delta bit allocation
//! and dynamic range control. Channels come out in WAV order (left, right,
//! center, LFE, surrounds). E-AC-3 (Dolby Digital Plus) is not read.

mod tables;

use bv_bitstream::BitReader;
use tables::{BAPTAB, FRAME_WORDS, HTH, LATAB};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A variant not read (E-AC-3, reserved values).
    Unsupported(String),
    /// Damaged data.
    Corrupt(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported(s) => write!(f, "unsupported AC-3: {s}"),
            Error::Corrupt(s) => write!(f, "damaged AC-3: {s}"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

impl From<bv_bitstream::Error> for Error {
    fn from(_: bv_bitstream::Error) -> Error {
        Error::Corrupt("frame ends early".into())
    }
}

/// What a syncframe header says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    pub sample_rate: u32,
    /// Output channels (the LFE counted).
    pub channels: usize,
    /// Bytes of the whole syncframe.
    pub bytes: usize,
}

const SAMPLE_RATES: [u32; 3] = [48_000, 44_100, 32_000];
/// Full-bandwidth channels per `acmod`.
const NFCHANS: [usize; 8] = [2, 1, 2, 3, 3, 4, 4, 5];

/// The header of the syncframe at the start of `d`.
pub fn frame_info(d: &[u8]) -> Option<FrameInfo> {
    if d.len() < 7 || d[0] != 0x0b || d[1] != 0x77 {
        return None;
    }
    let fscod = (d[4] >> 6) as usize;
    let frmsizecod = (d[4] & 0x3f) as usize;
    if fscod == 3 || frmsizecod >= 38 {
        return None;
    }
    let bsid = d[5] >> 3;
    if bsid > 8 {
        return None;
    }
    let acmod = (d[6] >> 5) as usize;
    // lfeon follows acmod and the 2-bit fields acmod brings.
    let mut bit = 3usize;
    if acmod & 1 != 0 && acmod != 1 {
        bit += 2;
    }
    if acmod & 4 != 0 {
        bit += 2;
    }
    if acmod == 2 {
        bit += 2;
    }
    let lfe = d
        .get(6 + bit / 8)
        .is_some_and(|b| b >> (7 - bit % 8) & 1 != 0);
    Some(FrameInfo {
        sample_rate: SAMPLE_RATES[fscod],
        channels: NFCHANS[acmod] + usize::from(lfe),
        bytes: FRAME_WORDS[frmsizecod][fscod] as usize * 2,
    })
}

const SLOWDEC: [i32; 4] = [0x0f, 0x11, 0x13, 0x15];
const FASTDEC: [i32; 4] = [0x3f, 0x53, 0x67, 0x7b];
const SLOWGAIN: [i32; 4] = [0x540, 0x4d8, 0x478, 0x410];
const DBPBTAB: [i32; 4] = [0x000, 0x700, 0x900, 0xb00];
/// The last value is −2048 (0xf800 as a 16-bit number).
const FLOORTAB: [i32; 8] = [0x2f0, 0x2b0, 0x270, 0x230, 0x1f0, 0x170, 0x0f0, -0x800];
const FASTGAIN: [i32; 8] = [0x080, 0x100, 0x180, 0x200, 0x280, 0x300, 0x380, 0x400];
/// Mantissa bits of the asymmetric quantizers, by bap (Table 7.18).
const QNTZTAB: [u32; 16] = [0, 0, 0, 0, 0, 0, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16];
/// Rematrixing bands' first bins (Tables 7.25 to 7.28).
const REMATRIX: [usize; 5] = [13, 25, 37, 61, 253];

/// The first bin of each of the 50 bands (Table 7.12) and the band of each
/// bin (Table 7.13).
fn bands() -> (&'static [usize; 51], &'static [usize; 256]) {
    use std::sync::OnceLock;
    static TABLES: OnceLock<([usize; 51], [usize; 256])> = OnceLock::new();
    let t = TABLES.get_or_init(|| {
        let mut start = [0usize; 51];
        let mut b = 0;
        let mut add = |from: usize, width: usize, count: usize| {
            for i in 0..count {
                start[b] = from + i * width;
                b += 1;
            }
        };
        add(0, 1, 28);
        add(28, 3, 7);
        add(49, 6, 6);
        add(85, 12, 4);
        add(133, 24, 5);
        start[50] = 253;
        let mut mask = [0usize; 256];
        for band in 0..50 {
            for m in &mut mask[start[band]..start[band + 1]] {
                *m = band;
            }
        }
        (start, mask)
    });
    (&t.0, &t.1)
}

/// The Kaiser-Bessel-derived window (α = 5) of Table 7.33.
fn window() -> &'static [f32; 256] {
    use std::sync::OnceLock;
    static W: OnceLock<[f32; 256]> = OnceLock::new();
    W.get_or_init(|| {
        fn i0(x: f64) -> f64 {
            let (mut s, mut term, mut k) = (1.0f64, 1.0f64, 1.0f64);
            while term > 1e-20 * s {
                term *= (x / 2.0 / k).powi(2);
                s += term;
                k += 1.0;
            }
            s
        }
        let n = 256usize;
        let mut cum = vec![0f64; n + 1];
        let mut acc = 0.0;
        for (i, c) in cum.iter_mut().enumerate() {
            let r = 2.0 * i as f64 / n as f64 - 1.0;
            acc += i0(std::f64::consts::PI * 5.0 * (1.0 - r * r).max(0.0).sqrt());
            *c = acc;
        }
        let mut w = [0f32; 256];
        for (i, v) in w.iter_mut().enumerate() {
            *v = (cum[i] / cum[n]).sqrt() as f32;
        }
        w
    })
}

/// Delta bit allocation of a channel: (first band offset, bands, change).
type Deltas = Vec<(usize, usize, i32)>;

/// What persists from block to block within a syncframe. Index 5 is the
/// LFE channel, 6 the coupling channel.
struct Frame {
    fscod: usize,
    acmod: usize,
    nfchans: usize,
    lfeon: bool,
    exps: [[u8; 256]; 7],
    bap: [[u8; 256]; 7],
    endmant: [usize; 7],
    chexpstr: [u32; 7],
    blksw: [bool; 5],
    dithflag: [bool; 5],
    dynrng: [f32; 2],
    // Coupling.
    cplinu: bool,
    chincpl: [bool; 5],
    phsflginu: bool,
    cplbegf: usize,
    cplendf: usize,
    /// Which coupling band each sub-band belongs to.
    cpl_band_of: [usize; 18],
    ncplbnd: usize,
    /// Coupling coordinates by channel and sub-band.
    cplco: [[f32; 18]; 5],
    phsflg: [bool; 18],
    rematflg: [bool; 4],
    // Bit allocation parameters.
    sdcycod: usize,
    fdcycod: usize,
    sgaincod: usize,
    dbpbcod: usize,
    floorcod: usize,
    csnroffst: i32,
    fsnroffst: [i32; 7],
    fgaincod: [usize; 7],
    cplfleak: i32,
    cplsleak: i32,
    deltas: [Deltas; 7],
    /// Bit allocation parameters were sent (block 0 must send them).
    have_alloc: bool,
}

impl Frame {
    fn new(fscod: usize, acmod: usize, lfeon: bool) -> Frame {
        Frame {
            fscod,
            acmod,
            nfchans: NFCHANS[acmod],
            lfeon,
            exps: [[0; 256]; 7],
            bap: [[0; 256]; 7],
            endmant: [0; 7],
            chexpstr: [0; 7],
            blksw: [false; 5],
            dithflag: [false; 5],
            dynrng: [1.0; 2],
            cplinu: false,
            chincpl: [false; 5],
            phsflginu: false,
            cplbegf: 0,
            cplendf: 0,
            cpl_band_of: [0; 18],
            ncplbnd: 0,
            cplco: [[0.0; 18]; 5],
            phsflg: [false; 18],
            rematflg: [false; 4],
            sdcycod: 0,
            fdcycod: 0,
            sgaincod: 0,
            dbpbcod: 0,
            floorcod: 0,
            csnroffst: 0,
            fsnroffst: [0; 7],
            fgaincod: [0; 7],
            cplfleak: 0,
            cplsleak: 0,
            deltas: Default::default(),
            have_alloc: false,
        }
    }

    fn cplstrtmant(&self) -> usize {
        37 + 12 * self.cplbegf
    }

    fn cplendmant(&self) -> usize {
        37 + 12 * (self.cplendf + 3)
    }
}

/// An AC-3 decoder: give it the stream in pieces of any size, it gives
/// back the samples of each whole syncframe.
#[derive(Debug)]
pub struct Decoder {
    pending: Vec<u8>,
    sample_rate: u32,
    channels: usize,
    /// The second half of the last block of each coded channel (5 + LFE).
    delay: [[f32; 256]; 6],
    seed: u32,
}

impl Default for Decoder {
    fn default() -> Decoder {
        Decoder::new()
    }
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            pending: Vec::new(),
            sample_rate: 0,
            channels: 0,
            delay: [[0.0; 256]; 6],
            seed: 0x1234_5678,
        }
    }

    /// Of the last syncframe decoded.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Forgets the bytes waiting and the overlap (after a seek).
    pub fn reset(&mut self) {
        self.pending.clear();
        self.delay = [[0.0; 256]; 6];
    }

    /// Appends to `out` the interleaved samples (1536 per channel) of every
    /// whole syncframe in what came so far; bytes before a syncword are
    /// skipped, a frame cut short waits for the next piece. A damaged frame
    /// comes out as silence.
    pub fn decode(&mut self, data: &[u8], out: &mut Vec<f32>) -> Result<()> {
        self.pending.extend_from_slice(data);
        let mut pos = 0;
        let mut result = Ok(());
        while pos + 7 <= self.pending.len() {
            let Some(info) = frame_info(&self.pending[pos..]) else {
                if self.pending[pos] == 0x0b
                    && self.pending[pos + 1] == 0x77
                    && self.pending[pos + 5] >> 3 > 10
                {
                    result = Err(Error::Unsupported("Dolby Digital Plus (E-AC-3)".into()));
                }
                pos += 1;
                continue;
            };
            if pos + info.bytes > self.pending.len() {
                break;
            }
            let frame = self.pending[pos..pos + info.bytes].to_vec();
            self.sample_rate = info.sample_rate;
            if self.channels != info.channels {
                self.channels = info.channels;
                self.delay = [[0.0; 256]; 6];
            }
            let start = out.len();
            if let Err(e) = self.frame(&frame, out) {
                // Silence in its place keeps the timing.
                out.truncate(start);
                out.resize(start + 1536 * info.channels, 0.0);
                self.delay = [[0.0; 256]; 6];
                result = Err(e);
            }
            pos += info.bytes;
        }
        self.pending.drain(..pos);
        result
    }

    /// A dither value, uniform in ±0.707 (section 7.3.4).
    fn dither(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        ((self.seed >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0) * 0.707
    }

    fn frame(&mut self, d: &[u8], out: &mut Vec<f32>) -> Result<()> {
        let mut r = BitReader::new(d);
        r.read_bits(16)?; // syncword
        r.read_bits(16)?; // crc1
        let fscod = r.read_bits(2)? as usize;
        r.read_bits(6)?; // frmsizecod
        // bsi
        let bsid = r.read_bits(5)?;
        if bsid > 8 {
            return Err(Error::Unsupported(format!("bsid {bsid}")));
        }
        r.read_bits(3)?; // bsmod
        let acmod = r.read_bits(3)? as usize;
        if acmod & 1 != 0 && acmod != 1 {
            r.read_bits(2)?; // cmixlev
        }
        if acmod & 4 != 0 {
            r.read_bits(2)?; // surmixlev
        }
        if acmod == 2 {
            r.read_bits(2)?; // dsurmod
        }
        let lfeon = r.read_bit()?;
        let dual = if acmod == 0 { 2 } else { 1 };
        for _ in 0..dual {
            r.read_bits(5)?; // dialnorm
            if r.read_bit()? {
                r.read_bits(8)?; // compr
            }
            if r.read_bit()? {
                r.read_bits(8)?; // langcod
            }
            if r.read_bit()? {
                r.read_bits(7)?; // mixlevel, roomtyp
            }
        }
        r.read_bits(2)?; // copyrightb, origbs
        for _ in 0..2 {
            // timecod1/2 (or xbsi1/2 with bsid 6).
            if r.read_bit()? {
                r.read_bits(14)?;
            }
        }
        if r.read_bit()? {
            let n = r.read_bits(6)? + 1;
            for _ in 0..n {
                r.read_bits(8)?;
            }
        }
        let mut f = Frame::new(fscod, acmod, lfeon);
        let nch = f.nfchans;
        // Coefficients of each block: the full channels, then the LFE.
        let mut coefs = [[0f32; 256]; 6];
        let outputs = order(acmod, lfeon);
        let mut pcm = vec![[0f32; 256]; 6];
        let start = out.len();
        out.resize(start + 1536 * outputs.len(), 0.0);
        for blk in 0..6 {
            self.block(&mut r, &mut f, blk, &mut coefs)?;
            for ch in 0..nch {
                imdct(&coefs[ch], f.blksw[ch], &mut self.delay[ch], &mut pcm[ch]);
            }
            if lfeon {
                imdct(&coefs[5], false, &mut self.delay[5], &mut pcm[5]);
            }
            for (o, &src) in outputs.iter().enumerate() {
                for n in 0..256 {
                    out[start + (blk * 256 + n) * outputs.len() + o] = pcm[src][n];
                }
            }
        }
        Ok(())
    }

    /// One audio block (Table 5.3): fills the coefficients of each channel.
    #[allow(clippy::needless_range_loop)]
    fn block(
        &mut self,
        r: &mut BitReader<'_>,
        f: &mut Frame,
        blk: usize,
        coefs: &mut [[f32; 256]; 6],
    ) -> Result<()> {
        let nch = f.nfchans;
        for ch in 0..nch {
            f.blksw[ch] = r.read_bit()?;
        }
        for ch in 0..nch {
            f.dithflag[ch] = r.read_bit()?;
        }
        let dynamic = if f.acmod == 0 { 2 } else { 1 };
        for d in 0..dynamic {
            if r.read_bit()? {
                f.dynrng[d] = dynrng(r.read_bits(8)?);
            } else if blk == 0 {
                f.dynrng[d] = 1.0;
            }
        }
        // Coupling strategy.
        if r.read_bit()? {
            f.cplinu = r.read_bit()?;
            if f.cplinu {
                for ch in 0..nch {
                    f.chincpl[ch] = r.read_bit()?;
                }
                if f.acmod == 2 {
                    f.phsflginu = r.read_bit()?;
                }
                f.cplbegf = r.read_bits(4)? as usize;
                f.cplendf = r.read_bits(4)? as usize;
                let nsub = 3 + f.cplendf as i64 - f.cplbegf as i64;
                if nsub < 1 {
                    return Err(Error::Corrupt("coupling ends before it begins".into()));
                }
                let mut band = 0;
                f.cpl_band_of = [0; 18];
                for s in 0..nsub as usize {
                    if s > 0 && !r.read_bit()? {
                        band += 1;
                    }
                    f.cpl_band_of[f.cplbegf + s] = band;
                }
                f.ncplbnd = band + 1;
            } else {
                f.chincpl = [false; 5];
            }
        } else if blk == 0 {
            return Err(Error::Corrupt("no coupling strategy in block 0".into()));
        }
        if f.cplinu {
            // Coupling coordinates.
            let mut any = false;
            for ch in 0..nch {
                if !f.chincpl[ch] || !r.read_bit()? {
                    continue;
                }
                any = true;
                let master = r.read_bits(2)? as i32;
                let mut per_band = [0f32; 18];
                for v in per_band.iter_mut().take(f.ncplbnd) {
                    let exp = r.read_bits(4)? as i32;
                    let mant = r.read_bits(4)? as f32;
                    let m = if exp == 15 {
                        mant / 16.0
                    } else {
                        (mant + 16.0) / 32.0
                    };
                    *v = m * 2f32.powi(-(exp + 3 * master));
                }
                for s in f.cplbegf..f.cplendf + 3 {
                    f.cplco[ch][s] = per_band[f.cpl_band_of[s]];
                }
            }
            if f.acmod == 2 && f.phsflginu && any {
                let mut per_band = [false; 18];
                for v in per_band.iter_mut().take(f.ncplbnd) {
                    *v = r.read_bit()?;
                }
                for s in f.cplbegf..f.cplendf + 3 {
                    f.phsflg[s] = per_band[f.cpl_band_of[s]];
                }
            }
        }
        // Rematrixing.
        if f.acmod == 2 && r.read_bit()? {
            let n = if !f.cplinu || f.cplbegf > 2 {
                4
            } else if f.cplbegf > 0 {
                3
            } else {
                2
            };
            f.rematflg = [false; 4];
            for flag in f.rematflg.iter_mut().take(n) {
                *flag = r.read_bit()?;
            }
        }
        // Exponent strategies.
        let cplexpstr = if f.cplinu { r.read_bits(2)? } else { 0 };
        for ch in 0..nch {
            f.chexpstr[ch] = r.read_bits(2)?;
        }
        let lfeexpstr = if f.lfeon { r.read_bit()? } else { false };
        if blk == 0
            && ((0..nch).any(|ch| f.chexpstr[ch] == 0)
                || (f.cplinu && cplexpstr == 0)
                || (f.lfeon && !lfeexpstr))
        {
            return Err(Error::Corrupt("exponents reused in block 0".into()));
        }
        for ch in 0..nch {
            if f.chexpstr[ch] != 0 {
                if f.chincpl[ch] && f.cplinu {
                    f.endmant[ch] = f.cplstrtmant();
                } else {
                    let bw = r.read_bits(6)? as usize;
                    if bw > 60 {
                        return Err(Error::Corrupt("channel bandwidth code".into()));
                    }
                    f.endmant[ch] = (bw + 12) * 3 + 37;
                }
            }
        }
        // Exponents.
        if f.cplinu && cplexpstr != 0 {
            let abs = (r.read_bits(4)? << 1) as u8;
            let (s, e) = (f.cplstrtmant(), f.cplendmant());
            let size = 3 << (cplexpstr - 1);
            let groups = (e - s) / size;
            let mut tmp = [0u8; 257];
            exponents(r, abs, groups, cplexpstr, &mut tmp)?;
            for (n, v) in tmp[1..].iter().enumerate().take(e - s) {
                f.exps[6][s + n] = *v;
            }
            f.endmant[6] = e;
        }
        for ch in 0..nch {
            if f.chexpstr[ch] != 0 {
                let abs = r.read_bits(4)? as u8;
                let end = f.endmant[ch];
                let groups = match f.chexpstr[ch] {
                    1 => (end - 1) / 3,
                    2 => (end - 1 + 3) / 6,
                    _ => (end - 1 + 9) / 12,
                };
                let mut tmp = [0u8; 257];
                exponents(r, abs, groups, f.chexpstr[ch], &mut tmp)?;
                f.exps[ch][..end].copy_from_slice(&tmp[..end]);
                r.read_bits(2)?; // gainrng
            }
        }
        if f.lfeon && lfeexpstr {
            let abs = r.read_bits(4)? as u8;
            let mut tmp = [0u8; 257];
            exponents(r, abs, 2, 1, &mut tmp)?;
            f.exps[5][..7].copy_from_slice(&tmp[..7]);
            f.endmant[5] = 7;
        }
        // Bit allocation parameters.
        if r.read_bit()? {
            f.sdcycod = r.read_bits(2)? as usize;
            f.fdcycod = r.read_bits(2)? as usize;
            f.sgaincod = r.read_bits(2)? as usize;
            f.dbpbcod = r.read_bits(2)? as usize;
            f.floorcod = r.read_bits(3)? as usize;
            f.have_alloc = true;
        }
        if r.read_bit()? {
            f.csnroffst = r.read_bits(6)? as i32;
            if f.cplinu {
                f.fsnroffst[6] = r.read_bits(4)? as i32;
                f.fgaincod[6] = r.read_bits(3)? as usize;
            }
            for ch in 0..nch {
                f.fsnroffst[ch] = r.read_bits(4)? as i32;
                f.fgaincod[ch] = r.read_bits(3)? as usize;
            }
            if f.lfeon {
                f.fsnroffst[5] = r.read_bits(4)? as i32;
                f.fgaincod[5] = r.read_bits(3)? as usize;
            }
        }
        if f.cplinu && r.read_bit()? {
            f.cplfleak = r.read_bits(3)? as i32;
            f.cplsleak = r.read_bits(3)? as i32;
        }
        // Delta bit allocation.
        if r.read_bit()? {
            let cpl = if f.cplinu { r.read_bits(2)? } else { 2 };
            let mut modes = [2u32; 5];
            for m in modes.iter_mut().take(nch) {
                *m = r.read_bits(2)?;
            }
            if f.cplinu {
                match cpl {
                    1 => f.deltas[6] = read_deltas(r)?,
                    2 => f.deltas[6].clear(),
                    _ => {}
                }
            }
            for (ch, &m) in modes.iter().enumerate().take(nch) {
                match m {
                    1 => f.deltas[ch] = read_deltas(r)?,
                    2 => f.deltas[ch].clear(),
                    _ => {}
                }
            }
        }
        // Dummy data.
        if r.read_bit()? {
            let n = r.read_bits(9)?;
            for _ in 0..n {
                r.read_bits(8)?;
            }
        }
        if !f.have_alloc {
            return Err(Error::Corrupt("no bit allocation in block 0".into()));
        }
        // Bit allocation of every channel (cheap enough to do each block).
        let silent = f.csnroffst == 0
            && (0..nch).all(|ch| f.fsnroffst[ch] == 0)
            && (!f.cplinu || f.fsnroffst[6] == 0)
            && (!f.lfeon || f.fsnroffst[5] == 0);
        for ch in (0..nch)
            .chain(f.cplinu.then_some(6))
            .chain(f.lfeon.then_some(5))
        {
            if silent {
                f.bap[ch] = [0; 256];
                continue;
            }
            let (start, end) = match ch {
                6 => (f.cplstrtmant(), f.cplendmant()),
                5 => (0, 7),
                _ => (0, f.endmant[ch]),
            };
            let leak = (ch == 6).then(|| ((f.cplfleak << 8) + 768, (f.cplsleak << 8) + 768));
            let snroffset = (((f.csnroffst - 15) << 4) + f.fsnroffst[ch]) << 2;
            let fgain = FASTGAIN[f.fgaincod[ch]];
            let mut bap = [0u8; 256];
            allocate(f, ch, start, end, fgain, snroffset, leak, &mut bap);
            f.bap[ch] = bap;
        }
        // Mantissas: the channels in turn, the coupling channel after the
        // first coupled one, then the LFE. Grouped codes carry over.
        let mut groups = Groups::default();
        let mut cpl_coefs = [0f32; 256];
        let mut cpl_done = false;
        for ch in 0..nch {
            coefs[ch] = [0.0; 256];
            for bin in 0..f.endmant[ch] {
                let bap = f.bap[ch][bin];
                let m = match bap {
                    0 if f.dithflag[ch] => self.dither(),
                    0 => 0.0,
                    _ => groups.mantissa(r, bap)?,
                };
                coefs[ch][bin] = m * 2f32.powi(-(f.exps[ch][bin] as i32));
            }
            if f.cplinu && f.chincpl[ch] && !cpl_done {
                cpl_done = true;
                for bin in f.cplstrtmant()..f.cplendmant() {
                    let bap = f.bap[6][bin];
                    // Zero mantissas are dithered per channel below.
                    let m = if bap == 0 {
                        f32::NAN
                    } else {
                        groups.mantissa(r, bap)?
                    };
                    cpl_coefs[bin] = m;
                }
            }
        }
        if f.lfeon {
            coefs[5] = [0.0; 256];
            for bin in 0..7 {
                let bap = f.bap[5][bin];
                let m = if bap == 0 {
                    0.0
                } else {
                    groups.mantissa(r, bap)?
                };
                coefs[5][bin] = m * 2f32.powi(-(f.exps[5][bin] as i32));
            }
        }
        // Decoupling (section 7.4).
        if f.cplinu {
            for ch in 0..nch {
                if !f.chincpl[ch] {
                    continue;
                }
                for s in f.cplbegf..f.cplendf + 3 {
                    let co = f.cplco[ch][s] * 8.0;
                    let flip = f.acmod == 2 && ch == 1 && f.phsflginu && f.phsflg[s];
                    for bin in 37 + s * 12..37 + s * 12 + 12 {
                        let mut m = cpl_coefs[bin];
                        if m.is_nan() {
                            m = if f.dithflag[ch] { self.dither() } else { 0.0 };
                        }
                        let v = m * 2f32.powi(-(f.exps[6][bin] as i32)) * co;
                        coefs[ch][bin] = if flip { -v } else { v };
                    }
                }
            }
        }
        // Rematrixing (section 7.5): up to the narrower of the two channels.
        if f.acmod == 2 {
            let end = if f.cplinu {
                f.cplstrtmant()
            } else {
                f.endmant[0].min(f.endmant[1])
            };
            for band in 0..4 {
                if !f.rematflg[band] {
                    continue;
                }
                for bin in REMATRIX[band]..REMATRIX[band + 1].min(end) {
                    let (l, rr) = (coefs[0][bin], coefs[1][bin]);
                    coefs[0][bin] = l + rr;
                    coefs[1][bin] = l - rr;
                }
            }
        }
        // Dynamic range (section 7.7): dynrng2 is the second channel's in
        // dual mono.
        for ch in 0..nch {
            let g = if f.acmod == 0 && ch == 1 {
                f.dynrng[1]
            } else {
                f.dynrng[0]
            };
            if g != 1.0 {
                coefs[ch].iter_mut().for_each(|c| *c *= g);
            }
        }
        if f.lfeon && f.dynrng[0] != 1.0 {
            coefs[5].iter_mut().for_each(|c| *c *= f.dynrng[0]);
        }
        Ok(())
    }
}

/// Delta bit allocation segments (section 7.2.2.6): ±6 dB steps of the
/// masking curve over runs of bands.
fn read_deltas(r: &mut BitReader<'_>) -> Result<Deltas> {
    let n = r.read_bits(3)? as usize + 1;
    let mut segs = Vec::with_capacity(n);
    for _ in 0..n {
        let offset = r.read_bits(5)? as usize;
        let len = r.read_bits(4)? as usize;
        let ba = r.read_bits(3)? as i32;
        let delta = if ba >= 4 {
            (ba - 3) << 7
        } else {
            (ba - 4) << 7
        };
        segs.push((offset, len, delta));
    }
    Ok(segs)
}

/// The gain a `dynrng` code says (section 7.7.1.2): 2^(X+1) · 0.1Y.
fn dynrng(v: u32) -> f32 {
    let x = ((v as u8 as i8) >> 5) as i32;
    let y = (v & 31) | 32;
    2f32.powi(x + 1) * y as f32 / 64.0
}

/// Decodes `groups` grouped differential exponents after the absolute one
/// into `out` (out[0] the absolute one), as section 7.1.3 says.
fn exponents(
    r: &mut BitReader<'_>,
    abs: u8,
    groups: usize,
    strategy: u32,
    out: &mut [u8; 257],
) -> Result<()> {
    let size = 1usize << (strategy - 1);
    out[0] = abs;
    let mut prev = abs as i32;
    let mut at = 1usize;
    for _ in 0..groups {
        let g = r.read_bits(7)? as i32;
        if g >= 125 {
            return Err(Error::Corrupt("exponent group".into()));
        }
        for m in [g / 25, (g % 25) / 5, g % 5] {
            prev += m - 2;
            if !(0..=24).contains(&prev) {
                return Err(Error::Corrupt("exponent out of range".into()));
            }
            for _ in 0..size {
                if at < out.len() {
                    out[at] = prev as u8;
                    at += 1;
                }
            }
        }
    }
    Ok(())
}

fn logadd(a: i32, b: i32) -> i32 {
    let c = a - b;
    let address = ((c.abs() >> 1) as usize).min(255);
    if c >= 0 {
        a + LATAB[address] as i32
    } else {
        b + LATAB[address] as i32
    }
}

fn calc_lowcomp(a: i32, b0: i32, b1: i32, bin: usize) -> i32 {
    if bin < 7 {
        if b0 + 256 == b1 {
            384
        } else if b0 > b1 {
            (a - 64).max(0)
        } else {
            a
        }
    } else if bin < 20 {
        if b0 + 256 == b1 {
            320
        } else if b0 > b1 {
            (a - 64).max(0)
        } else {
            a
        }
    } else {
        (a - 128).max(0)
    }
}

/// The parametric bit allocation of section 7.2.2 for one channel.
#[allow(clippy::too_many_arguments)]
fn allocate(
    f: &Frame,
    ch: usize,
    start: usize,
    end: usize,
    fgain: i32,
    snroffset: i32,
    leak: Option<(i32, i32)>,
    bap: &mut [u8; 256],
) {
    if end <= start {
        return;
    }
    let (bndtab, masktab) = bands();
    let exps = &f.exps[ch];
    let sdecay = SLOWDEC[f.sdcycod];
    let fdecay = FASTDEC[f.fdcycod];
    let sgain = SLOWGAIN[f.sgaincod];
    let dbknee = DBPBTAB[f.dbpbcod];
    let floor = FLOORTAB[f.floorcod];
    let mut psd = [0i32; 256];
    for bin in start..end {
        psd[bin] = 3072 - ((exps[bin] as i32) << 7);
    }
    // PSD integration.
    let mut bndpsd = [0i32; 51];
    let mut j = start;
    let mut k = masktab[start];
    loop {
        let lastbin = bndtab[k + 1].min(end);
        bndpsd[k] = psd[j];
        j += 1;
        while j < lastbin {
            bndpsd[k] = logadd(bndpsd[k], psd[j]);
            j += 1;
        }
        k += 1;
        if end <= lastbin {
            break;
        }
    }
    // Excitation.
    let bndstrt = masktab[start];
    let bndend = masktab[end - 1] + 1;
    let mut excite = [0i32; 50];
    let (mut fastleak, mut slowleak) = leak.unwrap_or((0, 0));

    let begin = if bndstrt == 0 {
        let lfe_last = |bin: usize| bndend == 7 && bin == 6;
        let mut lowcomp = calc_lowcomp(0, bndpsd[0], bndpsd[1], 0);
        excite[0] = bndpsd[0] - fgain - lowcomp;
        lowcomp = calc_lowcomp(lowcomp, bndpsd[1], bndpsd[2], 1);
        excite[1] = bndpsd[1] - fgain - lowcomp;
        let mut b = 7;
        for bin in 2..7 {
            if !lfe_last(bin) {
                lowcomp = calc_lowcomp(lowcomp, bndpsd[bin], bndpsd[bin + 1], bin);
            }
            fastleak = bndpsd[bin] - fgain;
            slowleak = bndpsd[bin] - sgain;
            excite[bin] = fastleak - lowcomp;
            if !lfe_last(bin) && bndpsd[bin] <= bndpsd[bin + 1] {
                b = bin + 1;
                break;
            }
        }
        for bin in b..bndend.min(22) {
            if !lfe_last(bin) {
                lowcomp = calc_lowcomp(lowcomp, bndpsd[bin], bndpsd[bin + 1], bin);
            }
            fastleak = (fastleak - fdecay).max(bndpsd[bin] - fgain);
            slowleak = (slowleak - sdecay).max(bndpsd[bin] - sgain);
            excite[bin] = (fastleak - lowcomp).max(slowleak);
        }
        22
    } else {
        bndstrt
    };
    for bin in begin..bndend {
        fastleak = (fastleak - fdecay).max(bndpsd[bin] - fgain);
        slowleak = (slowleak - sdecay).max(bndpsd[bin] - sgain);
        excite[bin] = fastleak.max(slowleak);
    }
    // Masking curve.
    let mut mask = [0i32; 50];
    for bin in bndstrt..bndend {
        if bndpsd[bin] < dbknee {
            excite[bin] += (dbknee - bndpsd[bin]) >> 2;
        }
        mask[bin] = excite[bin].max(HTH[f.fscod][bin] as i32);
    }
    // Delta bit allocation.
    let mut band = 0usize;
    for &(offset, len, delta) in &f.deltas[ch] {
        band += offset;
        for _ in 0..len {
            if band < 50 {
                mask[band] += delta;
            }
            band += 1;
        }
    }
    // The allocation.
    let mut i = start;
    let mut j = masktab[start];
    loop {
        let lastbin = bndtab[j + 1].min(end);
        let mut m = mask[j] - snroffset - floor;
        if m < 0 {
            m = 0;
        }
        m &= 0x1fe0;
        m += floor;
        while i < lastbin {
            let address = ((psd[i] - m) >> 5).clamp(0, 63);
            bap[i] = BAPTAB[address as usize];
            i += 1;
        }
        j += 1;
        if end <= lastbin {
            break;
        }
    }
}

/// Mantissas grouped by three (bap 1 and 2) or two (bap 4), shared by all
/// the channels of a block.
#[derive(Default)]
struct Groups {
    b1: Vec<f32>,
    b2: Vec<f32>,
    b4: Vec<f32>,
}

impl Groups {
    fn mantissa(&mut self, r: &mut BitReader<'_>, bap: u8) -> Result<f32> {
        Ok(match bap {
            1 => {
                if self.b1.is_empty() {
                    let g = r.read_bits(5)? as i32;
                    for c in [g % 3, (g % 9) / 3, g / 9] {
                        self.b1.push((c - 1) as f32 * 2.0 / 3.0);
                    }
                }
                self.b1.pop().unwrap_or(0.0)
            }
            2 => {
                if self.b2.is_empty() {
                    let g = r.read_bits(7)? as i32;
                    for c in [g % 5, (g % 25) / 5, g / 25] {
                        self.b2.push((c - 2) as f32 * 2.0 / 5.0);
                    }
                }
                self.b2.pop().unwrap_or(0.0)
            }
            3 => {
                let c = r.read_bits(3)? as i32;
                if c == 7 {
                    0.0
                } else {
                    (c - 3) as f32 * 2.0 / 7.0
                }
            }
            4 => {
                if self.b4.is_empty() {
                    let g = r.read_bits(7)? as i32;
                    for c in [g % 11, g / 11] {
                        self.b4.push((c - 5) as f32 * 2.0 / 11.0);
                    }
                }
                self.b4.pop().unwrap_or(0.0)
            }
            5 => {
                let c = r.read_bits(4)? as i32;
                if c == 15 {
                    0.0
                } else {
                    (c - 7) as f32 * 2.0 / 15.0
                }
            }
            b => {
                let bits = QNTZTAB[b as usize];
                r.read_signed(bits)? as f32 / (1u32 << (bits - 1)) as f32
            }
        })
    }
}

/// Coded channels in WAV order: left, right, center, LFE, surrounds (5 is
/// the LFE).
fn order(acmod: usize, lfe: bool) -> Vec<usize> {
    let lfe: &[usize] = if lfe { &[5] } else { &[] };
    match acmod {
        0 | 2 => [&[0, 1][..], lfe].concat(),
        1 => [&[0][..], lfe].concat(),
        3 => [&[0, 2, 1][..], lfe].concat(),
        4 => [&[0, 1][..], lfe, &[2]].concat(),
        5 => [&[0, 2, 1][..], lfe, &[3]].concat(),
        6 => [&[0, 1][..], lfe, &[2, 3]].concat(),
        _ => [&[0, 2, 1][..], lfe, &[3, 4]].concat(),
    }
}

/// The inverse transform of one block, windowed and overlapped with the
/// previous one (section 7.9.4): 256 new samples in `pcm`.
fn imdct(x: &[f32; 256], short: bool, delay: &mut [f32; 256], pcm: &mut [f32; 256]) {
    const N: usize = 512;
    let w = window();
    let mut out = [0f32; N];
    let tw = |k: usize, n: f64| {
        let a = 2.0 * std::f64::consts::PI * (8 * k + 1) as f64 / (8.0 * n);
        (-(a.cos()) as f32, -(a.sin()) as f32)
    };
    if !short {
        let mut z = [(0f32, 0f32); N / 4];
        for (k, zk) in z.iter_mut().enumerate() {
            let (c, s) = tw(k, N as f64);
            let (a, b) = (x[N / 2 - 2 * k - 1], x[2 * k]);
            *zk = (a * c - b * s, b * c + a * s);
        }
        fft(&mut z);
        let mut y = [(0f32, 0f32); N / 4];
        for (n, yn) in y.iter_mut().enumerate() {
            let (c, s) = tw(n, N as f64);
            let (zr, zi) = z[n];
            *yn = (zr * c - zi * s, zi * c + zr * s);
        }
        for n in 0..N / 8 {
            out[2 * n] = -y[N / 8 + n].1 * w[2 * n];
            out[2 * n + 1] = y[N / 8 - n - 1].0 * w[2 * n + 1];
            out[N / 4 + 2 * n] = -y[n].0 * w[N / 4 + 2 * n];
            out[N / 4 + 2 * n + 1] = y[N / 4 - n - 1].1 * w[N / 4 + 2 * n + 1];
            out[N / 2 + 2 * n] = -y[N / 8 + n].0 * w[N / 2 - 2 * n - 1];
            out[N / 2 + 2 * n + 1] = y[N / 8 - n - 1].1 * w[N / 2 - 2 * n - 2];
            out[3 * N / 4 + 2 * n] = y[n].1 * w[N / 4 - 2 * n - 1];
            out[3 * N / 4 + 2 * n + 1] = -y[N / 4 - n - 1].0 * w[N / 4 - 2 * n - 2];
        }
    } else {
        let mut z1 = [(0f32, 0f32); N / 8];
        let mut z2 = [(0f32, 0f32); N / 8];
        for k in 0..N / 8 {
            let (c, s) = tw(k, N as f64 / 2.0);
            // X1 = even coefficients, X2 = odd ones.
            let (a1, b1) = (x[2 * (N / 4 - 2 * k - 1)], x[2 * (2 * k)]);
            let (a2, b2) = (x[2 * (N / 4 - 2 * k - 1) + 1], x[2 * (2 * k) + 1]);
            z1[k] = (a1 * c - b1 * s, b1 * c + a1 * s);
            z2[k] = (a2 * c - b2 * s, b2 * c + a2 * s);
        }
        fft(&mut z1);
        fft(&mut z2);
        let mut y1 = [(0f32, 0f32); N / 8];
        let mut y2 = [(0f32, 0f32); N / 8];
        for n in 0..N / 8 {
            let (c, s) = tw(n, N as f64 / 2.0);
            let (r1, i1) = z1[n];
            let (r2, i2) = z2[n];
            y1[n] = (r1 * c - i1 * s, i1 * c + r1 * s);
            y2[n] = (r2 * c - i2 * s, i2 * c + r2 * s);
        }
        for n in 0..N / 8 {
            out[2 * n] = -y1[n].1 * w[2 * n];
            out[2 * n + 1] = y1[N / 8 - n - 1].0 * w[2 * n + 1];
            out[N / 4 + 2 * n] = -y1[n].0 * w[N / 4 + 2 * n];
            out[N / 4 + 2 * n + 1] = y1[N / 8 - n - 1].1 * w[N / 4 + 2 * n + 1];
            out[N / 2 + 2 * n] = -y2[n].0 * w[N / 2 - 2 * n - 1];
            out[N / 2 + 2 * n + 1] = y2[N / 8 - n - 1].1 * w[N / 2 - 2 * n - 2];
            out[3 * N / 4 + 2 * n] = y2[n].1 * w[N / 4 - 2 * n - 1];
            out[3 * N / 4 + 2 * n + 1] = -y2[N / 8 - n - 1].0 * w[N / 4 - 2 * n - 2];
        }
    }
    for n in 0..N / 2 {
        pcm[n] = 2.0 * (out[n] + delay[n]);
        delay[n] = out[N / 2 + n];
    }
}

/// An in-place complex inverse DFT (sum of z[k]·e^(+j2πkn/N)), radix 2.
fn fft(z: &mut [(f32, f32)]) {
    let n = z.len();
    // Bit-reversed order.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            z.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f64).cos() as f32, (ang * k as f64).sin() as f32);
                let (ar, ai) = z[start + k];
                let (br, bi) = z[start + k + len / 2];
                let (tr, ti) = (br * wr - bi * wi, br * wi + bi * wr);
                z[start + k] = (ar + tr, ai + ti);
                z[start + k + len / 2] = (ar - tr, ai - ti);
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_range_codes() {
        assert_eq!(dynrng(0), 1.0);
        // X = 3: +24 dB less a little; X = −4, Y = ½: −24 dB.
        assert!((dynrng(0b0111_1111) - 16.0 * 63.0 / 64.0).abs() < 1e-6);
        assert!((dynrng(0b1000_0000) - 0.125 * 0.5).abs() < 1e-6);
    }

    #[test]
    fn fft_matches_the_sum() {
        let mut z: Vec<(f32, f32)> = (0..16)
            .map(|i| ((i as f32).sin(), (i as f32 * 0.3).cos()))
            .collect();
        let orig = z.clone();
        fft(&mut z);
        for (n, &(zr, zi)) in z.iter().enumerate() {
            let (mut sr, mut si) = (0f64, 0f64);
            for (k, &(a, b)) in orig.iter().enumerate() {
                let ang = 2.0 * std::f64::consts::PI * (k * n) as f64 / 16.0;
                sr += a as f64 * ang.cos() - b as f64 * ang.sin();
                si += a as f64 * ang.sin() + b as f64 * ang.cos();
            }
            assert!((zr as f64 - sr).abs() < 1e-4 && (zi as f64 - si).abs() < 1e-4);
        }
    }

    /// The encoder's transform (A/52 section 8.2.3.2): long, or two short
    /// ones (α −1 then +1) interleaved.
    fn forward(input: &[f32], short: bool) -> [f32; 256] {
        let w = window();
        let x: Vec<f64> = (0..512)
            .map(|n| input[n] as f64 * w[if n < 256 { n } else { 511 - n }] as f64)
            .collect();
        let transform = |x: &[f64], n: usize, alpha: f64, k: usize| -> f64 {
            let nn = n as f64;
            -2.0 / nn
                * (0..n)
                    .map(|i| {
                        x[i] * (2.0 * std::f64::consts::PI / (4.0 * nn)
                            * (2.0 * i as f64 + 1.0)
                            * (2.0 * k as f64 + 1.0)
                            + std::f64::consts::PI / 4.0 * (2.0 * k as f64 + 1.0) * (1.0 + alpha))
                            .cos()
                    })
                    .sum::<f64>()
        };
        let mut out = [0f32; 256];
        if short {
            for k in 0..128 {
                out[2 * k] = transform(&x[..256], 256, -1.0, k) as f32;
                out[2 * k + 1] = transform(&x[256..], 256, 1.0, k) as f32;
            }
        } else {
            for (k, o) in out.iter_mut().enumerate() {
                *o = transform(&x, 512, 0.0, k) as f32;
            }
        }
        out
    }

    /// Long and short blocks in any order rebuild the signal (the time
    /// aliasing cancels): the inverse transform is the right one.
    #[test]
    fn transforms_rebuild_the_signal() {
        let pattern = [
            false, false, true, true, false, true, false, false, true, false,
        ];
        let len = 256 * (pattern.len() + 1);
        let mut seed = 7u32;
        let signal: Vec<f32> = (0..len)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / 16_777_216.0 - 0.5
            })
            .collect();
        let mut delay = [0f32; 256];
        let mut worst = 0f32;
        for (b, &short) in pattern.iter().enumerate() {
            let coefs = forward(&signal[256 * b..256 * b + 512], short);
            let mut pcm = [0f32; 256];
            imdct(&coefs, short, &mut delay, &mut pcm);
            if b > 0 {
                for n in 0..256 {
                    worst = worst.max((pcm[n] - signal[256 * b + n]).abs());
                }
            }
        }
        assert!(worst < 1e-4, "off by {worst}");
    }

    #[test]
    fn bands_and_frames() {
        let (start, mask) = bands();
        assert_eq!(start[28], 28);
        assert_eq!(start[35], 49);
        assert_eq!(start[49], 229);
        assert_eq!(mask[252], 49);
        assert_eq!(FRAME_WORDS[0], [64, 69, 96]);
        // 448 kbps at 48 kHz: 1792 bytes.
        assert_eq!(FRAME_WORDS[30][0] * 2, 1792);
    }
}
