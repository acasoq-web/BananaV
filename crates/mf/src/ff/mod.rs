//! FFmpeg (Linux): what Media Foundation does on Windows. libavformat
//! reads the files, libavcodec decodes them — on the processor, or on the
//! graphics card through VA-API or NVIDIA's decoder — and encodes (x264,
//! x265, SVT-AV1, NVENC, VA-API…), libswscale converts the pictures in
//! layouts BananaV has no copy of its own for.
//!
//! The API is that of the Windows modules (`imp`, `encode`, `encoders`)
//! and so are the rules: times count from each stream's first frame, a
//! seek starts earlier when the file is not sought to a keyframe, only the
//! frames that matter are copied out of the decoder.
//!
//! Needs FFmpeg 5.1 or newer (the channel layouts of `AVChannelLayout`).

use std::ffi::{CStr, CString, c_char, c_int};
use std::ptr;
use std::sync::Once;

use bv_core::{
    ColorInfo, ColorPrimaries, ColorRange, FrameRate, MatrixCoefficients, TICKS_PER_SECOND, Tick,
    TransferCharacteristic,
};
use ffmpeg_sys_next as ff;

use crate::{Error, Probe, Result, codec};

mod audio;
mod encode;
pub(crate) mod hw;
mod still;
mod video;

pub use audio::AudioDecoder;
pub use encode::{AudioEncoder, VideoEncoder, encoders};
pub use still::{read_still, still_size};
pub use video::{PacketReader, VideoDecoder};

/// 100 ns units: the times of [`crate::Packet`]s.
pub(crate) const HNS: ff::AVRational = ff::AVRational {
    num: 1,
    den: 10_000_000,
};

/// FFmpeg tells only what goes wrong: its warnings would fill the terminal.
/// `BV_FFMPEG_LOG` (`quiet`, `error`, `warning`, `info`, `debug`) changes it.
fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let level = match std::env::var("BV_FFMPEG_LOG").as_deref() {
            Ok("quiet") => ff::AV_LOG_QUIET,
            Ok("warning") => ff::AV_LOG_WARNING,
            Ok("info") => ff::AV_LOG_INFO,
            Ok("debug") => ff::AV_LOG_DEBUG,
            _ => ff::AV_LOG_ERROR,
        };
        // SAFETY: sets a global of FFmpeg's, before anything else uses it.
        unsafe { ff::av_log_set_level(level) };
    });
}

/// FFmpeg's message for an error code.
pub(crate) fn err(code: c_int) -> Error {
    let mut buf = [0 as c_char; 128];
    // SAFETY: the buffer is as long as said and FFmpeg ends it with a 0.
    let text = unsafe {
        ff::av_strerror(code, buf.as_mut_ptr(), buf.len());
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    };
    Error::Ffmpeg(text)
}

/// The code when it is not an error.
pub(crate) fn check(code: c_int) -> Result<c_int> {
    if code < 0 { Err(err(code)) } else { Ok(code) }
}

fn out_of_memory() -> Error {
    Error::Ffmpeg("out of memory".into())
}

/// The number an enum field of FFmpeg's holds: read as an integer, since a
/// newer FFmpeg may hold values the bindings do not name.
///
/// # Safety
/// `field` points to a 4-byte enum.
pub(crate) unsafe fn num<T>(field: *const T) -> i32 {
    debug_assert_eq!(std::mem::size_of::<T>(), 4);
    // SAFETY: as the caller says.
    unsafe { field.cast::<i32>().read() }
}

/// A time in `tb` units as ticks.
pub(crate) fn to_tick(v: i64, tb: ff::AVRational) -> Tick {
    let den = tb.den.max(1) as i128;
    let ticks = v as i128 * tb.num as i128 * TICKS_PER_SECOND as i128;
    Tick(ticks.div_euclid(den) as i64)
}

/// A time as `tb` units (rounded down).
pub(crate) fn from_tick(t: Tick, tb: ff::AVRational) -> i64 {
    let num = (tb.num.max(1) as i128) * TICKS_PER_SECOND as i128;
    (t.0 as i128 * tb.den as i128).div_euclid(num) as i64
}

