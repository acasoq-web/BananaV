//! GIF (87a and 89a) with its animation: the frames' LZW data is unpacked
//! once; a player lays them on the canvas in order, as browsers show them
//! (transparency, the three disposal methods, frames of 0 or 1 hundredth
//! shown for 10).

use super::{ReadError, Result, corrupt};

/// One image of the file, as stored: a rectangle of palette indices.
#[derive(Clone, Debug, PartialEq)]
pub struct GifFrame {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    /// How long it shows, in hundredths of a second (as browsers count).
    pub delay: u32,
    /// 0/1 leave it, 2 clear its rectangle, 3 put back what was under it.
    pub disposal: u8,
    transparent: Option<u8>,
    /// The frame's own palette, or the file's.
    palette: Vec<[u8; 3]>,
    indices: Vec<u8>,
}

/// A GIF file.
#[derive(Clone, Debug, PartialEq)]
pub struct Gif {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<GifFrame>,
}

impl Gif {
    pub fn parse(d: &[u8]) -> Result<Gif> {
        if d.len() < 13 || !(d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a")) {
            return corrupt("not a GIF");
        }
        let width = u16::from_le_bytes([d[6], d[7]]) as u32;
        let height = u16::from_le_bytes([d[8], d[9]]) as u32;
        let flags = d[10];
        let mut pos = 13;
        let global = if flags & 0x80 != 0 {
            let n = 2usize << (flags & 7);
            let t = palette(d.get(pos..pos + n * 3).ok_or_else(truncated)?);
            pos += n * 3;
            t
        } else {
            Vec::new()
        };
        let mut frames = Vec::new();
        // The graphic control extension applies to the next image.
        let mut delay = 0u32;
        let mut disposal = 0u8;
        let mut transparent = None;
        while let Some(&block) = d.get(pos) {
            pos += 1;
            match block {
                0x21 => {
                    let label = *d.get(pos).ok_or_else(truncated)?;
                    pos += 1;
                    let (data, next) = sub_blocks(d, pos);
                    if label == 0xf9 && data.len() >= 4 {
                        disposal = (data[0] >> 2) & 7;
                        delay = u16::from_le_bytes([data[1], data[2]]) as u32;
                        transparent = (data[0] & 1 != 0).then_some(data[3]);
                    }
                    pos = next;
                }
                0x2c => {
                    let h = d.get(pos..pos + 9).ok_or_else(truncated)?;
                    let left = u16::from_le_bytes([h[0], h[1]]) as u32;
                    let top = u16::from_le_bytes([h[2], h[3]]) as u32;
                    let fw = u16::from_le_bytes([h[4], h[5]]) as u32;
                    let fh = u16::from_le_bytes([h[6], h[7]]) as u32;
                    let f = h[8];
                    pos += 9;
                    let local = if f & 0x80 != 0 {
                        let n = 2usize << (f & 7);
                        let t = palette(d.get(pos..pos + n * 3).ok_or_else(truncated)?);
                        pos += n * 3;
                        Some(t)
                    } else {
                        None
                    };
                    let min_code = *d.get(pos).ok_or_else(truncated)?;
                    pos += 1;
                    let (data, next) = sub_blocks(d, pos);
                    pos = next;
                    let n = fw as usize * fh as usize;
                    let mut indices = lzw(&data, min_code, n);
                    // A short image (a damaged file) ends in index 0.
                    indices.resize(n, 0);
                    if f & 0x40 != 0 {
                        indices = deinterlace(&indices, fw as usize, fh as usize);
                    }
                    frames.push(GifFrame {
                        left,
                        top,
                        width: fw,
                        height: fh,
                        // Browsers show 0 and 1 hundredth for 10.
                        delay: if delay <= 1 { 10 } else { delay },
                        disposal,
                        transparent,
                        palette: local.unwrap_or_else(|| global.clone()),
                        indices,
                    });
                    delay = 0;
                    disposal = 0;
                    transparent = None;
                }
                0x3b => break,
                // Anything else: the file ends there (damaged).
                _ => break,
            }
        }
        if frames.is_empty() {
            return corrupt("GIF without images");
        }
        let (width, height) = if width == 0 || height == 0 {
            // Some writers leave the screen size out: the first image's.
            (
                frames[0].left + frames[0].width,
                frames[0].top + frames[0].height,
            )
        } else {
            (width, height)
        };
        Ok(Gif {
            width,
            height,
            frames,
        })
    }

    /// The whole animation's length, in hundredths of a second.
    pub fn duration(&self) -> u64 {
        self.frames.iter().map(|f| f.delay as u64).sum()
    }

    /// The frame showing at `t` hundredths of a second (the last one after
    /// the end).
    pub fn frame_at(&self, t: u64) -> usize {
        let mut end = 0u64;
        for (i, f) in self.frames.iter().enumerate() {
            end += f.delay as u64;
            if t < end {
                return i;
            }
        }
        self.frames.len() - 1
    }
}

