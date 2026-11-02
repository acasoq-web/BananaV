//! Encoding through FFmpeg: the processor's encoders (x264, x265, SVT-AV1,
//! libvpx; FFmpeg's own AAC, AC-3, FLAC and ALAC; LAME for MP3; Opus) and
//! the graphics cards' (NVENC on NVIDIA cards, VA-API on AMD and Intel
//! ones), giving packets for BananaV's own MP4/MOV writer.
//!
//! As on Windows, video frames come packed 4:2:0 (NV12, or P010 for 10
//! bits) and go out in display order (no B-frames), with the parameter sets
//! inside the keyframes.

use std::ffi::{CString, c_int};
use std::ptr;
use std::sync::OnceLock;

use bv_core::{FrameRate, VideoFrame};
use ffmpeg_sys_next as ff;

use super::hw::{self, Api};
use super::{
    AvFrame, AvPacket, BufferRef, Codec as Context, HNS, Options, check, err, init, rescale,
};
use crate::{
    AudioEncoderSettings, Codec, EncoderInfo, Error, Packet, RateControl, Result, Vendor,
    VideoEncoderSettings,
};

/// How an encoder is driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Software,
    /// NVIDIA's cards (the encoder opens the card itself).
    Nvenc,
    /// AMD's and Intel's cards: frames copied to the card's surfaces.
    Vaapi,
}

/// FFmpeg's video encoders BananaV uses, by format, the preferred first.
const VIDEO: [(Codec, Family, &str); 13] = [
    (Codec::H264, Family::Software, "libx264"),
    (Codec::H264, Family::Software, "libopenh264"),
    (Codec::Hevc, Family::Software, "libx265"),
    (Codec::Av1, Family::Software, "libsvtav1"),
    (Codec::Av1, Family::Software, "libaom-av1"),
    (Codec::Vp9, Family::Software, "libvpx-vp9"),
    (Codec::H264, Family::Nvenc, "h264_nvenc"),
    (Codec::Hevc, Family::Nvenc, "hevc_nvenc"),
    (Codec::Av1, Family::Nvenc, "av1_nvenc"),
    (Codec::H264, Family::Vaapi, "h264_vaapi"),
    (Codec::Hevc, Family::Vaapi, "hevc_vaapi"),
    (Codec::Av1, Family::Vaapi, "av1_vaapi"),
    (Codec::Vp9, Family::Vaapi, "vp9_vaapi"),
];

/// FFmpeg's audio encoders BananaV uses.
const AUDIO: [(Codec, &str); 6] = [
    (Codec::Aac, "aac"),
    (Codec::Mp3, "libmp3lame"),
    (Codec::Ac3, "ac3"),
    (Codec::Flac, "flac"),
    (Codec::Alac, "alac"),
    (Codec::Opus, "libopus"),
];

fn find(name: &str) -> *const ff::AVCodec {
    let Ok(c) = CString::new(name) else {
        return ptr::null();
    };
    init();
    // SAFETY: a lookup by name.
    unsafe { ff::avcodec_find_encoder_by_name(c.as_ptr()) }
}

fn label(codec: Codec) -> &'static str {
    match codec {
        Codec::H264 => "H.264",
        Codec::Hevc => "HEVC",
        Codec::Av1 => "AV1",
        Codec::Vp9 => "VP9",
        Codec::Wmv => "WMV",
        Codec::Aac => "AAC",
        Codec::Mp3 => "MP3",
        Codec::Ac3 => "AC-3",
        Codec::Mp2 => "MP2",
        Codec::Wma => "WMA",
        Codec::WmaLossless => "WMA Lossless",
        Codec::Flac => "FLAC",
        Codec::Alac => "ALAC",
        Codec::Opus => "Opus",
    }
}

/// The name of a processor's encoder ("x264 H.264").
fn software_name(encoder: &str, codec: Codec) -> String {
    let who = match encoder {
        "libx264" => "x264",
        "libopenh264" => "OpenH264",
        "libx265" => "x265",
        "libsvtav1" => "SVT-AV1",
        "libaom-av1" => "libaom",
        "libvpx-vp9" => "libvpx",
        "libmp3lame" => "LAME",
        "libopus" => "libopus",
        _ => "FFmpeg",
    };
    format!("{who} {}", label(codec))
}

