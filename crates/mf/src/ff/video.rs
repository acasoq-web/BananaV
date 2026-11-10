//! Video through FFmpeg: frames decoded on the processor or a graphics card
//! (copied to memory only when they matter), and the compressed frames of
//! the formats BananaV decodes itself (ProRes).

use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::ptr;
use std::sync::{Arc, Mutex};

use bv_core::{ColorInfo, ColorRange, FrameRate, PixelFormat, Plane, PlaneData, Tick, VideoFrame};
use ffmpeg_sys_next as ff;

use super::{
    AvFrame, AvPacket, Codec, Converter, HNS, Input, check, color_info, err, field_order,
    fourcc_of, from_tick, hw, num, open_decoder, pixel_aspect, rescale, stream_rate, to_tick,
};
use crate::{Error, Result, VideoInfo};

/// What a stream's parameters tell before decoding (for streams that do not
/// decode: their pictures' size, rate and format).
pub(super) fn stream_info(input: &Input, index: usize) -> VideoInfo {
    let st = input.stream(index);
    // SAFETY: a stream of the open file and its parameters.
    unsafe {
        let par = (*st).codecpar;
        VideoInfo {
            width: (*par).width.max(0) as u32,
            height: (*par).height.max(0) as u32,
            frame_rate: stream_rate(st),
            codec: fourcc_of(par),
            decodable: false,
            pixel_aspect: pixel_aspect(st),
            field_order: field_order(st),
        }
    }
}

/// Formats whose decoding on a graphics card failed once: decoded on the
/// processor from then on.
static CARD_FAILED: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Picks the card's pictures among the layouts the decoder offers (the
/// processor's when the card cannot take the stream).
unsafe extern "C" fn card_format(
    ctx: *mut ff::AVCodecContext,
    formats: *const ff::AVPixelFormat,
) -> ff::AVPixelFormat {
    // SAFETY: FFmpeg gives its context and a list ended by NONE; `opaque`
    // holds the card's layout (see `open_video_decoder`).
    unsafe {
        let want = (*ctx).opaque as isize as c_int;
        let mut p = formats;
        while num(p) != ff::AVPixelFormat::AV_PIX_FMT_NONE as i32 {
            if num(p) == want {
                return *p;
            }
            p = p.add(1);
        }
        ff::avcodec_default_get_format(ctx, formats)
    }
}

/// A decoder for the video stream `index`: on the card of `device`, or on
/// the processor with as many threads as it has.
fn open_video_decoder(input: &Input, index: usize, device: Option<&hw::Device>) -> Result<Codec> {
    open_decoder(input, index, |ctx| {
        // SAFETY: the context is being set up, before it opens.
        unsafe {
            match device {
                Some(d) => {
                    (*ctx).hw_device_ctx = d.buffer.new_ref();
                    (*ctx).opaque = d.pix_fmt as c_int as isize as *mut c_void;
                    (*ctx).get_format = Some(card_format);
                    // The card decodes; one thread feeds it.
                    (*ctx).thread_count = 1;
                    // The picture held on the way (see `decode_to`).
                    (*ctx).extra_hw_frames = 2;
                }
                None => {
                    (*ctx).thread_count = 0;
                    (*ctx).thread_type = ff::FF_THREAD_FRAME | ff::FF_THREAD_SLICE;
                }
            }
        }
    })
}

/// How a picture's layout is copied into BananaV's planes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copy {
    /// Planes of 8 bits.
    Planar8(PixelFormat),
    /// Planes of 9 to 16 bits (little-endian), shifted to 10.
    Planar16(PixelFormat, i32),
    /// Y, then U and V interleaved, 8 bits (NV12).
    Semi8,
    /// Y, then U and V interleaved, in the high bits of 16 (P010, P016).
    Semi16,
    Gray8,
    /// Converted to this layout first.
    Convert(ff::AVPixelFormat),
}

