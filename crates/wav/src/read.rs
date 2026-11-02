//! Reading sound files with random access (only the frames asked for are
//! read): WAV (RIFF, RF64, BW64), Sony Wave64, AIFF and AIFF-C, Sun/NeXT
//! AU and Apple CAF. Integer PCM of 8 to 32 bits and floats of 32 and 64
//! bits in either byte order, G.711 µ-law and A-law, and the WAV ADPCMs of
//! IMA/DVI and Microsoft.

use std::io::{Read, Seek, SeekFrom};

use crate::{Error, Result, SampleFormat};

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_ADPCM: u16 = 2;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_ALAW: u16 = 6;
const WAVE_FORMAT_MULAW: u16 = 7;
const WAVE_FORMAT_IMA_ADPCM: u16 = 0x11;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// The kind of sound file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Wav,
    Wave64,
    Aiff,
    Au,
    Caf,
}

/// How the samples are stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Codec {
    /// Integers of `bits` (8 to 32); 8-bit WAV is unsigned.
    Int {
        bits: u16,
        big: bool,
        unsigned8: bool,
    },
    Float {
        bits: u16,
        big: bool,
    },
    MuLaw,
    ALaw,
    /// IMA/DVI ADPCM in blocks of `align` bytes.
    Ima {
        align: usize,
        per_block: usize,
    },
    /// Microsoft ADPCM in blocks of `align` bytes, with its predictors.
    MsAdpcm {
        align: usize,
        per_block: usize,
        coefs: Vec<(i32, i32)>,
    },
}

impl Codec {
    /// Bytes of one sample of a channel (PCM-like codecs).
    fn bytes(&self) -> usize {
        match self {
            Codec::Int { bits, .. } | Codec::Float { bits, .. } => *bits as usize / 8,
            _ => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub sample_rate: u32,
    pub channels: u16,
    /// What the samples decode to (compressed ones: 16-bit integers).
    pub format: SampleFormat,
    /// Frames (samples of each channel).
    pub frames: u64,
    pub container: Container,
    pub(crate) codec: Codec,
    pub(crate) data_offset: u64,
}

fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().expect("8 bytes"))
}

