//! QuickTime's own video codecs, decoded by BananaV: what a `.mov` holds
//! when it was made by a program's "Animation" or "None" (uncompressed)
//! setting — formats Windows has no decoder for. A [`Decoder`] is made from
//! a track's sample entry (its code, size, depth and color table) and
//! decodes one frame's bytes at a time:
//!
//! - **Animation** (`rle `): run lengths line by line over the frame before
//!   (only the lines that changed); 2, 4 and 8 bits with a palette, 34,
//!   36 and 40 for grays, 16 (5 bits a color), 24 (RGB) and 32 (ARGB);
//! - **uncompressed Y′CbCr**: `2vuy` (8-bit 4:2:2, Cb Y Cr Y), `yuv2`
//!   (8-bit 4:2:2, Y Cb Y Cr, its colors signed), `v210` (10-bit 4:2:2,
//!   six pixels in four 32-bit words), `v410` (10-bit 4:4:4), `v308`
//!   (8-bit 4:4:4), `v408` (8-bit 4:4:4 with alpha);
//! - **uncompressed RGB**: `raw ` (as deep as its sample entry says),
//!   `r210` and `R10k` (10 bits a color), `b48r` and `b64a` (16 bits),
//!   `BGRA`.
//!
//! Rows may be padded: a frame's row length is its bytes over its height.