fn layout(format: c_int) -> Copy {
    use PixelFormat::*;
    use ff::AVPixelFormat::*;
    let is = |p: ff::AVPixelFormat| format == p as c_int;
    let any = |ps: &[ff::AVPixelFormat]| ps.iter().any(|&p| is(p));
    if any(&[AV_PIX_FMT_YUV420P, AV_PIX_FMT_YUVJ420P]) {
        Copy::Planar8(Yuv420P8)
    } else if any(&[AV_PIX_FMT_YUV422P, AV_PIX_FMT_YUVJ422P]) {
        Copy::Planar8(Yuv422P8)
    } else if any(&[AV_PIX_FMT_YUV444P, AV_PIX_FMT_YUVJ444P, AV_PIX_FMT_GBRP]) {
        // GBR's planes come as G, B, R: what the identity matrix reads.
        Copy::Planar8(Yuv444P8)
    } else if is(AV_PIX_FMT_NV12) {
        Copy::Semi8
    } else if any(&[AV_PIX_FMT_P010LE, AV_PIX_FMT_P016LE]) {
        Copy::Semi16
    } else if is(AV_PIX_FMT_GRAY8) {
        Copy::Gray8
    } else {
        let deep = [
            (AV_PIX_FMT_YUV420P9LE, Yuv420P10, 9),
            (AV_PIX_FMT_YUV420P10LE, Yuv420P10, 10),
            (AV_PIX_FMT_YUV420P12LE, Yuv420P10, 12),
            (AV_PIX_FMT_YUV420P14LE, Yuv420P10, 14),
            (AV_PIX_FMT_YUV420P16LE, Yuv420P10, 16),
            (AV_PIX_FMT_YUV422P9LE, Yuv422P10, 9),
            (AV_PIX_FMT_YUV422P10LE, Yuv422P10, 10),
            (AV_PIX_FMT_YUV422P12LE, Yuv422P10, 12),
            (AV_PIX_FMT_YUV422P14LE, Yuv422P10, 14),
            (AV_PIX_FMT_YUV422P16LE, Yuv422P10, 16),
            (AV_PIX_FMT_YUV444P9LE, Yuv444P10, 9),
            (AV_PIX_FMT_YUV444P10LE, Yuv444P10, 10),
            (AV_PIX_FMT_YUV444P12LE, Yuv444P10, 12),
            (AV_PIX_FMT_YUV444P14LE, Yuv444P10, 14),
            (AV_PIX_FMT_YUV444P16LE, Yuv444P10, 16),
            (AV_PIX_FMT_GBRP9LE, Yuv444P10, 9),
            (AV_PIX_FMT_GBRP10LE, Yuv444P10, 10),
            (AV_PIX_FMT_GBRP12LE, Yuv444P10, 12),
            (AV_PIX_FMT_GBRP14LE, Yuv444P10, 14),
            (AV_PIX_FMT_GBRP16LE, Yuv444P10, 16),
        ];
        if let Some(&(_, f, depth)) = deep.iter().find(|(p, _, _)| is(*p)) {
            return Copy::Planar16(f, depth);
        }
        // SAFETY: FFmpeg describes every layout it gives (null otherwise).
        let desc = unsafe {
            // SAFETY: a layout FFmpeg gave, so one it knows.
            ff::av_pix_fmt_desc_get(std::mem::transmute::<c_int, ff::AVPixelFormat>(format))
        };
        if desc.is_null() {
            return Copy::Convert(AV_PIX_FMT_RGBA);
        }
        // SAFETY: a descriptor of FFmpeg's.
        let (flags, depth) = unsafe { ((*desc).flags, (*desc).comp[0].depth) };
        let colors =
            (ff::AV_PIX_FMT_FLAG_ALPHA | ff::AV_PIX_FMT_FLAG_RGB | ff::AV_PIX_FMT_FLAG_PAL) as u64;
        if flags & colors != 0 {
            Copy::Convert(AV_PIX_FMT_RGBA)
        } else if depth > 8 {
            Copy::Convert(AV_PIX_FMT_YUV444P10LE)
        } else {
            Copy::Convert(AV_PIX_FMT_YUV444P)
        }
    }
}