fn truncated() -> ReadError {
    ReadError::Corrupt("truncated GIF".into())
}

fn palette(d: &[u8]) -> Vec<[u8; 3]> {
    d.as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2]])
        .collect()
}

/// The data of a run of sub-blocks starting at `pos`, and where it ends.
fn sub_blocks(d: &[u8], mut pos: usize) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    while let Some(&n) = d.get(pos) {
        pos += 1;
        if n == 0 {
            break;
        }
        let end = (pos + n as usize).min(d.len());
        out.extend_from_slice(&d[pos..end]);
        pos = end;
    }
    (out, pos)
}

/// GIF's variable-length LZW (codes LSB first, up to 12 bits).
fn lzw(data: &[u8], min_code: u8, max: usize) -> Vec<u8> {
    let min_code = min_code.clamp(2, 11) as u32;
    let clear = 1u32 << min_code;
    let end = clear + 1;
    // Each code: (previous code, last byte, length).
    let mut prefix = vec![0u16; 4096];
    let mut suffix = vec![0u8; 4096];
    let mut length = vec![0u16; 4096];
    for i in 0..clear as usize {
        suffix[i] = i as u8;
        length[i] = 1;
    }
    let mut next = end + 1;
    let mut size = min_code + 1;
    let mut out = Vec::with_capacity(max);
    let mut prev: Option<u32> = None;
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut stack = Vec::with_capacity(4096);
    for &byte in data {
        acc |= (byte as u32) << bits;
        bits += 8;
        while bits >= size {
            let code = acc & ((1 << size) - 1);
            acc >>= size;
            bits -= size;
            if code == clear {
                next = end + 1;
                size = min_code + 1;
                prev = None;
                continue;
            }
            if code == end || out.len() >= max {
                return out;
            }
            let Some(p) = prev else {
                if code < clear {
                    out.push(code as u8);
                    prev = Some(code);
                }
                continue;
            };
            // The string of `code`, or (the KwKwK case) of `prev` plus its
            // own first byte.
            let known = code < next;
            let mut c = if known { code } else { p };
            stack.clear();
            while c >= clear {
                stack.push(suffix[c as usize]);
                c = prefix[c as usize] as u32;
            }
            let first = c as u8;
            stack.push(first);
            out.extend(stack.iter().rev());
            if !known {
                out.push(first);
            }
            if next < 4096 {
                prefix[next as usize] = p as u16;
                suffix[next as usize] = first;
                length[next as usize] = length[p as usize] + 1;
                next += 1;
                if next == 1 << size && size < 12 {
                    size += 1;
                }
            }
            prev = Some(code);
        }
    }
    out
}

/// Interlaced rows (every 8th from 0, every 8th from 4, every 4th from 2,
/// every 2nd from 1) in display order.
fn deinterlace(d: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    let mut src = 0usize;
    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
        let mut y = start;
        while y < h {
            out[y * w..y * w + w].copy_from_slice(&d[src * w..src * w + w]);
            src += 1;
            y += step;
        }
    }
    out
}

/// Plays a [`Gif`]: the canvas after each frame, in order (going back
/// starts over from a frame kept on the way).
#[derive(Clone, Debug)]
pub struct GifPlayer {
    canvas: Vec<u8>,
    /// The frame on the canvas.
    shown: Option<usize>,
    /// What lay under the shown frame (for disposal 3).
    under: Vec<u8>,
    /// Canvases before frames 0, 32, 64… (as found on the way).
    kept: Vec<(usize, Vec<u8>)>,
}

const KEEP_EVERY: usize = 32;

impl GifPlayer {
    pub fn new(gif: &Gif) -> GifPlayer {
        GifPlayer {
            canvas: vec![0; gif.width as usize * gif.height as usize * 4],
            shown: None,
            under: Vec::new(),
            kept: Vec::new(),
        }
    }

    /// The canvas showing frame `k` (straight-alpha RGBA rows).
    pub fn frame(&mut self, gif: &Gif, k: usize) -> &[u8] {
        let k = k.min(gif.frames.len() - 1);
        if self.shown != Some(k) {
            // `restored`: the canvas is one kept before frame `from` (the
            // frame before it already disposed of).
            let (from, restored) = match self.shown {
                Some(s) if s < k => (s + 1, false),
                _ => {
                    // Back to the latest canvas kept before `k`.
                    let start = self
                        .kept
                        .iter()
                        .filter(|(i, _)| *i <= k)
                        .max_by_key(|(i, _)| *i)
                        .map(|(i, c)| (*i, c.clone()));
                    match start {
                        Some((i, c)) => {
                            self.canvas = c;
                            (i, true)
                        }
                        None => {
                            self.canvas.fill(0);
                            (0, true)
                        }
                    }
                }
            };
            for i in from..=k {
                if i > 0 && (i > from || !restored) {
                    self.dispose(gif, i - 1);
                }
                if i % KEEP_EVERY == 0 && !self.kept.iter().any(|(j, _)| *j == i) {
                    self.kept.push((i, self.canvas.clone()));
                }
                self.draw(gif, i);
            }
            self.shown = Some(k);
        }
        &self.canvas
    }