/// `v` from time base `from` to `to`, rounded to the nearest.
pub(crate) fn rescale(v: i64, from: ff::AVRational, to: ff::AVRational) -> i64 {
    // SAFETY: plain arithmetic.
    unsafe { ff::av_rescale_q(v, from, to) }
}

// ---- Owned FFmpeg objects -------------------------------------------------------------

/// An open media file.
pub(crate) struct Input {
    ctx: *mut ff::AVFormatContext,
}

// SAFETY: a format context is used by one thread at a time (its owner's).
unsafe impl Send for Input {}

impl Input {
    pub(crate) fn open(path: &str) -> Result<Input> {
        init();
        let c_path = CString::new(path).map_err(|_| Error::UnsupportedFile)?;
        let mut ctx = ptr::null_mut();
        // SAFETY: `ctx` is filled by FFmpeg (left null on failure).
        let r = unsafe {
            ff::avformat_open_input(&mut ctx, c_path.as_ptr(), ptr::null(), ptr::null_mut())
        };
        if r < 0 {
            return Err(if r == ff::AVERROR_INVALIDDATA {
                Error::UnsupportedFile
            } else {
                err(r)
            });
        }
        let input = Input { ctx };
        // SAFETY: an open context.
        check(unsafe { ff::avformat_find_stream_info(input.ctx, ptr::null_mut()) })?;
        Ok(input)
    }

    /// The file's streams, in the file's order.
    pub(crate) fn streams(&self) -> &[*mut ff::AVStream] {
        // SAFETY: FFmpeg keeps `nb_streams` streams at `streams`.
        unsafe {
            let n = (*self.ctx).nb_streams as usize;
            if n == 0 || (*self.ctx).streams.is_null() {
                &[]
            } else {
                std::slice::from_raw_parts((*self.ctx).streams, n)
            }
        }
    }

    pub(crate) fn stream(&self, index: usize) -> *mut ff::AVStream {
        self.streams()[index]
    }

    /// The kind of each stream: video, audio…
    fn kind(&self, index: usize) -> i32 {
        // SAFETY: a stream of the file and its parameters.
        unsafe { num(&raw const (*(*self.stream(index)).codecpar).codec_type) }
    }

    /// The first video stream (cover pictures left out).
    pub(crate) fn video_stream(&self) -> Option<usize> {
        (0..self.streams().len()).find(|&i| {
            // SAFETY: a stream of the file.
            let cover = unsafe { (*self.stream(i)).disposition } & ff::AV_DISPOSITION_ATTACHED_PIC;
            self.kind(i) == ff::AVMediaType::AVMEDIA_TYPE_VIDEO as i32 && cover == 0
        })
    }

    /// The audio streams, in the file's order.
    pub(crate) fn audio_streams(&self) -> Vec<usize> {
        (0..self.streams().len())
            .filter(|&i| self.kind(i) == ff::AVMediaType::AVMEDIA_TYPE_AUDIO as i32)
            .collect()
    }

    /// Only `index` is read: the demuxer skips the other streams' packets.
    pub(crate) fn only(&mut self, index: usize) {
        for (i, &st) in self.streams().iter().enumerate() {
            // SAFETY: a stream of the file; the field is the demuxer's input.
            unsafe {
                (*st).discard = if i == index {
                    ff::AVDiscard::AVDISCARD_DEFAULT
                } else {
                    ff::AVDiscard::AVDISCARD_ALL
                };
            }
        }
    }

