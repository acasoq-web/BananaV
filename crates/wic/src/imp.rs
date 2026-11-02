//! The encoders through WIC's COM interfaces.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::StructuredStorage::{CreateStreamOnHGlobal, PROPBAG2};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STATFLAG_NONAME,
    STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::System::Variant::{VARIANT, VT_R4};
use windows::core::{GUID, Interface, PWSTR};

use crate::{Container, Decoded, Error};

fn win(e: windows::core::Error) -> Error {
    Error::Windows(e.message().to_string())
}

fn guid(c: Container) -> GUID {
    match c {
        Container::Heif => GUID_ContainerFormatHeif,
        Container::JpegXr => GUID_ContainerFormatWmp,
    }
}

fn factory() -> Result<IWICImagingFactory, Error> {
    unsafe {
        // S_FALSE/RPC_E_CHANGED_MODE: the thread already has COM.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(win)
    }
}

pub fn can_write(container: Container) -> bool {
    static KNOWN: OnceLock<Mutex<HashMap<Container, bool>>> = OnceLock::new();
    let known = KNOWN.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = known.lock().ok().and_then(|k| k.get(&container).copied()) {
        return v;
    }
    // Asked on a thread of its own: COM stays out of the caller's thread.
    let ok = std::thread::spawn(move || {
        // A tiny picture must really encode: the HEIF container answers
        // without its HEVC encoder.
        encode(container, 16, 16, &[128; 16 * 16 * 4], false, 80).is_ok()
    })
    .join()
    .unwrap_or(false);
    if let Ok(mut k) = known.lock() {
        k.insert(container, ok);
    }
    ok
}

fn variant_f32(v: f32) -> VARIANT {
    let mut var = VARIANT::default();
    // SAFETY: a VT_R4 variant holds its value in `fltVal`; nothing to drop.
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = VT_R4;
        inner.Anonymous.fltVal = v;
    }
    var
}

pub fn encode(
    container: Container,
    width: u32,
    height: u32,
    rgba: &[u8],
    alpha: bool,
    quality: u8,
) -> Result<Vec<u8>, Error> {
    let factory = factory()?;
    unsafe {
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).map_err(win)?;
        let encoder = factory
            .CreateEncoder(&guid(container), std::ptr::null())
            .map_err(|_| Error::Unavailable)?;
        encoder
            .Initialize(&stream, WICBitmapEncoderNoCache)
            .map_err(win)?;
        let mut frame = None;
        let mut props = None;
        encoder
            .CreateNewFrame(&mut frame, &mut props)
            .map_err(win)?;
        let frame = frame.ok_or(Error::Unavailable)?;
        if let Some(props) = &props {
            let mut name: Vec<u16> = "ImageQuality".encode_utf16().chain([0]).collect();
            let bag = PROPBAG2 {
                pstrName: PWSTR(name.as_mut_ptr()),
                ..Default::default()
            };
            let value = variant_f32(quality.min(100) as f32 / 100.0);
            let _ = props.Write(1, &bag, &value);
        }
        frame.Initialize(props.as_ref()).map_err(win)?;
        frame.SetSize(width, height).map_err(win)?;
        // BGRA in memory; the encoder says what it takes and a converter
        // goes in between.
        let mut bgra = Vec::with_capacity(rgba.len());
        for px in rgba.as_chunks::<4>().0 {
            bgra.extend_from_slice(&[px[2], px[1], px[0], if alpha { px[3] } else { 255 }]);
        }
        let source_format = if alpha {
            GUID_WICPixelFormat32bppBGRA
        } else {
            GUID_WICPixelFormat32bppBGR
        };
        let mut want = source_format;
        frame.SetPixelFormat(&mut want).map_err(win)?;
        let bitmap = factory
            .CreateBitmapFromMemory(width, height, &source_format, width * 4, &bgra)
            .map_err(win)?;
        if want == source_format {
            frame.WriteSource(&bitmap, std::ptr::null()).map_err(win)?;
        } else {
            let conv = factory.CreateFormatConverter().map_err(win)?;
            conv.Initialize(
                &bitmap,
                &want,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(win)?;
            frame.WriteSource(&conv, std::ptr::null()).map_err(win)?;
        }
        frame.Commit().map_err(win)?;
        encoder.Commit().map_err(win)?;

        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).map_err(win)?;
        stream.Seek(0, STREAM_SEEK_SET, None).map_err(win)?;
        let mut out = vec![0u8; stat.cbSize as usize];
        let mut read = 0u32;
        stream
            .Read(
                out.as_mut_ptr() as *mut _,
                out.len() as u32,
                Some(&mut read),
            )
            .ok()
            .map_err(win)?;
        out.truncate(read as usize);
        Ok(out)
    }
}

