//! Sound through FFmpeg: a stream decoded to interleaved floats and read
//! with random access, as Windows' decoder is (`imp::AudioDecoder`).
//!
//! Each block goes where its time says: FFmpeg already leaves out the
//! encoder's priming (the MP4 edit list, MP3's LAME header, Opus' pre-skip),
//! or dates it before the start.

use std::ffi::c_int;
use std::ptr;

use bv_core::{AudioBuffer, ChannelLayout, SampleRate};
use ffmpeg_sys_next as ff;

use super::video::packet_time;
use super::{
    AvFrame, AvPacket, Codec, Input, decodable, err, fourcc_of, num, open_decoder, rescale,
};
use crate::common::remap;
use crate::{AudioInfo, Error, PacketDecoder, Result};

/// What a stream's parameters tell before decoding.
pub(super) fn stream_info(input: &Input, index: usize) -> AudioInfo {
    // SAFETY: a stream of the open file and its parameters.
    unsafe {
        let par = (*input.stream(index)).codecpar;
        let rate = (*par).sample_rate;
        let channels = (*par).ch_layout.nb_channels;
        AudioInfo {
            sample_rate: SampleRate(if rate > 0 { rate as u32 } else { 48_000 }),
            channels: if channels > 0 { channels as u16 } else { 2 },
            codec: fourcc_of(par),
            decodable: decodable(par),
        }
    }
}

/// Who decodes the stream.
enum Output {
    Ffmpeg(Codec),
    /// Packets for a decoder of BananaV's own (AC-3).
    Packets(Box<dyn PacketDecoder>),
}

pub struct AudioDecoder {
    input: Input,
    /// To open the file again (see `seek_samples`).
    path: String,
    stream: usize,
    tb: ff::AVRational,
    output: Output,
    packet: AvPacket,
    frame: AvFrame,
    /// The decoder was told the stream ended: it gives what it holds.
    drained: bool,
    rate: u32,
    /// The rate of the last block FFmpeg decoded.
    block_rate: u32,
    channels: usize,
    codec: u32,
    /// Interleaved samples from `buf_start` on.
    buf: Vec<f32>,
    buf_start: i64,
    eos: bool,
    /// The time (in samples) of the first sound: what counts as sample 0
    /// (see `VideoDecoder::time_offset`).
    offset: i64,
    /// Where the next block goes when it has no time.
    next_at: Option<i64>,
}

impl std::fmt::Debug for AudioDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDecoder")
            .field("rate", &self.rate)
            .field("channels", &self.channels)
            .finish()
    }
}

// SAFETY: FFmpeg's contexts are used by one thread at a time (the decoder's
// owner's); the packet decoder is `Send`.
unsafe impl Send for AudioDecoder {}

