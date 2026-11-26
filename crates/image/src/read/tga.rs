//! Truevision TGA: color-mapped, true-color (15, 16, 24 and 32 bits) and
//! gray, raw or run-length encoded, any of the four origins.

use super::{Picture, ReadError, Result, checked_size, corrupt, pixel_count, unsupported};

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let bad = || ReadError::Corrupt("damaged TGA".into());
    let h = d.get(..18).ok_or_else(bad)?;
    let id_len = h[0] as usize;
    let cmap_type = h[1];
    let kind = h[2];
    let cmap_first = u16::from_le_bytes([h[3], h[4]]) as usize;
    let cmap_len = u16::from_le_bytes([h[5], h[6]]) as usize;
    let cmap_bits = h[7] as usize;
    let width = u16::from_le_bytes([h[12], h[13]]) as usize;
    let height = u16::from_le_bytes([h[14], h[15]]) as usize;
    let depth = h[16] as usize;
    let desc = h[17];
    if width == 0 || height == 0 {
        return corrupt("TGA size");
    }
    if !matches!(kind, 1 | 2 | 3 | 9 | 10 | 11) {
        return unsupported(format!("TGA type {kind}"));
    }
    let rle = kind >= 9;
    let mapped = kind & 7 == 1;
    let gray = kind & 7 == 3;
    // True color is 15, 16, 24 or 32 bits: 8 are an index or a gray.
    if !matches!(depth, 8 | 15 | 16 | 24 | 32)
        || (mapped && depth != 8 && depth != 16)
        || (!mapped && !gray && depth == 8)
    {
        return unsupported(format!("TGA of {depth} bits"));
    }
    let n = pixel_count(width, height)?;
    let mut pos = 18 + id_len;
    // The color map (also skipped when the image does not use it).
    let mut cmap = Vec::new();
    if cmap_type == 1 {
        let entry = cmap_bits.div_ceil(8);
        // Its entries are colors of 15, 16, 24 or 32 bits: a map of any
        // other size is only passed over, and an image that uses it is
        // refused.
        if matches!(cmap_bits, 15 | 16 | 24 | 32) {
            for i in 0..cmap_len {
                let e = d
                    .get(pos + i * entry..pos + i * entry + entry)
                    .ok_or_else(bad)?;
                cmap.push(color(e, cmap_bits));
            }
        } else if mapped {
            return unsupported(format!("TGA color map of {cmap_bits} bits"));
        }
        pos += cmap_len * entry;
    }
    let bytes = depth.div_ceil(8);
    // Raw pixels are all in the file, or it is cut short: their room is
    // not made on the header's word alone.
    if !rle && d.len().saturating_sub(pos) < checked_size(n, bytes)? {
        return corrupt("truncated TGA");
    }
    // Pixels in file order.
    let mut px: Vec<[u8; 4]> = Vec::with_capacity(n);
    let pixel = |e: &[u8]| -> [u8; 4] {
        if mapped {
            let idx = if bytes == 2 {
                u16::from_le_bytes([e[0], e[1]]) as usize
            } else {
                e[0] as usize
            };
            cmap.get(idx.wrapping_sub(cmap_first))
                .copied()
                .unwrap_or([0, 0, 0, 255])
        } else if gray {
            [e[0], e[0], e[0], if bytes == 2 { e[1] } else { 255 }]
        } else {
            color(e, depth)
        }
    };
    if rle {
        while px.len() < n {
            let Some(&head) = d.get(pos) else { break };
            pos += 1;
            let count = (head & 0x7f) as usize + 1;
            if head & 0x80 != 0 {
                let Some(e) = d.get(pos..pos + bytes) else {
                    break;
                };
                let p = pixel(e);
                px.extend(std::iter::repeat_n(p, count.min(n - px.len())));
                pos += bytes;
            } else {
                for _ in 0..count.min(n - px.len()) {
                    let Some(e) = d.get(pos..pos + bytes) else {
                        break;
                    };
                    px.push(pixel(e));
                    pos += bytes;
                }
            }
        }
    } else {
        for i in 0..n {
            let Some(e) = d.get(pos + i * bytes..pos + i * bytes + bytes) else {
                break;
            };
            px.push(pixel(e));
        }
    }
    px.resize(n, [0, 0, 0, 255]);
    // Alpha: kept when the descriptor gives it bits and it is not all 0
    // (many writers leave zeros there); the TGA 2.0 footer may say there is
    // none.
    let alpha_bits = desc & 15;
    let footer_alpha = footer_alpha_type(d);
    let has_alpha = (depth == 32 || (depth == 16 && !mapped && !gray && alpha_bits > 0))
        && alpha_bits > 0
        && footer_alpha != Some(0)
        && px.iter().any(|p| p[3] != 0);
    let premultiplied = footer_alpha == Some(4);
    let right_to_left = desc & 0x10 != 0;
    let top_down = desc & 0x20 != 0;
    let mut out = vec![0u8; checked_size(n, 4)?];
    for y in 0..height {
        let sy = if top_down { y } else { height - 1 - y };
        for x in 0..width {
            let sx = if right_to_left { width - 1 - x } else { x };
            let mut p = px[sy * width + sx];
            if !has_alpha && !gray {
                p[3] = 255;
            }
            out[(y * width + x) * 4..][..4].copy_from_slice(&p);
        }
    }
    let mut picture = Picture::rgba8(width as u32, height as u32, out);
    picture.premultiplied = premultiplied && has_alpha;
    Ok(picture)
}