    /// The demuxer's names ("mov,mp4,m4a,3gp,3g2,mj2", "avi"…).
    pub(crate) fn format_name(&self) -> String {
        // SAFETY: an open context has its input format, named.
        unsafe {
            let f = (*self.ctx).iformat;
            if f.is_null() || (*f).name.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*f).name).to_string_lossy().into_owned()
            }
        }
    }

    /// Whether the file is an AVI (whose streams all start at zero).
    pub(crate) fn is_avi(&self) -> bool {
        self.format_name().split(',').any(|n| n == "avi")
    }

    /// The file's duration.
    pub(crate) fn duration(&self) -> Tick {
        // SAFETY: an open context and its streams.
        unsafe {
            let d = (*self.ctx).duration;
            if d > 0 && d != ff::AV_NOPTS_VALUE {
                return to_tick(d, ff::AV_TIME_BASE_Q);
            }
            self.streams()
                .iter()
                .filter(|&&st| (*st).duration > 0 && (*st).duration != ff::AV_NOPTS_VALUE)
                .map(|&st| to_tick((*st).duration, (*st).time_base))
                .max()
                .unwrap_or(Tick::ZERO)
        }
    }

    /// Reads the next packet of stream `index`; `false` at the end (or
    /// where a damaged file cannot be read on).
    pub(crate) fn read(&mut self, index: usize, pkt: &mut AvPacket) -> bool {
        loop {
            pkt.unref();
            // SAFETY: an open context and an allocated packet.
            let r = unsafe { ff::av_read_frame(self.ctx, pkt.0) };
            if r == ff::AVERROR(ff::EAGAIN) {
                continue;
            }
            if r < 0 {
                return false;
            }
            // SAFETY: a packet just read.
            if unsafe { (*pkt.0).stream_index } as usize == index {
                return true;
            }
        }
    }

    /// The next packets of stream `index` start at the keyframe at or
    /// before `ts` (in the stream's time base).
    pub(crate) fn seek(&mut self, index: usize, ts: i64) -> Result<()> {
        // SAFETY: an open context and one of its streams.
        check(unsafe { ff::av_seek_frame(self.ctx, index as c_int, ts, ff::AVSEEK_FLAG_BACKWARD) })
            .map(|_| ())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        // SAFETY: the context was opened by `avformat_open_input`.
        unsafe { ff::avformat_close_input(&mut self.ctx) };
    }
}

/// A packet (its data counted by FFmpeg).
pub(crate) struct AvPacket(pub(crate) *mut ff::AVPacket);

// SAFETY: used by one thread at a time (its owner's).
unsafe impl Send for AvPacket {}

impl AvPacket {
    pub(crate) fn new() -> Result<AvPacket> {
        // SAFETY: allocation.
        let p = unsafe { ff::av_packet_alloc() };
        if p.is_null() {
            Err(out_of_memory())
        } else {
            Ok(AvPacket(p))
        }
    }

    pub(crate) fn unref(&mut self) {
        // SAFETY: an allocated packet.
        unsafe { ff::av_packet_unref(self.0) };
    }

    pub(crate) fn data(&self) -> &[u8] {
        // SAFETY: an allocated packet holds `size` bytes at `data`.
        unsafe {
            let p = &*self.0;
            if p.data.is_null() || p.size <= 0 {
                &[]
            } else {
                std::slice::from_raw_parts(p.data, p.size as usize)
            }
        }
    }
}

impl Drop for AvPacket {
    fn drop(&mut self) {
        // SAFETY: allocated by `av_packet_alloc`.
        unsafe { ff::av_packet_free(&mut self.0) };
    }
}

/// A frame (its pictures or samples counted by FFmpeg).
pub(crate) struct AvFrame(pub(crate) *mut ff::AVFrame);

// SAFETY: used by one thread at a time (its owner's).
unsafe impl Send for AvFrame {}

impl AvFrame {
    pub(crate) fn new() -> Result<AvFrame> {
        // SAFETY: allocation.
        let f = unsafe { ff::av_frame_alloc() };
        if f.is_null() {
            Err(out_of_memory())
        } else {
            Ok(AvFrame(f))
        }
    }
}

impl Drop for AvFrame {
    fn drop(&mut self) {
        // SAFETY: allocated by `av_frame_alloc`.
        unsafe { ff::av_frame_free(&mut self.0) };
    }
}

/// A reference to a buffer of FFmpeg's (a graphics card's device or its
/// pictures).
pub(crate) struct BufferRef(pub(crate) *mut ff::AVBufferRef);

// SAFETY: FFmpeg counts the references atomically; the buffers it shares so
// (device and frames contexts) may be used by several threads' codecs.
unsafe impl Send for BufferRef {}
// SAFETY: as above.
unsafe impl Sync for BufferRef {}