/// What went wrong with a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The frame's bytes end before the picture does.
    Short,
    /// A kind of the format BananaV does not decode (Animation's 1 bit).
    Unsupported(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Short => f.write_str("the frame is shorter than its picture"),
            Error::Unsupported(what) => write!(f, "{what} is not supported"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A decoded frame's samples.
#[derive(Clone, Debug, PartialEq)]
pub enum Pixels {
    /// RGBA, 8 bits each, alpha straight (255 where the format has none).
    Rgba8(Vec<u8>),
    /// RGBA, 16 bits each, alpha straight.
    Rgba16(Vec<u16>),
    /// Y′CbCr planes of 10-bit studio levels (8-bit formats' values times
    /// four): full-width chroma when `chroma444`, else half (4:2:2).
    Yuv {
        chroma444: bool,
        y: Vec<u16>,
        cb: Vec<u16>,
        cr: Vec<u16>,
        /// 0–65535, straight.
        alpha: Option<Vec<u16>>,
    },
}

/// A decoded frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

/// The formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Animation,
    Uyvy,
    Yuv2,
    V210,
    V410,
    V308,
    V408,
    Raw,
    R210,
    R10k,
    B48r,
    B64a,
    Bgra,
}

impl Codec {
    /// The format a sample entry's code names.
    pub fn of(code: [u8; 4]) -> Option<Codec> {
        Some(match &code {
            b"rle " => Codec::Animation,
            b"2vuy" | b"2Vuy" => Codec::Uyvy,
            b"yuv2" => Codec::Yuv2,
            b"v210" => Codec::V210,
            b"v410" => Codec::V410,
            b"v308" => Codec::V308,
            b"v408" => Codec::V408,
            b"raw " => Codec::Raw,
            b"r210" => Codec::R210,
            b"R10k" => Codec::R10k,
            b"b48r" => Codec::B48r,
            b"b64a" => Codec::B64a,
            b"BGRA" => Codec::Bgra,
            _ => return None,
        })
    }

    /// Whether its frames have alpha at this depth.
    pub fn alpha(self, depth: u16) -> bool {
        match self {
            Codec::Animation | Codec::Raw => depth == 32,
            Codec::V408 | Codec::B64a | Codec::Bgra => true,
            _ => false,
        }
    }
}

/// Whether BananaV decodes a track of `code` at `depth` (Animation's 1
/// bit is not here).
pub fn supported(code: [u8; 4], depth: u16) -> bool {
    match Codec::of(code) {
        Some(Codec::Animation) => matches!(depth, 2 | 4 | 8 | 16 | 24 | 32 | 34 | 36 | 40),
        Some(Codec::Raw) => matches!(depth, 1 | 2 | 4 | 8 | 16 | 24 | 32 | 33 | 34 | 36 | 40),
        Some(_) => true,
        None => false,
    }
}

/// QuickTime's default 256 colors (index 0 white, 255 black): a cube of
/// six levels of each color, then ramps of red, green, blue and gray.
pub fn default_palette_256() -> Vec<[u8; 3]> {
    let mut p = Vec::with_capacity(256);
    for i in 0..215u32 {
        let level = |k: u32| (0xFF - 0x33 * k) as u8;
        p.push([level(i / 36), level((i / 6) % 6), level(i % 6)]);
    }
    let ramp = [0xEE, 0xDD, 0xBB, 0xAA, 0x88, 0x77, 0x55, 0x44, 0x22, 0x11];
    for c in 0..3 {
        for v in ramp {
            let mut rgb = [0u8; 3];
            rgb[c] = v;
            p.push(rgb);
        }
    }
    for v in ramp {
        p.push([v, v, v]);
    }
    p.push([0, 0, 0]);
    p
}

/// `n` grays from white (index 0) to black.
fn grays(n: usize) -> Vec<[u8; 3]> {
    (0..n)
        .map(|i| {
            let v = (255 - i * 255 / (n - 1)) as u8;
            [v, v, v]
        })
        .collect()
}

/// Bits per pixel of a palette's or gray's depth (34, 36 and 40 are 2, 4 and
/// 8 bits of gray).
fn index_bits(depth: u16) -> Option<u32> {
    match depth {
        1 | 33 => Some(1),
        2 | 34 => Some(2),
        4 | 36 => Some(4),
        8 | 40 => Some(8),
        _ => None,
    }
}

/// Decodes a track's frames.
#[derive(Clone, Debug)]
pub struct Decoder {
    codec: Codec,
    width: usize,
    height: usize,
    depth: u16,
    palette: Vec<[u8; 3]>,
    /// Animation: the picture so far (RGBA 8-bit), frames changing only
    /// some of its lines.
    canvas: Vec<u8>,
}

impl Decoder {
    /// A decoder for a track of `code` (its sample entry's), `width` ×
    /// `height`, `depth` bits a pixel and its own color table `palette`
    /// (empty: QuickTime's default ones); `None` for a format not here.
    pub fn new(
        code: [u8; 4],
        width: u32,
        height: u32,
        depth: u16,
        palette: &[[u8; 3]],
    ) -> Option<Decoder> {
        let codec = Codec::of(code)?;
        let palette = if !palette.is_empty() {
            let mut p = palette.to_vec();
            p.resize(256, [0, 0, 0]);
            p
        } else {
            match depth {
                33 | 34 | 36 | 40 => grays(1 << index_bits(depth).unwrap_or(8)),
                8 => default_palette_256(),
                // QuickTime's 2- and 4-bit defaults are its own colors:
                // grays stand in for them.
                d => grays(1 << index_bits(d).unwrap_or(8)),
            }
        };
        Some(Decoder {
            codec,
            width: width as usize,
            height: height as usize,
            depth,
            palette,
            canvas: Vec::new(),
        })
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Whether every frame stands alone (Animation's build on the one
    /// before, from a keyframe).
    pub fn intra_only(&self) -> bool {
        self.codec != Codec::Animation
    }

    /// Forgets the frames before (a seek: decoding goes on from a
    /// keyframe).
    pub fn reset(&mut self) {
        self.canvas.clear();
    }

    /// Decodes a frame's bytes.
    pub fn decode(&mut self, data: &[u8]) -> Result<Frame> {
        let pixels = match self.codec {
            Codec::Animation => Pixels::Rgba8(self.animation(data)?),
            Codec::Uyvy => self.yuv422_8(data, [1, 0, 3, 2], false)?,
            Codec::Yuv2 => self.yuv422_8(data, [0, 1, 2, 3], true)?,
            Codec::V210 => self.v210(data)?,
            Codec::V410 => self.v410(data)?,
            Codec::V308 => self.yuv444_8(data, 3)?,
            Codec::V408 => self.yuv444_8(data, 4)?,
            Codec::Raw => self.raw(data)?,
            Codec::R210 | Codec::R10k => self.rgb10(data)?,
            Codec::B48r => self.rgb16(data, false)?,
            Codec::B64a => self.rgb16(data, true)?,
            Codec::Bgra => self.bgra(data)?,
        };
        Ok(Frame {
            width: self.width as u32,
            height: self.height as u32,
            pixels,
        })
    }

    /// The bytes of each row (padded rows: the frame's bytes over its
    /// height), at least `min`.
    fn stride(&self, data: &[u8], min: usize) -> Result<usize> {
        let h = self.height.max(1);
        let stride = (data.len() / h).max(min);
        if data.len() < stride * (h - 1) + min {
            return Err(Error::Short);
        }
        Ok(stride)
    }

    /// 8-bit 4:2:2 in pairs of pixels: `order` gives Y0, Cb, Y1, Cr's places
    /// in each four bytes; `signed` colors are offset by half.
    fn yuv422_8(&self, data: &[u8], order: [usize; 4], signed: bool) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let cw = w.div_ceil(2);
        let stride = self.stride(data, cw * 4)?;
        let (mut y, mut cb, mut cr) = (vec![0u16; w * h], vec![0u16; cw * h], vec![0u16; cw * h]);
        let off = if signed { 0x80 } else { 0 };
        for row in 0..h {
            let line = &data[row * stride..];
            for k in 0..cw {
                let q = &line[k * 4..k * 4 + 4];
                y[row * w + 2 * k] = u16::from(q[order[0]]) << 2;
                if 2 * k + 1 < w {
                    y[row * w + 2 * k + 1] = u16::from(q[order[2]]) << 2;
                }
                cb[row * cw + k] = u16::from(q[order[1]] ^ off) << 2;
                cr[row * cw + k] = u16::from(q[order[3]] ^ off) << 2;
            }
        }
        Ok(Pixels::Yuv {
            chroma444: false,
            y,
            cb,
            cr,
            alpha: None,
        })
    }

    /// `v210`: 10-bit 4:2:2, six pixels in four little-endian words of three
    /// samples each (Cb Y Cr, Y Cb Y, Cr Y Cb, Y Cr Y).
    fn v210(&self, data: &[u8]) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let cw = w.div_ceil(2);
        let groups = w.div_ceil(6);
        let stride = self.stride(data, groups * 16)?;
        let (mut y, mut cb, mut cr) = (vec![0u16; w * h], vec![0u16; cw * h], vec![0u16; cw * h]);
        for row in 0..h {
            let line = &data[row * stride..];
            // The samples in order: Cb0 Y0 Cr0 Y1 Cb1 Y2 Cr1 Y3 Cb2 Y4 Cr2 Y5.
            let mut samples = Vec::with_capacity(groups * 12);
            for g in 0..groups {
                for k in 0..4 {
                    let at = g * 16 + k * 4;
                    let word =
                        u32::from_le_bytes([line[at], line[at + 1], line[at + 2], line[at + 3]]);
                    for s in 0..3 {
                        samples.push(((word >> (10 * s)) & 0x3FF) as u16);
                    }
                }
            }
            for (i, pair) in samples.as_chunks::<4>().0.iter().enumerate() {
                let x = 2 * i;
                if x >= w {
                    break;
                }
                cb[row * cw + i] = pair[0];
                y[row * w + x] = pair[1];
                cr[row * cw + i] = pair[2];
                if x + 1 < w {
                    y[row * w + x + 1] = pair[3];
                }
            }
        }
        Ok(Pixels::Yuv {
            chroma444: false,
            y,
            cb,
            cr,
            alpha: None,
        })
    }

    /// `v410`: 10-bit 4:4:4, a little-endian word a pixel (Cb, Y and Cr from
    /// bit 2 up).
    fn v410(&self, data: &[u8]) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let stride = self.stride(data, w * 4)?;
        let (mut y, mut cb, mut cr) = (vec![0u16; w * h], vec![0u16; w * h], vec![0u16; w * h]);
        for row in 0..h {
            let line = &data[row * stride..];
            for x in 0..w {
                let at = x * 4;
                let word = u32::from_le_bytes([line[at], line[at + 1], line[at + 2], line[at + 3]]);
                let i = row * w + x;
                cb[i] = ((word >> 2) & 0x3FF) as u16;
                y[i] = ((word >> 12) & 0x3FF) as u16;
                cr[i] = ((word >> 22) & 0x3FF) as u16;
            }
        }
        Ok(Pixels::Yuv {
            chroma444: true,
            y,
            cb,
            cr,
            alpha: None,
        })
    }

    /// `v308` (Cr Y Cb) and `v408` (Cb Y Cr A): 8-bit 4:4:4.
    fn yuv444_8(&self, data: &[u8], bytes: usize) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let stride = self.stride(data, w * bytes)?;
        let n = w * h;
        let (mut y, mut cb, mut cr) = (vec![0u16; n], vec![0u16; n], vec![0u16; n]);
        let mut alpha = (bytes == 4).then(|| vec![0u16; n]);
        for row in 0..h {
            let line = &data[row * stride..];
            for x in 0..w {
                let p = &line[x * bytes..x * bytes + bytes];
                let i = row * w + x;
                if bytes == 3 {
                    cr[i] = u16::from(p[0]) << 2;
                    y[i] = u16::from(p[1]) << 2;
                    cb[i] = u16::from(p[2]) << 2;
                } else {
                    cb[i] = u16::from(p[0]) << 2;
                    y[i] = u16::from(p[1]) << 2;
                    cr[i] = u16::from(p[2]) << 2;
                    if let Some(a) = alpha.as_mut() {
                        a[i] = u16::from(p[3]) * 257;
                    }
                }
            }
        }
        Ok(Pixels::Yuv {
            chroma444: true,
            y,
            cb,
            cr,
            alpha,
        })
    }

    /// `raw `: RGB as deep as the sample entry says.
    fn raw(&self, data: &[u8]) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let mut out = vec![255u8; w * h * 4];
        match self.depth {
            24 | 32 | 16 => {
                let bytes = usize::from(self.depth / 8);
                let stride = self.stride(data, w * bytes)?;
                for row in 0..h {
                    let line = &data[row * stride..];
                    for x in 0..w {
                        let o = (row * w + x) * 4;
                        let p = &line[x * bytes..x * bytes + bytes];
                        out[o..o + 4].copy_from_slice(&match bytes {
                            3 => [p[0], p[1], p[2], 255],
                            4 => [p[1], p[2], p[3], p[0]],
                            _ => rgb555(u16::from_be_bytes([p[0], p[1]])),
                        });
                    }
                }
            }
            d => {
                let bits = index_bits(d)
                    .ok_or_else(|| Error::Unsupported(format!("Uncompressed RGB of {d} bits")))?
                    as usize;
                let stride = self.stride(data, (w * bits).div_ceil(8))?;
                for row in 0..h {
                    let line = &data[row * stride..];
                    for x in 0..w {
                        let bit = x * bits;
                        let byte = line[bit / 8];
                        let index = (byte >> (8 - bits - bit % 8)) & ((1 << bits) - 1) as u8;
                        let c = self
                            .palette
                            .get(usize::from(index))
                            .copied()
                            .unwrap_or([0, 0, 0]);
                        let o = (row * w + x) * 4;
                        out[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
            }
        }
        Ok(Pixels::Rgba8(out))
    }

    /// `r210` (2 spare bits first) and `R10k` (2 spare bits last): 10-bit
    /// RGB in a big-endian word a pixel.
    fn rgb10(&self, data: &[u8]) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let stride = self.stride(data, w * 4)?;
        let shift = if self.codec == Codec::R10k { 2 } else { 0 };
        let wide = |v: u32| -> u16 {
            let v = (v & 0x3FF) as u16;
            (v << 6) | (v >> 4)
        };
        let mut out = vec![u16::MAX; w * h * 4];
        for row in 0..h {
            let line = &data[row * stride..];
            for x in 0..w {
                let at = x * 4;
                let word = u32::from_be_bytes([line[at], line[at + 1], line[at + 2], line[at + 3]])
                    >> shift;
                let o = (row * w + x) * 4;
                out[o] = wide(word >> 20);
                out[o + 1] = wide(word >> 10);
                out[o + 2] = wide(word);
            }
        }
        Ok(Pixels::Rgba16(out))
    }

    /// `b48r` (RGB) and `b64a` (ARGB): 16 bits each, big-endian.
    fn rgb16(&self, data: &[u8], alpha: bool) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let bytes = if alpha { 8 } else { 6 };
        let stride = self.stride(data, w * bytes)?;
        let mut out = vec![u16::MAX; w * h * 4];
        for row in 0..h {
            let line = &data[row * stride..];
            for x in 0..w {
                let p = &line[x * bytes..x * bytes + bytes];
                let v = |k: usize| u16::from_be_bytes([p[2 * k], p[2 * k + 1]]);
                let o = (row * w + x) * 4;
                if alpha {
                    out[o..o + 4].copy_from_slice(&[v(1), v(2), v(3), v(0)]);
                } else {
                    out[o..o + 3].copy_from_slice(&[v(0), v(1), v(2)]);
                }
            }
        }
        Ok(Pixels::Rgba16(out))
    }

    /// `BGRA`: 8 bits each.
    fn bgra(&self, data: &[u8]) -> Result<Pixels> {
        let (w, h) = (self.width, self.height);
        let stride = self.stride(data, w * 4)?;
        let mut out = vec![0u8; w * h * 4];
        for row in 0..h {
            let line = &data[row * stride..];
            for x in 0..w {
                let p = &line[x * 4..x * 4 + 4];
                let o = (row * w + x) * 4;
                out[o..o + 4].copy_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        Ok(Pixels::Rgba8(out))
    }

    /// Bytes of one pixel group of Animation at this depth, and the pixels
    /// it holds.
    fn group(&self) -> Result<(usize, usize)> {
        Ok(match self.depth {
            32 => (4, 1),
            24 => (3, 1),
            16 => (2, 1),
            d => match index_bits(d) {
                Some(8) => (4, 4),
                Some(4) => (4, 8),
                Some(2) => (4, 16),
                _ => {
                    return Err(Error::Unsupported(format!("Animation of {d} bits")));
                }
            },
        })
    }

    /// Animation: a frame's lines changed over the picture before.
    fn animation(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let (w, h) = (self.width, self.height);
        if self.canvas.len() != w * h * 4 {
            self.canvas = vec![0; w * h * 4];
        }
        // Fewer than 8 bytes: nothing changed.
        if data.len() < 8 {
            return Ok(self.canvas.clone());
        }
        let (group_bytes, per_group) = self.group()?;
        let size =
            (u32::from_be_bytes([data[0], data[1], data[2], data[3]]) & 0x3FFF_FFFF) as usize;
        let data = &data[..size.clamp(8, data.len())];
        let header = u16::from_be_bytes([data[4], data[5]]);
        let mut at = 6;
        let get = |at: &mut usize| -> Result<u8> {
            let b = *data.get(*at).ok_or(Error::Short)?;
            *at += 1;
            Ok(b)
        };
        let (start, lines) = if header & 0x0008 != 0 {
            let s = data.get(at..at + 8).ok_or(Error::Short)?;
            at += 8;
            (
                usize::from(u16::from_be_bytes([s[0], s[1]])),
                usize::from(u16::from_be_bytes([s[4], s[5]])),
            )
        } else {
            (0, h)
        };
        let mut group = Vec::with_capacity(16);
        let depth = self.depth;
        let palette = &self.palette;
        let canvas = &mut self.canvas;
        let put = |canvas: &mut Vec<u8>, row: usize, x: &mut usize, px: &[[u8; 4]]| {
            for p in px {
                if *x < w {
                    let o = (row * w + *x) * 4;
                    canvas[o..o + 4].copy_from_slice(p);
                }
                *x += 1;
            }
        };
        for row in start..(start + lines).min(h) {
            let skip = get(&mut at)?;
            if skip == 0 {
                break;
            }
            let mut x = usize::from(skip - 1) * per_group;
            loop {
                let code = get(&mut at)? as i8;
                match code {
                    0 => {
                        let skip = get(&mut at)?;
                        x += usize::from(skip.saturating_sub(1)) * per_group;
                    }
                    -1 => break,
                    n if n > 0 => {
                        for _ in 0..n {
                            let b = data.get(at..at + group_bytes).ok_or(Error::Short)?;
                            at += group_bytes;
                            group_pixels(depth, palette, b, &mut group);
                            put(canvas, row, &mut x, &group);
                        }
                    }
                    n => {
                        let b = data.get(at..at + group_bytes).ok_or(Error::Short)?;
                        at += group_bytes;
                        group_pixels(depth, palette, b, &mut group);
                        for _ in 0..usize::from(n.unsigned_abs()) {
                            put(canvas, row, &mut x, &group);
                        }
                    }
                }
            }
        }
        Ok(self.canvas.clone())
    }
}

/// The pixels of one Animation pixel group at `depth` (its colors from
/// `palette` when it holds indices).
fn group_pixels(depth: u16, palette: &[[u8; 3]], bytes: &[u8], out: &mut Vec<[u8; 4]>) {
    out.clear();
    match depth {
        32 => out.push([bytes[1], bytes[2], bytes[3], bytes[0]]),
        24 => out.push([bytes[0], bytes[1], bytes[2], 255]),
        16 => out.push(rgb555(u16::from_be_bytes([bytes[0], bytes[1]]))),
        d => {
            let bits = index_bits(d).unwrap_or(8) as usize;
            for &b in bytes {
                for k in 0..8 / bits {
                    let index = (b >> (8 - bits * (k + 1))) & ((1u16 << bits) - 1) as u8;
                    let c = palette
                        .get(usize::from(index))
                        .copied()
                        .unwrap_or([0, 0, 0]);
                    out.push([c[0], c[1], c[2], 255]);
                }
            }
        }
    }
}

/// A 16-bit pixel (a spare bit, then 5 bits each of red, green and blue).
fn rgb555(v: u16) -> [u8; 4] {
    let c = |shift: u16| {
        let x = ((v >> shift) & 0x1F) as u8;
        (x << 3) | (x >> 2)
    };
    [c(10), c(5), c(0), 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(code: &[u8; 4], w: u32, h: u32, depth: u16, data: &[u8]) -> Frame {
        Decoder::new(*code, w, h, depth, &[])
            .expect("a decoder")
            .decode(data)
            .expect("decodes")
    }

    #[test]
    fn the_default_colors_are_quicktimes() {
        let p = default_palette_256();
        assert_eq!(p.len(), 256);
        assert_eq!(p[0], [255, 255, 255]);
        assert_eq!(p[1], [255, 255, 0xCC]);
        assert_eq!(p[214], [0, 0, 0x33]);
        assert_eq!(p[215], [0xEE, 0, 0]);
        assert_eq!(p[245], [0xEE, 0xEE, 0xEE]);
        assert_eq!(p[255], [0, 0, 0]);
    }

    /// 8-bit 4:2:2 in both orders (`yuv2`'s colors signed), padded rows.
    #[test]
    fn eight_bit_422_comes_apart() {
        // 2 × 2, rows of 6 bytes (4 used).
        let data = [16, 100, 200, 110, 0, 0, 20, 120, 210, 130, 0, 0];
        let f = decode(b"2vuy", 2, 2, 24, &data);
        let Pixels::Yuv {
            y,
            cb,
            cr,
            chroma444,
            ..
        } = f.pixels
        else {
            panic!()
        };
        assert!(!chroma444);
        assert_eq!(y, [100 << 2, 110 << 2, 120 << 2, 130 << 2]);
        assert_eq!(cb, [16 << 2, 20 << 2]);
        assert_eq!(cr, [200 << 2, 210 << 2]);
        let f = decode(b"yuv2", 2, 1, 24, &[100, 0x10, 110, 0x90]);
        let Pixels::Yuv { y, cb, cr, .. } = f.pixels else {
            panic!()
        };
        assert_eq!(y, [100 << 2, 110 << 2]);
        assert_eq!(cb, [0x90 << 2]);
        assert_eq!(cr, [0x10 << 2]);
    }

    /// `v210`: twelve samples in four words, in their order.
    #[test]
    fn v210_unpacks_six_pixels() {
        let samples: Vec<u32> = (1..=12).map(|i| i * 50).collect();
        let mut data = Vec::new();
        for w in samples.chunks(3) {
            let word = w[0] | (w[1] << 10) | (w[2] << 20);
            data.extend_from_slice(&word.to_le_bytes());
        }
        data.resize(128, 0);
        let f = decode(b"v210", 6, 1, 24, &data);
        let Pixels::Yuv { y, cb, cr, .. } = f.pixels else {
            panic!()
        };
        // Cb0 Y0 Cr0 Y1 Cb1 Y2 Cr1 Y3 Cb2 Y4 Cr2 Y5.
        assert_eq!(y, [100, 200, 300, 400, 500, 600]);
        assert_eq!(cb, [50, 250, 450]);
        assert_eq!(cr, [150, 350, 550]);
    }

    #[test]
    fn four_four_four_formats() {
        let word = (300u32 << 2) | (500 << 12) | (700 << 22);
        let f = decode(b"v410", 1, 1, 24, &word.to_le_bytes());
        let Pixels::Yuv {
            y,
            cb,
            cr,
            chroma444,
            ..
        } = f.pixels
        else {
            panic!()
        };
        assert!(chroma444);
        assert_eq!((y[0], cb[0], cr[0]), (500, 300, 700));
        let f = decode(b"v308", 1, 1, 24, &[10, 20, 30]);
        let Pixels::Yuv { y, cb, cr, .. } = f.pixels else {
            panic!()
        };
        assert_eq!((cr[0], y[0], cb[0]), (40, 80, 120));
        let f = decode(b"v408", 1, 1, 32, &[10, 20, 30, 255]);
        let Pixels::Yuv {
            y, cb, cr, alpha, ..
        } = f.pixels
        else {
            panic!()
        };
        assert_eq!((cb[0], y[0], cr[0]), (40, 80, 120));
        assert_eq!(alpha, Some(vec![65535]));
    }

    /// Ten-bit RGB both ways, sixteen-bit RGB and ARGB, BGRA.
    #[test]
    fn rgb_formats() {
        let word = (1023u32 << 20) | (512 << 10) | 1;
        let f = decode(b"r210", 1, 1, 24, &word.to_be_bytes());
        assert_eq!(
            f.pixels,
            Pixels::Rgba16(vec![65535, (512 << 6) | (512 >> 4), (1 << 6), 65535])
        );
        let f = decode(b"R10k", 1, 1, 24, &(word << 2).to_be_bytes());
        assert_eq!(
            f.pixels,
            Pixels::Rgba16(vec![65535, (512 << 6) | (512 >> 4), (1 << 6), 65535])
        );
        let f = decode(
            b"b64a",
            1,
            1,
            64,
            &[0x80, 0, 0x10, 0x00, 0x20, 0x00, 0x30, 0x00],
        );
        assert_eq!(
            f.pixels,
            Pixels::Rgba16(vec![0x1000, 0x2000, 0x3000, 0x8000])
        );
        let f = decode(b"b48r", 1, 1, 48, &[0x10, 0, 0x20, 0, 0x30, 0]);
        assert_eq!(
            f.pixels,
            Pixels::Rgba16(vec![0x1000, 0x2000, 0x3000, 65535])
        );
        let f = decode(b"BGRA", 1, 1, 32, &[1, 2, 3, 4]);
        assert_eq!(f.pixels, Pixels::Rgba8(vec![3, 2, 1, 4]));
    }

    /// `raw `: 24 and 32 bits, 16 (5 bits each), 8 with QuickTime's colors.
    #[test]
    fn raw_rgb_at_its_depths() {
        let f = decode(b"raw ", 1, 1, 24, &[1, 2, 3]);
        assert_eq!(f.pixels, Pixels::Rgba8(vec![1, 2, 3, 255]));
        let f = decode(b"raw ", 1, 1, 32, &[9, 1, 2, 3]);
        assert_eq!(f.pixels, Pixels::Rgba8(vec![1, 2, 3, 9]));
        let f = decode(b"raw ", 1, 1, 16, &0x7C00u16.to_be_bytes());
        assert_eq!(f.pixels, Pixels::Rgba8(vec![255, 0, 0, 255]));
        let f = decode(b"raw ", 2, 1, 8, &[0, 255]);
        assert_eq!(
            f.pixels,
            Pixels::Rgba8(vec![255, 255, 255, 255, 0, 0, 0, 255])
        );
        let f = Decoder::new(
            *b"raw ",
            4,
            1,
            2,
            &[[1, 1, 1], [2, 2, 2], [3, 3, 3], [4, 4, 4]],
        )
        .unwrap()
        .decode(&[0b00_01_10_11])
        .unwrap();
        assert_eq!(
            f.pixels,
            Pixels::Rgba8(vec![1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255])
        );
    }

    /// Animation at 24 bits: a whole frame, then one changing a single line
    /// with a run (the other lines kept), then one changing nothing.
    #[test]
    fn animation_builds_on_the_frame_before() {
        let mut d = Decoder::new(*b"rle ", 3, 2, 24, &[]).unwrap();
        assert!(!d.intra_only());
        // Each line: skip 1 (none), 3 literal pixels, end of line.
        let mut body = vec![0, 0];
        for row in 0..2u8 {
            body.push(1);
            body.push(3);
            for x in 0..3u8 {
                body.extend_from_slice(&[row * 10 + x, 0, 0]);
            }
            body.push(0xFF);
        }
        body.push(0);
        let mut frame = ((body.len() + 4) as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        let f = d.decode(&frame).unwrap();
        let Pixels::Rgba8(px) = f.pixels else {
            panic!()
        };
        let red = |px: &[u8]| px.chunks(4).map(|p| p[0]).collect::<Vec<_>>();
        assert_eq!(red(&px), [0, 1, 2, 10, 11, 12]);
        // Line 1 alone: skip 2 (one pixel), then the same pixel twice.
        let mut body = vec![0, 0x08, 0, 1, 0, 0, 0, 1, 0, 0];
        body.extend_from_slice(&[2, 0xFE, 99, 0, 0, 0xFF]);
        let mut frame = ((body.len() + 4) as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        let f = d.decode(&frame).unwrap();
        let Pixels::Rgba8(px) = f.pixels else {
            panic!()
        };
        assert_eq!(red(&px), [0, 1, 2, 10, 99, 99]);
        let f = d.decode(&[0, 0, 0, 4]).unwrap();
        let Pixels::Rgba8(px) = f.pixels else {
            panic!()
        };
        assert_eq!(red(&px), [0, 1, 2, 10, 99, 99]);
        // After a reset the frame starts black.
        d.reset();
        let f = d.decode(&[0, 0, 0, 4]).unwrap();
        let Pixels::Rgba8(px) = f.pixels else {
            panic!()
        };
        assert_eq!(red(&px), [0; 6]);
    }

    #[test]
    fn short_frames_are_refused() {
        let mut d = Decoder::new(*b"2vuy", 4, 4, 24, &[]).unwrap();
        assert_eq!(d.decode(&[0; 10]), Err(Error::Short));
        assert!(Decoder::new(*b"avc1", 4, 4, 24, &[]).is_none());
    }
}