fn u32be(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// The sample rate in an AIFF COMM chunk: an 80-bit extended float.
fn extended(b: &[u8]) -> u32 {
    let exp = (u16::from_be_bytes([b[0], b[1]]) & 0x7fff) as i32;
    let mant = u64::from_be_bytes(b[2..10].try_into().expect("8 bytes"));
    if exp == 0 || mant == 0 {
        return 0;
    }
    let shift = exp - 16383 - 63;
    let v = if shift >= 0 {
        (mant as f64) * 2f64.powi(shift)
    } else {
        (mant as f64) / 2f64.powi(-shift)
    };
    v.round() as u32
}

/// What a WAVE format chunk (`fmt `) says.
fn wave_codec(b: &[u8]) -> Result<(Codec, u16, u32)> {
    if b.len() < 16 {
        return Err(Error::Corrupt("short format chunk".into()));
    }
    let mut tag = u16le(&b[0..]);
    let channels = u16le(&b[2..]);
    let rate = u32le(&b[4..]);
    let align = u16le(&b[12..]) as usize;
    let bits = u16le(&b[14..]);
    if tag == WAVE_FORMAT_EXTENSIBLE && b.len() >= 26 {
        // The subformat GUID starts with the format's code.
        tag = u16le(&b[24..]);
    }
    let per_block = || -> usize {
        if b.len() >= 20 {
            u16le(&b[18..]) as usize
        } else {
            0
        }
    };
    let codec = match (tag, bits) {
        (WAVE_FORMAT_PCM, 8) => Codec::Int {
            bits,
            big: false,
            unsigned8: true,
        },
        (WAVE_FORMAT_PCM, 16 | 24 | 32) => Codec::Int {
            bits,
            big: false,
            unsigned8: false,
        },
        (WAVE_FORMAT_IEEE_FLOAT, 32 | 64) => Codec::Float { bits, big: false },
        (WAVE_FORMAT_MULAW, 8) => Codec::MuLaw,
        (WAVE_FORMAT_ALAW, 8) => Codec::ALaw,
        (WAVE_FORMAT_IMA_ADPCM, 4) => {
            let ch = channels.max(1) as usize;
            let computed = (align.saturating_sub(4 * ch)) * 8 / (4 * ch) + 1;
            let per = per_block();
            Codec::Ima {
                align,
                per_block: if per > 0 { per } else { computed },
            }
        }
        (WAVE_FORMAT_ADPCM, 4) => {
            let ch = channels.max(1) as usize;
            let per = per_block();
            let n = if b.len() >= 22 {
                u16le(&b[20..]) as usize
            } else {
                0
            };
            let mut coefs: Vec<(i32, i32)> = (0..n)
                .filter_map(|i| {
                    let at = 22 + i * 4;
                    let c = b.get(at..at + 4)?;
                    Some((
                        i16::from_le_bytes([c[0], c[1]]) as i32,
                        i16::from_le_bytes([c[2], c[3]]) as i32,
                    ))
                })
                .collect();
            if coefs.is_empty() {
                coefs = vec![
                    (256, 0),
                    (512, -256),
                    (0, 0),
                    (192, 64),
                    (240, 0),
                    (460, -208),
                    (392, -232),
                ];
            }
            Codec::MsAdpcm {
                align,
                per_block: if per > 0 {
                    per
                } else {
                    (align.saturating_sub(7 * ch)) * 2 / ch + 2
                },
                coefs,
            }
        }
        _ => {
            return Err(Error::Unsupported(format!(
                "format {tag:#x} of {bits} bits"
            )));
        }
    };
    Ok((codec, channels, rate))
}

/// What a sound of `codec` decodes to.
fn decoded_format(codec: &Codec) -> SampleFormat {
    match codec {
        Codec::Int { bits, .. } => SampleFormat::Int(*bits),
        Codec::Float { bits, .. } => SampleFormat::Float(*bits),
        _ => SampleFormat::Int(16),
    }
}

/// Frames in `bytes` of data.
fn frames_in(codec: &Codec, channels: usize, bytes: u64) -> u64 {
    match codec {
        Codec::Ima { align, per_block }
        | Codec::MsAdpcm {
            align, per_block, ..
        } => {
            let blocks = bytes / (*align).max(1) as u64;
            // A last, shorter block still holds whole samples.
            blocks * *per_block as u64
        }
        c => bytes / (c.bytes() * channels).max(1) as u64,
    }
}

fn info(
    container: Container,
    codec: Codec,
    channels: u16,
    rate: u32,
    data_offset: u64,
    bytes: u64,
) -> Result<WavInfo> {
    if channels == 0 || rate == 0 {
        return Err(Error::Corrupt("no channels or no sample rate".into()));
    }
    Ok(WavInfo {
        sample_rate: rate,
        channels,
        format: decoded_format(&codec),
        frames: frames_in(&codec, channels as usize, bytes),
        container,
        codec,
        data_offset,
    })
}

/// Reads the header of a sound file and finds its samples.
pub fn read_info<R: Read + Seek>(r: &mut R) -> Result<WavInfo> {
    r.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 16];
    let n = r.read(&mut head)?;
    let head = &head[..n];
    let end = r.seek(SeekFrom::End(0))?;
    if head.len() >= 12
        && matches!(&head[..4], b"RIFF" | b"RF64" | b"BW64")
        && &head[8..12] == b"WAVE"
    {
        return wav(r, end, &head[..4] != b"RIFF");
    }
    if head.len() >= 16 && head[..16] == crate::w64::RIFF {
        return wave64(r, end);
    }
    if head.len() >= 12 && &head[..4] == b"FORM" && matches!(&head[8..12], b"AIFF" | b"AIFC") {
        return aiff(r, end);
    }
    if head.starts_with(b".snd") {
        return au(r, end);
    }
    if head.starts_with(b"caff") {
        return caf(r, end);
    }
    Err(Error::NotWav)
}