/// `w`×`h` bytes of plane `i`, from its top left.
///
/// # Safety
/// The plane holds them.
unsafe fn bytes(f: *const ff::AVFrame, i: usize, (w, h): (usize, usize)) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h);
    // SAFETY: as the caller says; rows are `linesize` apart (maybe upwards).
    unsafe {
        let (base, step) = ((*f).data[i], (*f).linesize[i] as isize);
        for y in 0..h {
            out.extend_from_slice(std::slice::from_raw_parts(
                base.offset(y as isize * step),
                w,
            ));
        }
    }
    out
}

/// `w`×`h` little-endian 16-bit samples of plane `i`, shifted right by
/// `shift` (left when negative).
///
/// # Safety
/// The plane holds them.
unsafe fn words(f: *const ff::AVFrame, i: usize, (w, h): (usize, usize), shift: i32) -> Vec<u16> {
    let mut out = Vec::with_capacity(w * h);
    // SAFETY: as the caller says.
    unsafe {
        let (base, step) = ((*f).data[i], (*f).linesize[i] as isize);
        for y in 0..h {
            let row = std::slice::from_raw_parts(base.offset(y as isize * step), w * 2);
            out.extend(row.as_chunks::<2>().0.iter().map(|b| {
                let v = u16::from_le_bytes(*b);
                if shift >= 0 { v >> shift } else { v << -shift }
            }));
        }
    }
    out
}

fn plane(data: PlaneData, width: usize, height: usize) -> Plane {
    Plane {
        width,
        height,
        stride: width,
        data,
    }
}

pub struct VideoDecoder {
    input: Input,
    stream: usize,
    tb: ff::AVRational,
    codec: Codec,
    packet: AvPacket,
    /// The decoder was told the stream ended: it gives what it holds.
    drained: bool,
    /// The card it decodes on (it may still decode a stream the card cannot
    /// take on the processor).
    device: Option<Arc<hw::Device>>,
    /// Pictures come from the graphics card.
    on_card: bool,
    /// To open the file again (see `frame_at_with`).
    path: String,
    width: u32,
    height: u32,
    rate: FrameRate,
    color: ColorInfo,
    codec_code: u32,
    pixel_aspect: (u32, u32),
    field_order: bv_core::FieldOrder,
    /// The last frames decoded (shared: a picture is not copied each time it
    /// is asked for).
    recent: VecDeque<Arc<VideoFrame>>,
    /// The time of the last frame decoded (to go on without seeking).
    last: Option<Tick>,
    /// The first frame decoded since the last seek.
    first: Option<Tick>,
    /// The keyframes' times, when the file says (MP4, MOV): going on to a
    /// later frame with no keyframe on the way beats a seek, however far.
    keyframes: Option<std::sync::Arc<Vec<Tick>>>,
    eos: bool,
    /// Time (the stream's time base) of the first frame: everything is
    /// shifted for it to be zero, as on Windows (MPEG-TS times start
    /// anywhere).
    time_offset: i64,
    /// The time of the picture before (for pictures without one).
    previous: Option<i64>,
    convert: Converter,
}

impl std::fmt::Debug for VideoDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoDecoder")
            .field("size", &(self.width, self.height))
            .field("on_card", &self.on_card)
            .finish()
    }
}

