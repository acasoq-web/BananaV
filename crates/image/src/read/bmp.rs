//! Windows and OS/2 bitmaps: 1, 2, 4 and 8 bits with a palette (also RLE4
//! and RLE8), 16, 24 and 32 bits (with bit fields and alpha).

use super::{Picture, Result, corrupt, unsupported};

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// A bit field: where it starts and how many bits.
#[derive(Clone, Copy)]
struct Field {
    shift: u32,
    bits: u32,
}

impl Field {
    fn of(mask: u32) -> Field {
        if mask == 0 {
            return Field { shift: 0, bits: 0 };
        }
        let shift = mask.trailing_zeros();
        Field {
            shift,
            bits: (mask >> shift).trailing_ones(),
        }
    }

    /// The field of `v` in 8 bits.
    fn get(self, v: u32) -> u8 {
        if self.bits == 0 {
            return 0;
        }
        let x = (v >> self.shift) & ((1u64 << self.bits) - 1) as u32;
        let max = ((1u64 << self.bits) - 1) as u32;
        ((x * 255 + max / 2) / max) as u8
    }
}

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let bad = || super::ReadError::Corrupt("damaged BMP header".into());
    let offset = u32_at(d, 10).ok_or_else(bad)? as usize;
    let dib = u32_at(d, 14).ok_or_else(bad)? as usize;
    let core = dib == 12;
    let (width, height, bpp, compression) = if core {
        (
            u16_at(d, 18).ok_or_else(bad)? as i64,
            u16_at(d, 20).ok_or_else(bad)? as i16 as i64,
            u16_at(d, 24).ok_or_else(bad)?,
            0u32,
        )
    } else {
        (
            u32_at(d, 18).ok_or_else(bad)? as i32 as i64,
            u32_at(d, 22).ok_or_else(bad)? as i32 as i64,
            u16_at(d, 28).ok_or_else(bad)?,
            u32_at(d, 30).ok_or_else(bad)?,
        )
    };
    let top_down = height < 0;
    let (w, h) = (
        width.unsigned_abs() as usize,
        height.unsigned_abs() as usize,
    );
    if w == 0 || h == 0 || w > 65_535 || h > 65_535 {
        return corrupt("BMP size");
    }
    // 0 BI_RGB, 1 RLE8, 2 RLE4, 3 BITFIELDS, 6 ALPHABITFIELDS.
    match compression {
        0..=3 | 6 => {}
        4 => return unsupported("BMP with JPEG inside"),
        5 => return unsupported("BMP with PNG inside"),
        c => return unsupported(format!("BMP compression {c}")),
    }
    // Masks: in the header from V2 on, after a 40-byte header otherwise.
    let mask_at = 14 + 40;
    let masks = if compression == 3 || compression == 6 {
        let n = if dib >= 56 || compression == 6 { 4 } else { 3 };
        let m: Vec<u32> = (0..n)
            .map(|i| u32_at(d, mask_at + i * 4).unwrap_or(0))
            .collect();
        Some([m[0], m[1], m[2], m.get(3).copied().unwrap_or(0)])
    } else if dib >= 56 && bpp == 32 {
        // V3+ headers declare an alpha mask even with BI_RGB.
        Some([
            0x00ff_0000,
            0x0000_ff00,
            0x0000_00ff,
            u32_at(d, 14 + 52).unwrap_or(0),
        ])
    } else {
        None
    };
    // The palette follows the header (and the masks of a 40-byte one).
    let palette = if bpp <= 8 {
        let entry = if core { 3 } else { 4 };
        let used = if core {
            0
        } else {
            u32_at(d, 46).unwrap_or(0) as usize
        };
        let count = if used == 0 || used > 256 {
            1 << bpp
        } else {
            used
        };
        let mut start = 14 + dib;
        if dib == 40 && (compression == 3 || compression == 6) {
            start += if compression == 6 { 16 } else { 12 };
        }
        (0..count)
            .map(|i| {
                let at = start + i * entry;
                d.get(at..at + 3)
                    .map_or([0, 0, 0, 255], |p| [p[2], p[1], p[0], 255])
            })
            .collect::<Vec<[u8; 4]>>()
    } else {
        Vec::new()
    };
    let pixels = d.get(offset..).ok_or_else(bad)?;
    let mut out = vec![0u8; w * h * 4];
    // Rows from the bottom unless the height is negative.
    let row_of = |r: usize| if top_down { r } else { h - 1 - r };
    match (compression, bpp) {
        (1, 8) | (2, 4) => rle(pixels, w, h, bpp, &palette, &mut out, row_of)?,
        (_, 1 | 2 | 4 | 8) => {
            let stride = (w * bpp as usize).div_ceil(32) * 4;
            for r in 0..h {
                let line = pixels.get(r * stride..r * stride + stride);
                let Some(line) = line else { break };
                let y = row_of(r);
                for x in 0..w {
                    let bit = x * bpp as usize;
                    let byte = line[bit / 8];
                    let idx = (byte >> (8 - bpp as usize - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    let c = palette.get(idx as usize).copied().unwrap_or([0, 0, 0, 255]);
                    out[(y * w + x) * 4..][..4].copy_from_slice(&c);
                }
            }
        }
        (_, 16 | 24 | 32) => {
            let bytes = bpp as usize / 8;
            let stride = (w * bytes).div_ceil(4) * 4;
            let [rm, gm, bm, am] = masks.unwrap_or(if bpp == 16 {
                [0x7c00, 0x03e0, 0x001f, 0]
            } else {
                [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0]
            });
            let (rf, gf, bf, af) = (Field::of(rm), Field::of(gm), Field::of(bm), Field::of(am));
            // A 32-bit BI_RGB file's fourth byte is usually zero padding,
            // but some programs keep alpha there: used when it is not all 0.
            let padding_alpha = bpp == 32 && am == 0;
            let mut any_alpha = false;
            for r in 0..h {
                let Some(line) = pixels.get(r * stride..r * stride + w * bytes) else {
                    break;
                };
                let y = row_of(r);
                for x in 0..w {
                    let p = &line[x * bytes..x * bytes + bytes];
                    let v = match bytes {
                        2 => u16::from_le_bytes([p[0], p[1]]) as u32,
                        3 => u32::from_le_bytes([p[0], p[1], p[2], 0]),
                        _ => u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                    };
                    let a = if padding_alpha {
                        any_alpha |= p[3] != 0;
                        p[3]
                    } else if af.bits > 0 {
                        af.get(v)
                    } else {
                        255
                    };
                    let o = (y * w + x) * 4;
                    out[o..o + 4].copy_from_slice(&[rf.get(v), gf.get(v), bf.get(v), a]);
                }
            }
            if padding_alpha && !any_alpha {
                out.as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .for_each(|p| p[3] = 255);
            }
        }
        (_, b) => return unsupported(format!("BMP of {b} bits")),
    }
    Ok(Picture::rgba8(w as u32, h as u32, out))
}

/// RLE8 and RLE4 (only bottom-up files use them).
fn rle(
    data: &[u8],
    w: usize,
    h: usize,
    bpp: u16,
    palette: &[[u8; 4]],
    out: &mut [u8],
    row_of: impl Fn(usize) -> usize,
) -> Result<()> {
    let (mut x, mut r) = (0usize, 0usize);
    let mut i = 0usize;
    let put = |x: usize, r: usize, idx: u8, out: &mut [u8]| {
        if x < w && r < h {
            let c = palette.get(idx as usize).copied().unwrap_or([0, 0, 0, 255]);
            let o = (row_of(r) * w + x) * 4;
            out[o..o + 4].copy_from_slice(&c);
        }
    };
    // Pixels not written stay transparent (RLE's "skipped" ones).
    while i + 1 < data.len() {
        let (n, v) = (data[i] as usize, data[i + 1]);
        i += 2;
        if n > 0 {
            for k in 0..n {
                let idx = if bpp == 8 {
                    v
                } else if k % 2 == 0 {
                    v >> 4
                } else {
                    v & 15
                };
                put(x, r, idx, out);
                x += 1;
            }
            continue;
        }
        match v {
            0 => {
                x = 0;
                r += 1;
            }
            1 => return Ok(()),
            2 => {
                let (Some(&dx), Some(&dy)) = (data.get(i), data.get(i + 1)) else {
                    return Ok(());
                };
                i += 2;
                x += dx as usize;
                r += dy as usize;
            }
            n => {
                let n = n as usize;
                let bytes = if bpp == 8 { n } else { n.div_ceil(2) };
                let Some(run) = data.get(i..i + bytes) else {
                    return Ok(());
                };
                for k in 0..n {
                    let idx = if bpp == 8 {
                        run[k]
                    } else if k % 2 == 0 {
                        run[k / 2] >> 4
                    } else {
                        run[k / 2] & 15
                    };
                    put(x, r, idx, out);
                    x += 1;
                }
                // Runs are padded to whole words.
                i += bytes.div_ceil(2) * 2;
            }
        }
        if r >= h {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Image, Pixels, bmp, read::PictureData};

    #[test]
    fn what_the_writer_wrote_comes_back() {
        let px: Vec<u8> = (0..5 * 3 * 4).map(|i| (i * 7 % 256) as u8).collect();
        for alpha in [false, true] {
            let img = Image {
                width: 5,
                height: 3,
                pixels: Pixels::Rgba8(&px),
                alpha,
            };
            let p = super::read(&bmp(&img)).unwrap();
            let PictureData::Rgba8(got) = p.data else {
                panic!()
            };
            for (i, (a, b)) in got.chunks(4).zip(px.chunks(4)).enumerate() {
                assert_eq!(&a[..3], &b[..3], "pixel {i}");
                assert_eq!(a[3], if alpha { b[3] } else { 255 });
            }
        }
    }
}
