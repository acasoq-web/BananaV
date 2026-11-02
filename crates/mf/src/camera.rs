//! Cameras (webcams, capture devices) as the system lists them, and their
//! pictures as they come — the Screen Capture window's video sources.

use bv_core::FrameRate;

use crate::Result;

/// A camera of this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Camera {
    pub name: String,
    /// The system's link to it, the same while it stays plugged in.
    pub link: String,
}

/// One of a camera's picture formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    pub rate: FrameRate,
}

/// A picture from a camera: 8-bit BGRA (opaque), the top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraPicture {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// The cameras plugged in now.
pub fn cameras() -> Vec<Camera> {
    imp::cameras()
}

/// A camera's picture formats (each size and rate once, the largest
/// first).
pub fn camera_modes(link: &str) -> Vec<CameraMode> {
    imp::modes(link)
}

/// A camera giving its pictures.
pub struct CameraReader(imp::Reader);

impl std::fmt::Debug for CameraReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CameraReader")
            .field("mode", &self.mode())
            .finish()
    }
}

impl CameraReader {
    /// The camera at `link`, in `mode` when it has it (else its own).
    pub fn open(link: &str, mode: Option<CameraMode>) -> Result<CameraReader> {
        imp::Reader::open(link, mode).map(CameraReader)
    }

    /// The format its pictures come in.
    pub fn mode(&self) -> CameraMode {
        self.0.mode
    }