impl VideoDecoder {
    pub fn open(path: &str) -> Result<VideoDecoder> {
        let setting = if crate::PROCESSOR_ONLY.get() {
            crate::HardwareDecoding::Off
        } else {
            crate::hardware_decoding()
        };
        // On the graphics card first, when hardware decoding is on; on the
        // processor when the card fails, or does not take the stream (FFmpeg
        // then decodes on the processor, but with one thread: opened again
        // with all of them).
        if let Some(device) = hw::device(&setting) {
            match VideoDecoder::start(path, Some(device)) {
                // Without a device: the stream went to the processor.
                Ok(d) if d.on_card || d.device.is_none() => return Ok(d),
                Err(Error::NoStream) => return Err(Error::NoStream),
                _ => {}
            }
        }
        VideoDecoder::start(path, None)
    }

    fn start(path: &str, device: Option<Arc<hw::Device>>) -> Result<VideoDecoder> {
        let mut input = Input::open(path)?;
        let stream = input.video_stream().ok_or(Error::NoStream)?;
        input.only(stream);
        let st = input.stream(stream);
        // SAFETY: a stream of the open file and its parameters.
        let (par, tb) = unsafe { ((*st).codecpar, (*st).time_base) };
        // SAFETY: as above.
        let id = unsafe { num(&raw const (*par).codec_id) } as u32;
        if id == ff::AVCodecID::AV_CODEC_ID_PRORES as u32 {
            // BananaV decodes ProRes itself (`PacketReader`), as on Windows:
            // the same pictures on both systems, alpha included.
            return Err(Error::Ffmpeg("ProRes is decoded by BananaV".into()));
        }
        // MPEG-1 and 2 are cheap on the processor.
        let cheap = [
            ff::AVCodecID::AV_CODEC_ID_MPEG1VIDEO as u32,
            ff::AVCodecID::AV_CODEC_ID_MPEG2VIDEO as u32,
        ];
        let failed = CARD_FAILED.lock().is_ok_and(|f| f.contains(&id));
        let device = device.filter(|_| !cheap.contains(&id) && !failed);
        let on_card_asked = device.is_some();
        let codec = open_video_decoder(&input, stream, device.as_deref())?;
        let mut d = VideoDecoder {
            input,
            stream,
            tb,
            codec,
            packet: AvPacket::new()?,
            drained: false,
            device,
            on_card: false,
            path: path.to_string(),
            width: 0,
            height: 0,
            // SAFETY: as above.
            rate: unsafe { stream_rate(st) },
            color: ColorInfo::default(),
            // SAFETY: as above.
            codec_code: unsafe { fourcc_of(par) },
            // SAFETY: as above.
            pixel_aspect: unsafe { pixel_aspect(st) },
            // SAFETY: as above.
            field_order: unsafe { field_order(st) },
            recent: VecDeque::new(),
            last: None,
            first: None,
            keyframes: crate::mp4::video_keyframes_cached(path),
            eos: false,
            time_offset: 0,
            previous: None,
            convert: Converter::new(),
        };
        // The first frame must come, and in a layout understood.
        let first = match d.next_frame() {
            Ok(Some(f)) => f,
            Ok(None) => return Err(Error::Ffmpeg("no frames".into())),
            Err(e) => {
                if on_card_asked && let Ok(mut f) = CARD_FAILED.lock() {
                    f.push(id);
                }
                return Err(e);
            }
        };
        // SAFETY: a decoded picture.
        unsafe {
            let f = first.0;
            d.on_card = !(*f).hw_frames_ctx.is_null();
            d.width = ((*f).width.max(0) as u32) & !1;
            d.height = ((*f).height.max(0) as u32) & !1;
            if d.width == 0 || d.height == 0 {
                return Err(Error::Ffmpeg("a picture of no size".into()));
            }
            d.color = color_info(
                (d.width, d.height),
                num(&raw const (*f).color_primaries),
                num(&raw const (*f).color_trc),
                num(&raw const (*f).colorspace),
                num(&raw const (*f).color_range),
            );
            d.time_offset = frame_time(f).unwrap_or(0).max(0);
        }
        let pts = d.pts_of(&first);
        let frame = d
            .keep(pts, first)?
            .ok_or_else(|| Error::Ffmpeg("unexpected frame layout".into()))?;
        d.last = Some(frame.pts);
        d.first = Some(frame.pts);
        Ok(d)
    }

