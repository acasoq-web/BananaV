//! Decoding with Media Foundation's Source Reader.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::Once;

use bv_core::{
    AudioBuffer, ChannelLayout, ColorInfo, ColorPrimaries, ColorRange, FrameRate,
    MatrixCoefficients, PixelFormat, Plane, PlaneData, SampleRate, Tick, TransferCharacteristic,
    VideoFrame,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{GUID, HSTRING, Interface};

use crate::common::{rate_near, remap};
use crate::{AudioInfo, Error, Packet, PacketDecoder, Probe, Result, VideoInfo};

const HNS: u32 = 10_000_000;

pub(crate) fn win(e: windows::core::Error) -> Error {
    Error::Windows(e.message().to_string())
}

/// COM per thread, and Media Foundation once per process.
pub(crate) fn init() -> Result<()> {
    thread_local! {
        static COM: Cell<bool> = const { Cell::new(false) };
    }
    COM.with(|c| {
        if !c.get() {
            // S_FALSE/RPC_E_CHANGED_MODE: the thread already has COM; fine.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            c.set(true);
        }
    });
    static MF: Once = Once::new();
    static mut MF_OK: bool = false;
    MF.call_once(|| unsafe {
        MF_OK = MFStartup(MF_VERSION, MFSTARTUP_FULL).is_ok();
    });
    if unsafe { MF_OK } {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}

/// The Source Reader is free-threaded (MTA); each decoder is used by one
/// thread at a time.
struct Reader(IMFSourceReader);

// SAFETY: IMFSourceReader is created in the MTA and only used by one thread
// at a time (the decoder's owner), as Media Foundation's documentation allows.
unsafe impl Send for Reader {}

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const SOURCE: u32 = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;

/// A reader of `path`. `processing` converts what the decoder gives (10
/// bits, other layouts) to the layout asked for; `hardware` lets graphics
/// cards' decoders in.
fn open_reader(path: &str, processing: bool, hardware: bool) -> Result<IMFSourceReader> {
    open_reader_with(path, processing, hardware, None)
}

/// [`open_reader`] whose decoders may decode on the card of `d3d` (DXVA).
fn open_reader_with(
    path: &str,
    processing: bool,
    hardware: bool,
    d3d: Option<&IMFDXGIDeviceManager>,
) -> Result<IMFSourceReader> {
    init()?;
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 3).map_err(win)?;
        let attrs = attrs.ok_or(Error::Unavailable)?;
        if let Some(m) = d3d {
            attrs
                .SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, m)
                .map_err(win)?;
        }
        if hardware {
            attrs
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(win)?;
        }
        if processing {
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                .map_err(win)?;
        }
        MFCreateSourceReaderFromURL(&HSTRING::from(path), &attrs).map_err(|e| {
            if e.code() == MF_E_UNSUPPORTED_BYTESTREAM_TYPE {
                Error::UnsupportedFile
            } else {
                win(e)
            }
        })
    }
}

fn hns_to_tick(v: i64) -> Tick {
    Tick::from_timescale(v, HNS)
}

fn tick_to_hns(t: Tick) -> i64 {
    t.to_timescale(HNS)
}

fn duration(reader: &IMFSourceReader) -> Tick {
    unsafe {
        reader
            .GetPresentationAttribute(SOURCE, &MF_PD_DURATION)
            .ok()
            .and_then(|pv| u64::try_from(&pv).ok())
            .map_or(Tick::ZERO, |v| hns_to_tick(v as i64))
    }
}

fn split_u64(v: u64) -> (u32, u32) {
    ((v >> 32) as u32, v as u32)
}

/// The size shown: the minimum display aperture (without the decoder's
/// alignment rows) or the frame size.
fn display_size(t: &IMFMediaType) -> Option<(u32, u32, u32, u32)> {
    unsafe {
        let (w, h) = split_u64(t.GetUINT64(&MF_MT_FRAME_SIZE).ok()?);
        let mut blob = [0u8; 16];
        if t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut blob, None)
            .is_ok()
        {
            let ox = i16::from_le_bytes([blob[2], blob[3]]).max(0) as u32;
            let oy = i16::from_le_bytes([blob[6], blob[7]]).max(0) as u32;
            let aw = i32::from_le_bytes(blob[8..12].try_into().ok()?) as u32;
            let ah = i32::from_le_bytes(blob[12..16].try_into().ok()?) as u32;
            if aw > 0 && ah > 0 && aw <= w && ah <= h {
                return Some((aw, ah, ox, oy));
            }
        }
        Some((w, h, 0, 0))
    }
}

/// The shape of the pixels, width : height (1 : 1 when the type does not
/// say).
fn pixel_aspect(t: &IMFMediaType) -> (u32, u32) {
    unsafe {
        t.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO)
            .ok()
            .map(split_u64)
            .filter(|&(n, d)| n > 0 && d > 0)
            .unwrap_or((1, 1))
    }
}

/// A media type's field order (Media Foundation's interlace mode; mixed
/// streams, whose frames say it each, count as progressive).
fn field_order(t: &IMFMediaType) -> bv_core::FieldOrder {
    let mode = unsafe { t.GetUINT32(&MF_MT_INTERLACE_MODE) }.unwrap_or(0);
    if mode == MFVideoInterlace_FieldInterleavedUpperFirst.0 as u32
        || mode == MFVideoInterlace_FieldSingleUpper.0 as u32
    {
        bv_core::FieldOrder::UpperFirst
    } else if mode == MFVideoInterlace_FieldInterleavedLowerFirst.0 as u32
        || mode == MFVideoInterlace_FieldSingleLower.0 as u32
    {
        bv_core::FieldOrder::LowerFirst
    } else {
        bv_core::FieldOrder::Progressive
    }
}

fn native_rate(t: &IMFMediaType) -> FrameRate {
    unsafe {
        t.GetUINT64(&MF_MT_FRAME_RATE)
            .ok()
            .map(split_u64)
            .filter(|&(n, d)| n > 0 && d > 0)
            .map_or(FrameRate::FPS_30, |(n, d)| FrameRate::new(n, d))
    }
}

/// The subtype's first 32 bits: the FourCC or WAVE tag of the format.
fn subtype_code(t: &IMFMediaType) -> u32 {
    unsafe { t.GetGUID(&MF_MT_SUBTYPE).map_or(0, |g| g.data1) }
}

fn color_info(t: &IMFMediaType, width: u32, height: u32) -> ColorInfo {
    let mut c = ColorInfo::guess_for(width, height);
    unsafe {
        // MFVideoTransferMatrix: 1 = BT.709, 2 = BT.601, 4 = BT.2020.
        match t.GetUINT32(&MF_MT_YUV_MATRIX) {
            Ok(1) => c.matrix = MatrixCoefficients::Bt709,
            Ok(2) => c.matrix = MatrixCoefficients::Bt601,
            Ok(4) => {
                c.matrix = MatrixCoefficients::Bt2020Ncl;
                c.primaries = ColorPrimaries::Bt2020;
            }
            _ => {}
        }
        // MFNominalRange: 1 = 0–255, 2 = 16–235.
        match t.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE) {
            Ok(1) => c.range = ColorRange::Full,
            Ok(2) => c.range = ColorRange::Limited,
            _ => {}
        }
    }
    c.transfer = TransferCharacteristic::Bt709;
    c
}

fn head(path: &str) -> [u8; 8] {
    use std::io::Read;
    let mut head = [0u8; 8];
    if let Ok(mut f) = std::fs::File::open(path) {
        let _ = f.read(&mut head);
    }
    head
}