/// The name of a card's encoder ("NVIDIA NVENC HEVC", "AMD Radeon RX 6600
/// H.264 (VA-API)").
fn card_name(family: Family, card: Option<usize>, codec: Codec) -> String {
    match (family, card.and_then(|i| hw::cards().get(i))) {
        (Family::Vaapi, Some(c)) => format!("{} {} (VA-API)", c.name, label(codec)),
        (Family::Vaapi, None) => format!("VA-API {}", label(codec)),
        _ => format!("NVIDIA NVENC {}", label(codec)),
    }
}

/// Every encoder of the system (found on the first call).
pub fn encoders() -> &'static [EncoderInfo] {
    static LIST: OnceLock<Vec<EncoderInfo>> = OnceLock::new();
    LIST.get_or_init(|| std::thread::spawn(list).join().unwrap_or_default())
}

fn add(out: &mut Vec<EncoderInfo>, codec: Codec, vendor: Vendor, name: String) {
    let hardware = vendor != Vendor::Software;
    let index = out
        .iter()
        .filter(|e| e.codec == codec && e.hardware == hardware)
        .count();
    out.push(EncoderInfo {
        codec,
        vendor,
        name,
        hardware,
        index,
    });
}

fn list() -> Vec<EncoderInfo> {
    let mut out = Vec::new();
    // The processor: the first encoder FFmpeg has of each format.
    for codec in [Codec::H264, Codec::Hevc, Codec::Av1, Codec::Vp9] {
        if let Some(&(_, _, enc)) = VIDEO
            .iter()
            .find(|(c, f, n)| *c == codec && *f == Family::Software && !find(n).is_null())
        {
            add(&mut out, codec, Vendor::Software, software_name(enc, codec));
        }
    }
    for (codec, enc) in AUDIO {
        if !find(enc).is_null() {
            add(&mut out, codec, Vendor::Software, software_name(enc, codec));
        }
    }
    // The graphics cards, each encoder tried once: FFmpeg has NVENC and
    // VA-API whatever cards the computer has.
    let nvidia = hw::cards().iter().position(|c| c.vendor == Vendor::Nvidia);
    for (i, card) in hw::cards().iter().enumerate() {
        for &(codec, family, enc) in &VIDEO {
            let fits = match family {
                Family::Nvenc => Some(i) == nvidia,
                Family::Vaapi => matches!(card.vendor, Vendor::Amd | Vendor::Intel),
                Family::Software => false,
            };
            let known = out
                .iter()
                .any(|e| e.codec == codec && e.vendor == card.vendor);
            if fits && !known && !find(enc).is_null() && works(codec, family, enc, i) {
                add(
                    &mut out,
                    codec,
                    card.vendor,
                    card_name(family, Some(i), codec),
                );
            }
        }
    }
    out
}

/// Whether the encoder opens on the card (a small picture).
fn works(codec: Codec, family: Family, encoder: &str, card: usize) -> bool {
    let s = VideoEncoderSettings {
        codec,
        vendor: Vendor::OtherHardware,
        width: 256,
        height: 256,
        frame_rate: FrameRate::FPS_30,
        bitrate: 1_000_000,
        max_bitrate: 1_000_000,
        rate_control: RateControl::Cbr,
        gop: 30,
        ten_bit: false,
        field_order: bv_core::FieldOrder::Progressive,
    };
    VideoEncoder::open(s, encoder, family, Some(card)).is_ok()
}

/// A video encoder of the system, fed 4:2:0 frames (8 bits as NV12, 10
/// bits as P010).
pub struct VideoEncoder {
    context: Context,
    settings: VideoEncoderSettings,
    name: String,
    /// The layout of the frames in memory.
    input: ff::AVPixelFormat,
    frame: AvFrame,
    /// VA-API: the card's surfaces the frames are copied to.
    surfaces: Option<BufferRef>,
    packet: AvPacket,
    /// The encoder's time base: a frame.
    tb: ff::AVRational,
}

impl std::fmt::Debug for VideoEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoEncoder")
            .field("name", &self.name)
            .field("settings", &self.settings)
            .finish()
    }
}

// SAFETY: FFmpeg's contexts are used by one thread at a time (the render's).
unsafe impl Send for VideoEncoder {}

