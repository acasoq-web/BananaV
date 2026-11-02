//! Pictures FFmpeg reads (WebP, AVIF, HEIF from FFmpeg 7.1 on, icons…):
//! what the Windows Imaging Component reads on Windows. Camera RAW files
//! need a developer FFmpeg does not have.

use std::ptr;

use ffmpeg_sys_next as ff;

use super::{AvFrame, AvPacket, Converter, Input, color_info, err, num, open_decoder};
use crate::{Error, Result, Still};

/// The size of the file's picture.
pub fn still_size(path: &str) -> Result<(u32, u32)> {
    let input = Input::open(path)?;
    let index = input.video_stream().ok_or(Error::NoStream)?;
    // SAFETY: a stream of the open file and its parameters.
    let (w, h) = unsafe {
        let par = (*input.stream(index)).codecpar;
        ((*par).width, (*par).height)
    };
    if w > 0 && h > 0 {
        Ok((w as u32, h as u32))
    } else {
        read_still(path).map(|s| (s.width, s.height))
    }
}

/// The file's picture as RGBA of 16 bits.
pub fn read_still(path: &str) -> Result<Still> {
    let mut input = Input::open(path)?;
    let index = input.video_stream().ok_or(Error::NoStream)?;
    input.only(index);
    let codec = open_decoder(&input, index, |ctx| {
        // SAFETY: the context is being set up, before it opens.
        unsafe { (*ctx).thread_count = 0 };
    })?;
    let mut packet = AvPacket::new()?;
    let frame = AvFrame::new()?;
    let mut drained = false;
    loop {
        // SAFETY: an open decoder and an allocated frame.
        let r = unsafe { ff::avcodec_receive_frame(codec.0, frame.0) };
        if r >= 0 {
            break;
        }
        if r != ff::AVERROR(ff::EAGAIN) || drained {
            return Err(if r == ff::AVERROR_EOF {
                Error::Ffmpeg("no picture".into())
            } else {
                err(r)
            });
        }
        if input.read(index, &mut packet) {
            // SAFETY: an open decoder and a packet of its stream.
            unsafe { ff::avcodec_send_packet(codec.0, packet.0) };
            packet.unref();
        } else {
            // SAFETY: an open decoder; null asks it to drain.
            unsafe { ff::avcodec_send_packet(codec.0, ptr::null()) };
            drained = true;
        }
    }
    // SAFETY: a decoded picture, converted to RGBA of 16 bits.
    unsafe {
        let f = frame.0;
        let (w, h) = ((*f).width.max(0) as usize, (*f).height.max(0) as usize);
        if w == 0 || h == 0 {
            return Err(Error::Ffmpeg("a picture of no size".into()));
        }
        let color = color_info(
            (w as u32, h as u32),
            num(&raw const (*f).color_primaries),
            num(&raw const (*f).color_trc),
            num(&raw const (*f).colorspace),
            num(&raw const (*f).color_range),
        );
        // RGB of 8 or 16 bits is copied (exactly: 255 is 65535); other
        // layouts go through libswscale.
        use ff::AVPixelFormat::*;
        let format = (*f).format;
        let packed = [
            (AV_PIX_FMT_RGBA, [0, 1, 2, 3], 1),
            (AV_PIX_FMT_BGRA, [2, 1, 0, 3], 1),
            (AV_PIX_FMT_ARGB, [1, 2, 3, 0], 1),
            (AV_PIX_FMT_ABGR, [3, 2, 1, 0], 1),
            (AV_PIX_FMT_RGB24, [0, 1, 2, 4], 1),
            (AV_PIX_FMT_BGR24, [2, 1, 0, 4], 1),
            (AV_PIX_FMT_RGBA64LE, [0, 1, 2, 3], 2),
            (AV_PIX_FMT_RGB48LE, [0, 1, 2, 4], 2),
        ];
        let direct = packed
            .iter()
            .find(|(p, _, _)| format == *p as std::ffi::c_int);
        let converted;
        let (src, order, bytes) = match direct {
            Some(&(_, order, bytes)) => (f.cast_const(), order, bytes),
            None => {
                converted = Converter::new().convert(f, (w, h), AV_PIX_FMT_RGBA64LE, &color)?;
                (converted.0.cast_const(), [0, 1, 2, 3], 2)
            }
        };
        // Channels a pixel has: 3 when the order has no alpha (4: opaque).
        let channels = if order[3] == 4 { 3 } else { 4 };
        let (base, step) = ((*src).data[0], (*src).linesize[0] as isize);
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let len = w * channels * bytes;
            let row = std::slice::from_raw_parts(base.offset(y as isize * step), len);
            for px in row.chunks_exact(channels * bytes) {
                for &c in &order {
                    out.push(match (c, bytes) {
                        (4, _) => 65_535,
                        (c, 1) => px[c] as u16 * 257,
                        (c, _) => u16::from_le_bytes([px[2 * c], px[2 * c + 1]]),
                    });
                }
            }
        }
        Ok(Still {
            width: w as u32,
            height: h as u32,
            rgba: out,
        })
    }
}
