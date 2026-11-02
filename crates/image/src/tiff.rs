//! TIFF 6.0 (little-endian): RGB or RGBA, 8 or 16 bits per sample, in
//! strips, uncompressed or Deflate with the horizontal predictor.

use crate::Image;

const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const ASCII: u16 = 2;

/// `bits`: 8 or 16. `deflate`: compress the strips (lossless).
pub fn tiff(img: &Image<'_>, bits: u16, deflate: bool) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let channels = if img.alpha { 4 } else { 3 };
    let bytes = (bits / 8) as usize;
    let row = w * channels * bytes;
    // Strips of about 64 KB.
    let rows_per_strip = (65_536 / row.max(1)).clamp(1, h.max(1));
    let strips = h.div_ceil(rows_per_strip);

    let mut out = vec![b'I', b'I', 42, 0, 0, 0, 0, 0];
    let mut offsets = Vec::with_capacity(strips);
    let mut counts = Vec::with_capacity(strips);
    for s in 0..strips {
        let y0 = s * rows_per_strip;
        let y1 = (y0 + rows_per_strip).min(h);
        let mut data = Vec::with_capacity((y1 - y0) * row);
        for y in y0..y1 {
            let start = data.len();
            for x in 0..w {
                let i = y * w + x;
                for c in 0..channels {
                    if bits == 16 {
                        data.extend_from_slice(&img.get16(i, c).to_le_bytes());
                    } else {
                        data.push(img.get8(i, c));
                    }
                }
            }
            if deflate {
                // Horizontal differencing (predictor 2), per sample.
                let line = &mut data[start..];
                if bits == 16 {
                    for x in (1..w).rev() {
                        for c in 0..channels {
                            let at = (x * channels + c) * 2;
                            let prev = ((x - 1) * channels + c) * 2;
                            let v = u16::from_le_bytes([line[at], line[at + 1]]);
                            let p = u16::from_le_bytes([line[prev], line[prev + 1]]);
                            line[at..at + 2].copy_from_slice(&v.wrapping_sub(p).to_le_bytes());
                        }
                    }
                } else {
                    for k in (channels..line.len()).rev() {
                        line[k] = line[k].wrapping_sub(line[k - channels]);
                    }
                }
            }
        }
        let data = if deflate {
            bv_deflate::zlib_compress(&data, 6)
        } else {
            data
        };
        offsets.push(out.len() as u32);
        counts.push(data.len() as u32);
        out.extend_from_slice(&data);
        if out.len() % 2 == 1 {
            out.push(0);
        }
    }

    // Values that do not fit in an entry go before the directory.
    let extra = |out: &mut Vec<u8>, data: &[u8]| -> u32 {
        let at = out.len() as u32;
        out.extend_from_slice(data);
        if out.len() % 2 == 1 {
            out.push(0);
        }
        at
    };
    let bits_at = {
        let b: Vec<u8> = (0..channels).flat_map(|_| bits.to_le_bytes()).collect();
        extra(&mut out, &b)
    };
    let offsets_at = {
        let b: Vec<u8> = offsets.iter().flat_map(|o| o.to_le_bytes()).collect();
        extra(&mut out, &b)
    };
    let counts_at = {
        let b: Vec<u8> = counts.iter().flat_map(|o| o.to_le_bytes()).collect();
        extra(&mut out, &b)
    };
    let res_at = {
        let mut b = Vec::new();
        b.extend_from_slice(&72u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        extra(&mut out, &b)
    };
    let software_at = extra(&mut out, b"BananaV\0");

    // (tag, type, count, value or offset)
    let mut entries: Vec<(u16, u16, u32, u32)> = vec![
        (256, LONG, 1, w as u32),
        (257, LONG, 1, h as u32),
        (258, SHORT, channels as u32, bits_at),
        (259, SHORT, 1, if deflate { 8 } else { 1 }),
        (262, SHORT, 1, 2), // RGB
        (
            273,
            LONG,
            strips as u32,
            if strips == 1 { offsets[0] } else { offsets_at },
        ),
        (277, SHORT, 1, channels as u32),
        (278, LONG, 1, rows_per_strip as u32),
        (
            279,
            LONG,
            strips as u32,
            if strips == 1 { counts[0] } else { counts_at },
        ),
        (282, RATIONAL, 1, res_at),
        (283, RATIONAL, 1, res_at),
        (284, SHORT, 1, 1), // chunky
        (296, SHORT, 1, 2), // inches
        (305, ASCII, 8, software_at),
    ];
    if deflate {
        entries.push((317, SHORT, 1, 2));
    }
    if img.alpha {
        entries.push((338, SHORT, 1, 2)); // unassociated alpha
    }
    entries.sort_by_key(|e| e.0);
    let ifd = out.len() as u32;
    out[4..8].copy_from_slice(&ifd.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        // A single SHORT sits in the low half of the value field.
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pixels;

    #[test]
    fn header_and_directory() {
        let px = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let t = tiff(
            &Image {
                width: 2,
                height: 1,
                pixels: Pixels::Rgba8(&px),
                alpha: false,
            },
            8,
            false,
        );
        assert_eq!(&t[..4], &[b'I', b'I', 42, 0]);
        // The strip right after the header.
        assert_eq!(&t[8..14], &[1, 2, 3, 5, 6, 7]);
        let ifd = u32::from_le_bytes(t[4..8].try_into().unwrap()) as usize;
        let n = u16::from_le_bytes([t[ifd], t[ifd + 1]]);
        assert_eq!(n, 14);
    }
}