/// A TGA's size, from its header.
pub(super) fn dimensions(d: &[u8]) -> Option<(u32, u32)> {
    let h = d.get(..18)?;
    let (w, ht) = (
        u16::from_le_bytes([h[12], h[13]]) as u32,
        u16::from_le_bytes([h[14], h[15]]) as u32,
    );
    (w > 0 && ht > 0 && matches!(h[2], 1 | 2 | 3 | 9 | 10 | 11)).then_some((w, ht))
}

/// A BGR(A) value of 15, 16, 24 or 32 bits as RGBA.
fn color(e: &[u8], bits: usize) -> [u8; 4] {
    match bits {
        15 | 16 => {
            let v = u16::from_le_bytes([e[0], e[1]]);
            let five = |s: u16| (((v >> s) & 31) as u32 * 255 / 31) as u8;
            let a = if bits == 16 && v & 0x8000 == 0 {
                0
            } else {
                255
            };
            [five(10), five(5), five(0), a]
        }
        24 => [e[2], e[1], e[0], 255],
        _ => [e[2], e[1], e[0], e[3]],
    }
}

/// The TGA 2.0 extension area's attribute type: 0 no alpha, 3 alpha, 4
/// premultiplied alpha.
fn footer_alpha_type(d: &[u8]) -> Option<u8> {
    let foot = d.get(d.len().checked_sub(26)?..)?;
    if &foot[8..24] != b"TRUEVISION-XFILE" {
        return None;
    }
    let ext = u32::from_le_bytes(foot[..4].try_into().ok()?) as usize;
    if ext == 0 {
        return None;
    }
    d.get(ext + 494).copied()
}

#[cfg(test)]
mod tests {
    use crate::read::PictureData;

    #[test]
    fn rle_and_bottom_up() {
        // 2×2, 24 bits, RLE, bottom-up: a run of two blue, two raw (red,
        // green).
        let mut d = vec![0u8, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 24, 0];
        d.extend_from_slice(&[0x81, 255, 0, 0]);
        d.extend_from_slice(&[0x01, 0, 0, 255, 0, 255, 0]);
        let p = super::read(&d).unwrap();
        let PictureData::Rgba8(px) = p.data else {
            panic!()
        };
        // The file's first row is the bottom one.
        assert_eq!(&px[..8], &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(&px[8..], &[0, 0, 255, 255, 0, 0, 255, 255]);
    }

    /// A TGA header: the color map's use, length and entry size, the
    /// image's type, its size and its depth.
    fn header(
        cmap: u8,
        cmap_len: u8,
        cmap_bits: u8,
        kind: u8,
        size: [u16; 2],
        depth: u8,
    ) -> Vec<u8> {
        let [w, h] = size.map(u16::to_le_bytes);
        vec![
            0, cmap, kind, 0, 0, cmap_len, 0, cmap_bits, 0, 0, 0, 0, w[0], w[1], h[0], h[1], depth,
            0,
        ]
    }

    #[test]
    fn color_sizes_without_a_layout_are_refused() {
        // True color of 8 bits; a color map of 8-bit entries, and of none.
        for (mut d, pixels) in [
            (header(0, 0, 0, 2, [1, 1], 8), 1),
            (header(1, 1, 8, 1, [1, 1], 8), 2),
            (header(1, 1, 0, 1, [1, 1], 8), 1),
        ] {
            d.extend(std::iter::repeat_n(7, pixels));
            assert!(matches!(
                super::read(&d),
                Err(crate::read::ReadError::Unsupported(_))
            ));
        }
        // Such a map before true color pixels is only passed over.
        let mut d = header(1, 2, 8, 2, [1, 1], 24);
        d.extend([9, 9, 1, 2, 3]);
        let PictureData::Rgba8(px) = super::read(&d).unwrap().data else {
            panic!()
        };
        assert_eq!(px, [3, 2, 1, 255]);
    }

    #[test]
    fn raw_pixels_missing_from_the_file_are_refused() {
        // 2×2 of 24 bits with one pixel short.
        let mut d = header(0, 0, 0, 2, [2, 2], 24);
        d.extend([0u8; 9]);
        assert!(matches!(
            super::read(&d),
            Err(crate::read::ReadError::Corrupt(_))
        ));
        d.extend([0u8; 3]);
        assert!(super::read(&d).is_ok());
    }

    #[test]
    fn a_picture_over_the_pixel_budget_is_refused() {
        // 18 bytes that claim 65535 × 65535 pixels, raw and run-length
        // encoded.
        for kind in [2, 10] {
            let d = header(0, 0, 0, kind, [65_535, 65_535], 24);
            assert!(matches!(
                super::read(&d),
                Err(crate::read::ReadError::Unsupported(_))
            ));
        }
        // Within the budget, raw: the pixels are not there.
        let d = header(0, 0, 0, 2, [20_000, 20_000], 24);
        assert!(matches!(
            super::read(&d),
            Err(crate::read::ReadError::Corrupt(_))
        ));
    }
}