/// `true` for the MPEG-4 family (MP4, MOV, M4A…): its first box is one of
/// the top-level ISO boxes.
fn is_mpeg4(path: &str) -> bool {
    matches!(
        &head(path)[4..8],
        b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip"
    )
}

/// `true` for Matroska and WebM (EBML).
fn is_matroska(path: &str) -> bool {
    head(path).starts_with(&[0x1a, 0x45, 0xdf, 0xa3])
}

/// `true` for AVI files (whose streams all start at zero).
fn is_avi(path: &str) -> bool {
    let h = head(path);
    &h[..4] == b"RIFF" && {
        let mut more = [0u8; 12];
        std::fs::File::open(path)
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut more))
            .is_ok()
            && &more[8..12] == b"AVI "
    }
}

/// `true` for Windows Media (ASF: WMV, WMA).
fn is_asf(path: &str) -> bool {
    head(path).starts_with(&[0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11])
}

/// The rate the first packets' times show. Media Foundation's Matroska
/// reader gives half the real rate for files without a default duration;
/// ASF files may not say theirs (it then says 30).
fn measured_rate(path: &str) -> Option<FrameRate> {
    let reader = open_reader(path, false, false).ok()?;
    let mut times = Vec::new();
    unsafe {
        reader.SetStreamSelection(ALL, false).ok()?;
        reader.SetStreamSelection(VIDEO, true).ok()?;
        for _ in 0..64 {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample: Option<IMFSample> = None;
            reader
                .ReadSample(
                    VIDEO,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )
                .ok()?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            if sample.is_some() {
                times.push(time);
            }
        }
    }
    // Packets come in decoding order: B-frames show earlier.
    times.sort_unstable();
    times.dedup();
    if times.len() < 3 {
        return None;
    }
    let span = (times[times.len() - 1] - times[0]) as f64;
    rate_near((times.len() - 1) as f64 * HNS as f64 / span)
}

fn stream_rate(path: &str, native: &IMFMediaType) -> FrameRate {
    if (is_matroska(path) || is_asf(path))
        && let Some(r) = measured_rate(path)
    {
        return r;
    }
    native_rate(native)
}

/// The reader's stream numbers of the file's audio streams, in the file's
/// order. Media Foundation's MPEG-4 source lists the tracks backwards
/// (checked with MP4 and MOV files of three audio streams; MKV keeps the
/// order), so for those files the list is turned around.
fn audio_stream_numbers(reader: &IMFSourceReader, path: &str) -> Vec<u32> {
    let mut found = Vec::new();
    // Stream numbers run from 0 until the reader says there is no such one.
    for n in 0..64u32 {
        let Ok(t) = (unsafe { reader.GetNativeMediaType(n, 0) }) else {
            break;
        };
        if unsafe { t.GetGUID(&MF_MT_MAJOR_TYPE) }.is_ok_and(|g| g == MFMediaType_Audio) {
            found.push(n);
        }
    }
    if is_mpeg4(path) {
        found.reverse();
    }
    found
}

fn audio_info(t: &IMFMediaType, decodable: bool) -> AudioInfo {
    unsafe {
        let rate = t
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .unwrap_or(48_000);
        let ch = t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(2);
        AudioInfo {
            sample_rate: SampleRate(rate),
            channels: ch as u16,
            codec: subtype_code(t),
            decodable,
        }
    }
}

/// Sets stream `n` to give samples Windows decoded: floats, else integers
/// (the ACM wrappers of A-law, ADPCM… give only those). The layout set.
fn set_audio_output(reader: &IMFSourceReader, n: u32) -> Option<GUID> {
    [MFAudioFormat_Float, MFAudioFormat_PCM]
        .into_iter()
        .find(|sub| unsafe {
            let Ok(mt) = MFCreateMediaType() else {
                return false;
            };
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).is_ok()
                && mt.SetGUID(&MF_MT_SUBTYPE, sub).is_ok()
                && reader.SetCurrentMediaType(n, None, &mt).is_ok()
        })
}

pub fn probe(path: &str) -> Result<Probe> {
    let reader = open_reader(path, false, false)?;
    let mut probe = Probe {
        duration: duration(&reader),
        video: None,
        audio: None,
        audio_streams: Vec::new(),
    };
    unsafe {
        if let Ok(t) = reader.GetNativeMediaType(VIDEO, 0) {
            let codec = subtype_code(&t);
            // The decoder's size and rate: DV's frames come widened to square
            // pixels, Matroska's rate is measured.
            probe.video = match VideoDecoder::open(path) {
                Ok(d) => Some(d.info()),
                Err(_) => display_size(&t).map(|(w, h, _, _)| VideoInfo {
                    width: w,
                    height: h,
                    frame_rate: stream_rate(path, &t),
                    codec,
                    decodable: false,
                    pixel_aspect: pixel_aspect(&t),
                    field_order: field_order(&t),
                }),
            };
        }
        for n in audio_stream_numbers(&reader, path) {
            if let Ok(t) = reader.GetNativeMediaType(n, 0) {
                let decodable = set_audio_output(&reader, n).is_some();
                probe.audio_streams.push(audio_info(&t, decodable));
            }
        }
        if probe.audio_streams.is_empty()
            && let Ok(t) = reader.GetNativeMediaType(AUDIO, 0)
        {
            let decodable = set_audio_output(&reader, AUDIO).is_some();
            probe.audio_streams.push(audio_info(&t, decodable));
        }
        probe.audio = probe.audio_streams.first().copied();
    }
    if probe.video.is_none() && probe.audio.is_none() {
        return Err(Error::NoStream);
    }
    Ok(probe)
}

fn seek(reader: &IMFSourceReader, t: Tick) -> Result<()> {
    unsafe {
        let pv = PROPVARIANT::from(tick_to_hns(t.max(Tick::ZERO)));
        reader.SetCurrentPosition(&GUID::zeroed(), &pv).map_err(win)
    }
}

/// A sample read: (time, duration, bytes).
struct RawSample {
    time: i64,
    duration: i64,
    data: Vec<u8>,
    pitch: usize,
    keyframe: bool,
    /// The decoder's output changed before this sample (it found the real
    /// channel count, frame size…): the current type says it.
    changed: bool,
    /// The picture was decoded on the graphics card (and copied from it).
    on_card: bool,
}

/// A sample read whose data was not looked at yet: a picture decoded on
/// the graphics card may still be decoding, or being copied to memory.
struct Unopened {
    sample: Option<IMFSample>,
    time: i64,
    changed: bool,
    /// The picture's copy from the card, started when it was read.
    copy: Option<Copying>,
}

/// A decoded picture being copied from the card to memory.
struct Copying {
    staging: windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
    context: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    width: u32,
    height: u32,
    duration: i64,
    keyframe: bool,
}

// SAFETY: Media Foundation's samples are free-threaded; a sample is used by
// one thread at a time (its decoder's owner).
unsafe impl Send for Unopened {}

/// Reads the stream's next sample; `None` at the end.
fn read_sample(reader: &IMFSourceReader, stream: u32) -> Result<Option<RawSample>> {
    next_sample(reader, stream)?.map(open_sample).transpose()
}