    /// Its next picture (waits for it).
    pub fn next_picture(&mut self) -> Result<CameraPicture> {
        self.0.next()
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use bv_core::FrameRate;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::core::{GUID, PWSTR};

    use super::{Camera, CameraMode, CameraPicture};
    use crate::imp::{init, win};
    use crate::{Error, Result};

    const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    /// The system's camera devices.
    fn devices() -> Result<Vec<IMFActivate>> {
        init()?;
        // SAFETY: the attribute store and the device list are created
        // here; the list's entries are taken out and the list freed.
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 1).map_err(win)?;
            let attrs = attrs.ok_or(Error::Unavailable)?;
            attrs
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(win)?;
            let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            MFEnumDeviceSources(&attrs, &mut list, &mut count).map_err(win)?;
            if list.is_null() {
                return Ok(Vec::new());
            }
            let found = std::slice::from_raw_parts_mut(list, count as usize)
                .iter_mut()
                .filter_map(Option::take)
                .collect();
            CoTaskMemFree(Some(list as *const core::ffi::c_void));
            Ok(found)
        }
    }

    fn text(a: &IMFActivate, key: &GUID) -> Option<String> {
        // SAFETY: the string the device hands over is read, then freed.
        unsafe {
            let mut p = PWSTR::null();
            let mut len = 0u32;
            a.GetAllocatedString(key, &mut p, &mut len).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
            s
        }
    }

    pub fn cameras() -> Vec<Camera> {
        devices()
            .unwrap_or_default()
            .iter()
            .filter_map(|a| {
                Some(Camera {
                    name: text(a, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)?,
                    link: text(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK)?,
                })
            })
            .collect()
    }

    /// A source reader of the camera at `link`, converting to 32-bit RGB.
    fn reader(link: &str) -> Result<IMFSourceReader> {
        let device = devices()?
            .into_iter()
            .find(|a| {
                text(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK).as_deref()
                    == Some(link)
            })
            .ok_or(Error::Unavailable)?;
        // SAFETY: COM calls on objects created here.
        unsafe {
            let source: IMFMediaSource = device.ActivateObject().map_err(win)?;
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 1).map_err(win)?;
            let attrs = attrs.ok_or(Error::Unavailable)?;
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(win)?;
            MFCreateSourceReaderFromMediaSource(&source, &attrs).map_err(win)
        }
    }

    fn mode_of(t: &IMFMediaType) -> Option<CameraMode> {
        // SAFETY: attribute queries of a media type.
        unsafe {
            let size = t.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
            let rate = t.GetUINT64(&MF_MT_FRAME_RATE).ok()?;
            let (num, den) = ((rate >> 32) as u32, rate as u32);
            (num > 0 && den > 0).then_some(CameraMode {
                width: (size >> 32) as u32,
                height: size as u32,
                rate: FrameRate::new(num, den),
            })
        }
    }

    /// The camera's native formats, with their media types.
    fn native(r: &IMFSourceReader) -> Vec<(CameraMode, IMFMediaType)> {
        let mut out = Vec::new();
        for i in 0.. {
            // SAFETY: a query of the reader's formats, until there are no
            // more.
            let Ok(t) = (unsafe { r.GetNativeMediaType(VIDEO, i) }) else {
                break;
            };
            if let Some(m) = mode_of(&t) {
                out.push((m, t));
            }
        }
        out
    }

    pub fn modes(link: &str) -> Vec<CameraMode> {
        let Ok(r) = reader(link) else {
            return Vec::new();
        };
        let mut modes: Vec<CameraMode> = native(&r).into_iter().map(|(m, _)| m).collect();
        modes.sort_by(|a, b| {
            (b.width * b.height)
                .cmp(&(a.width * a.height))
                .then(b.rate.as_f64().total_cmp(&a.rate.as_f64()))
        });
        modes.dedup();
        modes
    }

    pub struct Reader {
        reader: IMFSourceReader,
        pub mode: CameraMode,
        /// Bytes from one row to the next (negative: the bottom row first).
        stride: i32,
    }

    // SAFETY: the source reader is created in the multithreaded apartment
    // and used by one thread at a time (the reader's owner).
    unsafe impl Send for Reader {}

    impl Reader {
        pub fn open(link: &str, mode: Option<CameraMode>) -> Result<Reader> {
            let reader = reader(link)?;
            // SAFETY: COM calls on the reader and on media types made here.
            unsafe {
                if let Some(want) = mode
                    && let Some((_, t)) = native(&reader).into_iter().find(|(m, _)| *m == want)
                {
                    reader.SetCurrentMediaType(VIDEO, None, &t).map_err(win)?;
                }
                let out = MFCreateMediaType().map_err(win)?;
                out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(win)?;
                out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                    .map_err(win)?;
                reader.SetCurrentMediaType(VIDEO, None, &out).map_err(win)?;
                let current = reader.GetCurrentMediaType(VIDEO).map_err(win)?;
                let mode = mode_of(&current).ok_or(Error::Unavailable)?;
                let stride = current
                    .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                    .map_or(mode.width as i32 * 4, |s| s as i32);
                Ok(Reader {
                    reader,
                    mode,
                    stride,
                })
            }
        }

        pub fn next(&mut self) -> Result<CameraPicture> {
            let (w, h) = (self.mode.width as usize, self.mode.height as usize);
            loop {
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                // SAFETY: a synchronous read into locals; the buffer is
                // locked while it is copied.
                unsafe {
                    self.reader
                        .ReadSample(VIDEO, 0, None, Some(&mut flags), None, Some(&mut sample))
                        .map_err(win)?;
                    if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        return Err(Error::Unavailable);
                    }
                    let Some(sample) = sample else {
                        continue;
                    };
                    let buffer = sample.ConvertToContiguousBuffer().map_err(win)?;
                    let mut ptr: *mut u8 = std::ptr::null_mut();
                    let mut len = 0u32;
                    buffer.Lock(&mut ptr, None, Some(&mut len)).map_err(win)?;
                    let data = std::slice::from_raw_parts(ptr, len as usize);
                    let row = self.stride.unsigned_abs() as usize;
                    let mut bgra = vec![0u8; w * h * 4];
                    for y in 0..h {
                        let from = if self.stride < 0 {
                            (h - 1 - y) * row
                        } else {
                            y * row
                        };
                        if let Some(src) = data.get(from..from + w * 4) {
                            bgra[y * w * 4..(y + 1) * w * 4].copy_from_slice(src);
                        }
                    }
                    buffer.Unlock().map_err(win)?;
                    for px in bgra.as_chunks_mut::<4>().0 {
                        px[3] = 255;
                    }
                    return Ok(CameraPicture {
                        width: w as u32,
                        height: h as u32,
                        bgra,
                    });
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Camera, CameraMode, CameraPicture};
    use crate::{Error, Result};

    pub fn cameras() -> Vec<Camera> {
        Vec::new()
    }

    pub fn modes(_link: &str) -> Vec<CameraMode> {
        Vec::new()
    }

    pub struct Reader {
        pub mode: CameraMode,
    }

    impl Reader {
        pub fn open(_link: &str, _mode: Option<CameraMode>) -> Result<Reader> {
            Err(Error::Unavailable)
        }

        pub fn next(&mut self) -> Result<CameraPicture> {
            Err(Error::Unavailable)
        }
    }
}