/// The orientation (1–8) the photo's metadata gives, if any.
fn orientation(frame: &IWICBitmapFrameDecode) -> u16 {
    unsafe {
        let Ok(reader) = frame.GetMetadataQueryReader() else {
            return 1;
        };
        let mut value = windows::Win32::System::Com::StructuredStorage::PROPVARIANT::default();
        let name: Vec<u16> = "System.Photo.Orientation"
            .encode_utf16()
            .chain([0])
            .collect();
        if reader
            .GetMetadataByName(windows::core::PCWSTR(name.as_ptr()), &mut value)
            .is_err()
        {
            return 1;
        }
        u16::try_from(&value).unwrap_or(1)
    }
}

/// The transform that turns a picture stored with `orientation` upright.
fn transform(orientation: u16) -> WICBitmapTransformOptions {
    match orientation {
        2 => WICBitmapTransformFlipHorizontal,
        3 => WICBitmapTransformRotate180,
        4 => WICBitmapTransformFlipVertical,
        5 => WICBitmapTransformOptions(
            WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0,
        ),
        6 => WICBitmapTransformRotate90,
        7 => WICBitmapTransformOptions(
            WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0,
        ),
        8 => WICBitmapTransformRotate270,
        _ => WICBitmapTransformRotate0,
    }
}

fn open(factory: &IWICImagingFactory, path: &str) -> Result<IWICBitmapDecoder, Error> {
    let name: Vec<u16> = path.encode_utf16().chain([0]).collect();
    unsafe {
        factory
            .CreateDecoderFromFilename(
                windows::core::PCWSTR(name.as_ptr()),
                None,
                windows::Win32::Foundation::GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .map_err(win)
    }
}

pub fn dimensions(path: &str) -> Result<(u32, u32), Error> {
    let factory = factory()?;
    let decoder = open(&factory, path)?;
    unsafe {
        let frame = decoder.GetFrame(0).map_err(win)?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).map_err(win)?;
        Ok(if orientation(&frame) >= 5 {
            (h, w)
        } else {
            (w, h)
        })
    }
}

pub fn decode(path: &str) -> Result<Decoded, Error> {
    let factory = factory()?;
    let decoder = open(&factory, path)?;
    unsafe {
        let count = decoder.GetFrameCount().map_err(win)?.max(1);
        // An icon holds the same picture in several sizes: the largest.
        let is_icon = decoder
            .GetContainerFormat()
            .is_ok_and(|g| g == GUID_ContainerFormatIco);
        let mut pick = 0;
        if is_icon {
            let mut best = 0u64;
            for i in 0..count {
                if let Ok(f) = decoder.GetFrame(i) {
                    let (mut w, mut h) = (0u32, 0u32);
                    if f.GetSize(&mut w, &mut h).is_ok() && (w as u64 * h as u64) > best {
                        best = w as u64 * h as u64;
                        pick = i;
                    }
                }
            }
        }
        let frame = decoder.GetFrame(pick).map_err(win)?;
        let conv = factory.CreateFormatConverter().map_err(win)?;
        conv.Initialize(
            &frame,
            &GUID_WICPixelFormat64bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .map_err(win)?;
        let turn = transform(orientation(&frame));
        let upright: IWICBitmapSource = if turn == WICBitmapTransformRotate0 {
            conv.cast().map_err(win)?
        } else {
            let rot = factory.CreateBitmapFlipRotator().map_err(win)?;
            rot.Initialize(&conv, turn).map_err(win)?;
            rot.cast().map_err(win)?
        };
        let (mut w, mut h) = (0u32, 0u32);
        upright.GetSize(&mut w, &mut h).map_err(win)?;
        if w == 0 || h == 0 || w as u64 * h as u64 > 1 << 29 {
            return Err(Error::Windows("picture size".into()));
        }
        let mut px = vec![0u16; w as usize * h as usize * 4];
        let bytes = std::slice::from_raw_parts_mut(px.as_mut_ptr() as *mut u8, px.len() * 2);
        upright
            .CopyPixels(std::ptr::null(), w * 8, bytes)
            .map_err(win)?;
        Ok(Decoded {
            width: w,
            height: h,
            rgba: px,
        })
    }
}