/// Reads the stream's next sample without looking at its data; `None` at
/// the end.
fn next_sample(reader: &IMFSourceReader, stream: u32) -> Result<Option<Unopened>> {
    let mut changed = false;
    loop {
        let mut flags = 0u32;
        let mut time = 0i64;
        let mut sample: Option<IMFSample> = None;
        unsafe {
            reader
                .ReadSample(
                    stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )
                .map_err(win)?;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            return Ok(None);
        }
        changed |= flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0;
        let Some(sample) = sample else {
            // A gap in the stream or a format change: try again.
            continue;
        };
        return Ok(Some(Unopened {
            sample: Some(sample),
            time,
            changed,
            copy: None,
        }));
    }
}

/// A sample's data (waiting, for a picture still decoding on the card).
fn open_sample(u: Unopened) -> Result<RawSample> {
    let Unopened {
        sample,
        time,
        changed,
        copy,
    } = u;
    if let Some(c) = copy {
        return open_copy(c, time, changed).map(|(raw, _)| raw);
    }
    let sample = sample.ok_or(Error::Unavailable)?;
    {
        unsafe {
            let duration = sample.GetSampleDuration().unwrap_or(0);
            let keyframe = sample
                .GetUINT32(&MFSampleExtension_CleanPoint)
                .is_ok_and(|v| v != 0);
            let buffer = sample.ConvertToContiguousBuffer().map_err(win)?;
            let on_card = buffer.cast::<IMFDXGIBuffer>().is_ok();
            let mut pitch = 0usize;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            // A picture's buffer is read in its own layout (rows `pitch`
            // apart); its current length is that of the packed picture,
            // which some decoders (VP9, AV1, H.263) leave at 0 or below the
            // real size, so the size comes from the lock.
            let data = if let Ok(b2) = buffer.cast::<IMF2DBuffer2>() {
                let mut p: i32 = 0;
                let mut start: *mut u8 = std::ptr::null_mut();
                let mut size = 0u32;
                b2.Lock2DSize(
                    MF2DBuffer_LockFlags_Read,
                    &mut ptr,
                    &mut p,
                    &mut start,
                    &mut size,
                )
                .map_err(win)?;
                pitch = p.unsigned_abs() as usize;
                let skip = ptr.offset_from(start).max(0) as usize;
                let len = (size as usize).saturating_sub(skip);
                let v = std::slice::from_raw_parts(ptr, len).to_vec();
                b2.Unlock2D().map_err(win)?;
                v
            } else {
                let len = buffer.GetCurrentLength().map_err(win)? as usize;
                buffer.Lock(&mut ptr, None, None).map_err(win)?;
                let v = std::slice::from_raw_parts(ptr, len).to_vec();
                buffer.Unlock().map_err(win)?;
                v
            };
            Ok(RawSample {
                time,
                duration,
                data,
                pitch,
                keyframe,
                changed,
                on_card,
            })
        }
    }
}

/// Samples read ahead of the one opened when decoding on the card: their
/// copies to memory start when they are read, and by the time a picture is
/// opened it is there.
const CARD_AHEAD: usize = 3;

/// Starts copying the sample's picture from the card to memory (a staging
/// texture from `spare` or a new one); `false` when it is not a picture on
/// the card that can be copied this way (Media Foundation's lock reads it).
fn start_copy(
    u: &mut Unopened,
    spare: &mut Vec<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
) -> Result<bool> {
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_NV12, DXGI_FORMAT_P010, DXGI_SAMPLE_DESC,
    };
    let Some(sample) = &u.sample else {
        return Ok(false);
    };
    unsafe {
        let Ok(buffer) = sample.GetBufferByIndex(0) else {
            return Ok(false);
        };
        let Ok(dxgi) = buffer.cast::<IMFDXGIBuffer>() else {
            return Ok(false);
        };
        let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();
        dxgi.GetResource(&ID3D11Texture2D::IID, &mut raw)
            .map_err(win)?;
        if raw.is_null() {
            return Ok(false);
        }
        let texture = ID3D11Texture2D::from_raw(raw);
        let index = dxgi.GetSubresourceIndex().map_err(win)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        if desc.Format != DXGI_FORMAT_NV12 && desc.Format != DXGI_FORMAT_P010 {
            return Ok(false);
        }
        let device = texture.GetDevice().map_err(win)?;
        let context = device.GetImmediateContext().map_err(win)?;
        let staging = match spare.iter().position(|s| {
            let mut d = D3D11_TEXTURE2D_DESC::default();
            s.GetDesc(&mut d);
            (d.Width, d.Height, d.Format) == (desc.Width, desc.Height, desc.Format)
        }) {
            Some(i) => spare.swap_remove(i),
            None => {
                let sd = D3D11_TEXTURE2D_DESC {
                    Width: desc.Width,
                    Height: desc.Height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: desc.Format,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                };
                let mut t = None;
                device
                    .CreateTexture2D(&sd, None, Some(&mut t))
                    .map_err(win)?;
                t.ok_or(Error::Unavailable)?
            }
        };
        context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, index, None);
        context.Flush();
        let duration = sample.GetSampleDuration().unwrap_or(0);
        let keyframe = sample
            .GetUINT32(&MFSampleExtension_CleanPoint)
            .is_ok_and(|v| v != 0);
        u.copy = Some(Copying {
            staging,
            context,
            width: desc.Width,
            height: desc.Height,
            duration,
            keyframe,
        });
        // The card keeps the surface until the copy is done.
        u.sample = None;
        Ok(true)
    }
}

/// The picture a copy brought to memory (NV12 or P010: the luma rows,
/// then the chroma rows), and its staging texture for the next copy.
fn open_copy(
    c: Copying,
    time: i64,
    changed: bool,
) -> Result<(
    RawSample,
    windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
)> {
    use windows::Win32::Graphics::Direct3D11::*;
    unsafe {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // A map that waits holds the device, and the decoders sharing it
        // wait too: while the copy is not done, ask again a little later.
        let flag = D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32;
        let started = std::time::Instant::now();
        loop {
            match c
                .context
                .Map(&c.staging, 0, D3D11_MAP_READ, flag, Some(&mut mapped))
            {
                Ok(()) => break,
                Err(e)
                    if e.code() == windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING
                        && started.elapsed() < std::time::Duration::from_secs(2) =>
                {
                    std::thread::sleep(std::time::Duration::from_micros(250));
                }
                Err(e) => return Err(win(e)),
            }
        }
        let pitch = mapped.RowPitch as usize;
        let rows = c.height as usize + c.height as usize / 2;
        let data = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * rows).to_vec();
        c.context.Unmap(&c.staging, 0);
        let _ = c.width;
        Ok((
            RawSample {
                time,
                duration: c.duration,
                data,
                pitch,
                keyframe: c.keyframe,
                changed,
                on_card: true,
            },
            c.staging,
        ))
    }
}

/// How the decoded frames are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// 10-bit samples (in the high bits of 16), Y then U and V
    /// interleaved: what decoders give for 10-bit video, kept as it is.
    P010,
    /// Y, then U and V interleaved (what Windows' decoders give).
    Nv12,
    /// Y, U, V (AV1's decoder gives no NV12).
    I420,
}

/// Whether a stream is 10-bit video its decoder gives as such: HEVC Main10
/// (the camera and phone recordings). Asking other decoders for P010 is not
/// safe (VP9's never answers), so their 10-bit video comes in 8 bits.
fn ten_bit(native: &IMFMediaType) -> bool {
    const HEVC_MAIN_10: u32 = 2;
    subtype_code(native) == crate::codec::HEVC
        && unsafe { native.GetUINT32(&MF_MT_VIDEO_PROFILE) }.is_ok_and(|p| p == HEVC_MAIN_10)
}

