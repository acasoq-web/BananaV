//! Image files from their specifications. Written, for image-sequence
//! renders: BMP, TIFF, DPX (SMPTE 268M), OpenEXR, DDS and baseline JPEG
//! (ITU-T T.81 / JFIF). Read, for import ([`read`]): BMP, GIF (animated
//! too), JPEG, TIFF, TGA, DPX, OpenEXR and Photoshop.
//!
//! Pixels come as RGBA rows, top to bottom: display-encoded integers with
//! straight alpha for most formats, linear light with premultiplied alpha
//! (OpenEXR's convention) for EXR.

pub mod read;

mod bmp;
mod dds;
mod dpx;
mod exr;
mod jpeg;
mod tiff;

pub use bmp::bmp;
pub use dds::dds;
pub use dpx::dpx;
pub use exr::{ExrCompression, exr, f32_to_half};
pub use jpeg::jpeg;
pub use tiff::tiff;

/// RGBA pixels, row after row from the top.
#[derive(Clone, Copy, Debug)]
pub enum Pixels<'a> {
    /// 8 bits per channel.
    Rgba8(&'a [u8]),
    /// 16 bits per channel.
    Rgba16(&'a [u16]),
}

#[derive(Clone, Copy, Debug)]
pub struct Image<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels<'a>,
    /// Keep the alpha channel (otherwise it is left out of the file).
    pub alpha: bool,
}

impl Image<'_> {
    /// Channel `c` of pixel `i` in 8 bits.
    fn get8(&self, i: usize, c: usize) -> u8 {
        match self.pixels {
            Pixels::Rgba8(d) => d[i * 4 + c],
            Pixels::Rgba16(d) => (d[i * 4 + c] >> 8) as u8,
        }
    }

    /// Channel `c` of pixel `i` in 16 bits.
    fn get16(&self, i: usize, c: usize) -> u16 {
        match self.pixels {
            Pixels::Rgba8(d) => d[i * 4 + c] as u16 * 257,
            Pixels::Rgba16(d) => d[i * 4 + c],
        }
    }

    fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}