    /// The file again, from its start (see `frame_at_with`).
    fn reopen(&mut self) -> Result<()> {
        let mut input = Input::open(&self.path)?;
        input.only(self.stream);
        self.codec = open_video_decoder(&input, self.stream, self.device.as_deref())?;
        self.input = input;
        self.forget();
        Ok(())
    }

    /// After a seek: nothing decoded counts.
    fn forget(&mut self) {
        self.drained = false;
        self.recent.clear();
        self.eos = false;
        self.last = None;
        self.first = None;
        self.previous = None;
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
            codec: self.codec_code,
            decodable: true,
            pixel_aspect: self.pixel_aspect,
            field_order: self.field_order,
        }
    }

    /// The next picture the decoder gives (maybe on the card); `None` at
    /// the end.
    fn next_frame(&mut self) -> Result<Option<AvFrame>> {
        let frame = AvFrame::new()?;
        loop {
            // SAFETY: an open decoder and an allocated frame.
            let r = unsafe { ff::avcodec_receive_frame(self.codec.0, frame.0) };
            if r >= 0 {
                return Ok(Some(frame));
            }
            if r == ff::AVERROR_EOF {
                return Ok(None);
            }
            // A damaged picture is skipped: the ones after it still decode.
            if r != ff::AVERROR(ff::EAGAIN) && r != ff::AVERROR_INVALIDDATA {
                return Err(err(r));
            }
            if self.drained {
                return Ok(None);
            }
            if self.input.read(self.stream, &mut self.packet) {
                // SAFETY: an open decoder and a packet of its stream.
                let r = unsafe { ff::avcodec_send_packet(self.codec.0, self.packet.0) };
                self.packet.unref();
                if r < 0 && r != ff::AVERROR_INVALIDDATA && r != ff::AVERROR(ff::EAGAIN) {
                    return Err(err(r));
                }
            } else {
                // The end of the file: the decoder gives what it still holds.
                // SAFETY: an open decoder; null asks it to drain.
                unsafe { ff::avcodec_send_packet(self.codec.0, ptr::null()) };
                self.drained = true;
            }
        }
    }

    /// The media time of a picture.
    fn pts_of(&mut self, f: &AvFrame) -> Tick {
        // SAFETY: a decoded picture.
        let t = unsafe { frame_time(f.0) }.unwrap_or_else(|| {
            // No time: the frame after the one before.
            let step = from_tick(self.rate.frame_duration(), self.tb).max(1);
            self.previous.map_or(self.time_offset, |p| p + step)
        });
        self.previous = Some(t);
        self.snap(to_tick(t - self.time_offset, self.tb))
    }

    /// Times rounded by the file (Matroska's are in milliseconds): what is
    /// within 0.5 ms of the frame grid goes on it.
    fn snap(&self, t: Tick) -> Tick {
        let s = self.rate.snap_nearest(t);
        if (t - s).abs() < Tick::from_seconds_f64(0.0005) {
            s
        } else {
            t
        }
    }

    /// A picture copied out of the decoder (and from the card) into a frame
    /// kept among the recent ones (`None`: a layout not understood).
    fn keep(&mut self, pts: Tick, f: AvFrame) -> Result<Option<Arc<VideoFrame>>> {
        // SAFETY: a decoded picture.
        let picture = if unsafe { !(*f.0).hw_frames_ctx.is_null() } {
            download(&f)?
        } else {
            f
        };
        let Some(frame) = self.copy_out(&picture, pts)? else {
            return Ok(None);
        };
        let f = Arc::new(frame);
        self.recent.push_back(f.clone());
        if self.recent.len() > 8 {
            self.recent.pop_front();
        }
        Ok(Some(f))
    }

    /// A picture not wanted: not copied out at all. The recent frames stop
    /// being one after another, so they are let go.
    fn discard(&mut self, f: AvFrame) {
        drop(f);
        self.recent.clear();
    }

    /// A picture in the processor's memory as BananaV's planes.
    fn copy_out(&mut self, picture: &AvFrame, pts: Tick) -> Result<Option<VideoFrame>> {
        let f = picture.0;
        let (w, h) = (self.width as usize, self.height as usize);
        // SAFETY: a decoded picture in memory; the copies stay within its
        // size, checked first.
        unsafe {
            if ((*f).width as usize) < w || ((*f).height as usize) < h {
                // The stream changed size: not taken.
                return Ok(None);
            }
            let mut color = self.color;
            let copy = layout((*f).format);
            let converted;
            let (src, copy) = match copy {
                Copy::Convert(to) => {
                    converted = self.convert.convert(f, (w, h), to, &self.color)?;
                    let again = layout(to as c_int);
                    (converted.0 as *const ff::AVFrame, again)
                }
                c => (f as *const ff::AVFrame, c),
            };
            let (format, planes) = match copy {
                Copy::Planar8(format) => {
                    let (sx, sy) = format.chroma_shift();
                    let c = (w >> sx, h >> sy);
                    let planes = vec![
                        plane(PlaneData::U8(bytes(src, 0, (w, h))), w, h),
                        plane(PlaneData::U8(bytes(src, 1, c)), c.0, c.1),
                        plane(PlaneData::U8(bytes(src, 2, c)), c.0, c.1),
                    ];
                    (format, planes)
                }
                Copy::Planar16(format, depth) => {
                    let (sx, sy) = format.chroma_shift();
                    let c = (w >> sx, h >> sy);
                    let s = depth - 10;
                    let planes = vec![
                        plane(PlaneData::U16(words(src, 0, (w, h), s)), w, h),
                        plane(PlaneData::U16(words(src, 1, c, s)), c.0, c.1),
                        plane(PlaneData::U16(words(src, 2, c, s)), c.0, c.1),
                    ];
                    (format, planes)
                }
                Copy::Semi8 => {
                    let (cw, ch) = (w / 2, h / 2);
                    let uv = bytes(src, 1, (cw * 2, ch));
                    let (u, v): (Vec<u8>, Vec<u8>) =
                        uv.as_chunks::<2>().0.iter().map(|&[a, b]| (a, b)).unzip();
                    let planes = vec![
                        plane(PlaneData::U8(bytes(src, 0, (w, h))), w, h),
                        plane(PlaneData::U8(u), cw, ch),
                        plane(PlaneData::U8(v), cw, ch),
                    ];
                    (PixelFormat::Yuv420P8, planes)
                }
                Copy::Semi16 => {
                    let (cw, ch) = (w / 2, h / 2);
                    let uv = words(src, 1, (cw * 2, ch), 6);
                    let (u, v): (Vec<u16>, Vec<u16>) =
                        uv.as_chunks::<2>().0.iter().map(|&[a, b]| (a, b)).unzip();
                    let planes = vec![
                        plane(PlaneData::U16(words(src, 0, (w, h), 6)), w, h),
                        plane(PlaneData::U16(u), cw, ch),
                        plane(PlaneData::U16(v), cw, ch),
                    ];
                    (PixelFormat::Yuv420P10, planes)
                }
                Copy::Gray8 => (
                    PixelFormat::Gray8,
                    vec![plane(PlaneData::U8(bytes(src, 0, (w, h))), w, h)],
                ),
                Copy::Convert(_) => {
                    // RGBA: one plane of four bytes a pixel, full range.
                    color.range = ColorRange::Full;
                    (
                        PixelFormat::Rgba8,
                        vec![plane(PlaneData::U8(bytes(src, 0, (w * 4, h))), w * 4, h)],
                    )
                }
            };
            Ok(Some(VideoFrame {
                width: self.width,
                height: self.height,
                format,
                color,
                planes,
                pts,
                duration: self.rate.frame_duration(),
            }))
        }
    }

    /// The frame visible at `t` among those decoded: the last one at or
    /// before `t`, as long as the next one (if any) starts after `t`.
    fn cached(&self, t: Tick) -> Option<&Arc<VideoFrame>> {
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
    pub fn frame_at(&mut self, t: Tick) -> Result<Arc<VideoFrame>> {
        self.frame_at_with(t, &mut crate::Straight)
    }

    /// [`frame_at`](Self::frame_at) that tells `way` about the frames on
    /// the way to `t` (after a seek, from the keyframe on): it may stop the
    /// decoding ([`Error::Interrupted`]) or ask for some of them.
    pub fn frame_at_with(&mut self, t: Tick, way: &mut dyn crate::Way) -> Result<Arc<VideoFrame>> {
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
            if back.is_finite() {
                let from = (t - Tick::from_seconds_f64(back)).max(Tick::ZERO);
                if from == Tick::ZERO {
                    // The first frame's time once is enough.
                    if tried_first {
                        continue;
                    }
                    tried_first = true;
                }
                // Some files refuse some positions: an earlier one may do.
                let ts = from_tick(from, self.tb) + self.time_offset;
                if self.input.seek(self.stream, ts).is_err() {
                    continue;
                }
                // SAFETY: an open decoder.
                unsafe { ff::avcodec_flush_buffers(self.codec.0) };
                self.forget();
            } else {
                // At last, the file from its very start, read again.
                self.reopen()?;
            }
            let found = self.decode_to(t, way)?;
            let in_time = self.first.is_some_and(|f| f <= t + late);
            match found {
                Some(f) if in_time || !back.is_finite() => return Ok(f),
                _ => {}
            }
        }
        Err(Error::Ffmpeg("no frames".into()))
    }

    /// Decodes on until the frame visible at `t` (the last one at the end).
    /// Only the frames that matter are copied out of the decoder: the one
    /// at `t`, the few before it (asked for next, often) and those `way`
    /// wants to show.
    fn decode_to(&mut self, t: Tick, way: &mut dyn crate::Way) -> Result<Option<Arc<VideoFrame>>> {
        let near = self.rate.frame_duration().0 * 8;
        // The picture before, not copied yet: it is the one visible at `t`
        // when the next one starts after `t`.
        let mut held: Option<(Tick, AvFrame)> = None;
        loop {
            if !way.go_on() {
                if let Some((_, h)) = held.take() {
                    self.discard(h);
                }
                return Err(Error::Interrupted);
            }
            let Some(f) = self.next_frame()? else {
                self.eos = true;
                if let Some((p, h)) = held.take() {
                    self.keep(p, h)?;
                }
                // After the end, the last frame stays.
                return Ok(self.recent.back().cloned());
            };
            let pts = self.pts_of(&f);
            self.first.get_or_insert(pts);
            self.last = Some(pts);
            if let Some((held_pts, h)) = held.take() {
                if pts > t || held_pts.0 + near >= t.0 {
                    self.keep(held_pts, h)?;
                } else if way.want() {
                    if let Some(shown) = self.keep(held_pts, h)? {
                        way.show(&shown);
                    }
                } else {
                    self.discard(h);
                }
            }
            if pts > t {
                // The frame after: its start ends the visible one.
                self.keep(pts, f)?;
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
            held = Some((pts, f));
        }
    }
}