/// Surfaces of `w`×`h` in layout `sw` on the card of `device`.
fn surfaces(device: &hw::Device, sw: ff::AVPixelFormat, w: c_int, h: c_int) -> Result<BufferRef> {
    // SAFETY: a frames context of the device, set up before its start.
    unsafe {
        let r = ff::av_hwframe_ctx_alloc(device.buffer.0);
        if r.is_null() {
            return Err(Error::Ffmpeg("out of memory".into()));
        }
        let buffer = BufferRef(r);
        let fc = (*r).data.cast::<ff::AVHWFramesContext>();
        (*fc).format = ff::AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*fc).sw_format = sw;
        (*fc).width = w;
        (*fc).height = h;
        (*fc).initial_pool_size = 20;
        check(ff::av_hwframe_ctx_init(r))?;
        Ok(buffer)
    }
}

/// The encoder's rate control, quality and speed, as the settings ask.
/// Quality (0–100) goes on each encoder's scale: CRF or CQ 51–9 for
/// H.264 and HEVC, 63–13 for AV1 and VP9.
///
/// # Safety
/// `ctx` is an encoder's context not opened yet.
unsafe fn tune(
    encoder: &str,
    s: &VideoEncoderSettings,
    ctx: *mut ff::AVCodecContext,
    o: &mut Options,
) {
    let quality = match s.rate_control {
        RateControl::Quality(q) => Some(q.min(100) as f64),
        _ => None,
    };
    let crf51 = quality.map(|q| ((51.0 - 0.42 * q).round() as i64).to_string());
    let crf63 = quality.map(|q| ((63.0 - 0.5 * q).round() as i64).to_string());
    let cbr = s.rate_control == RateControl::Cbr;
    // SAFETY: as the caller says.
    unsafe {
        match s.rate_control {
            RateControl::Cbr => {
                (*ctx).bit_rate = s.bitrate as i64;
                (*ctx).rc_max_rate = s.bitrate as i64;
                (*ctx).rc_buffer_size = s.bitrate.min(i32::MAX as u32) as c_int;
            }
            RateControl::Vbr => {
                let max = s.max_bitrate.max(s.bitrate) as i64;
                (*ctx).bit_rate = s.bitrate as i64;
                (*ctx).rc_max_rate = max;
                (*ctx).rc_buffer_size = (max * 2).min(i32::MAX as i64) as c_int;
            }
            RateControl::Quality(_) => {}
        }
        match encoder {
            "libx264" => {
                o.set("preset", "medium");
                if let Some(c) = &crf51 {
                    o.set("crf", c);
                } else if cbr {
                    o.set("nal-hrd", "cbr");
                }
            }
            "libx265" => {
                o.set("preset", "fast");
                o.set("x265-params", "log-level=error");
                if let Some(c) = &crf51 {
                    o.set("crf", c);
                }
            }
            "libopenh264" => {
                if quality.is_some() {
                    (*ctx).bit_rate = s.bitrate.max(1) as i64;
                    o.set("rc_mode", "quality");
                } else {
                    o.set("rc_mode", "bitrate");
                }
            }
            "libsvtav1" => {
                o.set("preset", "8");
                if let Some(c) = &crf63 {
                    o.set("crf", c);
                } else {
                    // A cap equal to the rate asks for SVT-AV1's CBR, which
                    // needs its low-delay structure: VBR instead.
                    (*ctx).rc_max_rate = 0;
                }
            }
            "libaom-av1" | "libvpx-vp9" => {
                o.set("cpu-used", if encoder == "libaom-av1" { "6" } else { "4" });
                o.set("row-mt", "1");
                if let Some(c) = &crf63 {
                    o.set("crf", c);
                    (*ctx).bit_rate = 0;
                }
            }
            e if e.ends_with("_nvenc") => {
                o.set("preset", "p5");
                o.set("tune", "hq");
                match (&crf51, cbr) {
                    (Some(c), _) => {
                        o.set("rc", "vbr");
                        o.set("cq", c);
                        (*ctx).bit_rate = 0;
                    }
                    (None, true) => o.set("rc", "cbr"),
                    (None, false) => o.set("rc", "vbr"),
                }
            }
            e if e.ends_with("_vaapi") => match quality {
                Some(q) => {
                    o.set("rc_mode", "CQP");
                    // AV1's and VP9's quantizers run to 255.
                    let wide = e.starts_with("av1") || e.starts_with("vp9");
                    (*ctx).global_quality = if wide {
                        (255.0 - 2.1 * q).round() as c_int
                    } else {
                        (51.0 - 0.42 * q).round() as c_int
                    };
                }
                None => o.set("rc_mode", if cbr { "CBR" } else { "VBR" }),
            },
            _ => {}
        }
    }
}

