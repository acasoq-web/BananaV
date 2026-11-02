//! Windows bitmaps: 24-bit BGR, or 32-bit BGRA with a BITMAPV5HEADER that
//! declares the alpha channel.

use crate::Image;

pub fn bmp(img: &Image<'_>) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let bytes = if img.alpha { 4 } else { 3 };
    let row = (w * bytes).div_ceil(4) * 4;
    let header = if img.alpha { 124 } else { 40 };
    let offset = 14 + header;
    let size = offset + row * h;
    let mut v = Vec::with_capacity(size);
    v.extend_from_slice(b"BM");
    v.extend_from_slice(&(size as u32).to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&(offset as u32).to_le_bytes());
    v.extend_from_slice(&(header as u32).to_le_bytes());
    v.extend_from_slice(&(w as i32).to_le_bytes());
    // Positive height: rows from the bottom.
    v.extend_from_slice(&(h as i32).to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&((bytes * 8) as u16).to_le_bytes());
    // BI_BITFIELDS with alpha, BI_RGB without.
    v.extend_from_slice(&(if img.alpha { 3u32 } else { 0 }).to_le_bytes());
    v.extend_from_slice(&((row * h) as u32).to_le_bytes());
    // 72 dpi.
    v.extend_from_slice(&2835i32.to_le_bytes());
    v.extend_from_slice(&2835i32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    if img.alpha {
        for mask in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
            v.extend_from_slice(&mask.to_le_bytes());
        }
        v.extend_from_slice(b"BGRs"); // LCS_sRGB
        v.extend_from_slice(&[0; 36]); // endpoints
        v.extend_from_slice(&[0; 12]); // gamma
        v.extend_from_slice(&4u32.to_le_bytes()); // LCS_GM_IMAGES
        v.extend_from_slice(&[0; 12]); // profile, reserved
    }
    for y in (0..h).rev() {
        let start = v.len();
        for x in 0..w {
            let i = y * w + x;
            v.push(img.get8(i, 2));
            v.push(img.get8(i, 1));
            v.push(img.get8(i, 0));
            if img.alpha {
                v.push(img.get8(i, 3));
            }
        }
        v.resize(start + row, 0);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pixels;

    #[test]
    fn rows_are_padded_and_upside_down() {
        // 1×2: red on top, blue below.
        let px = [255, 0, 0, 255, 0, 0, 255, 255];
        let img = Image {
            width: 1,
            height: 2,
            pixels: Pixels::Rgba8(&px),
            alpha: false,
        };
        let b = bmp(&img);
        assert_eq!(&b[..2], b"BM");
        assert_eq!(b.len(), 54 + 2 * 4);
        // The first row of the file is the bottom one (blue).
        assert_eq!(&b[54..58], &[255, 0, 0, 0]);
        assert_eq!(&b[58..62], &[0, 0, 255, 0]);
    }
}
