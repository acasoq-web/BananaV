//! Pictures between BananaV and plug-ins: BananaV's are rows of
//! premultiplied RGBA floats, top row first; a plug-in's are rows of the
//! depth and channels it asked for, bottom row first (OpenFX's images go
//! up), at a pointer with a row stride.
#![allow(unsafe_code)]

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ffi::CStr;
use std::ptr::NonNull;

use bv_core::ImageRgbaF32;
use bv_ofx_sys::names::*;

/// A channel's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Depth {
    Byte,
    Short,
    Float,
}

impl Depth {
    pub fn ofx_name(self) -> &'static CStr {
        match self {
            Depth::Byte => kOfxBitDepthByte,
            Depth::Short => kOfxBitDepthShort,
            Depth::Float => kOfxBitDepthFloat,
        }
    }

    pub fn from_ofx_name(s: &str) -> Option<Depth> {
        [Depth::Float, Depth::Short, Depth::Byte]
            .into_iter()
            .find(|d| d.ofx_name().to_bytes() == s.as_bytes())
    }

    pub fn bytes(self) -> usize {
        match self {
            Depth::Byte => 1,
            Depth::Short => 2,
            Depth::Float => 4,
        }
    }
}

/// The channels of a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Components {
    Rgba,
    Rgb,
    Alpha,
}

impl Components {
    pub fn ofx_name(self) -> &'static CStr {
        match self {
            Components::Rgba => kOfxImageComponentRGBA,
            Components::Rgb => kOfxImageComponentRGB,
            Components::Alpha => kOfxImageComponentAlpha,
        }
    }

    pub fn from_ofx_name(s: &str) -> Option<Components> {
        [Components::Rgba, Components::Rgb, Components::Alpha]
            .into_iter()
            .find(|c| c.ofx_name().to_bytes() == s.as_bytes())
    }

    pub fn count(self) -> usize {
        match self {
            Components::Rgba => 4,
            Components::Rgb => 3,
            Components::Alpha => 1,
        }
    }
}

/// Whether a picture's colors are multiplied by its alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Premult {
    PreMultiplied,
    UnPreMultiplied,
    Opaque,
}

impl Premult {
    pub fn ofx_name(self) -> &'static CStr {
        match self {
            Premult::PreMultiplied => kOfxImagePreMultiplied,
            Premult::UnPreMultiplied => kOfxImageUnPreMultiplied,
            Premult::Opaque => kOfxImageOpaque,
        }
    }

    pub fn from_ofx_name(s: &str) -> Option<Premult> {
        [
            Premult::PreMultiplied,
            Premult::UnPreMultiplied,
            Premult::Opaque,
        ]
        .into_iter()
        .find(|p| p.ofx_name().to_bytes() == s.as_bytes())
    }
}

/// A picture's layout in a plug-in's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub depth: Depth,
    pub components: Components,
}

impl Format {
    pub fn pixel_bytes(self) -> usize {
        self.depth.bytes() * self.components.count()
    }
}

/// Memory aligned for vector instructions, zeroed.
#[derive(Debug)]
pub struct Buffer {
    ptr: NonNull<u8>,
    layout: Layout,
}

// SAFETY: the buffer owns its memory; plug-ins write it while the host
// waits for them, then the host reads it.
unsafe impl Send for Buffer {}
// SAFETY: as above.
unsafe impl Sync for Buffer {}

impl Buffer {
    pub fn new(len: usize) -> Buffer {
        let layout = Layout::from_size_align(len.max(1), 64).expect("buffer size");
        // SAFETY: the layout's size is not zero.
        let ptr = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
        Buffer { ptr, layout }
    }

    pub fn len(&self) -> usize {
        self.layout.size()
    }