impl BufferRef {
    /// Another reference to the same buffer.
    pub(crate) fn new_ref(&self) -> *mut ff::AVBufferRef {
        // SAFETY: a live reference.
        unsafe { ff::av_buffer_ref(self.0) }
    }
}

impl Drop for BufferRef {
    fn drop(&mut self) {
        // SAFETY: a reference of FFmpeg's (or null).
        unsafe { ff::av_buffer_unref(&mut self.0) };
    }
}

/// Converts pictures of layouts BananaV has no copy of its own for
/// (libswscale), keeping their matrix and range (RGB: full range).
pub(crate) struct Converter {
    ctx: *mut ff::SwsContext,
    /// Of the context: source layout, size, result layout.
    key: (c_int, c_int, c_int, c_int),
}

// SAFETY: used by one thread at a time (its owner's).
unsafe impl Send for Converter {}

impl Converter {
    pub(crate) fn new() -> Converter {
        Converter {
            ctx: ptr::null_mut(),
            key: (0, 0, 0, 0),
        }
    }

    /// `w`×`h` of picture `f` (from its top left) as a new picture of
    /// layout `to`; `color` is the picture's.
    ///
    /// # Safety
    /// `f` is a decoded picture of at least `w`×`h`, in the processor's
    /// memory.
    pub(crate) unsafe fn convert(
        &mut self,
        f: *const ff::AVFrame,
        (w, h): (usize, usize),
        to: ff::AVPixelFormat,
        color: &ColorInfo,
    ) -> Result<AvFrame> {
        // SAFETY: as the caller says; the result is allocated for `to`.
        unsafe {
            let from = (*f).format;
            let key = (from, w as c_int, h as c_int, to as c_int);
            if self.ctx.is_null() || self.key != key {
                ff::sws_freeContext(self.ctx);
                // SAFETY: a layout FFmpeg gave, so one it knows.
                let source = std::mem::transmute::<c_int, ff::AVPixelFormat>(from);
                self.ctx = ff::sws_getContext(
                    w as c_int,
                    h as c_int,
                    source,
                    w as c_int,
                    h as c_int,
                    to,
                    ff::SWS_BILINEAR | ff::SWS_ACCURATE_RND | ff::SWS_FULL_CHR_H_INT,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                );
                if self.ctx.is_null() {
                    return Err(Error::Ffmpeg("no conversion for this picture".into()));
                }
                self.key = key;
                let table = ff::sws_getCoefficients(match color.matrix {
                    MatrixCoefficients::Bt601 => ff::SWS_CS_ITU601,
                    MatrixCoefficients::Bt2020Ncl => ff::SWS_CS_BT2020,
                    _ => ff::SWS_CS_ITU709,
                });
                let full = c_int::from(color.range == ColorRange::Full);
                let rgb = !ff::av_pix_fmt_desc_get(to).is_null()
                    && (*ff::av_pix_fmt_desc_get(to)).flags & ff::AV_PIX_FMT_FLAG_RGB as u64 != 0;
                ff::sws_setColorspaceDetails(
                    self.ctx,
                    table,
                    full,
                    table,
                    if rgb { 1 } else { full },
                    0,
                    1 << 16,
                    1 << 16,
                );
            }
            let out = AvFrame::new()?;
            (*out.0).format = to as c_int;
            (*out.0).width = w as c_int;
            (*out.0).height = h as c_int;
            check(ff::av_frame_get_buffer(out.0, 0))?;
            ff::sws_scale(
                self.ctx,
                (*f).data.as_ptr().cast::<*const u8>(),
                (*f).linesize.as_ptr(),
                0,
                h as c_int,
                (*out.0).data.as_ptr(),
                (*out.0).linesize.as_ptr(),
            );
            Ok(out)
        }
    }
}

impl Drop for Converter {
    fn drop(&mut self) {
        // SAFETY: a context of libswscale's (or null).
        unsafe { ff::sws_freeContext(self.ctx) };
    }
}

/// A decoder's or encoder's context.
pub(crate) struct Codec(pub(crate) *mut ff::AVCodecContext);