fn open_audio_decoder(input: &Input, index: usize) -> Result<Codec> {
    open_decoder(input, index, |ctx| {
        // SAFETY: the context is being set up, before it opens.
        unsafe {
            (*ctx).request_sample_fmt = ff::AVSampleFormat::AV_SAMPLE_FMT_FLT;
            (*ctx).thread_count = 1;
        }
    })
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
    /// with `decoder` (BananaV's own: the same sound on every system).
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
        let mut input = Input::open(path)?;
        let stream = *input.audio_streams().get(index).ok_or(Error::NoStream)?;
        input.only(stream);
        let info = stream_info(&input, stream);
        // SAFETY: a stream of the open file and its parameters.
        let (id, version_one) = unsafe {
            let id = num(&raw const (*(*input.stream(stream)).codecpar).codec_id) as u32;
            (id, id == ff::AVCodecID::AV_CODEC_ID_WMAV1 as u32)
        };
        let wma = version_one || id == ff::AVCodecID::AV_CODEC_ID_WMAV2 as u32;
        let output = match decoder {
            Some(d) => Output::Packets(d),
            None => Output::Ffmpeg(open_audio_decoder(&input, stream)?),
        };
        // SAFETY: a stream of the open file.
        let tb = unsafe { (*input.stream(stream)).time_base };
        let avi = input.is_avi();
        let mut d = AudioDecoder {
            input,
            path: path.to_string(),
            stream,
            tb,
            output,
            packet: AvPacket::new()?,
            frame: AvFrame::new()?,
            drained: false,
            rate: info.sample_rate.0,
            block_rate: 0,
            channels: info.channels.max(1) as usize,
            codec: info.codec,
            buf: Vec::new(),
            buf_start: 0,
            eos: false,
            offset: 0,
            next_at: None,
        };
        // The first block says what the stream holds (rate, channels: an
        // MPEG-TS stream may claim stereo and hold mono; AC-3 says them in
        // its frames) and where its sound starts.
        let first = d.next_block()?;
        match &mut d.output {
            Output::Packets(p) => {
                if p.sample_rate() > 0 && p.channels() > 0 {
                    d.rate = p.sample_rate();
                    d.channels = p.channels();
                }
                p.reset();
            }
            Output::Ffmpeg(_) if d.block_rate > 0 => d.rate = d.block_rate,
            Output::Ffmpeg(_) => {}
        }
        // An AVI's sound starts at zero.
        let start = if avi {
            0
        } else {
            first.and_then(|(t, _)| t).unwrap_or(0).max(0)
        };
        d.offset = rescale(start, d.tb, d.sample_tb());
        if wma && matches!(d.output, Output::Ffmpeg(_)) {
            // FFmpeg's WMA decoder gives the sound one frame early (Windows'
            // decoder, like the encoders, a frame later): put back.
            d.offset -= wma_frame(d.rate, version_one);
        }
        d.seek_samples(0)?;
        Ok(d)
    }

    pub fn info(&self) -> AudioInfo {
        AudioInfo {
            sample_rate: SampleRate(self.rate),
            channels: self.channels as u16,
            codec: self.codec,
            decodable: !matches!(self.output, Output::Packets(_)),
        }
    }

    /// The time base of one sample.
    fn sample_tb(&self) -> ff::AVRational {
        ff::AVRational {
            num: 1,
            den: self.rate.max(1) as c_int,
        }
    }

    fn buf_end(&self) -> i64 {
        self.buf_start + (self.buf.len() / self.channels) as i64
    }

    /// The next packets start before sample `from` (the stream's very start
    /// for 0).
    fn seek_samples(&mut self, from: i64) -> Result<()> {
        let ts = if from <= 0 {
            // SAFETY: a stream of the open file.
            let start = unsafe { (*self.input.stream(self.stream)).start_time };
            if start == ff::AV_NOPTS_VALUE {
                0
            } else {
                start
            }
        } else {
            rescale(from + self.offset, self.sample_tb(), self.tb)
        };
        if self.input.seek(self.stream, ts).is_err() {
            // Some files refuse a seek: read again from the start.
            let mut input = Input::open(&self.path)?;
            input.only(self.stream);
            if let Output::Ffmpeg(c) = &mut self.output {
                *c = open_audio_decoder(&input, self.stream)?;
            }
            self.input = input;
        }
        match &mut self.output {
            // SAFETY: an open decoder.
            Output::Ffmpeg(c) => unsafe { ff::avcodec_flush_buffers(c.0) },
            Output::Packets(d) => d.reset(),
        }
        self.drained = false;
        self.next_at = None;
        Ok(())
    }

    /// Reads `frames` samples from sample `start` on (planar). What lies
    /// before the start or after the end comes back as silence.
    pub fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer> {
        let end = start + frames as i64;
        let rate = self.rate as i64;
        let outside = start < self.buf_start || start > self.buf_end() + rate;
        if self.buf.is_empty() || outside {
            // A little earlier, for the compressed frames to be whole.
            let from = (start - rate / 2).max(0);
            self.seek_samples(from)?;
            self.buf.clear();
            self.buf_start = from;
            self.eos = false;
            let mut first = true;
            while !self.eos && (first || self.buf_end() < end) {
                first = false;
                self.pull()?;
            }
        }
        while !self.eos && self.buf_end() < end {
            self.pull()?;
        }
        let ch = self.channels;
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

    /// The next block of sound: its time (the stream's time base), when it
    /// has one, and its samples interleaved; `None` at the end.
    fn next_block(&mut self) -> Result<Option<(Option<i64>, Vec<f32>)>> {
        match &mut self.output {
            Output::Packets(d) => {
                if !self.input.read(self.stream, &mut self.packet) {
                    return Ok(None);
                }
                // SAFETY: a packet just read.
                let time = unsafe { packet_time(self.packet.0) };
                let mut out = Vec::new();
                // A damaged packet is silence, not the end of the media.
                if d.decode(self.packet.data(), &mut out).is_err() {
                    d.reset();
                }
                self.packet.unref();
                if d.channels() > 0 && d.channels() != self.channels && !self.buf.is_empty() {
                    // Channels changed mid-stream: kept as many as said.
                    out = remap(&out, d.channels(), self.channels);
                }
                Ok(Some((time, out)))
            }
            Output::Ffmpeg(codec) => loop {
                // SAFETY: an open decoder and an allocated frame.
                let r = unsafe { ff::avcodec_receive_frame(codec.0, self.frame.0) };
                if r >= 0 {
                    let f = self.frame.0;
                    // SAFETY: a decoded block.
                    let (ch, samples, time) = unsafe {
                        if (*f).sample_rate > 0 {
                            self.block_rate = (*f).sample_rate as u32;
                        }
                        let ch = (*f).ch_layout.nb_channels.max(1) as usize;
                        let best = (*f).best_effort_timestamp;
                        let time = [best, (*f).pts]
                            .into_iter()
                            .find(|&t| t != ff::AV_NOPTS_VALUE);
                        (ch, interleaved(f, ch), time)
                    };
                    // SAFETY: the block was copied out.
                    unsafe { ff::av_frame_unref(f) };
                    if ch != self.channels {
                        // The decoder found the real format: what was
                        // decoded keeps its sound.
                        self.buf = remap(&self.buf, self.channels, ch);
                        self.channels = ch;
                    }
                    return Ok(Some((time, samples)));
                }
                if r == ff::AVERROR_EOF {
                    return Ok(None);
                }
                if r != ff::AVERROR(ff::EAGAIN) && r != ff::AVERROR_INVALIDDATA {
                    return Err(err(r));
                }
                if self.drained {
                    return Ok(None);
                }
                if self.input.read(self.stream, &mut self.packet) {
                    // SAFETY: an open decoder and a packet of its stream.
                    let r = unsafe { ff::avcodec_send_packet(codec.0, self.packet.0) };
                    self.packet.unref();
                    if r < 0 && r != ff::AVERROR_INVALIDDATA && r != ff::AVERROR(ff::EAGAIN) {
                        return Err(err(r));
                    }
                } else {
                    // SAFETY: an open decoder; null asks it to drain.
                    unsafe { ff::avcodec_send_packet(codec.0, ptr::null()) };
                    self.drained = true;
                }
            },
        }
    }

    /// Decodes the next block and fits it in the buffer by its time.
    fn pull(&mut self) -> Result<()> {
        let Some((time, samples)) = self.next_block()? else {
            self.eos = true;
            return Ok(());
        };
        let ch = self.channels;
        let frames = (samples.len() / ch) as i64;
        let at = match time {
            Some(t) => rescale(t, self.tb, self.sample_tb()) - self.offset,
            None => self.next_at.unwrap_or_else(|| self.buf_end()),
        };
        self.next_at = Some(at + frames);
        if self.buf.is_empty() {
            self.buf_start = at;
            self.buf = samples;
            return Ok(());
        }
        let end = self.buf_end();
        if matches!(self.output, Output::Packets(_)) {
            // A packet's time is that of the first frame starting in it: the
            // decoder carries the frames split across packets, so the sound
            // runs on without gaps (a real gap is rare here).
            if at > end + self.rate as i64 / 2 {
                self.buf
                    .resize(self.buf.len() + (at - end) as usize * ch, 0.0);
            }
            self.buf.extend_from_slice(&samples);
            return Ok(());
        }
        // A block a sample or two off still follows the previous one (times
        // rounded by the file): only a real gap is silence, and only a real
        // overlap drops samples.
        let slack = (self.rate as i64 / 1000).max(2);
        if at > end + slack {
            self.buf
                .resize(self.buf.len() + (at - end) as usize * ch, 0.0);
            self.buf.extend_from_slice(&samples);
        } else if at < end - slack {
            let skip = ((end - at) as usize * ch).min(samples.len());
            self.buf.extend_from_slice(&samples[skip..]);
        } else {
            self.buf.extend_from_slice(&samples);
        }
        Ok(())
    }
}