/// A row of plane `plane` of `f`, `len` bytes long.
///
/// # Safety
/// The plane holds it.
unsafe fn row<'a>(f: *mut ff::AVFrame, plane: usize, y: usize, len: usize) -> &'a mut [u8] {
    // SAFETY: as the caller says.
    unsafe {
        let step = (*f).linesize[plane] as isize;
        std::slice::from_raw_parts_mut((*f).data[plane].offset(y as isize * step), len)
    }
}

/// Copies a frame packed as `VideoFrame::packed_420` lays it out (NV12, or
/// P010 for 10 bits) into `f`, whose layout is `input`.
///
/// # Safety
/// `f` is a writable picture of `w`×`h` in layout `input`.
unsafe fn fill(
    f: *mut ff::AVFrame,
    data: &[u8],
    (w, h): (usize, usize),
    ten: bool,
    input: ff::AVPixelFormat,
) {
    use ff::AVPixelFormat::*;
    let bps = if ten { 2 } else { 1 };
    let (luma, chroma) = data.split_at(w * h * bps);
    let (cw, ch) = (w / 2, h / 2);
    let line = cw * 2 * bps;
    let ten_bits = |b: &[u8]| (u16::from_le_bytes([b[0], b[1]]) >> 6).to_le_bytes();
    // SAFETY: as the caller says.
    unsafe {
        if input == AV_PIX_FMT_NV12 || input == AV_PIX_FMT_P010LE {
            for y in 0..h {
                row(f, 0, y, w * bps).copy_from_slice(&luma[y * w * bps..(y + 1) * w * bps]);
            }
            for y in 0..ch {
                row(f, 1, y, line).copy_from_slice(&chroma[y * line..(y + 1) * line]);
            }
        } else if !ten {
            for y in 0..h {
                row(f, 0, y, w).copy_from_slice(&luma[y * w..(y + 1) * w]);
            }
            for y in 0..ch {
                let src = &chroma[y * line..(y + 1) * line];
                let (u, v) = (row(f, 1, y, cw), row(f, 2, y, cw));
                for i in 0..cw {
                    u[i] = src[2 * i];
                    v[i] = src[2 * i + 1];
                }
            }
        } else {
            // 10 bits in the low bits of each word, from P010's high ones.
            for y in 0..h {
                let src = &luma[y * w * 2..(y + 1) * w * 2];
                let dst = row(f, 0, y, w * 2);
                for (d, s) in dst
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(src.as_chunks::<2>().0)
                {
                    *d = ten_bits(s);
                }
            }
            for y in 0..ch {
                let src = &chroma[y * line..(y + 1) * line];
                let (u, v) = (row(f, 1, y, cw * 2), row(f, 2, y, cw * 2));
                for (i, pair) in src.as_chunks::<4>().0.iter().enumerate() {
                    u[2 * i..2 * i + 2].copy_from_slice(&ten_bits(&pair[..2]));
                    v[2 * i..2 * i + 2].copy_from_slice(&ten_bits(&pair[2..]));
                }
            }
        }
    }
}

impl VideoEncoder {
    pub fn new(s: VideoEncoderSettings) -> Result<VideoEncoder> {
        let cards = hw::cards();
        let mut last = Error::Ffmpeg(format!("no {:?} {:?} encoder", s.vendor, s.codec));
        for &(codec, family, enc) in &VIDEO {
            if codec != s.codec || find(enc).is_null() {
                continue;
            }
            let on: Vec<Option<usize>> = match (family, s.vendor) {
                (Family::Software, Vendor::Software) => vec![None],
                (Family::Nvenc, Vendor::Nvidia | Vendor::OtherHardware) => {
                    vec![cards.iter().position(|c| c.vendor == Vendor::Nvidia)]
                }
                (Family::Vaapi, v) if v != Vendor::Software && v != Vendor::Nvidia => (0..cards
                    .len())
                    .filter(|&i| v == Vendor::OtherHardware || cards[i].vendor == v)
                    .map(Some)
                    .collect(),
                _ => Vec::new(),
            };
            for card in on {
                match VideoEncoder::open(s, enc, family, card) {
                    Ok(e) => return Ok(e),
                    Err(e) => last = e,
                }
            }
        }
        Err(last)
    }