// SAFETY: FFmpeg's contexts are used by one thread at a time (the decoder's
// owner's); the card's device is shared safely (see `hw::Device`).
unsafe impl Send for VideoDecoder {}

/// A picture's time (the stream's time base), when it has one.
///
/// # Safety
/// `f` is a decoded picture.
unsafe fn frame_time(f: *const ff::AVFrame) -> Option<i64> {
    // SAFETY: as the caller says.
    let (best, pts) = unsafe { ((*f).best_effort_timestamp, (*f).pts) };
    [best, pts].into_iter().find(|&t| t != ff::AV_NOPTS_VALUE)
}

/// A picture decoded on the card, copied to memory (cropped as decoded).
fn download(f: &AvFrame) -> Result<AvFrame> {
    let sw = AvFrame::new()?;
    // SAFETY: a picture on the card; the copy is allocated by FFmpeg.
    unsafe {
        check(ff::av_hwframe_transfer_data(sw.0, f.0, 0))?;
        check(ff::av_frame_copy_props(sw.0, f.0))?;
        if (*sw.0).crop_left > 0 || (*sw.0).crop_top > 0 {
            check(ff::av_frame_apply_cropping(
                sw.0,
                ff::AV_FRAME_CROP_UNALIGNED as c_int,
            ))?;
        }
    }
    Ok(sw)
}