/// The samples of a WMA (1 or 2) frame at `rate`, as its decoder sizes
/// them.
fn wma_frame(rate: u32, version_one: bool) -> i64 {
    if rate <= 16_000 {
        512
    } else if rate <= 22_050 || (rate <= 32_000 && version_one) {
        1024
    } else {
        2048
    }
}

/// A decoded block's samples, interleaved, as floats.
///
/// # Safety
/// `f` is a decoded block of `ch` channels.
unsafe fn interleaved(f: *const ff::AVFrame, ch: usize) -> Vec<f32> {
    use ff::AVSampleFormat::*;
    // SAFETY: as the caller says: `extended_data` holds a plane per
    // channel (planar layouts) or one plane of all.
    unsafe {
        let n = (*f).nb_samples.max(0) as usize;
        let format = (*f).format;
        let is = |s: ff::AVSampleFormat| format == s as c_int;
        type Read = unsafe fn(*const u8, usize) -> f32;
        unsafe fn u8_(p: *const u8, i: usize) -> f32 {
            // SAFETY: the caller's sample `i`.
            (unsafe { *p.add(i) } as f32 - 128.0) / 128.0
        }
        unsafe fn s16(p: *const u8, i: usize) -> f32 {
            // SAFETY: as above.
            unsafe { p.cast::<i16>().add(i).read_unaligned() as f32 / 32_768.0 }
        }
        unsafe fn s32(p: *const u8, i: usize) -> f32 {
            // SAFETY: as above.
            unsafe { p.cast::<i32>().add(i).read_unaligned() as f32 / 2_147_483_648.0 }
        }
        unsafe fn flt(p: *const u8, i: usize) -> f32 {
            // SAFETY: as above.
            unsafe { p.cast::<f32>().add(i).read_unaligned() }
        }
        unsafe fn dbl(p: *const u8, i: usize) -> f32 {
            // SAFETY: as above.
            unsafe { p.cast::<f64>().add(i).read_unaligned() as f32 }
        }
        unsafe fn s64(p: *const u8, i: usize) -> f32 {
            // SAFETY: as above.
            unsafe { (p.cast::<i64>().add(i).read_unaligned() >> 32) as f32 / 2_147_483_648.0 }
        }
        let (read, planar): (Read, bool) = if is(AV_SAMPLE_FMT_U8) || is(AV_SAMPLE_FMT_U8P) {
            (u8_, is(AV_SAMPLE_FMT_U8P))
        } else if is(AV_SAMPLE_FMT_S16) || is(AV_SAMPLE_FMT_S16P) {
            (s16, is(AV_SAMPLE_FMT_S16P))
        } else if is(AV_SAMPLE_FMT_S32) || is(AV_SAMPLE_FMT_S32P) {
            (s32, is(AV_SAMPLE_FMT_S32P))
        } else if is(AV_SAMPLE_FMT_FLT) || is(AV_SAMPLE_FMT_FLTP) {
            (flt, is(AV_SAMPLE_FMT_FLTP))
        } else if is(AV_SAMPLE_FMT_DBL) || is(AV_SAMPLE_FMT_DBLP) {
            (dbl, is(AV_SAMPLE_FMT_DBLP))
        } else if is(AV_SAMPLE_FMT_S64) || is(AV_SAMPLE_FMT_S64P) {
            (s64, is(AV_SAMPLE_FMT_S64P))
        } else {
            return vec![0.0; n * ch];
        };
        let data = (*f).extended_data;
        let mut out = vec![0.0f32; n * ch];
        for c in 0..ch {
            let (base, step, first) = if planar {
                (*data.add(c), 1, 0)
            } else {
                (*data, ch, c)
            };
            for i in 0..n {
                out[i * ch + c] = read(base, i * step + first);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_of_any_layout_come_interleaved() {
        use ff::AVSampleFormat::AV_SAMPLE_FMT_S16P;
        // Two channels of three samples, planar 16-bit.
        let frame = AvFrame::new().unwrap();
        // SAFETY: the frame is set up and allocated by FFmpeg.
        unsafe {
            let f = frame.0;
            (*f).format = AV_SAMPLE_FMT_S16P as c_int;
            (*f).nb_samples = 3;
            ff::av_channel_layout_default(&mut (*f).ch_layout, 2);
            assert!(ff::av_frame_get_buffer(f, 0) >= 0);
            let left = (*f).data[0].cast::<i16>();
            let right = (*f).data[1].cast::<i16>();
            for i in 0..3 {
                *left.add(i) = 16_384;
                *right.add(i) = -16_384;
            }
            assert_eq!(interleaved(f, 2), [0.5, -0.5, 0.5, -0.5, 0.5, -0.5]);
        }
    }
}
