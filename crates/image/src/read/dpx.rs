//! DPX (SMPTE 268M, versions 1 and 2), the first image element: RGB, RGBA,
//! ABGR and luma of 8, 10, 12 and 16 bits, packed or filled (methods A and
//! B), either byte order.

use super::{Picture, PictureData, ReadError, Result, corrupt, unsupported};

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let be = d.starts_with(b"SDPX");
    let u32_at = |at: usize| -> Option<u32> {
        let b: [u8; 4] = d.get(at..at + 4)?.try_into().ok()?;
        Some(if be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b: [u8; 2] = d.get(at..at + 2)?.try_into().ok()?;
        Some(if be {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let bad = || ReadError::Corrupt("damaged DPX header".into());
    let offset = u32_at(4).ok_or_else(bad)? as usize;
    let orientation = u16_at(768).ok_or_else(bad)?;
    let width = u32_at(772).ok_or_else(bad)? as usize;
    let height = u32_at(776).ok_or_else(bad)? as usize;
    if width == 0 || height == 0 || width > 1 << 16 || height > 1 << 16 {
        return corrupt("DPX size");
    }
    let descriptor = *d.get(800).ok_or_else(bad)?;
    let transfer = *d.get(801).ok_or_else(bad)?;
    let bits = *d.get(803).ok_or_else(bad)? as usize;
    let packing = u16_at(804).ok_or_else(bad)?;
    let encoding = u16_at(806).ok_or_else(bad)?;
    let data_at = u32_at(808)
        .filter(|&o| o != u32::MAX && o != 0)
        .map_or(offset, |o| o as usize);
    if encoding == 1 {
        return unsupported("run-length encoded DPX");
    }
    // 50 RGB, 51 RGBA, 52 ABGR, 6 luma.
    let order: &[usize] = match descriptor {
        50 => &[0, 1, 2],
        51 => &[0, 1, 2, 3],
        52 => &[3, 2, 1, 0],
        6 => &[0],
        n => return unsupported(format!("DPX element {n}")),
    };
    if !matches!(bits, 8 | 10 | 12 | 16) {
        return unsupported(format!("DPX of {bits} bits"));
    }
    let n = order.len();
    let max = ((1u32 << bits) - 1) as f32;
    let data = d.get(data_at..).ok_or_else(bad)?;
    let mut values = Vec::with_capacity(width * height * n);
    match (bits, packing) {
        (8, _) => {
            // Rows padded to 32 bits.
            let row = (width * n).div_ceil(4) * 4;
            for y in 0..height {
                let r = data.get(y * row..y * row + width * n).ok_or_else(bad)?;
                values.extend(r.iter().map(|&b| b as u32));
            }
        }
        (16, _) | (12, 1 | 2) => {
            // One sample to 16 bits, rows padded to 32 bits; 12-bit
            // samples sit at the top with method A.
            let row = (width * n * 2).div_ceil(4) * 4;
            for y in 0..height {
                for i in 0..width * n {
                    let at = y * row + i * 2;
                    let b = [
                        *data.get(at).ok_or_else(bad)?,
                        *data.get(at + 1).ok_or_else(bad)?,
                    ];
                    let v = if be {
                        u16::from_be_bytes(b)
                    } else {
                        u16::from_le_bytes(b)
                    } as u32;
                    values.push(match (bits, packing) {
                        (12, 1) => v >> 4,
                        (12, _) => v & 4095,
                        _ => v,
                    });
                }
            }
        }
        (10, 1 | 2) => {
            // Three samples to a 32-bit word, rows on whole words; method
            // A leaves the 2 free bits at the bottom, B at the top.
            let per_row = (width * n).div_ceil(3);
            for y in 0..height {
                let mut got = 0;
                for k in 0..per_row {
                    let at = (y * per_row + k) * 4;
                    let b: [u8; 4] = data.get(at..at + 4).ok_or_else(bad)?.try_into().unwrap();
                    let w = if be {
                        u32::from_be_bytes(b)
                    } else {
                        u32::from_le_bytes(b)
                    };
                    let w = if packing == 1 { w >> 2 } else { w };
                    for s in [20, 10, 0] {
                        if got < width * n {
                            values.push((w >> s) & 1023);
                            got += 1;
                        }
                    }
                }
            }
        }
        _ => {
            // Packed: samples one after the other in 32-bit words, rows on
            // whole words.
            let row_words = (width * n * bits).div_ceil(32);
            for y in 0..height {
                let mut bit = 0usize;
                for _ in 0..width * n {
                    let word = |k: usize| -> Option<u32> {
                        let at = (y * row_words + k) * 4;
                        let b: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
                        Some(if be {
                            u32::from_be_bytes(b)
                        } else {
                            u32::from_le_bytes(b)
                        })
                    };
                    let (k, s) = (bit / 32, bit % 32);
                    let lo = word(k).ok_or_else(bad)? as u64;
                    let hi = word(k + 1).unwrap_or(0) as u64;
                    let both = lo | (hi << 32);
                    values.push(((both >> s) & ((1 << bits) - 1)) as u32);
                    bit += bits;
                }
            }
        }
    }
    let mut rgba = vec![0u16; width * height * 4];
    for p in 0..width * height {
        let px = &values[p * n..p * n + n];
        let to16 = |v: u32| ((v as f32 / max) * 65535.0).round() as u16;
        let out = &mut rgba[p * 4..p * 4 + 4];
        if n == 1 {
            let g = to16(px[0]);
            out.copy_from_slice(&[g, g, g, 65535]);
        } else {
            out[3] = 65535;
            for (i, &c) in order.iter().enumerate() {
                out[c] = to16(px[i]);
            }
        }
    }
    let picture = Picture {
        width: width as u32,
        height: height as u32,
        data: PictureData::Rgba16(rgba),
        // Transfer 2: linear.
        linear: transfer == 2,
        premultiplied: false,
    };
    // DPX orientation: 0 normal, 1 mirrored, 2 upside down, 3 both.
    Ok(match orientation {
        1 => picture.orient(2),
        2 => picture.orient(4),
        3 => picture.orient(3),
        _ => picture,
    })
}

#[cfg(test)]
mod tests {
    use crate::{Image, Pixels, dpx, read::PictureData};

    #[test]
    fn what_the_writer_wrote_comes_back() {
        let px: Vec<u16> = (0..6 * 4 * 4).map(|i| (i * 2731 % 65536) as u16).collect();
        let img = Image {
            width: 6,
            height: 4,
            pixels: Pixels::Rgba16(&px),
            alpha: false,
        };
        let p = super::read(&dpx(&img, "test")).unwrap();
        let PictureData::Rgba16(got) = p.data else {
            panic!()
        };
        for (a, b) in got.chunks(4).zip(px.chunks(4)) {
            for c in 0..3 {
                // 10 bits: within a step of 64.
                assert!((a[c] as i32 - b[c] as i32).abs() <= 64, "{a:?} vs {b:?}");
            }
        }
    }
}
