//! DPX (SMPTE 268M, version 2.0): 10-bit RGB, three samples packed in
//! each 32-bit big-endian word (method A), with the BT.709 transfer.

use crate::Image;

const HEADER: usize = 2048;

fn put(v: &mut [u8], at: usize, bytes: &[u8]) {
    v[at..at + bytes.len()].copy_from_slice(bytes);
}

/// A DPX file; `creator` goes into the header.
pub fn dpx(img: &Image<'_>, creator: &str) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let size = HEADER + w * h * 4;
    let mut v = vec![0u8; HEADER];
    // Fields without a value are all ones.
    for r in [(664usize, 768usize), (1408, 2048)] {
        v[r.0..r.1].fill(0xFF);
    }
    put(&mut v, 0, b"SDPX");
    put(&mut v, 4, &(HEADER as u32).to_be_bytes());
    put(&mut v, 8, b"V2.0\0\0\0\0");
    put(&mut v, 16, &(size as u32).to_be_bytes());
    put(&mut v, 20, &1u32.to_be_bytes()); // ditto key: new frame
    put(&mut v, 24, &1664u32.to_be_bytes()); // generic header
    put(&mut v, 28, &384u32.to_be_bytes()); // industry header
    put(&mut v, 32, &0u32.to_be_bytes()); // user data
    let name = creator.as_bytes();
    put(&mut v, 160, &name[..name.len().min(99)]);
    put(&mut v, 660, &u32::MAX.to_be_bytes()); // no encryption

    // Image information header.
    put(&mut v, 768, &0u16.to_be_bytes()); // left to right, top to bottom
    put(&mut v, 770, &1u16.to_be_bytes()); // one element
    put(&mut v, 772, &(w as u32).to_be_bytes());
    put(&mut v, 776, &(h as u32).to_be_bytes());
    // Element 0.
    put(&mut v, 780, &0u32.to_be_bytes()); // unsigned
    put(&mut v, 784, &0u32.to_be_bytes()); // reference low
    put(&mut v, 788, &0f32.to_bits().to_be_bytes());
    put(&mut v, 792, &1023u32.to_be_bytes()); // reference high
    put(&mut v, 796, &2.047f32.to_bits().to_be_bytes());
    v[800] = 50; // RGB
    v[801] = 6; // ITU-R 709
    v[802] = 6;
    v[803] = 10; // bits
    put(&mut v, 804, &1u16.to_be_bytes()); // filled to 32 bits, method A
    put(&mut v, 806, &0u16.to_be_bytes()); // no run-length encoding
    put(&mut v, 808, &(HEADER as u32).to_be_bytes());
    put(&mut v, 812, &0u32.to_be_bytes());
    put(&mut v, 816, &0u32.to_be_bytes());
    // Unused elements 1–7 and the rest of the image header.
    v[852..1408].fill(0xFF);

    v.reserve(w * h * 4);
    for i in 0..w * h {
        let ten = |c| (img.get16(i, c) as u32 * 1023 + 32767) / 65535;
        let word = (ten(0) << 22) | (ten(1) << 12) | (ten(2) << 2);
        v.extend_from_slice(&word.to_be_bytes());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pixels;

    #[test]
    fn ten_bit_words() {
        let px = [65535u16, 0, 32768, 65535];
        let d = dpx(
            &Image {
                width: 1,
                height: 1,
                pixels: Pixels::Rgba16(&px),
                alpha: false,
            },
            "BananaV",
        );
        assert_eq!(&d[..4], b"SDPX");
        assert_eq!(d.len(), 2052);
        let word = u32::from_be_bytes(d[2048..2052].try_into().unwrap());
        assert_eq!(word >> 22, 1023);
        assert_eq!((word >> 12) & 1023, 0);
        assert_eq!((word >> 2) & 1023, 512);
    }
}