    fn open(
        s: VideoEncoderSettings,
        encoder: &str,
        family: Family,
        card: Option<usize>,
    ) -> Result<VideoEncoder> {
        use ff::AVPixelFormat::*;
        let codec = find(encoder);
        if codec.is_null() {
            return Err(Error::Ffmpeg(format!("no {encoder} encoder")));
        }
        let (w, h) = (s.width as c_int, s.height as c_int);
        let ten = s.ten_bit;
        let input = match family {
            Family::Software if !ten && encoder == "libx264" => AV_PIX_FMT_NV12,
            Family::Software if ten => AV_PIX_FMT_YUV420P10LE,
            Family::Software => AV_PIX_FMT_YUV420P,
            _ if ten => AV_PIX_FMT_P010LE,
            _ => AV_PIX_FMT_NV12,
        };
        let device = match family {
            Family::Vaapi => Some(
                card.and_then(|c| hw::open(Api::Vaapi, c))
                    .ok_or_else(|| Error::Ffmpeg("no VA-API on this card".into()))?,
            ),
            _ => None,
        };
        // SAFETY: the context is set up before it opens, then only used
        // through FFmpeg's calls.
        unsafe {
            let ctx = ff::avcodec_alloc_context3(codec);
            if ctx.is_null() {
                return Err(Error::Ffmpeg("out of memory".into()));
            }
            let context = Context(ctx);
            (*ctx).width = w;
            (*ctx).height = h;
            (*ctx).time_base = ff::AVRational {
                num: s.frame_rate.den as c_int,
                den: s.frame_rate.num as c_int,
            };
            (*ctx).framerate = ff::AVRational {
                num: s.frame_rate.num as c_int,
                den: s.frame_rate.den as c_int,
            };
            (*ctx).sample_aspect_ratio = ff::AVRational { num: 1, den: 1 };
            (*ctx).gop_size = s.gop.max(1) as c_int;
            // No B-frames: frames come out in display order.
            (*ctx).max_b_frames = 0;
            (*ctx).color_primaries = ff::AVColorPrimaries::AVCOL_PRI_BT709;
            (*ctx).color_trc = ff::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*ctx).colorspace = ff::AVColorSpace::AVCOL_SPC_BT709;
            (*ctx).color_range = ff::AVColorRange::AVCOL_RANGE_MPEG;
            (*ctx).pix_fmt = input;
            let surfaces = match &device {
                Some(d) => {
                    let s = surfaces(d, input, w, h)?;
                    (*ctx).hw_frames_ctx = s.new_ref();
                    (*ctx).pix_fmt = AV_PIX_FMT_VAAPI;
                    Some(s)
                }
                None => None,
            };
            let mut options = Options::new();
            tune(encoder, &s, ctx, &mut options);
            check(ff::avcodec_open2(ctx, codec, &mut options.0))?;
            let frame = AvFrame::new()?;
            (*frame.0).format = input as c_int;
            (*frame.0).width = w;
            (*frame.0).height = h;
            check(ff::av_frame_get_buffer(frame.0, 0))?;
            let name = match family {
                Family::Software => software_name(encoder, s.codec),
                f => card_name(f, card, s.codec),
            };
            Ok(VideoEncoder {
                tb: (*ctx).time_base,
                context,
                settings: s,
                name,
                input,
                frame,
                surfaces,
                packet: AvPacket::new()?,
            })
        }
    }

    /// The encoder's name ("x264 H.264", "NVIDIA NVENC HEVC"…).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The parameter sets announced outside the stream: none, they come in
    /// the keyframes.
    pub fn sequence_header(&self) -> Option<Vec<u8>> {
        None
    }

    /// Encodes a 4:2:0 frame shown at `time` for `duration` (100 ns units);
    /// returns the packets that are ready.
    pub fn encode(&mut self, f: &VideoFrame, time: i64, duration: i64) -> Result<Vec<Packet>> {
        let packed = f.packed_420(self.settings.ten_bit);
        self.encode_packed(&packed, time, duration)
    }

    /// Encodes a frame packed already as [`VideoFrame::packed_420`] lays
    /// it out (NV12, or P010 for a 10-bit encoder) at the encoder's size.
    pub fn encode_packed(&mut self, data: &[u8], time: i64, duration: i64) -> Result<Vec<Packet>> {
        let _ = duration;
        let (w, h) = (self.settings.width as usize, self.settings.height as usize);
        let ten = self.settings.ten_bit;
        let bytes = (w * h + 2 * (w / 2) * (h / 2)) * if ten { 2 } else { 1 };
        if data.len() < bytes {
            return Err(Error::Ffmpeg(format!(
                "a frame of {} bytes for a {w}×{h} encoder",
                data.len()
            )));
        }
        let f = self.frame.0;
        // SAFETY: the encoder's frame, made writable before it is filled;
        // a surface of the card's for VA-API.
        unsafe {
            check(ff::av_frame_make_writable(f))?;
            fill(f, &data[..bytes], (w, h), ten, self.input);
            (*f).pts = rescale(time, HNS, self.tb);
            match &self.surfaces {
                Some(surfaces) => {
                    let card = AvFrame::new()?;
                    check(ff::av_hwframe_get_buffer(surfaces.0, card.0, 0))?;
                    check(ff::av_hwframe_transfer_data(card.0, f, 0))?;
                    (*card.0).pts = (*f).pts;
                    self.send(card.0)
                }
                None => self.send(f),
            }
        }
    }

    fn send(&mut self, f: *const ff::AVFrame) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        // SAFETY: an open encoder and a filled frame.
        let mut r = unsafe { ff::avcodec_send_frame(self.context.0, f) };
        if r == ff::AVERROR(ff::EAGAIN) {
            self.take(&mut out)?;
            // SAFETY: as above.
            r = unsafe { ff::avcodec_send_frame(self.context.0, f) };
        }
        check(r)?;
        self.take(&mut out)?;
        Ok(out)
    }

    /// The packets the encoder has ready.
    fn take(&mut self, out: &mut Vec<Packet>) -> Result<()> {
        loop {
            // SAFETY: an open encoder and an allocated packet.
            let r = unsafe { ff::avcodec_receive_packet(self.context.0, self.packet.0) };
            if r == ff::AVERROR(ff::EAGAIN) || r == ff::AVERROR_EOF {
                return Ok(());
            }
            check(r)?;
            // SAFETY: a packet just made.
            let (pts, dts, duration, key) = unsafe {
                let p = &*self.packet.0;
                (p.pts, p.dts, p.duration, p.flags & ff::AV_PKT_FLAG_KEY != 0)
            };
            let pts = if pts == ff::AV_NOPTS_VALUE { dts } else { pts };
            out.push(Packet {
                data: self.packet.data().to_vec(),
                time: rescale(pts, self.tb, HNS),
                duration: rescale(duration.max(1), self.tb, HNS),
                decode_time: (dts != ff::AV_NOPTS_VALUE && dts != pts)
                    .then(|| rescale(dts, self.tb, HNS)),
                keyframe: key,
            });
            self.packet.unref();
        }
    }

    /// Ends the stream: the packets still inside the encoder.
    pub fn finish(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        // SAFETY: an open encoder; null asks it to drain.
        let r = unsafe { ff::avcodec_send_frame(self.context.0, ptr::null()) };
        if r < 0 && r != ff::AVERROR_EOF {
            return Err(err(r));
        }
        self.take(&mut out)?;
        Ok(out)
    }
}