/// The compressed frames of a file's video, for formats BananaV decodes
/// itself (ProRes).
pub struct PacketReader {
    input: Input,
    stream: usize,
    tb: ff::AVRational,
    packet: AvPacket,
    info: VideoInfo,
    /// Time (the stream's time base) of the first packet: see
    /// `VideoDecoder::time_offset`.
    time_offset: i64,
}

impl std::fmt::Debug for PacketReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PacketReader")
            .field("info", &self.info)
            .finish()
    }
}

// SAFETY: as for `VideoDecoder`.
unsafe impl Send for PacketReader {}

impl PacketReader {
    pub fn video(path: &str) -> Result<PacketReader> {
        let mut input = Input::open(path)?;
        let stream = input.video_stream().ok_or(Error::NoStream)?;
        input.only(stream);
        let info = stream_info(&input, stream);
        // SAFETY: a stream of the open file.
        let tb = unsafe { (*input.stream(stream)).time_base };
        let mut packet = AvPacket::new()?;
        let first = if input.read(stream, &mut packet) {
            // SAFETY: a packet just read.
            unsafe { packet_time(packet.0) }.unwrap_or(0).max(0)
        } else {
            0
        };
        packet.unref();
        input.seek(stream, first)?;
        Ok(PacketReader {
            input,
            stream,
            tb,
            packet,
            info,
            time_offset: first,
        })
    }