impl Layout {
    /// Whether the decoder's pictures are converted to this layout (8-bit
    /// ones); 10-bit pictures are taken only as the decoder gives them.
    fn converted(self) -> bool {
        self != Layout::P010
    }

    fn bytes_per_sample(self) -> usize {
        if self == Layout::P010 { 2 } else { 1 }
    }
}

/// Who decodes a video.
#[derive(Clone)]
enum Mode {
    /// Windows' decoders, on the processor.
    Processor,
    /// Windows' decoders, on a graphics card (DXVA).
    Card(std::sync::Arc<crate::dxva::DecodeDevice>),
    /// The graphics cards' own decoders (for formats only they decode).
    VendorMft,
}

impl Mode {
    fn reader(&self, path: &str, processing: bool) -> Result<IMFSourceReader> {
        match self {
            Mode::Processor => open_reader_with(path, processing, false, None),
            Mode::Card(d) => open_reader_with(path, processing, false, Some(&d.manager)),
            Mode::VendorMft => open_reader_with(path, processing, true, None),
        }
    }
}

pub struct VideoDecoder {
    reader: Reader,
    /// To open the file again (see `frame_at`).
    path: String,
    mode: Mode,
    /// Pictures come from the graphics card.
    on_card: bool,
    /// Samples read and not opened yet (see `CARD_AHEAD`).
    queue: VecDeque<Unopened>,
    /// Staging textures free for the next copies from the card.
    spare: Vec<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    /// The reader got to the end of the stream (the queue may still hold
    /// samples).
    source_end: bool,
    layout: Layout,
    width: u32,
    height: u32,
    offset: (u32, u32),
    rate: FrameRate,
    color: ColorInfo,
    codec: u32,
    /// The shape of the frames' pixels (as the decoder gives them: DV's
    /// come widened to square ones).
    pixel_aspect: (u32, u32),
    /// The fields' order the file says.
    field_order: bv_core::FieldOrder,
    /// The last frames decoded (shared: a picture is not copied each time
    /// it is asked for).
    recent: VecDeque<std::sync::Arc<VideoFrame>>,
    /// The time of the last frame decoded (to go on without seeking).
    last: Option<Tick>,
    /// The first frame decoded since the last seek.
    first: Option<Tick>,
    /// The keyframes' times, when the file says (MP4, MOV): going on to a
    /// later frame with no keyframe on the way beats a seek, however far.
    keyframes: Option<std::sync::Arc<Vec<Tick>>>,
    eos: bool,
    /// Time (100 ns) of the first frame. Media Foundation does not apply the
    /// MP4 edit list (B-frame delay, say) and MPEG-TS times start anywhere:
    /// everything is shifted for the first frame to be zero.
    time_offset: i64,
}

impl std::fmt::Debug for VideoDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoDecoder")
            .field("size", &(self.width, self.height))
            .field("layout", &self.layout)
            .finish()
    }
}

/// Formats and layouts the graphics cards' decoders failed once: not tried
/// again (one can take a second to fail). Windows' own decoders are always
/// tried: whether they decode depends on the file (H.264 4:2:2 does not).
static FAILED: std::sync::Mutex<Vec<(u32, Layout)>> = std::sync::Mutex::new(Vec::new());

/// Formats Windows' decoders failed on a graphics card: decoded on the
/// processor from then on.
static CARD_FAILED: std::sync::Mutex<Vec<(u32, Layout)>> = std::sync::Mutex::new(Vec::new());

fn hardware_failed(codec: u32, layout: Layout) -> bool {
    FAILED.lock().is_ok_and(|f| f.contains(&(codec, layout)))
}

impl VideoDecoder {
    pub fn open(path: &str) -> Result<VideoDecoder> {
        let mut error = Error::NoStream;
        // On the graphics card first, when hardware decoding is on: Windows'
        // decoders drive the card's video engine through Direct3D 11.
        // 10-bit video is decoded as it is (P010) first.
        let setting = if crate::PROCESSOR_ONLY.get() {
            crate::HardwareDecoding::Off
        } else {
            crate::hardware_decoding()
        };
        if let Some(device) = crate::dxva::device(&setting) {
            for layout in [Layout::P010, Layout::Nv12] {
                match VideoDecoder::open_as(path, Mode::Card(device.clone()), layout) {
                    Ok(d) => return Ok(d),
                    Err(Error::NoStream) => return Err(Error::NoStream),
                    Err(e) => error = e,
                }
            }
        }
        // Then Windows' own decoders on the processor: without a Direct3D
        // device the graphics cards' own decoders are slow to start (AMD's
        // MPEG-4 one takes a second) or fail (NVIDIA's MJPEG one). They stay
        // as a way out for formats only they decode. Some decoders give no
        // NV12 (AV1's gives nothing): I420 then.
        for mode in [Mode::Processor, Mode::VendorMft] {
            for layout in [Layout::P010, Layout::Nv12, Layout::I420] {
                match VideoDecoder::open_as(path, mode.clone(), layout) {
                    Ok(d) => return Ok(d),
                    Err(Error::NoStream) => return Err(Error::NoStream),
                    Err(e) => error = e,
                }
            }
        }
        Err(error)
    }

    fn open_as(path: &str, mode: Mode, layout: Layout) -> Result<VideoDecoder> {
        let reader = mode.reader(path, layout.converted())?;
        unsafe {
            let native = reader
                .GetNativeMediaType(VIDEO, 0)
                .map_err(|_| Error::NoStream)?;
            let codec = subtype_code(&native);
            // Windows' MPEG-1/2 decoder loses a file's last frames on the
            // graphics card after a seek; on the processor it is cheap.
            let mpeg = |g: GUID| native.GetGUID(&MF_MT_SUBTYPE).is_ok_and(|s| s == g);
            if matches!(mode, Mode::Card(_))
                && (mpeg(MFVideoFormat_MPEG2) || mpeg(MFVideoFormat_MPG1))
            {
                return Err(Error::Windows("decoded on the processor".into()));
            }
            let failed_before = match mode {
                Mode::VendorMft => hardware_failed(codec, layout),
                Mode::Card(_) => CARD_FAILED
                    .lock()
                    .is_ok_and(|f| f.contains(&(codec, layout))),
                Mode::Processor => false,
            };
            if failed_before {
                return Err(Error::Windows("no decoder".into()));
            }
            if layout == Layout::P010 && !ten_bit(&native) {
                return Err(Error::Windows("not 10-bit".into()));
            }
            let result = VideoDecoder::start(path, mode.clone(), reader, &native, layout);
            if result.is_err() {
                match mode {
                    Mode::VendorMft => {
                        if let Ok(mut f) = FAILED.lock() {
                            f.push((codec, layout));
                        }
                    }
                    // An 8-bit video does not open as P010: not a failure.
                    Mode::Card(_) if layout != Layout::P010 => {
                        if let Ok(mut f) = CARD_FAILED.lock() {
                            f.push((codec, layout));
                        }
                    }
                    Mode::Card(_) => {}
                    Mode::Processor => {}
                }
            }
            result
        }
    }

