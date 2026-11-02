//! Windows Media files (ASF: `.wmv`, `.wma`) through Media Foundation's
//! sink writer, which has Windows' WMV 9 and WMA encoders.

use bv_core::{PlaneData, Tick, VideoFrame};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::{HSTRING, Interface};

use crate::imp::{init, win};
use crate::{AsfSettings, Error, Result};

const HNS: u32 = 10_000_000;

fn pack(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

pub struct AsfWriter {
    writer: IMFSinkWriter,
    video: Option<u32>,
    audio: Option<u32>,
    settings: AsfSettings,
}

// SAFETY: the sink writer is created in the MTA and used by one thread at
// a time (the render's).
unsafe impl Send for AsfWriter {}

impl std::fmt::Debug for AsfWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsfWriter")
            .field("settings", &self.settings)
            .finish()
    }
}

/// The WMA output type closest to the asked rate, channels and bit rate.
fn wma_type(rate: u32, channels: u16, bitrate: u32, lossless: bool) -> Result<IMFMediaType> {
    let subtype = if lossless {
        MFAudioFormat_WMAudio_Lossless
    } else {
        // Standard WMA (what "WMA 9" files are); V9 is WMA Pro.
        MFAudioFormat_WMAudioV8
    };
    unsafe {
        let list =
            MFTranscodeGetAudioOutputAvailableTypes(&subtype, MFT_ENUM_FLAG_ALL.0 as u32, None)
                .map_err(win)?;
        let mut best: Option<(u64, IMFMediaType)> = None;
        for i in 0..list.GetElementCount().map_err(win)? {
            let Ok(t) = list.GetElement(i).and_then(|u| u.cast::<IMFMediaType>()) else {
                continue;
            };
            let r = t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0);
            let c = t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0);
            let bytes = t.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND).unwrap_or(0);
            let bits = t.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE).unwrap_or(16);
            // The rate and channels must match; then the closest bit rate
            // (16-bit types first).
            if r != rate || c != channels as u32 {
                continue;
            }
            let d =
                (bytes as u64 * 8).abs_diff(bitrate as u64) + if bits == 16 { 0 } else { 1 << 40 };
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, t));
            }
        }
        best.map(|(_, t)| t)
            .ok_or_else(|| Error::Windows(format!("WMA has no {rate} Hz {channels}-channel type")))
    }
}

impl AsfWriter {
    pub fn create(path: &str, s: AsfSettings) -> Result<AsfWriter> {
        init()?;
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 3).map_err(win)?;
            let attrs = attrs.ok_or(Error::Unavailable)?;
            attrs
                .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
                .map_err(win)?;
            attrs
                .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_ASF)
                .map_err(win)?;
            let writer =
                MFCreateSinkWriterFromURL(&HSTRING::from(path), None, &attrs).map_err(win)?;

            let video = match s.video {
                Some(v) => {
                    let size = pack(v.width, v.height);
                    let rate = pack(v.frame_rate.num, v.frame_rate.den);
                    let out = MFCreateMediaType().map_err(win)?;
                    out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                        .map_err(win)?;
                    out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_WMV3)
                        .map_err(win)?;
                    out.SetUINT32(&MF_MT_AVG_BITRATE, v.bitrate).map_err(win)?;
                    out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                        .map_err(win)?;
                    out.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(win)?;
                    out.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(win)?;
                    out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))
                        .map_err(win)?;
                    let idx = writer.AddStream(&out).map_err(win)?;
                    let inp = MFCreateMediaType().map_err(win)?;
                    inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                        .map_err(win)?;
                    inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
                        .map_err(win)?;
                    inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                        .map_err(win)?;
                    inp.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(win)?;
                    inp.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(win)?;
                    inp.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))
                        .map_err(win)?;
                    writer.SetInputMediaType(idx, &inp, None).map_err(win)?;
                    Some(idx)
                }
                None => None,
            };
            let audio = match s.audio {
                Some(a) => {
                    let out = wma_type(a.sample_rate, a.channels, a.bitrate, a.lossless)?;
                    let idx = writer.AddStream(&out).map_err(win)?;
                    let inp = MFCreateMediaType().map_err(win)?;
                    inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                        .map_err(win)?;
                    inp.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
                        .map_err(win)?;
                    inp.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, a.sample_rate)
                        .map_err(win)?;
                    inp.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, a.channels as u32)
                        .map_err(win)?;
                    inp.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                        .map_err(win)?;
                    let block = a.channels as u32 * 2;
                    inp.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block)
                        .map_err(win)?;
                    inp.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, block * a.sample_rate)
                        .map_err(win)?;
                    writer.SetInputMediaType(idx, &inp, None).map_err(win)?;
                    Some(idx)
                }
                None => None,
            };
            writer.BeginWriting().map_err(win)?;
            Ok(AsfWriter {
                writer,
                video,
                audio,
                settings: s,
            })
        }
    }

    fn write(&mut self, stream: u32, bytes: &[u8], time: Tick, duration: Tick) -> Result<()> {
        unsafe {
            let buffer = MFCreateMemoryBuffer(bytes.len().max(1) as u32).map_err(win)?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None).map_err(win)?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            buffer.Unlock().map_err(win)?;
            buffer.SetCurrentLength(bytes.len() as u32).map_err(win)?;
            let sample = MFCreateSample().map_err(win)?;
            sample.AddBuffer(&buffer).map_err(win)?;
            sample.SetSampleTime(time.to_timescale(HNS)).map_err(win)?;
            sample
                .SetSampleDuration(duration.to_timescale(HNS))
                .map_err(win)?;
            self.writer.WriteSample(stream, &sample).map_err(win)
        }
    }

    /// A 4:2:0 8-bit frame (sent as NV12).
    pub fn write_video(&mut self, f: &VideoFrame, time: Tick, duration: Tick) -> Result<()> {
        let Some(stream) = self.video else {
            return Ok(());
        };
        let (w, h) = (f.width as usize, f.height as usize);
        let plane = |i: usize| -> Result<&[u8]> {
            match &f.planes.get(i).ok_or(Error::NoStream)?.data {
                PlaneData::U8(d) => Ok(d),
                PlaneData::U16(_) => Err(Error::Windows("expected an 8-bit frame".into())),
            }
        };
        let (y, u, v) = (plane(0)?, plane(1)?, plane(2)?);
        let (ys, cs) = (f.planes[0].stride, f.planes[1].stride);
        let mut nv12 = Vec::with_capacity(w * h * 3 / 2);
        for row in 0..h {
            nv12.extend_from_slice(&y[row * ys..row * ys + w]);
        }
        for row in 0..h / 2 {
            for x in 0..w / 2 {
                nv12.push(u[row * cs + x]);
                nv12.push(v[row * cs + x]);
            }
        }
        self.write(stream, &nv12, time, duration)
    }

    /// Interleaved float audio (sent as 16-bit PCM).
    pub fn write_audio(&mut self, interleaved: &[f32], time: Tick, duration: Tick) -> Result<()> {
        let Some(stream) = self.audio else {
            return Ok(());
        };
        let pcm: Vec<u8> = interleaved
            .iter()
            .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes())
            .collect();
        self.write(stream, &pcm, time, duration)
    }

    /// Closes the file.
    pub fn finish(self) -> Result<()> {
        unsafe { self.writer.Finalize().map_err(win) }
    }
}