    pub fn info(&self) -> VideoInfo {
        self.info
    }

    /// The next packets start at the keyframe at or before `t`.
    pub fn seek(&mut self, t: Tick) -> Result<()> {
        let ts = from_tick(t, self.tb) + self.time_offset;
        self.input.seek(self.stream, ts)
    }

    /// The next packet (times from the first one); `None` at the end.
    pub fn next_packet(&mut self) -> Result<Option<crate::Packet>> {
        if !self.input.read(self.stream, &mut self.packet) {
            return Ok(None);
        }
        // SAFETY: a packet just read.
        let (time, duration, key) = unsafe {
            let p = &*self.packet.0;
            (
                packet_time(self.packet.0).unwrap_or(self.time_offset),
                p.duration,
                p.flags & ff::AV_PKT_FLAG_KEY != 0,
            )
        };
        let packet = crate::Packet {
            data: self.packet.data().to_vec(),
            time: rescale(time - self.time_offset, self.tb, HNS),
            duration: if duration > 0 {
                rescale(duration, self.tb, HNS)
            } else {
                0
            },
            decode_time: None,
            keyframe: key,
        };
        self.packet.unref();
        Ok(Some(packet))
    }
}

/// A packet's time (the stream's time base), when it has one.
///
/// # Safety
/// `p` is a packet read.
pub(super) unsafe fn packet_time(p: *const ff::AVPacket) -> Option<i64> {
    // SAFETY: as the caller says.
    let (pts, dts) = unsafe { ((*p).pts, (*p).dts) };
    [pts, dts].into_iter().find(|&t| t != ff::AV_NOPTS_VALUE)
}