    /// Sets a reader to give only the video, decoded in `layout`; the type
    /// it gives.
    fn configure(reader: &IMFSourceReader, layout: Layout) -> Result<IMFMediaType> {
        unsafe {
            reader.SetStreamSelection(ALL, false).map_err(win)?;
            reader.SetStreamSelection(VIDEO, true).map_err(win)?;
            let mt = MFCreateMediaType().map_err(win)?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(win)?;
            let sub = match layout {
                Layout::P010 => MFVideoFormat_P010,
                Layout::Nv12 => MFVideoFormat_NV12,
                Layout::I420 => MFVideoFormat_I420,
            };
            mt.SetGUID(&MF_MT_SUBTYPE, &sub).map_err(win)?;
            reader.SetCurrentMediaType(VIDEO, None, &mt).map_err(win)?;
            reader.GetCurrentMediaType(VIDEO).map_err(win)
        }
    }

    /// Sets the reader to decode in `layout` and reads the first frame.
    fn start(
        path: &str,
        mode: Mode,
        reader: IMFSourceReader,
        native: &IMFMediaType,
        layout: Layout,
    ) -> Result<VideoDecoder> {
        {
            let current = VideoDecoder::configure(&reader, layout)?;
            let (w, h, ox, oy) = display_size(&current)
                .or_else(|| display_size(native))
                .ok_or(Error::NoStream)?;
            let mut d = VideoDecoder {
                reader: Reader(reader),
                path: path.to_string(),
                mode,
                on_card: false,
                queue: VecDeque::new(),
                spare: Vec::new(),
                source_end: false,
                layout,
                width: w & !1,
                height: h & !1,
                offset: (ox & !1, oy & !1),
                rate: stream_rate(path, native),
                color: color_info(native, w, h),
                codec: subtype_code(native),
                pixel_aspect: pixel_aspect(&current),
                field_order: field_order(native),
                recent: VecDeque::new(),
                last: None,
                first: None,
                keyframes: crate::mp4::video_keyframes_cached(path),
                eos: false,
                time_offset: 0,
            };
            // The first frame must come, and in a layout understood. It is
            // kept: playing from the start goes on from it (seeking to the
            // start of an MPEG-TS is not reliable).
            let first = read_sample(&d.reader.0, VIDEO)?
                .ok_or_else(|| Error::Windows("no frames".into()))?;
            d.time_offset = first.time.max(0);
            d.on_card = first.on_card;
            let frame = d
                .to_frame(&first)
                .ok_or_else(|| Error::Windows("unexpected frame layout".into()))?;
            d.last = Some(frame.pts);
            d.first = Some(frame.pts);
            d.recent.push_back(std::sync::Arc::new(frame));
            Ok(d)
        }
    }

    /// A new reader of the file, at its start.
    fn reopen(&mut self) -> Result<()> {
        let reader = self.mode.reader(&self.path, self.layout.converted())?;
        VideoDecoder::configure(&reader, self.layout)?;
        self.reader = Reader(reader);
        self.queue.clear();
        self.source_end = false;
        self.recent.clear();
        self.eos = false;
        self.last = None;
        self.first = None;
        Ok(())
    }

    /// Whether the pictures are decoded on the graphics card.
    pub fn on_graphics_card(&self) -> bool {
        self.on_card
    }

    pub fn info(&self) -> VideoInfo {
        VideoInfo {
            width: self.width,
            height: self.height,
            frame_rate: self.rate,
            codec: self.codec,
            decodable: true,
            pixel_aspect: self.pixel_aspect,
            field_order: self.field_order,
        }
    }