/// An audio encoder of the system, fed interleaved float samples.
pub struct AudioEncoder {
    context: Context,
    settings: AudioEncoderSettings,
    name: String,
    config: Vec<u8>,
    frame: AvFrame,
    packet: AvPacket,
    format: ff::AVSampleFormat,
    /// Samples (each channel's) a frame takes.
    frame_size: usize,
    /// The encoder takes a shorter last frame.
    short_last: bool,
    /// Interleaved samples waiting for a whole frame.
    pending: Vec<f32>,
    sent: i64,
    tb: ff::AVRational,
}

impl std::fmt::Debug for AudioEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioEncoder")
            .field("name", &self.name)
            .field("settings", &self.settings)
            .finish()
    }
}

// SAFETY: as for [`VideoEncoder`].
unsafe impl Send for AudioEncoder {}

impl AudioEncoder {
    pub fn new(s: AudioEncoderSettings) -> Result<AudioEncoder> {
        use ff::AVSampleFormat::*;
        let enc = AUDIO
            .iter()
            .find(|(c, _)| *c == s.codec)
            .map(|(_, n)| *n)
            .ok_or(Error::Unavailable)?;
        let codec = find(enc);
        if codec.is_null() {
            return Err(Error::Ffmpeg(format!("no {enc} encoder")));
        }
        let deep = s.bits > 16;
        let format = match s.codec {
            Codec::Flac if deep => AV_SAMPLE_FMT_S32,
            Codec::Flac => AV_SAMPLE_FMT_S16,
            Codec::Alac if deep => AV_SAMPLE_FMT_S32P,
            Codec::Alac => AV_SAMPLE_FMT_S16P,
            Codec::Opus => AV_SAMPLE_FMT_FLT,
            _ => AV_SAMPLE_FMT_FLTP,
        };
        let tb = ff::AVRational {
            num: 1,
            den: s.sample_rate as c_int,
        };
        // SAFETY: the context is set up before it opens; the frame is
        // allocated for its layout.
        unsafe {
            let ctx = ff::avcodec_alloc_context3(codec);
            if ctx.is_null() {
                return Err(Error::Ffmpeg("out of memory".into()));
            }
            let context = Context(ctx);
            (*ctx).sample_rate = s.sample_rate as c_int;
            ff::av_channel_layout_default(&mut (*ctx).ch_layout, s.channels as c_int);
            (*ctx).sample_fmt = format;
            (*ctx).time_base = tb;
            if matches!(s.codec, Codec::Aac | Codec::Mp3 | Codec::Ac3 | Codec::Opus) {
                (*ctx).bit_rate = s.bitrate as i64;
            }
            if deep && matches!(s.codec, Codec::Flac | Codec::Alac) {
                (*ctx).bits_per_raw_sample = 24;
            }
            // The decoder's configuration (AAC's AudioSpecificConfig…)
            // outside the packets.
            (*ctx).flags |= ff::AV_CODEC_FLAG_GLOBAL_HEADER as c_int;
            check(ff::avcodec_open2(ctx, codec, ptr::null_mut()))?;
            let frame_size = if (*ctx).frame_size > 0 {
                (*ctx).frame_size as usize
            } else {
                4096
            };
            let caps = (*codec).capabilities as u32;
            let short_last = caps
                & (ff::AV_CODEC_CAP_SMALL_LAST_FRAME | ff::AV_CODEC_CAP_VARIABLE_FRAME_SIZE)
                != 0;
            let frame = AvFrame::new()?;
            (*frame.0).format = format as c_int;
            (*frame.0).nb_samples = frame_size as c_int;
            (*frame.0).sample_rate = s.sample_rate as c_int;
            check(ff::av_channel_layout_copy(
                &mut (*frame.0).ch_layout,
                &(*ctx).ch_layout,
            ))?;
            check(ff::av_frame_get_buffer(frame.0, 0))?;
            let mut encoder = AudioEncoder {
                context,
                settings: s,
                name: software_name(enc, s.codec),
                config: Vec::new(),
                frame,
                packet: AvPacket::new()?,
                format,
                frame_size,
                short_last,
                pending: Vec::new(),
                sent: 0,
                tb,
            };
            encoder.config = encoder.read_config();
            Ok(encoder)
        }
    }

