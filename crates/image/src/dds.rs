//! DirectDraw Surface: one uncompressed 32-bit BGRA (or BGRX) surface.

use crate::Image;

pub fn dds(img: &Image<'_>) -> Vec<u8> {
    let (w, h) = (img.width, img.height);
    let mut v = Vec::with_capacity(128 + img.pixel_count() * 4);
    v.extend_from_slice(b"DDS ");
    let mut header = [0u32; 31];
    header[0] = 124;
    // CAPS | HEIGHT | WIDTH | PITCH | PIXELFORMAT
    header[1] = 0x1 | 0x2 | 0x4 | 0x8 | 0x1000;
    header[2] = h;
    header[3] = w;
    header[4] = w * 4;
    // DDS_PIXELFORMAT, at dword 18.
    header[18] = 32;
    header[19] = if img.alpha { 0x40 | 0x1 } else { 0x40 };
    header[21] = 32;
    header[22] = 0x00FF_0000;
    header[23] = 0x0000_FF00;
    header[24] = 0x0000_00FF;
    header[25] = if img.alpha { 0xFF00_0000 } else { 0 };
    header[26] = 0x1000; // DDSCAPS_TEXTURE
    for d in header {
        v.extend_from_slice(&d.to_le_bytes());
    }
    for i in 0..img.pixel_count() {
        v.push(img.get8(i, 2));
        v.push(img.get8(i, 1));
        v.push(img.get8(i, 0));
        v.push(if img.alpha { img.get8(i, 3) } else { 255 });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pixels;

    #[test]
    fn header_and_pixels() {
        let px = [10, 20, 30, 40];
        let d = dds(&Image {
            width: 1,
            height: 1,
            pixels: Pixels::Rgba8(&px),
            alpha: true,
        });
        assert_eq!(d.len(), 128 + 4);
        assert_eq!(&d[..4], b"DDS ");
        assert_eq!(u32::from_le_bytes(d[4..8].try_into().unwrap()), 124);
        assert_eq!(u32::from_le_bytes(d[76..80].try_into().unwrap()), 32);
        assert_eq!(&d[128..], &[30, 20, 10, 40]);
    }
}
