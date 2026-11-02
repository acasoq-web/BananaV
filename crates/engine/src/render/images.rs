//! Image sequences: one file per frame, `name_000000.ext` onwards.

use std::path::PathBuf;

use bv_core::ImageRgbaF32;
use bv_image::{ExrCompression, Image, Pixels};

use super::{FileFormat, Output, RenderSettings, sequence_path};

/// Whether Windows can write `format` (HEIC and JPEG XR come from the
/// Windows Imaging Component and its extensions).
pub(crate) fn system_image_encoder(format: FileFormat) -> bool {
    match format {
        FileFormat::Heic => bv_wic::can_write(bv_wic::Container::Heif),
        FileFormat::WmPhoto => bv_wic::can_write(bv_wic::Container::JpegXr),
        _ => true,
    }
}

/// Display-encoded RGBA (BT.709 curve, like the preview) with straight
/// alpha, in 16 bits.
fn display16(img: &ImageRgbaF32, alpha: bool) -> Vec<u16> {
    let mut out = Vec::with_capacity(img.data.len() * 4);
    for px in &img.data {
        let a = px[3].clamp(0.0, 1.0);
        let k = if alpha && a > 0.0 { 1.0 / a } else { 1.0 };
        for c in &px[..3] {
            let v = bv_color::transfer::linear_to_bt709((c * k).clamp(0.0, 1.0));
            out.push((v * 65535.0).round() as u16);
        }
        out.push(if alpha {
            (a * 65535.0).round() as u16
        } else {
            65535
        });
    }
    out
}

fn to8(v: &[u16]) -> Vec<u8> {
    v.iter()
        .map(|x| ((*x as u32 * 255 + 32767) / 65535) as u8)
        .collect()
}

pub(super) struct ImageOut {
    format: FileFormat,
    path: PathBuf,
    alpha: bool,
    bit_depth: u8,
    quality: u8,
    files: Vec<PathBuf>,
    bytes: u64,
}

pub(super) fn open(s: &RenderSettings) -> Result<Box<dyn Output>, String> {
    let v = s.video.ok_or("no video settings")?;
    if !system_image_encoder(s.format) {
        return Err(format!(
            "Windows cannot write {} files here",
            s.format.name()
        ));
    }
    Ok(Box::new(ImageOut {
        format: s.format,
        path: s.path.clone(),
        alpha: v.alpha && s.format.supports_alpha(),
        bit_depth: v.bit_depth,
        quality: v.quality,
        files: Vec::new(),
        bytes: 0,
    }))
}

impl ImageOut {
    fn encode(&self, img: &ImageRgbaF32) -> Result<Vec<u8>, String> {
        let (w, h) = (img.width, img.height);
        let deep = self.bit_depth > 8;
        if self.format == FileFormat::Exr {
            let comp = ExrCompression::Zip;
            // Half floats unless 32-bit was asked for.
            return Ok(bv_image::exr(
                w,
                h,
                &img.data,
                self.alpha,
                self.bit_depth < 32,
                comp,
            ));
        }
        let px16 = display16(img, self.alpha);
        let px8;
        let image = if deep {
            Image {
                width: w,
                height: h,
                pixels: Pixels::Rgba16(&px16),
                alpha: self.alpha,
            }
        } else {
            px8 = to8(&px16);
            Image {
                width: w,
                height: h,
                pixels: Pixels::Rgba8(&px8),
                alpha: self.alpha,
            }
        };
        Ok(match self.format {
            FileFormat::Png => match image.pixels {
                Pixels::Rgba16(d) => bv_png::encode_rgba16(w, h, d, 6),
                Pixels::Rgba8(d) if self.alpha => bv_png::encode_rgba8(w, h, d, 6),
                Pixels::Rgba8(d) => bv_png::encode_rgb8_from_rgba(w, h, d, 6),
            },
            FileFormat::Bmp => bv_image::bmp(&image),
            FileFormat::Tiff => bv_image::tiff(&image, if deep { 16 } else { 8 }, true),
            FileFormat::Dpx => bv_image::dpx(&image, "BananaV"),
            FileFormat::Dds => bv_image::dds(&image),
            FileFormat::Jpeg => bv_image::jpeg(&image, self.quality),
            FileFormat::Heic | FileFormat::WmPhoto => {
                let rgba = to8(&px16);
                let container = if self.format == FileFormat::Heic {
                    bv_wic::Container::Heif
                } else {
                    bv_wic::Container::JpegXr
                };
                bv_wic::encode(container, w, h, &rgba, self.alpha, self.quality)
                    .map_err(|e| e.to_string())?
            }
            f => return Err(format!("{} is not an image format", f.name())),
        })
    }
}

impl Output for ImageOut {
    fn video(&mut self, img: &ImageRgbaF32, n: u64) -> Result<(), String> {
        let bytes = self.encode(img)?;
        let path = sequence_path(&self.path, n);
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.bytes += bytes.len() as u64;
        self.files.push(path);
        Ok(())
    }

    fn bytes(&self) -> u64 {
        self.bytes
    }

    fn finish(self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        Ok(self.files)
    }
}