    /// The frame of a sample, as separate Y, U and V planes.
    fn to_frame(&self, s: &RawSample) -> Option<VideoFrame> {
        let (w, h) = (self.width as usize, self.height as usize);
        let bps = self.layout.bytes_per_sample();
        let pitch = if s.pitch > 0 { s.pitch } else { w * bps };
        // The allocated height (with alignment) from the buffer's size.
        let alloc_h = s.data.len() * 2 / 3 / pitch;
        let (ox, oy) = (self.offset.0 as usize, self.offset.1 as usize);
        if alloc_h < h + oy || pitch < (w + ox) * bps {
            return None;
        }
        if self.layout == Layout::P010 {
            return self.to_frame_10(s, pitch, alloc_h);
        }
        let mut y = Vec::with_capacity(w * h);
        for row in 0..h {
            let start = (row + oy) * pitch + ox;
            y.extend_from_slice(&s.data[start..start + w]);
        }
        let (cw, ch) = (w / 2, h / 2);
        let chroma = pitch * alloc_h;
        let mut u = Vec::with_capacity(cw * ch);
        let mut v = Vec::with_capacity(cw * ch);
        match self.layout {
            Layout::Nv12 => {
                for row in 0..ch {
                    let start = chroma + (row + oy / 2) * pitch + ox;
                    let line = s.data.get(start..start + cw * 2)?;
                    for &[cb, cr] in line.as_chunks::<2>().0 {
                        u.push(cb);
                        v.push(cr);
                    }
                }
            }
            Layout::I420 => {
                let cp = pitch / 2;
                let v_base = chroma + cp * (alloc_h / 2);
                for row in 0..ch {
                    let at = (row + oy / 2) * cp + ox / 2;
                    u.extend_from_slice(s.data.get(chroma + at..chroma + at + cw)?);
                    v.extend_from_slice(s.data.get(v_base + at..v_base + at + cw)?);
                }
            }
            Layout::P010 => return None,
        }
        let plane = |data: Vec<u8>, pw: usize, ph: usize| Plane {
            width: pw,
            height: ph,
            stride: pw,
            data: PlaneData::U8(data),
        };
        let duration = if s.duration > 0 {
            hns_to_tick(s.duration)
        } else {
            self.rate.frame_duration()
        };
        Some(VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Yuv420P8,
            color: self.color,
            planes: vec![plane(y, w, h), plane(u, cw, ch), plane(v, cw, ch)],
            pts: self.snap(hns_to_tick(s.time - self.time_offset)),
            duration,
        })
    }

    /// A P010 sample as 10-bit planes (the samples' high 10 bits).
    fn to_frame_10(&self, s: &RawSample, pitch: usize, alloc_h: usize) -> Option<VideoFrame> {
        let (w, h) = (self.width as usize, self.height as usize);
        let (ox, oy) = (self.offset.0 as usize, self.offset.1 as usize);
        let mut y = Vec::with_capacity(w * h);
        for row in 0..h {
            let start = (row + oy) * pitch + ox * 2;
            let line = s.data.get(start..start + w * 2)?;
            y.extend(
                line.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_le_bytes(*b) >> 6),
            );
        }
        let (cw, ch) = (w / 2, h / 2);
        let chroma = pitch * alloc_h;
        let mut u = Vec::with_capacity(cw * ch);
        let mut v = Vec::with_capacity(cw * ch);
        for row in 0..ch {
            let start = chroma + (row + oy / 2) * pitch + ox * 2;
            let line = s.data.get(start..start + cw * 4)?;
            for b in line.as_chunks::<4>().0 {
                u.push(u16::from_le_bytes([b[0], b[1]]) >> 6);
                v.push(u16::from_le_bytes([b[2], b[3]]) >> 6);
            }
        }
        let plane = |data: Vec<u16>, pw: usize, ph: usize| Plane {
            width: pw,
            height: ph,
            stride: pw,
            data: PlaneData::U16(data),
        };
        let duration = if s.duration > 0 {
            hns_to_tick(s.duration)
        } else {
            self.rate.frame_duration()
        };
        Some(VideoFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::Yuv420P10,
            color: self.color,
            planes: vec![plane(y, w, h), plane(u, cw, ch), plane(v, cw, ch)],
            pts: self.snap(hns_to_tick(s.time - self.time_offset)),
            duration,
        })
    }

    /// Media Foundation's times are rounded to 100 ns: what is within 0.5 ms
    /// of the frame grid goes on it.
    fn snap(&self, t: Tick) -> Tick {
        let s = self.rate.snap_nearest(t);
        if (t - s).abs() < Tick::from_seconds_f64(0.0005) {
            s
        } else {
            t
        }
    }

    /// The frame visible at `t` among those decoded: the last one at or
    /// before `t`, as long as the next one (if any) starts after `t`.
    fn cached(&self, t: Tick) -> Option<&std::sync::Arc<VideoFrame>> {
        let nominal = self.rate.frame_duration();
        let n = self.recent.len();
        (0..n).find_map(|i| {
            let f = &self.recent[i];
            let next = self.recent.get(i + 1);
            let visible = f.pts <= t
                && match next {
                    Some(nx) => t < nx.pts,
                    None => t < f.pts + nominal,
                };
            visible.then_some(f)
        })
    }

    /// The frame visible at the media's time `t`.
    pub fn frame_at(&mut self, t: Tick) -> Result<std::sync::Arc<VideoFrame>> {
        self.frame_at_with(t, &mut crate::Straight)
    }

    /// [`frame_at`](Self::frame_at) that tells `way` about the frames on
    /// the way to `t` (after a seek, from the keyframe on): it may stop the
    /// decoding ([`Error::Interrupted`]) or ask for some of them.
    pub fn frame_at_with(
        &mut self,
        t: Tick,
        way: &mut dyn crate::Way,
    ) -> Result<std::sync::Arc<VideoFrame>> {
        init()?;
        if let Some(f) = self.cached(t) {
            return Ok(f.clone());
        }
        // Going on is much cheaper than seeking (normal playback), and so
        // is any later frame with no keyframe on the way (a seek would
        // start at the keyframe before: a long-GOP 4K recording decodes
        // hundreds of frames again).
        let slack = Tick(self.rate.frame_duration().0 / 2);
        let ahead = !self.eos
            && self.last.is_some_and(|last| {
                t >= last
                    && (t < last + Tick::from_seconds(2)
                        || self
                            .keyframes
                            .as_deref()
                            .is_some_and(|k| crate::mp4::same_group(k, last, t, slack)))
            });
        if ahead && let Some(f) = self.decode_to(t, way)? {
            return Ok(f);
        }
        // Some files are not sought to a keyframe (MPEG-TS, some Matroska):
        // the decoder then shows nothing until the next one. The search
        // starts earlier until the first frame decoded is not after `t`.
        let late = Tick(self.rate.frame_duration().0 / 2);
        let mut tried_first = false;
        for back in [0.0, 1.0, 4.0, 15.0, f64::INFINITY] {
            let from = if back.is_finite() {
                (t - Tick::from_seconds_f64(back)).max(Tick::ZERO)
            } else {
                Tick::ZERO
            };
            if back.is_finite() && from == Tick::ZERO {
                // The first frame's time once is enough.
                if tried_first {
                    continue;
                }
                tried_first = true;
            }
            if back.is_finite() {
                // Some sources refuse some positions (an MPEG program
                // stream its last seconds): an earlier one may do.
                if seek(&self.reader.0, from + hns_to_tick(self.time_offset)).is_err() {
                    continue;
                }
                self.queue.clear();
                self.source_end = false;
                self.recent.clear();
                self.eos = false;
                self.last = None;
                self.first = None;
            } else {
                // At last, the file from its very start, read again (an
                // MPEG-TS's first keyframe may sit before the first frame's
                // time, and its seek to the start is not reliable).
                self.reopen()?;
            }
            let found = self.decode_to(t, way)?;
            let in_time = self.first.is_some_and(|f| f <= t + late);
            match found {
                Some(f) if in_time || !back.is_finite() => return Ok(f),
                _ => {}
            }
        }
        Err(Error::Windows("no frames".into()))
    }

    /// The next sample, not opened yet; `None` at the end.
    fn next_unopened(&mut self) -> Result<Option<Unopened>> {
        let ahead = if matches!(self.mode, Mode::Card(_)) {
            CARD_AHEAD
        } else {
            0
        };
        while !self.source_end && self.queue.len() <= ahead {
            match next_sample(&self.reader.0, VIDEO)? {
                Some(mut u) => {
                    if ahead > 0 {
                        start_copy(&mut u, &mut self.spare)?;
                    }
                    self.queue.push_back(u);
                }
                None => self.source_end = true,
            }
        }
        Ok(self.queue.pop_front())
    }

    /// Opens a sample (its copy from the card, or its buffer) into a frame
    /// kept among the recent ones (`None`: a layout not understood).
    fn keep(&mut self, mut u: Unopened) -> Result<Option<std::sync::Arc<VideoFrame>>> {
        let raw = match u.copy.take() {
            Some(c) => {
                let (raw, staging) = open_copy(c, u.time, u.changed)?;
                self.spare.push(staging);
                raw
            }
            None => open_sample(u)?,
        };
        let Some(frame) = self.to_frame(&raw) else {
            return Ok(None);
        };
        let f = std::sync::Arc::new(frame);
        self.recent.push_back(f.clone());
        if self.recent.len() > 8 {
            self.recent.pop_front();
        }
        Ok(Some(f))
    }

    /// A sample not wanted: not copied out at all. The recent frames stop
    /// being one after another, so they are let go.
    fn discard(&mut self, mut u: Unopened) {
        if let Some(c) = u.copy.take() {
            self.spare.push(c.staging);
        }
        self.recent.clear();
    }

    /// Decodes on until the frame visible at `t` (the last one at the end).
    /// Only the frames that matter are copied out of the decoder: the one
    /// at `t`, the few before it (asked for next, often) and those `way`
    /// wants to show — a seek in a recording with a keyframe every few
    /// seconds decodes hundreds.
    fn decode_to(
        &mut self,
        t: Tick,
        way: &mut dyn crate::Way,
    ) -> Result<Option<std::sync::Arc<VideoFrame>>> {
        let near = self.rate.frame_duration().0 * 8;
        // The sample before, not opened yet: it is the one visible at `t`
        // when the next one starts after `t`.
        let mut held: Option<(Tick, Unopened)> = None;
        loop {
            if !way.go_on() {
                if let Some((_, h)) = held.take() {
                    self.discard(h);
                }
                return Err(Error::Interrupted);
            }
            let Some(u) = self.next_unopened()? else {
                self.eos = true;
                if let Some((_, h)) = held.take() {
                    self.keep(h)?;
                }
                // After the end, the last frame stays.
                return Ok(self.recent.back().cloned());
            };
            let pts = self.snap(hns_to_tick(u.time - self.time_offset));
            self.first.get_or_insert(pts);
            self.last = Some(pts);
            if let Some((held_pts, h)) = held.take() {
                if pts > t || held_pts.0 + near >= t.0 {
                    self.keep(h)?;
                } else if way.want() {
                    if let Some(f) = self.keep(h)? {
                        way.show(&f);
                    }
                } else {
                    self.discard(h);
                }
            }
            if pts > t {
                // The frame after: its start ends the visible one.
                self.keep(u)?;
                let n = self.recent.len();
                if n == 0 {
                    continue;
                }
                // The visible one is the one before (if at or before `t`);
                // right after an imprecise seek, the first one does.
                let pick = if n >= 2 && self.recent[n - 2].pts <= t {
                    n - 2
                } else {
                    n - 1
                };
                return Ok(Some(self.recent[pick].clone()));
            }
            held = Some((pts, u));
        }
    }
}