    pub fn is_empty(&self) -> bool {
        self.layout.size() == 0
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// The bytes, to read once the plug-in is done writing.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the buffer owns `len` bytes.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size()) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: the buffer owns `len` bytes, borrowed mutably here.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: allocated with this layout.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

/// A picture in a plug-in's layout: rows of `row_bytes`, bottom first.
#[derive(Debug)]
pub struct OfxPicture {
    pub buffer: Buffer,
    pub width: usize,
    pub height: usize,
    pub row_bytes: usize,
    pub format: Format,
}

impl OfxPicture {
    pub fn new(width: usize, height: usize, format: Format) -> OfxPicture {
        let row_bytes = width * format.pixel_bytes();
        OfxPicture {
            buffer: Buffer::new(row_bytes * height),
            width,
            height,
            row_bytes,
            format,
        }
    }
}

/// A picture in the memory of a graphics card CUDA sees (OpenFX's CUDA
/// render), in a plug-in's layout: rows of `row_bytes`, bottom first, at
/// `address`.
#[derive(Debug)]
pub struct DevicePicture {
    pub address: u64,
    pub width: usize,
    pub height: usize,
    pub row_bytes: usize,
    pub format: Format,
    /// The memory, when the host allocated it for the picture; otherwise
    /// it belongs to whoever handed the picture over, and outlives the
    /// render.
    _memory: Option<bv_cuda::Memory>,
}

impl DevicePicture {
    /// A picture in memory its owner keeps while the plug-in uses it.
    pub fn borrowed(
        address: u64,
        width: usize,
        height: usize,
        row_bytes: usize,
        format: Format,
    ) -> DevicePicture {
        DevicePicture {
            address,
            width,
            height,
            row_bytes,
            format,
            _memory: None,
        }
    }

    /// `img` (premultiplied) in a plug-in's layout, sent to `device`.
    pub fn upload(
        device: &'static bv_cuda::Device,
        img: &ImageRgbaF32,
        format: Format,
    ) -> Result<DevicePicture, bv_cuda::CudaError> {
        let pic = to_ofx(img, format);
        let memory = device.alloc(pic.buffer.len())?;
        // SAFETY: memory just allocated, as big as the picture.
        unsafe { device.upload(memory.address(), pic.buffer.bytes())? };
        Ok(DevicePicture {
            address: memory.address(),
            width: pic.width,
            height: pic.height,
            row_bytes: pic.row_bytes,
            format,
            _memory: Some(memory),
        })
    }

    /// The bytes its rows take.
    pub fn len(&self) -> usize {
        self.row_bytes * self.height
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn put(dst: &mut [u8], depth: Depth, i: usize, v: f32) {
    match depth {
        Depth::Byte => dst[i] = (v.clamp(0.0, 1.0) * 255.0).round() as u8,
        Depth::Short => {
            let s = (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
            dst[i * 2..i * 2 + 2].copy_from_slice(&s.to_ne_bytes());
        }
        Depth::Float => dst[i * 4..i * 4 + 4].copy_from_slice(&v.to_ne_bytes()),
    }
}

fn take(src: &[u8], depth: Depth, i: usize) -> f32 {
    match depth {
        Depth::Byte => src[i] as f32 / 255.0,
        Depth::Short => u16::from_ne_bytes([src[i * 2], src[i * 2 + 1]]) as f32 / 65535.0,
        Depth::Float => {
            f32::from_ne_bytes([src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]])
        }
    }
}

/// Works out `rows` rows of `bytes` (`row_len` each) in bands on the
/// processor's threads: `f` gets each row's number and its bytes.
fn in_bands(bytes: &mut [u8], row_len: usize, rows: usize, f: impl Fn(usize, &mut [u8]) + Sync) {
    if row_len == 0 || rows == 0 {
        return;
    }
    // Copies are bound by memory: a few threads, never for small pictures.
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(8)
        .min(rows * row_len / (256 * 1024) + 1);
    let per = rows.div_ceil(threads.max(1));
    if threads <= 1 {
        for (y, row) in bytes.chunks_mut(row_len).take(rows).enumerate() {
            f(y, row);
        }
        return;
    }
    std::thread::scope(|s| {
        for (band, chunk) in bytes[..rows * row_len]
            .chunks_mut(per * row_len)
            .enumerate()
        {
            let f = &f;
            s.spawn(move || {
                for (i, row) in chunk.chunks_mut(row_len).enumerate() {
                    f(band * per + i, row);
                }
            });
        }
    });
}

/// `img` (premultiplied) in a plug-in's layout.
pub fn to_ofx(img: &ImageRgbaF32, format: Format) -> OfxPicture {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut pic = OfxPicture::new(w, h, format);
    let (n, depth, rb) = (format.components.count(), format.depth, pic.row_bytes);
    // Floats in RGBA (most plug-ins'): each row's numbers as they are.
    let plain = depth == Depth::Float && format.components == Components::Rgba;
    in_bands(pic.buffer.bytes_mut(), rb, h, |y, row| {
        // OpenFX's row 0 is the bottom one.
        let src = &img.data[(h - 1 - y) * w..(h - y) * w];
        if plain {
            for (d, v) in row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(src.as_flattened())
            {
                *d = v.to_ne_bytes();
            }
            return;
        }
        for (x, px) in src.iter().enumerate() {
            match format.components {
                Components::Rgba => (0..4).for_each(|c| put(row, depth, x * 4 + c, px[c])),
                Components::Rgb => (0..3).for_each(|c| put(row, depth, x * 3 + c, px[c])),
                Components::Alpha => put(row, depth, x * n, px[3]),
            }
        }
    });
    pic
}

/// A plug-in's picture of `width` × `height` at `data` with `row_bytes`
/// per row (bottom first), back to BananaV's (premultiplied).
///
/// # Safety
/// `data` holds `height` rows of `row_bytes` (which may be negative: rows
/// going down in memory), each with `width` pixels of `format`.
pub unsafe fn from_ofx(
    data: *const u8,
    width: usize,
    height: usize,
    row_bytes: isize,
    format: Format,
    premult: Premult,
) -> ImageRgbaF32 {
    let mut img = ImageRgbaF32::transparent(width as u32, height as u32);
    let row_len = width * format.pixel_bytes();
    // Premultiplied floats in RGBA (most plug-ins'): each row's numbers as
    // they are.
    let plain = format.depth == Depth::Float
        && format.components == Components::Rgba
        && premult == Premult::PreMultiplied;
    // The plug-in's rows, by address (the threads below only read them).
    let base = data as usize;
    let rows = img.data.as_flattened_mut();
    // SAFETY: four floats are sixteen bytes, the picture's rows its own.
    let bytes =
        unsafe { std::slice::from_raw_parts_mut(rows.as_mut_ptr() as *mut u8, rows.len() * 4) };
    in_bands(bytes, width * 16, height, |j, out| {
        // BananaV's row `j` from the top is OpenFX's `height - 1 - j`.
        let y = (height - 1 - j) as isize;
        // SAFETY: as the caller promises.
        let row = unsafe {
            let start = (base as *const u8).offset(y * row_bytes);
            std::slice::from_raw_parts(start, row_len)
        };
        // SAFETY: sixteen bytes a pixel, the row's own.
        let dst =
            unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut [f32; 4], width) };
        if plain {
            for (d, b) in dst
                .as_flattened_mut()
                .iter_mut()
                .zip(row.as_chunks::<4>().0)
            {
                *d = f32::from_ne_bytes(*b);
            }
            return;
        }
        for (x, px) in dst.iter_mut().enumerate() {
            let d = format.depth;
            *px = match format.components {
                Components::Rgba => [
                    take(row, d, x * 4),
                    take(row, d, x * 4 + 1),
                    take(row, d, x * 4 + 2),
                    take(row, d, x * 4 + 3),
                ],
                Components::Rgb => [
                    take(row, d, x * 3),
                    take(row, d, x * 3 + 1),
                    take(row, d, x * 3 + 2),
                    1.0,
                ],
                Components::Alpha => {
                    let a = take(row, d, x);
                    [a, a, a, a]
                }
            };
            match premult {
                Premult::UnPreMultiplied => {
                    let a = px[3];
                    px[0] *= a;
                    px[1] *= a;
                    px[2] *= a;
                }
                Premult::Opaque if format.components == Components::Rgba => px[3] = 1.0,
                _ => {}
            }
        }
    });
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ImageRgbaF32 {
        let mut img = ImageRgbaF32::transparent(3, 2);
        // Top row red, bottom row half-transparent green.
        for x in 0..3 {
            *img.pixel_mut(x, 0) = [1.0, 0.0, 0.0, 1.0];
            *img.pixel_mut(x, 1) = [0.0, 0.5, 0.0, 0.5];
        }
        img
    }

    #[test]
    fn floats_go_and_come_back_with_rows_turned() {
        let img = sample();
        let f = Format {
            depth: Depth::Float,
            components: Components::Rgba,
        };
        let pic = to_ofx(&img, f);
        // The plug-in's first row is the picture's bottom one.
        assert_eq!(take(pic.buffer.bytes(), Depth::Float, 1), 0.5);
        // SAFETY: the picture's own layout.
        let back = unsafe {
            from_ofx(
                pic.buffer.as_ptr(),
                3,
                2,
                pic.row_bytes as isize,
                f,
                Premult::PreMultiplied,
            )
        };
        assert_eq!(back, img);
    }

    #[test]
    fn bytes_shorts_and_fewer_channels() {
        let img = sample();
        for depth in [Depth::Byte, Depth::Short] {
            let f = Format {
                depth,
                components: Components::Rgb,
            };
            let pic = to_ofx(&img, f);
            // SAFETY: the picture's own layout.
            let back = unsafe {
                from_ofx(
                    pic.buffer.as_ptr(),
                    3,
                    2,
                    pic.row_bytes as isize,
                    f,
                    Premult::Opaque,
                )
            };
            assert_eq!(back.pixel(0, 0), [1.0, 0.0, 0.0, 1.0]);
            assert!((back.pixel(0, 1)[1] - 0.5).abs() < 0.003);
            assert_eq!(back.pixel(0, 1)[3], 1.0, "RGB is opaque");
        }
        // Straight colors are premultiplied on the way back.
        let f = Format {
            depth: Depth::Float,
            components: Components::Rgba,
        };
        let mut straight = ImageRgbaF32::transparent(1, 1);
        *straight.pixel_mut(0, 0) = [1.0, 1.0, 1.0, 0.25];
        let pic = to_ofx(&straight, f);
        // SAFETY: the picture's own layout.
        let back = unsafe {
            from_ofx(
                pic.buffer.as_ptr(),
                1,
                1,
                pic.row_bytes as isize,
                f,
                Premult::UnPreMultiplied,
            )
        };
        assert_eq!(back.pixel(0, 0), [0.25, 0.25, 0.25, 0.25]);
    }
}