// SAFETY: used by one thread at a time (its owner's).
unsafe impl Send for Codec {}

impl Drop for Codec {
    fn drop(&mut self) {
        // SAFETY: allocated by `avcodec_alloc_context3`.
        unsafe { ff::avcodec_free_context(&mut self.0) };
    }
}

/// An option of FFmpeg's (a dictionary of names and values).
pub(crate) struct Options(pub(crate) *mut ff::AVDictionary);

impl Options {
    pub(crate) fn new() -> Options {
        Options(ptr::null_mut())
    }

    pub(crate) fn set(&mut self, key: &str, value: &str) {
        let (Ok(k), Ok(v)) = (CString::new(key), CString::new(value)) else {
            return;
        };
        // SAFETY: FFmpeg copies both strings.
        unsafe { ff::av_dict_set(&mut self.0, k.as_ptr(), v.as_ptr(), 0) };
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: a dictionary of FFmpeg's (or null).
        unsafe { ff::av_dict_free(&mut self.0) };
    }
}

/// A decoder for stream `index` of `input`, `setup` before it opens.
pub(crate) fn open_decoder(
    input: &Input,
    index: usize,
    setup: impl FnOnce(*mut ff::AVCodecContext),
) -> Result<Codec> {
    // SAFETY: the stream and its parameters are the file's; the context is
    // set up before it opens, as FFmpeg asks.
    unsafe {
        let st = input.stream(index);
        let par = (*st).codecpar;
        let dec = ff::avcodec_find_decoder((*par).codec_id);
        if dec.is_null() {
            return Err(Error::Ffmpeg(format!("no decoder for {}", codec_name(par))));
        }
        let ctx = ff::avcodec_alloc_context3(dec);
        if ctx.is_null() {
            return Err(out_of_memory());
        }
        let codec = Codec(ctx);
        check(ff::avcodec_parameters_to_context(ctx, par))?;
        (*ctx).pkt_timebase = (*st).time_base;
        setup(ctx);
        check(ff::avcodec_open2(ctx, dec, ptr::null_mut()))?;
        Ok(codec)
    }
}

/// FFmpeg's name for the stream's format ("h264", "aac"…).
///
/// # Safety
/// `par` is a stream's parameters.
pub(crate) unsafe fn codec_name(par: *const ff::AVCodecParameters) -> String {
    // SAFETY: FFmpeg names every codec id (and says "unknown" otherwise).
    unsafe {
        let name = ff::avcodec_get_name((*par).codec_id);
        if name.is_null() {
            String::new()
        } else {
            CStr::from_ptr(name).to_string_lossy().into_owned()
        }
    }
}

/// Whether FFmpeg has a decoder for the stream.
///
/// # Safety
/// `par` is a stream's parameters.
pub(crate) unsafe fn decodable(par: *const ff::AVCodecParameters) -> bool {
    // SAFETY: as the caller says.
    unsafe { !ff::avcodec_find_decoder((*par).codec_id).is_null() }
}