/// The compressed frames of a file's video, for formats Windows reads but
/// has no decoder for (ProRes…).
pub struct PacketReader {
    reader: Reader,
    info: VideoInfo,
    /// Time (100 ns) of the first packet: see `VideoDecoder::time_offset`.
    time_offset: i64,
}

impl std::fmt::Debug for PacketReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PacketReader")
            .field("info", &self.info)
            .finish()
    }
}

impl PacketReader {
    pub fn video(path: &str) -> Result<PacketReader> {
        let reader = open_reader(path, false, false)?;
        unsafe {
            let native = reader
                .GetNativeMediaType(VIDEO, 0)
                .map_err(|_| Error::NoStream)?;
            reader.SetStreamSelection(ALL, false).map_err(win)?;
            reader.SetStreamSelection(VIDEO, true).map_err(win)?;
            // The native type: the packets as stored, no decoder.
            reader
                .SetCurrentMediaType(VIDEO, None, &native)
                .map_err(win)?;
            let (w, h, _, _) = display_size(&native).ok_or(Error::NoStream)?;
            let first = read_sample(&reader, VIDEO)?.map_or(0, |s| s.time.max(0));
            seek(&reader, Tick::ZERO)?;
            Ok(PacketReader {
                info: VideoInfo {
                    width: w,
                    height: h,
                    frame_rate: stream_rate(path, &native),
                    codec: subtype_code(&native),
                    decodable: false,
                    pixel_aspect: pixel_aspect(&native),
                    field_order: field_order(&native),
                },
                reader: Reader(reader),
                time_offset: first,
            })
        }
    }

    pub fn info(&self) -> VideoInfo {
        self.info
    }

    /// The next packets start at the keyframe at or before `t`.
    pub fn seek(&mut self, t: Tick) -> Result<()> {
        init()?;
        seek(&self.reader.0, t + hns_to_tick(self.time_offset))
    }

    /// The next packet (times from the first one); `None` at the end.
    pub fn next_packet(&mut self) -> Result<Option<Packet>> {
        init()?;
        Ok(read_sample(&self.reader.0, VIDEO)?.map(|s| Packet {
            data: s.data,
            time: s.time - self.time_offset,
            duration: s.duration,
            decode_time: None,
            keyframe: s.keyframe,
        }))
    }
}

/// What the reader's samples hold.
enum Output {
    Float,
    /// Integers of these bits.
    Pcm(u16),
    /// Packets for a decoder of our own.
    Packets(Box<dyn PacketDecoder>),
}

pub struct AudioDecoder {
    reader: Reader,
    /// The reader's number of the stream being decoded.
    stream: u32,
    output: Output,
    rate: u32,
    channels: usize,
    codec: u32,
    /// Interleaved samples from `buf_start` on.
    buf: Vec<f32>,
    buf_start: i64,
    eos: bool,
    /// Time (in samples) of the first block: see `VideoDecoder::time_offset`.
    offset: i64,
    /// Reading from the stream's start, and blocks since then.
    from_start: bool,
    blocks: usize,
}

impl std::fmt::Debug for AudioDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDecoder")
            .field("rate", &self.rate)
            .field("channels", &self.channels)
            .finish()
    }
}

impl AudioDecoder {
    /// Decodes the file's first audio stream.
    pub fn open(path: &str) -> Result<AudioDecoder> {
        AudioDecoder::open_stream(path, 0)
    }

    /// Decodes the file's audio stream `index` (0 = the first one).
    pub fn open_stream(path: &str, index: usize) -> Result<AudioDecoder> {
        AudioDecoder::open_as(path, index, None)
    }

    /// Reads the packets of the file's audio stream `index` and decodes them
    /// with `decoder` (a format Windows has no decoder for).
    pub fn open_with(
        path: &str,
        index: usize,
        decoder: Box<dyn PacketDecoder>,
    ) -> Result<AudioDecoder> {
        AudioDecoder::open_as(path, index, Some(decoder))
    }