    fn read_config(&self) -> Vec<u8> {
        // SAFETY: an open encoder's configuration, `extradata_size` long.
        unsafe {
            let ctx = self.context.0;
            if (*ctx).extradata.is_null() || (*ctx).extradata_size <= 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts((*ctx).extradata, (*ctx).extradata_size as usize)
                    .to_vec()
            }
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The decoder configuration: AAC's AudioSpecificConfig, ALAC's magic
    /// cookie; empty for formats without one.
    pub fn config(&self) -> &[u8] {
        &self.config
    }

    pub fn bitrate(&self) -> u32 {
        // SAFETY: an open encoder.
        unsafe { (*self.context.0).bit_rate.clamp(0, u32::MAX as i64) as u32 }
    }

    /// Encodes interleaved samples; returns the packets that are ready.
    pub fn encode(&mut self, interleaved: &[f32]) -> Result<Vec<Packet>> {
        self.pending.extend_from_slice(interleaved);
        let ch = self.settings.channels.max(1) as usize;
        let mut out = Vec::new();
        let mut used = 0;
        while self.pending.len() - used >= self.frame_size * ch {
            self.put(used, self.frame_size, &mut out)?;
            used += self.frame_size * ch;
        }
        self.pending.drain(..used);
        Ok(out)
    }

    /// Sends `n` samples of `pending` from `from` on as a frame.
    fn put(&mut self, from: usize, n: usize, out: &mut Vec<Packet>) -> Result<()> {
        use ff::AVSampleFormat::*;
        let ch = self.settings.channels.max(1) as usize;
        let f = self.frame.0;
        // SAFETY: the encoder's frame, made writable, holds `frame_size ≥ n`
        // samples of its layout.
        unsafe {
            check(ff::av_frame_make_writable(f))?;
            (*f).nb_samples = n as c_int;
            let samples = &self.pending[from..from + n * ch];
            let format = self.format;
            let planar = matches!(
                format,
                AV_SAMPLE_FMT_FLTP | AV_SAMPLE_FMT_S16P | AV_SAMPLE_FMT_S32P
            );
            for c in 0..ch {
                let base = if planar {
                    *(*f).extended_data.add(c)
                } else {
                    (*f).data[0]
                };
                for i in 0..n {
                    let v = samples[i * ch + c].clamp(-1.0, 1.0);
                    let at = if planar { i } else { i * ch + c };
                    match format {
                        AV_SAMPLE_FMT_FLTP | AV_SAMPLE_FMT_FLT => {
                            base.cast::<f32>().add(at).write(v)
                        }
                        AV_SAMPLE_FMT_S16 | AV_SAMPLE_FMT_S16P => base
                            .cast::<i16>()
                            .add(at)
                            .write((v * 32_767.0).round() as i16),
                        _ => base
                            .cast::<i32>()
                            .add(at)
                            .write((v as f64 * 2_147_483_647.0).round() as i32),
                    }
                }
            }
            (*f).pts = self.sent;
            self.sent += n as i64;
            let r = ff::avcodec_send_frame(self.context.0, f);
            check(r)?;
        }
        self.take(out)
    }

    fn take(&mut self, out: &mut Vec<Packet>) -> Result<()> {
        loop {
            // SAFETY: an open encoder and an allocated packet.
            let r = unsafe { ff::avcodec_receive_packet(self.context.0, self.packet.0) };
            if r == ff::AVERROR(ff::EAGAIN) || r == ff::AVERROR_EOF {
                return Ok(());
            }
            check(r)?;
            // SAFETY: a packet just made.
            let (pts, duration) = unsafe { ((*self.packet.0).pts, (*self.packet.0).duration) };
            let duration = if duration > 0 {
                duration
            } else {
                self.frame_size as i64
            };
            out.push(Packet {
                data: self.packet.data().to_vec(),
                time: rescale(pts, self.tb, HNS),
                duration: rescale(duration, self.tb, HNS),
                decode_time: None,
                keyframe: true,
            });
            self.packet.unref();
        }
    }

    /// Ends the stream: the packets still inside the encoder.
    pub fn finish(&mut self) -> Result<Vec<Packet>> {
        let ch = self.settings.channels.max(1) as usize;
        let mut out = Vec::new();
        let left = self.pending.len() / ch;
        if left > 0 {
            if !self.short_last {
                self.pending.resize(self.frame_size * ch, 0.0);
            }
            let n = self.pending.len() / ch;
            self.put(0, n, &mut out)?;
            self.pending.clear();
        }
        // SAFETY: an open encoder; null asks it to drain.
        let r = unsafe { ff::avcodec_send_frame(self.context.0, ptr::null()) };
        if r < 0 && r != ff::AVERROR_EOF {
            return Err(err(r));
        }
        self.take(&mut out)?;
        if self.config.is_empty() {
            self.config = self.read_config();
        }
        Ok(out)
    }
}