fn wav<R: Read + Seek>(r: &mut R, end: u64, rf64: bool) -> Result<WavInfo> {
    let mut fmt: Option<(Codec, u16, u32)> = None;
    let mut data64: Option<u64> = None;
    let mut pos = 12u64;
    r.seek(SeekFrom::Start(pos))?;
    loop {
        let mut ch = [0u8; 8];
        if r.read_exact(&mut ch).is_err() {
            return Err(Error::Corrupt("no data chunk".into()));
        }
        let size = u32le(&ch[4..]) as u64;
        let body = pos + 8;
        match &ch[..4] {
            b"ds64" => {
                let mut b = vec![0u8; size.min(64) as usize];
                r.read_exact(&mut b)?;
                if b.len() >= 16 {
                    data64 = Some(u64le(&b[8..]));
                }
            }
            b"fmt " => {
                let mut b = vec![0u8; size.min(4096) as usize];
                r.read_exact(&mut b)?;
                fmt = Some(wave_codec(&b)?);
            }
            b"data" => {
                let (codec, channels, rate) =
                    fmt.ok_or_else(|| Error::Corrupt("data before the format".into()))?;
                let size = if rf64 && size == 0xFFFF_FFFF {
                    data64.ok_or_else(|| Error::Corrupt("RF64 without ds64".into()))?
                } else {
                    size
                };
                // Files cut short: what is there.
                let bytes = end.saturating_sub(body).min(size);
                return info(Container::Wav, codec, channels, rate, body, bytes);
            }
            _ => {}
        }
        // Chunks have an even size (a pad byte).
        pos = body + size + (size & 1);
        r.seek(SeekFrom::Start(pos))?;
    }
}

fn wave64<R: Read + Seek>(r: &mut R, end: u64) -> Result<WavInfo> {
    // The 'riff' GUID, a 64-bit size and the 'wave' GUID; then chunks of a
    // GUID and a 64-bit size counting the 24-byte header, 8-byte aligned.
    let mut pos = 40u64;
    let mut fmt: Option<(Codec, u16, u32)> = None;
    loop {
        r.seek(SeekFrom::Start(pos))?;
        let mut ch = [0u8; 24];
        if r.read_exact(&mut ch).is_err() {
            return Err(Error::Corrupt("no data chunk".into()));
        }
        let size = u64le(&ch[16..]);
        if size < 24 {
            return Err(Error::Corrupt("Wave64 chunk size".into()));
        }
        if ch[..16] == crate::w64::FMT {
            let mut b = vec![0u8; (size - 24).min(4096) as usize];
            r.read_exact(&mut b)?;
            fmt = Some(wave_codec(&b)?);
        } else if ch[..16] == crate::w64::DATA {
            let (codec, channels, rate) =
                fmt.ok_or_else(|| Error::Corrupt("data before the format".into()))?;
            let body = pos + 24;
            let bytes = end.saturating_sub(body).min(size - 24);
            return info(Container::Wave64, codec, channels, rate, body, bytes);
        }
        pos += size.div_ceil(8) * 8;
    }
}