    fn open_as(
        path: &str,
        index: usize,
        decoder: Option<Box<dyn PacketDecoder>>,
    ) -> Result<AudioDecoder> {
        let reader = open_reader(path, false, false)?;
        let numbers = audio_stream_numbers(&reader, path);
        let stream = match numbers.get(index) {
            Some(&n) => n,
            None if index == 0 => AUDIO,
            None => return Err(Error::NoStream),
        };
        unsafe {
            let native = reader
                .GetNativeMediaType(stream, 0)
                .map_err(|_| Error::NoStream)?;
            reader.SetStreamSelection(ALL, false).map_err(win)?;
            reader.SetStreamSelection(stream, true).map_err(win)?;
            let mut output = match decoder {
                Some(d) => {
                    reader
                        .SetCurrentMediaType(stream, None, &native)
                        .map_err(win)?;
                    Output::Packets(d)
                }
                None => {
                    let sub = set_audio_output(&reader, stream).ok_or_else(|| {
                        Error::Windows(format!(
                            "no decoder for {}",
                            crate::codec::name(subtype_code(&native))
                        ))
                    })?;
                    if sub == MFAudioFormat_Float {
                        Output::Float
                    } else {
                        let current = reader.GetCurrentMediaType(stream).map_err(win)?;
                        let bits = current
                            .GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)
                            .map_err(win)?;
                        Output::Pcm(bits as u16)
                    }
                }
            };
            let current = reader.GetCurrentMediaType(stream).map_err(win)?;
            let mut rate = current
                .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                .map_err(win)?;
            let mut channels = current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map_err(win)? as usize;
            let first_sample = read_sample(&reader, stream)?;
            // An AVI's sound starts at zero: Windows dates its first block a
            // little later (having left those samples out).
            let first = if is_avi(path) {
                0
            } else {
                first_sample.as_ref().map_or(0, |s| s.time.max(0))
            };
            if let (Output::Packets(d), Some(s)) = (&mut output, &first_sample) {
                // The decoder says what the stream holds.
                let mut out = Vec::new();
                d.decode(&s.data, &mut out).map_err(Error::Windows)?;
                if d.sample_rate() > 0 && d.channels() > 0 {
                    rate = d.sample_rate();
                    channels = d.channels();
                }
                d.reset();
            }
            seek(&reader, Tick::ZERO)?;
            // The encoder's priming samples (AAC: about 1024) come first in
            // the stream but are not part of the sound: Media Foundation
            // plays them, so they are skipped here as the file's edit list
            // says (like ffmpeg and VEGAS), or the audio runs late.
            let delay = if is_mpeg4(path) {
                crate::mp4::audio_skip(path, index, rate)
            } else if let Some(skip) = crate::mp3::info_skip(path) {
                skip
            } else {
                reader
                    .GetPresentationAttribute(stream, &MF_SD_AUDIO_ENCODER_DELAY)
                    .ok()
                    .and_then(|pv| u32::try_from(&pv).ok())
                    .map_or(0, i64::from)
            };
            Ok(AudioDecoder {
                reader: Reader(reader),
                stream,
                output,
                rate,
                channels: channels.max(1),
                codec: subtype_code(&native),
                buf: Vec::new(),
                buf_start: 0,
                eos: false,
                from_start: false,
                blocks: 0,
                // Rounded like the blocks' times (see `pull`), or the sound
                // runs a sample late.
                offset: ((first as i128 * rate as i128 + HNS as i128 / 2) / HNS as i128) as i64
                    + delay,
            })
        }
    }

    pub fn info(&self) -> AudioInfo {
        AudioInfo {
            sample_rate: SampleRate(self.rate),
            channels: self.channels as u16,
            codec: self.codec,
            decodable: !matches!(self.output, Output::Packets(_)),
        }
    }

    fn buf_end(&self) -> i64 {
        self.buf_start + (self.buf.len() / self.channels) as i64
    }

    /// Reads `frames` samples from sample `start` on (planar). What lies
    /// before the start or after the end comes back as silence.
    pub fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer> {
        init()?;
        let ch = self.channels;
        let end = start + frames as i64;
        let rate = self.rate as i64;
        let outside = start < self.buf_start || start > self.buf_end() + rate;
        if self.buf.is_empty() || outside {
            // A little earlier, for the compressed frames to be whole; from
            // the start, the very start of the file (an MPEG program
            // stream's seek can land a little after the time asked for).
            let from = (start - rate / 2).max(0);
            let to = if from == 0 {
                Tick::ZERO
            } else {
                SampleRate(self.rate).sample_start(from + self.offset)
            };
            seek(&self.reader.0, to)?;
            if let Output::Packets(d) = &mut self.output {
                d.reset();
            }
            self.buf.clear();
            self.buf_start = from;
            self.eos = false;
            self.from_start = from == 0;
            self.blocks = 0;
            let mut first = true;
            while !self.eos && (first || self.buf_end() < end) {
                first = false;
                self.pull()?;
            }
        }
        while !self.eos && self.buf_end() < end {
            self.pull()?;
        }
        let mut out = vec![vec![0.0f32; frames]; ch];
        for i in 0..frames {
            let idx = start + i as i64 - self.buf_start;
            if idx < 0 {
                continue;
            }
            let base = idx as usize * ch;
            if base + ch > self.buf.len() {
                break;
            }
            for (c, o) in out.iter_mut().enumerate() {
                o[i] = self.buf[base + c];
            }
        }
        // Keeps the buffer small: only about 1 s before the point read.
        let keep_from = start - rate;
        if keep_from > self.buf_start {
            let drop = ((keep_from - self.buf_start) as usize * ch).min(self.buf.len());
            self.buf.drain(..drop);
            self.buf_start += (drop / ch) as i64;
        }
        Ok(AudioBuffer {
            sample_rate: SampleRate(self.rate),
            layout: ChannelLayout::from_count(ch),
            channels: out,
        })
    }

    /// Decodes the next sample and fits it in the buffer by its time.
    fn pull(&mut self) -> Result<()> {
        let Some(raw) = read_sample(&self.reader.0, self.stream)? else {
            self.eos = true;
            return Ok(());
        };
        if raw.changed && !matches!(self.output, Output::Packets(_)) {
            self.output_changed()?;
        }
        let samples: Vec<f32> = match &mut self.output {
            Output::Float => raw
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect(),
            Output::Pcm(bits) => pcm_to_float(&raw.data, *bits),
            Output::Packets(d) => {
                let mut out = Vec::new();
                // A damaged packet is silence, not the end of the media.
                if d.decode(&raw.data, &mut out).is_err() {
                    d.reset();
                }
                if d.channels() != self.channels {
                    // Channels changed mid-stream: kept as many as said.
                    out = remap(&out, d.channels(), self.channels);
                }
                out
            }
        };
        // The block's length in time tells its channels: Windows' AAC
        // decoder keeps the channel count an MPEG-TS stream claims (stereo)
        // and gives what the stream holds (mono).
        if self.blocks < 3 && !matches!(self.output, Output::Packets(_)) && raw.duration > 0 {
            let frames = ((raw.duration as i128 * self.rate as i128 + HNS as i128 / 2)
                / HNS as i128) as usize;
            if frames > 0 && samples.len().is_multiple_of(frames) {
                let k = samples.len() / frames;
                if (1..=8).contains(&k) && k != self.channels {
                    self.buf = remap(&self.buf, self.channels, k);
                    self.channels = k;
                }
            }
        }
        // Blocks dated before the stream's start (Ogg Opus's first one, by
        // its pre-skip, already cut): its sound starts at 0.
        let time = if self.from_start {
            raw.time.max(0)
        } else {
            raw.time
        };
        let at = ((time as i128 * self.rate as i128 + HNS as i128 / 2) / HNS as i128) as i64
            - self.offset;
        let ch = self.channels;
        self.blocks += 1;
        if self.buf.is_empty() {
            self.buf_start = at;
            self.buf = samples;
            return Ok(());
        }
        if self.from_start && self.blocks == 2 {
            // The second block may be dated as if the first had not been
            // cut (WebM Opus by its codec delay, MP3 by the decoder's): it
            // follows the first, and the later blocks are dated alike.
            let end = self.buf_end();
            if at > end && at - end < self.rate as i64 / 20 {
                self.offset += at - end;
                self.buf.extend_from_slice(&samples);
                return Ok(());
            }
        }
        if matches!(self.output, Output::Packets(_)) {
            // A packet's time is that of the first frame starting in it: the
            // decoder carries the frames split across packets, so the sound
            // runs on without gaps (a real gap is rare here).
            let end = self.buf_end();
            if at > end + self.rate as i64 / 2 {
                self.buf
                    .resize(self.buf.len() + (at - end) as usize * ch, 0.0);
            }
            self.buf.extend_from_slice(&samples);
            return Ok(());
        }
        let end = self.buf_end();
        // Sample times come rounded to 100 ns: a block a sample or two off
        // still follows the previous one. Taking that as a gap (silence) or
        // an overlap (samples dropped) clicked once per AAC frame.
        let slack = (self.rate as i64 / 1000).max(2);
        if at > end + slack {
            // A real gap in the stream: silence up to the block.
            self.buf
                .resize(self.buf.len() + (at - end) as usize * ch, 0.0);
            self.buf.extend_from_slice(&samples);
        } else if at < end - slack {
            // A real overlap: the repeated samples are dropped.
            let skip = ((end - at) as usize * ch).min(samples.len());
            self.buf.extend_from_slice(&samples[skip..]);
        } else {
            self.buf.extend_from_slice(&samples);
        }
        Ok(())
    }
}

impl AudioDecoder {
    /// The decoder found the real format (an AAC stream in MPEG-TS said
    /// stereo and was mono): channels and bits from its output type; what
    /// was decoded keeps its sound.
    fn output_changed(&mut self) -> Result<()> {
        let current = unsafe { self.reader.0.GetCurrentMediaType(self.stream) }.map_err(win)?;
        let channels = unsafe { current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }
            .map_err(win)?
            .max(1) as usize;
        if let Output::Pcm(bits) = &mut self.output
            && let Ok(b) = unsafe { current.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE) }
        {
            *bits = b as u16;
        }
        if channels != self.channels {
            self.buf = remap(&self.buf, self.channels, channels);
            self.channels = channels;
        }
        Ok(())
    }
}

/// Integer samples (8-bit unsigned, 16, 24 or 32-bit signed) as floats.
fn pcm_to_float(data: &[u8], bits: u16) -> Vec<f32> {
    match bits {
        8 => data.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        16 => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
            .collect(),
        24 => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0)
            .collect(),
        _ => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| i32::from_le_bytes(*b) as f32 / 2_147_483_648.0)
            .collect(),
    }
}