/// The code BananaV knows a stream's format by ([`crate::VideoInfo::codec`]):
/// the one Windows gives it, so that both systems tell formats apart alike.
///
/// # Safety
/// `par` is a stream's parameters.
pub(crate) unsafe fn fourcc_of(par: *const ff::AVCodecParameters) -> u32 {
    use ff::AVCodecID::*;
    // SAFETY: as the caller says.
    let (id, tag, profile) = unsafe {
        (
            num(&raw const (*par).codec_id) as u32,
            (*par).codec_tag,
            (*par).profile,
        )
    };
    let is = |c: ff::AVCodecID| id == c as u32;
    let fourcc = codec::fourcc;
    if is(AV_CODEC_ID_H264) {
        codec::H264
    } else if is(AV_CODEC_ID_HEVC) {
        codec::HEVC
    } else if is(AV_CODEC_ID_PRORES) {
        if codec::is_prores(tag) {
            codec::normalized(tag)
        } else {
            // Matroska keeps no FourCC: the profile says the flavour.
            match profile {
                0 => codec::PRORES_PROXY,
                1 => codec::PRORES_LT,
                3 => codec::PRORES_HQ,
                4 => codec::PRORES_4444,
                5 => codec::PRORES_4444_XQ,
                _ => codec::PRORES_422,
            }
        }
    } else if is(AV_CODEC_ID_AC3) {
        codec::AC3
    } else if is(AV_CODEC_ID_EAC3) {
        codec::EAC3
    } else if is(AV_CODEC_ID_DTS) {
        codec::DTS
    } else if is(AV_CODEC_ID_AAC) {
        0x1610
    } else if is(AV_CODEC_ID_MP3) {
        0x55
    } else if is(AV_CODEC_ID_MP2) || is(AV_CODEC_ID_MP1) {
        0x50
    } else if is(AV_CODEC_ID_FLAC) {
        0xf1ac
    } else if is(AV_CODEC_ID_ALAC) {
        0x6c61
    } else if is(AV_CODEC_ID_OPUS) {
        0x704f
    } else if is(AV_CODEC_ID_VP8) {
        fourcc(b"VP80")
    } else if is(AV_CODEC_ID_VP9) {
        fourcc(b"VP90")
    } else if is(AV_CODEC_ID_AV1) {
        fourcc(b"AV01")
    } else if is(AV_CODEC_ID_MPEG1VIDEO) {
        fourcc(b"MPG1")
    } else if is(AV_CODEC_ID_MPEG2VIDEO) {
        0xe06d_8026
    } else if is(AV_CODEC_ID_MPEG4) {
        fourcc(b"MP4V")
    } else if is(AV_CODEC_ID_MJPEG) {
        fourcc(b"MJPG")
    } else if is(AV_CODEC_ID_DVVIDEO) {
        fourcc(b"dvsd")
    } else if tag != 0 {
        codec::normalized(tag)
    } else {
        // A FourCC from FFmpeg's name ("theora" → "THEO"), for messages.
        // SAFETY: as the caller says.
        let name = unsafe { codec_name(par) }.to_ascii_uppercase();
        let mut b = *b"    ";
        for (d, s) in b
            .iter_mut()
            .zip(name.bytes().filter(u8::is_ascii_alphanumeric))
        {
            *d = s;
        }
        fourcc(&b)
    }
}

/// The frame rate of a video stream: the average, or the stream's base
/// rate when the file does not tell it; the common rate when within
/// 0.2 % (29.97 for 30000/1001 written as 29.97…).
///
/// # Safety
/// `st` is a stream of an open file.
pub(crate) unsafe fn stream_rate(st: *const ff::AVStream) -> FrameRate {
    let valid =
        |r: ff::AVRational| r.num > 0 && r.den > 0 && (r.num as f64 / r.den as f64) < 1000.0;
    // SAFETY: as the caller says.
    let (avg, base) = unsafe { ((*st).avg_frame_rate, (*st).r_frame_rate) };
    let r = if valid(avg) {
        avg
    } else if valid(base) {
        base
    } else {
        return FrameRate::FPS_30;
    };
    crate::common::rate_near(r.num as f64 / r.den as f64).unwrap_or(FrameRate::FPS_30)
}

/// The shape of a video stream's pixels, width : height (1 : 1 when the
/// file does not say).
///
/// # Safety
/// `st` is a stream of an open file.
/// A stream's field order, as its parameters give it (FFmpeg's
/// AVFieldOrder: TT and BB, and MOV's interleaved fields' TB and BT, top
/// and bottom first).
pub(crate) unsafe fn field_order(st: *const ff::AVStream) -> bv_core::FieldOrder {
    // SAFETY: as the caller says.
    match unsafe { num(&raw const (*(*st).codecpar).field_order) } {
        2 | 4 => bv_core::FieldOrder::UpperFirst,
        3 | 5 => bv_core::FieldOrder::LowerFirst,
        _ => bv_core::FieldOrder::Progressive,
    }
}