fn aiff<R: Read + Seek>(r: &mut R, end: u64) -> Result<WavInfo> {
    let mut comm: Option<(Codec, u16, u32)> = None;
    let mut pos = 12u64;
    loop {
        r.seek(SeekFrom::Start(pos))?;
        let mut ch = [0u8; 8];
        if r.read_exact(&mut ch).is_err() {
            return Err(Error::Corrupt("no sound data".into()));
        }
        let size = u32be(&ch[4..]) as u64;
        match &ch[..4] {
            b"COMM" => {
                let mut b = vec![0u8; size.min(256) as usize];
                r.read_exact(&mut b)?;
                if b.len() < 18 {
                    return Err(Error::Corrupt("short COMM chunk".into()));
                }
                let channels = u16::from_be_bytes([b[0], b[1]]);
                let bits = u16::from_be_bytes([b[6], b[7]]);
                let rate = extended(&b[8..18]);
                // AIFF-C names its compression; plain AIFF is big-endian.
                let kind: [u8; 4] = b
                    .get(18..22)
                    .map_or(*b"NONE", |k| k.try_into().unwrap_or(*b"NONE"));
                let whole = |bits: u16| bits.div_ceil(8) * 8;
                let codec = match &kind {
                    b"NONE" | b"twos" => Codec::Int {
                        bits: whole(bits),
                        big: true,
                        unsigned8: false,
                    },
                    b"sowt" => Codec::Int {
                        bits: whole(bits),
                        big: false,
                        unsigned8: false,
                    },
                    b"in24" => Codec::Int {
                        bits: 24,
                        big: true,
                        unsigned8: false,
                    },
                    b"in32" => Codec::Int {
                        bits: 32,
                        big: true,
                        unsigned8: false,
                    },
                    b"raw " => Codec::Int {
                        bits: 8,
                        big: true,
                        unsigned8: true,
                    },
                    b"fl32" | b"FL32" => Codec::Float {
                        bits: 32,
                        big: true,
                    },
                    b"fl64" | b"FL64" => Codec::Float {
                        bits: 64,
                        big: true,
                    },
                    b"ulaw" | b"ULAW" => Codec::MuLaw,
                    b"alaw" | b"ALAW" => Codec::ALaw,
                    k => {
                        return Err(Error::Unsupported(format!(
                            "AIFF-C compression '{}'",
                            String::from_utf8_lossy(k)
                        )));
                    }
                };
                if let Codec::Int { bits, .. } = codec
                    && !matches!(bits, 8 | 16 | 24 | 32)
                {
                    return Err(Error::Unsupported(format!("AIFF of {bits} bits")));
                }
                comm = Some((codec, channels, rate));
            }
            b"SSND" => {
                let (codec, channels, rate) =
                    comm.ok_or_else(|| Error::Corrupt("sound data before COMM".into()))?;
                let mut b = [0u8; 8];
                r.read_exact(&mut b)?;
                let offset = u32be(&b) as u64;
                let body = pos + 16 + offset;
                let bytes = end
                    .saturating_sub(body)
                    .min(size.saturating_sub(8 + offset));
                return info(Container::Aiff, codec, channels, rate, body, bytes);
            }
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
}

fn au<R: Read + Seek>(r: &mut R, end: u64) -> Result<WavInfo> {
    r.seek(SeekFrom::Start(0))?;
    let mut h = [0u8; 24];
    r.read_exact(&mut h)?;
    let offset = u32be(&h[4..]) as u64;
    let size = u32be(&h[8..]) as u64;
    let encoding = u32be(&h[12..]);
    let rate = u32be(&h[16..]);
    let channels = u32be(&h[20..]) as u16;
    let int = |bits: u16| Codec::Int {
        bits,
        big: true,
        unsigned8: false,
    };
    let codec = match encoding {
        1 => Codec::MuLaw,
        2 => int(8),
        3 => int(16),
        4 => int(24),
        5 => int(32),
        6 => Codec::Float {
            bits: 32,
            big: true,
        },
        7 => Codec::Float {
            bits: 64,
            big: true,
        },
        27 => Codec::ALaw,
        e => return Err(Error::Unsupported(format!("AU encoding {e}"))),
    };
    // A size of all ones: up to the end.
    let available = end.saturating_sub(offset);
    let bytes = if size == 0xFFFF_FFFF {
        available
    } else {
        size.min(available)
    };
    info(Container::Au, codec, channels, rate, offset, bytes)
}

fn caf<R: Read + Seek>(r: &mut R, end: u64) -> Result<WavInfo> {
    let mut desc: Option<(Codec, u16, u32)> = None;
    let mut pos = 8u64;
    loop {
        r.seek(SeekFrom::Start(pos))?;
        let mut ch = [0u8; 12];
        if r.read_exact(&mut ch).is_err() {
            return Err(Error::Corrupt("no audio data".into()));
        }
        let size = i64::from_be_bytes(ch[4..12].try_into().expect("8 bytes"));
        match &ch[..4] {
            b"desc" => {
                let mut b = [0u8; 32];
                r.read_exact(&mut b)?;
                let rate = f64::from_be_bytes(b[..8].try_into().expect("8 bytes")).round() as u32;
                let id = &b[8..12];
                let flags = u32be(&b[12..]);
                let channels = u32be(&b[24..]) as u16;
                let bits = u32be(&b[28..]) as u16;
                let (float, little) = (flags & 1 != 0, flags & 2 != 0);
                let codec = match id {
                    b"lpcm" if float => Codec::Float { bits, big: !little },
                    b"lpcm" => Codec::Int {
                        bits: bits.div_ceil(8) * 8,
                        big: !little,
                        unsigned8: false,
                    },
                    b"ulaw" => Codec::MuLaw,
                    b"alaw" => Codec::ALaw,
                    k => {
                        return Err(Error::Unsupported(format!(
                            "CAF of '{}'",
                            String::from_utf8_lossy(k)
                        )));
                    }
                };
                desc = Some((codec, channels, rate));
            }
            b"data" => {
                let (codec, channels, rate) =
                    desc.ok_or_else(|| Error::Corrupt("data before the description".into()))?;
                // An edit count, then the samples; −1: up to the end.
                let body = pos + 12 + 4;
                let available = end.saturating_sub(body);
                let bytes = if size < 0 {
                    available
                } else {
                    (size as u64).saturating_sub(4).min(available)
                };
                return info(Container::Caf, codec, channels, rate, body, bytes);
            }
            _ => {}
        }
        if size < 0 {
            return Err(Error::Corrupt("no audio data".into()));
        }
        pos += 12 + size as u64;
    }
}

/// A reader with random access.
#[derive(Debug)]
pub struct WavReader<R> {
    pub info: WavInfo,
    inner: R,
}

impl<R: Read + Seek> WavReader<R> {
    pub fn new(mut inner: R) -> Result<WavReader<R>> {
        let info = read_info(&mut inner)?;
        Ok(WavReader { info, inner })
    }

    /// Reads `count` frames from `start` on, as planar floats. What lies
    /// outside the file (negative indices too) comes back as silence.
    pub fn read_planar(&mut self, start: i64, count: usize) -> Result<Vec<Vec<f32>>> {
        let ch = self.info.channels as usize;
        let mut out = vec![vec![0.0f32; count]; ch];
        let first = start.max(0) as u64;
        let last = ((start + count as i64).max(0) as u64).min(self.info.frames);
        if last <= first {
            return Ok(out);
        }
        let skip = (first as i64 - start) as usize;
        let n = (last - first) as usize;
        match &self.info.codec {
            Codec::Ima { align, per_block }
            | Codec::MsAdpcm {
                align, per_block, ..
            } => {
                let (align, per_block) = (*align, *per_block);
                // The blocks that hold the frames, decoded whole.
                let b0 = first / per_block as u64;
                let b1 = (last - 1) / per_block as u64;
                self.inner
                    .seek(SeekFrom::Start(self.info.data_offset + b0 * align as u64))?;
                let mut raw = vec![0u8; (b1 - b0 + 1) as usize * align];
                let got = read_some(&mut self.inner, &mut raw)?;
                raw.truncate(got);
                let mut planes = vec![Vec::with_capacity(raw.len() * 2 / ch.max(1)); ch];
                for block in raw.chunks(align) {
                    match &self.info.codec {
                        Codec::Ima { .. } => ima_block(block, ch, per_block, &mut planes),
                        Codec::MsAdpcm { coefs, .. } => {
                            ms_block(block, ch, per_block, coefs, &mut planes)
                        }
                        _ => unreachable!(),
                    }
                }
                let from = (first - b0 * per_block as u64) as usize;
                for (c, plane) in planes.iter().enumerate() {
                    for i in 0..n {
                        if let Some(v) = plane.get(from + i) {
                            out[c][skip + i] = *v;
                        }
                    }
                }
            }
            codec => {
                let bytes = codec.bytes();
                let block = bytes * ch;
                self.inner.seek(SeekFrom::Start(
                    self.info.data_offset + first * block as u64,
                ))?;
                let mut raw = vec![0u8; n * block];
                let got = read_some(&mut self.inner, &mut raw)?;
                raw.truncate(got - got % block);
                for (i, frame) in raw.chunks_exact(block).enumerate() {
                    for (c, s) in frame.chunks_exact(bytes).enumerate() {
                        out[c][skip + i] = sample(codec, s);
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Reads as much of `buf` as the file has.
fn read_some<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..])? {
            0 => break,
            n => got += n,
        }
    }
    Ok(got)
}

#[inline]
fn sample(codec: &Codec, s: &[u8]) -> f32 {
    match *codec {
        Codec::Int {
            bits: 8, unsigned8, ..
        } => {
            if unsigned8 {
                (s[0] as f32 - 128.0) / 128.0
            } else {
                s[0] as i8 as f32 / 128.0
            }
        }
        Codec::Int { bits: 16, big, .. } => {
            let b = [s[0], s[1]];
            (if big {
                i16::from_be_bytes(b)
            } else {
                i16::from_le_bytes(b)
            }) as f32
                / 32768.0
        }
        Codec::Int { bits: 24, big, .. } => {
            let v = if big {
                i32::from_be_bytes([s[0], s[1], s[2], 0])
            } else {
                i32::from_le_bytes([0, s[0], s[1], s[2]])
            } >> 8;
            v as f32 / 8_388_608.0
        }
        Codec::Int { big, .. } => {
            let b = [s[0], s[1], s[2], s[3]];
            (if big {
                i32::from_be_bytes(b)
            } else {
                i32::from_le_bytes(b)
            }) as f32
                / 2_147_483_648.0
        }
        Codec::Float { bits: 32, big } => {
            let b = [s[0], s[1], s[2], s[3]];
            if big {
                f32::from_be_bytes(b)
            } else {
                f32::from_le_bytes(b)
            }
        }
        Codec::Float { big, .. } => {
            let b: [u8; 8] = s[..8].try_into().expect("8 bytes");
            (if big {
                f64::from_be_bytes(b)
            } else {
                f64::from_le_bytes(b)
            }) as f32
        }
        Codec::MuLaw => mulaw(s[0]) as f32 / 32768.0,
        Codec::ALaw => alaw(s[0]) as f32 / 32768.0,
        _ => 0.0,
    }
}

/// G.711 µ-law to 16-bit linear.
fn mulaw(b: u8) -> i16 {
    let u = !b;
    let t = (((u & 0x0f) as i32) << 3) + 0x84;
    let t = t << ((u & 0x70) >> 4);
    (if u & 0x80 != 0 { 0x84 - t } else { t - 0x84 }) as i16
}

/// G.711 A-law to 16-bit linear.
fn alaw(b: u8) -> i16 {
    let a = b ^ 0x55;
    let mut t = ((a & 0x0f) as i32) << 4;
    let seg = ((a & 0x70) >> 4) as i32;
    match seg {
        0 => t += 8,
        1 => t += 0x108,
        _ => {
            t += 0x108;
            t <<= seg - 1;
        }
    }
    (if a & 0x80 != 0 { t } else { -t }) as i16
}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// One IMA/DVI ADPCM block (WAV layout): each channel's first sample and
/// step, then 4 bytes (8 samples) of each channel in turn.
fn ima_block(b: &[u8], ch: usize, per_block: usize, out: &mut [Vec<f32>]) {
    if b.len() < 4 * ch {
        return;
    }
    let mut pred = vec![0i32; ch];
    let mut index = vec![0i32; ch];
    for c in 0..ch {
        pred[c] = i16::from_le_bytes([b[c * 4], b[c * 4 + 1]]) as i32;
        index[c] = (b[c * 4 + 2] as i32).clamp(0, 88);
        out[c].push(pred[c] as f32 / 32768.0);
    }
    let data = &b[4 * ch..];
    let groups = data.len() / (4 * ch);
    let mut decoded = 1;
    for g in 0..groups {
        let mut produced = 0;
        for c in 0..ch {
            let chunk = &data[(g * ch + c) * 4..(g * ch + c) * 4 + 4];
            produced = 0;
            for &byte in chunk {
                for nibble in [byte & 15, byte >> 4] {
                    if decoded + produced >= per_block {
                        break;
                    }
                    let step = IMA_STEPS[index[c] as usize];
                    let mut diff = step >> 3;
                    if nibble & 4 != 0 {
                        diff += step;
                    }
                    if nibble & 2 != 0 {
                        diff += step >> 1;
                    }
                    if nibble & 1 != 0 {
                        diff += step >> 2;
                    }
                    pred[c] = if nibble & 8 != 0 {
                        pred[c] - diff
                    } else {
                        pred[c] + diff
                    }
                    .clamp(-32768, 32767);
                    index[c] = (index[c] + IMA_INDEX[nibble as usize]).clamp(0, 88);
                    out[c].push(pred[c] as f32 / 32768.0);
                    produced += 1;
                }
            }
        }
        decoded += produced;
    }
}

const MS_ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

/// One Microsoft ADPCM block: each channel's predictor, step and two first
/// samples, then nibbles (high first, channels in turn).
fn ms_block(b: &[u8], ch: usize, per_block: usize, coefs: &[(i32, i32)], out: &mut [Vec<f32>]) {
    if b.len() < 7 * ch {
        return;
    }
    let mut c1 = vec![0i32; ch];
    let mut c2 = vec![0i32; ch];
    let mut delta = vec![0i32; ch];
    let mut s1 = vec![0i32; ch];
    let mut s2 = vec![0i32; ch];
    for c in 0..ch {
        let (a, bb) = coefs.get(b[c] as usize).copied().unwrap_or((256, 0));
        c1[c] = a;
        c2[c] = bb;
        let at = |k: usize| i16::from_le_bytes([b[k], b[k + 1]]) as i32;
        delta[c] = at(ch + c * 2);
        s1[c] = at(3 * ch + c * 2);
        s2[c] = at(5 * ch + c * 2);
    }
    for c in 0..ch {
        out[c].push(s2[c] as f32 / 32768.0);
        out[c].push(s1[c] as f32 / 32768.0);
    }
    let mut c = 0usize;
    let mut decoded = 2usize;
    for &byte in &b[7 * ch..] {
        for nibble in [byte >> 4, byte & 15] {
            if decoded >= per_block {
                return;
            }
            let signed = if nibble & 8 != 0 {
                nibble as i32 - 16
            } else {
                nibble as i32
            };
            let pred = (s1[c] * c1[c] + s2[c] * c2[c]) / 256 + signed * delta[c];
            let v = pred.clamp(-32768, 32767);
            s2[c] = s1[c];
            s1[c] = v;
            delta[c] = (MS_ADAPT[nibble as usize] * delta[c] / 256).max(16);
            out[c].push(v as f32 / 32768.0);
            c += 1;
            if c == ch {
                c = 0;
                decoded += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g711_tables() {
        // Known values of the G.711 tables.
        assert_eq!(mulaw(0xff), 0);
        assert_eq!(mulaw(0x80), 32124);
        assert_eq!(mulaw(0x00), -32124);
        assert_eq!(alaw(0xd5), 8);
        assert_eq!(alaw(0x55), -8);
        assert_eq!(alaw(0xaa), 32256);
    }

    #[test]
    fn aiff_rate_is_extended() {
        // 44100 as an 80-bit extended float.
        let b = [0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0];
        assert_eq!(extended(&b), 44100);
    }
}
