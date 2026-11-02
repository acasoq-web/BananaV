//! The Windows Imaging Component (WIC). Encoders: HEIF/HEIC (with the
//! HEIF and HEVC extensions of Windows) and JPEG XR (Windows Media Photo,
//! built in). Decoders, for the formats BananaV does not read itself: WebP,
//! HEIF/HEIC, AVIF, JPEG XR, camera RAW and DNG (with Windows' extensions),
//! icons. Outside Windows nothing can be read or written.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container {
    /// HEIF with HEVC pictures (`.heic`).
    Heif,
    /// JPEG XR / HD Photo (`.jxr`, `.wdp`).
    JpegXr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Windows(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unavailable => f.write_str("this image encoder is not installed"),
            Error::Windows(s) => write!(f, "Windows Imaging Component: {s}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp;

/// A picture Windows decoded: upright (its orientation applied), RGBA of
/// 16 bits per channel with straight alpha, encoded for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u16>,
}

#[cfg(windows)]
pub use imp::{can_write, decode, dimensions, encode};

/// The size a picture shows at (its orientation applied), without decoding
/// it.
#[cfg(not(windows))]
pub fn dimensions(_path: &str) -> Result<(u32, u32), Error> {
    Err(Error::Unavailable)
}

/// Decodes the image file at `path` (the largest picture of an icon, the
/// first of the others).
#[cfg(not(windows))]
pub fn decode(_path: &str) -> Result<Decoded, Error> {
    Err(Error::Unavailable)
}

/// Whether Windows can write `container` files here.
#[cfg(not(windows))]
pub fn can_write(_container: Container) -> bool {
    false
}

/// `width`×`height` RGBA 8-bit pixels (straight alpha) as a file of
/// `container`; `quality` 0–100.
#[cfg(not(windows))]
pub fn encode(
    _container: Container,
    _width: u32,
    _height: u32,
    _rgba: &[u8],
    _alpha: bool,
    _quality: u8,
) -> Result<Vec<u8>, Error> {
    Err(Error::Unavailable)
}