pub(crate) unsafe fn pixel_aspect(st: *const ff::AVStream) -> (u32, u32) {
    let valid = |r: ff::AVRational| r.num > 0 && r.den > 0;
    // SAFETY: as the caller says.
    let (container, coded) = unsafe {
        (
            (*st).sample_aspect_ratio,
            (*(*st).codecpar).sample_aspect_ratio,
        )
    };
    [container, coded]
        .into_iter()
        .find(|&r| valid(r))
        .map_or((1, 1), |r| (r.num as u32, r.den as u32))
}

/// The colors a picture's codes say (ITU-T H.273, as FFmpeg numbers them),
/// over the usual guess for its size.
pub(crate) fn color_info(
    (width, height): (u32, u32),
    primaries: i32,
    transfer: i32,
    matrix: i32,
    range: i32,
) -> ColorInfo {
    let mut c = ColorInfo::guess_for(width, height);
    match matrix {
        0 => c.matrix = MatrixCoefficients::Identity,
        1 => c.matrix = MatrixCoefficients::Bt709,
        5 | 6 => c.matrix = MatrixCoefficients::Bt601,
        9 | 10 => {
            c.matrix = MatrixCoefficients::Bt2020Ncl;
            c.primaries = ColorPrimaries::Bt2020;
        }
        _ => {}
    }
    match primaries {
        1 => c.primaries = ColorPrimaries::Bt709,
        5 => c.primaries = ColorPrimaries::Bt601_625,
        6 | 7 => c.primaries = ColorPrimaries::Bt601_525,
        9 => c.primaries = ColorPrimaries::Bt2020,
        12 => c.primaries = ColorPrimaries::DisplayP3,
        _ => {}
    }
    match transfer {
        8 => c.transfer = TransferCharacteristic::Linear,
        13 => c.transfer = TransferCharacteristic::Srgb,
        16 => c.transfer = TransferCharacteristic::Pq,
        18 => c.transfer = TransferCharacteristic::Hlg,
        _ => c.transfer = TransferCharacteristic::Bt709,
    }
    match range {
        1 => c.range = ColorRange::Limited,
        2 => c.range = ColorRange::Full,
        // RGB pictures that do not say: full range.
        _ if c.matrix == MatrixCoefficients::Identity => c.range = ColorRange::Full,
        _ => {}
    }
    c
}

pub fn probe(path: &str) -> Result<Probe> {
    let input = Input::open(path)?;
    let mut probe = Probe {
        duration: input.duration(),
        video: None,
        audio: None,
        audio_streams: Vec::new(),
    };
    if let Some(index) = input.video_stream() {
        // The decoder's size and rate (its first picture's), as on Windows.
        probe.video = Some(match VideoDecoder::open(path) {
            Ok(d) => d.info(),
            Err(_) => video::stream_info(&input, index),
        });
    }
    for index in input.audio_streams() {
        probe.audio_streams.push(audio::stream_info(&input, index));
    }
    probe.audio = probe.audio_streams.first().copied();
    if probe.video.is_none() && probe.audio.is_none() {
        return Err(Error::NoStream);
    }
    Ok(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_go_between_time_bases() {
        let tb = ff::AVRational {
            num: 1001,
            den: 30000,
        };
        // Frame 30 of 29.97: 1.001 s.
        assert_eq!(to_tick(30, tb), Tick::from_seconds_f64(1.001));
        assert_eq!(from_tick(Tick::from_seconds_f64(1.001), tb), 30);
        assert_eq!(
            rescale(
                15_000,
                ff::AVRational {
                    num: 1,
                    den: 90_000
                },
                HNS
            ),
            1_666_667
        );
    }

    #[test]
    fn colors_from_their_codes() {
        let hd = color_info((1920, 1080), 9, 18, 9, 1);
        assert_eq!(hd.primaries, ColorPrimaries::Bt2020);
        assert_eq!(hd.transfer, TransferCharacteristic::Hlg);
        assert_eq!(hd.matrix, MatrixCoefficients::Bt2020Ncl);
        // Unspecified: the guess for the size.
        let sd = color_info((720, 576), 2, 2, 2, 0);
        assert_eq!(sd, ColorInfo::guess_for(720, 576));
        assert_eq!(color_info((640, 480), 2, 2, 2, 2).range, ColorRange::Full);
    }
}