    /// Undoes frame `i` as its disposal method says (before the next one).
    fn dispose(&mut self, gif: &Gif, i: usize) {
        let f = &gif.frames[i];
        match f.disposal {
            2 => self.each_pixel(gif, f, |canvas, at, _| canvas[at..at + 4].fill(0)),
            3 if !self.under.is_empty() => {
                let under = std::mem::take(&mut self.under);
                self.canvas.copy_from_slice(&under);
                self.under = under;
            }
            _ => {}
        }
    }

    fn draw(&mut self, gif: &Gif, i: usize) {
        let f = &gif.frames[i];
        if f.disposal == 3 {
            self.under.clone_from(&self.canvas);
        }
        self.each_pixel(gif, f, |canvas, at, idx| {
            if Some(idx) == f.transparent {
                return;
            }
            let c = f.palette.get(idx as usize).copied().unwrap_or([0, 0, 0]);
            canvas[at..at + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        });
    }

    /// Calls `op(canvas, byte offset, index)` for the frame's pixels inside
    /// the canvas.
    fn each_pixel(&mut self, gif: &Gif, f: &GifFrame, mut op: impl FnMut(&mut [u8], usize, u8)) {
        let (cw, ch) = (gif.width as usize, gif.height as usize);
        for y in 0..f.height as usize {
            let cy = f.top as usize + y;
            if cy >= ch {
                break;
            }
            for x in 0..f.width as usize {
                let cx = f.left as usize + x;
                if cx >= cw {
                    break;
                }
                let idx = f.indices[y * f.width as usize + x];
                op(&mut self.canvas, (cy * cw + cx) * 4, idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2, two-frame GIF built by hand: red then (blue in the corner,
    /// the rest transparent so red shows).
    fn two_frames() -> Vec<u8> {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(&[2, 0, 2, 0, 0x80 | 1, 0, 0]);
        // Global palette of 4: red, blue, black, black.
        v.extend_from_slice(&[255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0]);
        for (frame, idx) in [(0, [0u8, 0, 0, 0]), (1, [1, 2, 2, 2])] {
            // Graphic control: 20 hundredths, transparent index 2 on the
            // second frame.
            let flags = if frame == 1 { 1 } else { 0 };
            v.extend_from_slice(&[0x21, 0xf9, 4, flags, 20, 0, 2, 0]);
            v.extend_from_slice(&[0x2c, 0, 0, 0, 0, 2, 0, 2, 0, 0]);
            // LZW at 2 bits: clear, the 4 indices, end — 3-bit codes.
            let codes = [
                4u32,
                idx[0] as u32,
                idx[1] as u32,
                idx[2] as u32,
                idx[3] as u32,
                5,
            ];
            let mut bits = Vec::new();
            let mut size = 3;
            let mut next = 6;
            for (i, c) in codes.iter().enumerate() {
                for b in 0..size {
                    bits.push((c >> b) & 1);
                }
                // The decoder adds a code after each one but the first.
                if i >= 2 && *c != 5 {
                    next += 1;
                    if next == 1 << size {
                        size += 1;
                    }
                }
            }
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (i, b) in bits.iter().enumerate() {
                bytes[i / 8] |= (*b as u8) << (i % 8);
            }
            v.push(2);
            v.push(bytes.len() as u8);
            v.extend_from_slice(&bytes);
            v.push(0);
        }
        v.push(0x3b);
        v
    }

    #[test]
    fn frames_lay_over_each_other() {
        let gif = Gif::parse(&two_frames()).unwrap();
        assert_eq!((gif.width, gif.height, gif.frames.len()), (2, 2, 2));
        assert_eq!(gif.duration(), 40);
        assert_eq!(gif.frame_at(25), 1);
        let mut p = GifPlayer::new(&gif);
        assert_eq!(p.frame(&gif, 0), &[255, 0, 0, 255].repeat(4)[..]);
        let second = p.frame(&gif, 1).to_vec();
        assert_eq!(&second[..4], &[0, 0, 255, 255]);
        assert_eq!(
            &second[4..8],
            &[255, 0, 0, 255],
            "transparent shows the first"
        );
        // Back to the first frame.
        assert_eq!(p.frame(&gif, 0), &[255, 0, 0, 255].repeat(4)[..]);
    }
}
