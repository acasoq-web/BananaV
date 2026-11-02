//! Encoding with Media Foundation transforms (MFTs), driven directly: the
//! encoder of a chosen graphics card (NVIDIA NVENC, AMD AMF, Intel Quick
//! Sync) or of the processor, fed frames and samples, returning compressed
//! packets for BananaV's own MP4/MOV writer.
//!
//! Hardware encoders are asynchronous MFTs: they ask for input and announce
//! output through events. Software ones are synchronous. Both are handled.

use std::mem::ManuallyDrop;

use bv_core::VideoFrame;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Variant::{VARIANT, VT_UI4};
use windows::core::{GUID, Interface};

use crate::encoders::{activates, string_attr, vendor_of};
use crate::imp::{init, win};
use crate::{
    AudioEncoderSettings, Codec, Error, Packet, RateControl, Result, Vendor, VideoEncoderSettings,
};

const HNS: i64 = 10_000_000;

fn pack(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

fn video_subtype(codec: Codec) -> Result<GUID> {
    match codec {
        Codec::H264 => Ok(MFVideoFormat_H264),
        Codec::Hevc => Ok(MFVideoFormat_HEVC),
        Codec::Av1 => Ok(MFVideoFormat_AV1),
        Codec::Vp9 => Ok(MFVideoFormat_VP90),
        _ => Err(Error::Windows(format!("{codec:?} is not a video codec"))),
    }
}

fn audio_subtype(codec: Codec) -> Result<GUID> {
    match codec {
        Codec::Aac => Ok(MFAudioFormat_AAC),
        Codec::Mp3 => Ok(MFAudioFormat_MP3),
        Codec::Ac3 => Ok(MFAudioFormat_Dolby_AC3),
        Codec::Mp2 => Ok(MFAudioFormat_MPEG),
        Codec::Flac => Ok(MFAudioFormat_FLAC),
        Codec::Alac => Ok(MFAudioFormat_ALAC),
        Codec::Opus => Ok(MFAudioFormat_Opus),
        Codec::Wma => Ok(MFAudioFormat_WMAudioV8),
        Codec::WmaLossless => Ok(MFAudioFormat_WMAudio_Lossless),
        _ => Err(Error::Windows(format!("{codec:?} is not an audio codec"))),
    }
}

/// A `VT_UI4` variant.
fn variant_u32(v: u32) -> VARIANT {
    let mut var = VARIANT::default();
    // SAFETY: a VT_UI4 variant holds its value in `ulVal`; nothing to drop.
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = v;
    }
    var
}

/// Sets an encoder property; encoders ignore the ones they lack.
fn set_api(api: &ICodecAPI, key: &GUID, value: u32) {
    let v = variant_u32(value);
    let _ = unsafe { api.SetValue(key, &v) };
}

/// A blob attribute (`MF_MT_MPEG_SEQUENCE_HEADER`, `MF_MT_USER_DATA`).
fn blob(t: &IMFMediaType, key: &GUID) -> Option<Vec<u8>> {
    unsafe {
        let n = t.GetBlobSize(key).ok()?;
        let mut v = vec![0u8; n as usize];
        t.GetBlob(key, &mut v, None).ok()?;
        Some(v)
    }
}

fn new_sample(bytes: &[u8], capacity: u32) -> Result<IMFSample> {
    unsafe {
        let len = bytes.len().max(capacity as usize).max(1);
        let buffer = MFCreateMemoryBuffer(len as u32).map_err(win)?;
        if !bytes.is_empty() {
            let mut ptr: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None).map_err(win)?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            buffer.Unlock().map_err(win)?;
            buffer.SetCurrentLength(bytes.len() as u32).map_err(win)?;
        }
        let sample = MFCreateSample().map_err(win)?;
        sample.AddBuffer(&buffer).map_err(win)?;
        Ok(sample)
    }
}

fn read_packet(s: &IMFSample) -> Result<Packet> {
    unsafe {
        let buffer = s.ConvertToContiguousBuffer().map_err(win)?;
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        buffer.Lock(&mut ptr, None, Some(&mut len)).map_err(win)?;
        let data = std::slice::from_raw_parts(ptr, len as usize).to_vec();
        buffer.Unlock().map_err(win)?;
        Ok(Packet {
            data,
            time: s.GetSampleTime().unwrap_or(0),
            duration: s.GetSampleDuration().unwrap_or(0),
            decode_time: s
                .GetUINT64(&MFSampleExtension_DecodeTimestamp)
                .ok()
                .map(|v| v as i64),
            keyframe: s.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0,
        })
    }
}

/// An MFT being driven: synchronous (software) or asynchronous (hardware,
/// event driven).
struct Mft {
    activate: IMFActivate,
    mft: IMFTransform,
    events: Option<IMFMediaEventGenerator>,
    /// Inputs the asynchronous MFT asked for and has not been given.
    need_input: u32,
    provides_samples: bool,
    out_size: u32,
    streaming: bool,
}

impl Mft {
    fn create(activate: IMFActivate) -> Result<Mft> {
        unsafe {
            let mft: IMFTransform = activate.ActivateObject().map_err(win)?;
            let attrs = mft.GetAttributes().ok();
            let is_async = attrs
                .as_ref()
                .and_then(|a| a.GetUINT32(&MF_TRANSFORM_ASYNC).ok())
                == Some(1);
            if is_async && let Some(a) = &attrs {
                a.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).map_err(win)?;
            }
            let events = if is_async {
                Some(mft.cast::<IMFMediaEventGenerator>().map_err(win)?)
            } else {
                None
            };
            Ok(Mft {
                activate,
                mft,
                events,
                need_input: 0,
                provides_samples: false,
                out_size: 0,
                streaming: false,
            })
        }
    }

    fn refresh_output_info(&mut self) -> Result<()> {
        let info = unsafe { self.mft.GetOutputStreamInfo(0).map_err(win)? };
        let provides =
            (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32;
        self.provides_samples = info.dwFlags & provides != 0;
        self.out_size = info.cbSize.max(1 << 16);
        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        self.refresh_output_info()?;
        unsafe {
            self.mft
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(win)?;
            self.mft
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(win)?;
        }
        self.streaming = true;
        Ok(())
    }

    /// One `ProcessOutput`: false when the MFT needs more input.
    fn pull(&mut self, out: &mut Vec<Packet>) -> Result<bool> {
        let sample = if self.provides_samples {
            None
        } else {
            Some(new_sample(&[], self.out_size)?)
        };
        let mut buf = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(sample),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        }];
        let mut status = 0u32;
        let r = unsafe { self.mft.ProcessOutput(0, &mut buf, &mut status) };
        // SAFETY: each field is taken once, so it is released once.
        let sample = unsafe { ManuallyDrop::take(&mut buf[0].pSample) };
        drop(unsafe { ManuallyDrop::take(&mut buf[0].pEvents) });
        match r {
            Ok(()) => {
                if let Some(s) = sample {
                    let p = read_packet(&s)?;
                    if !p.data.is_empty() {
                        out.push(p);
                    }
                }
                Ok(true)
            }
            Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(false),
            Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                unsafe {
                    let t = self.mft.GetOutputAvailableType(0, 0).map_err(win)?;
                    self.mft.SetOutputType(0, &t, 0).map_err(win)?;
                }
                self.refresh_output_info()?;
                Ok(true)
            }
            Err(e) => Err(win(e)),
        }
    }

    /// Handles one event of an asynchronous MFT; true at the end of a drain.
    fn event(&mut self, e: &IMFMediaEvent, out: &mut Vec<Packet>) -> Result<bool> {
        let t = unsafe { e.GetType().map_err(win)? };
        if t == METransformNeedInput.0 as u32 {
            self.need_input += 1;
        } else if t == METransformHaveOutput.0 as u32 {
            self.pull(out)?;
        } else if t == METransformDrainComplete.0 as u32 {
            return Ok(true);
        } else if let Ok(status) = unsafe { e.GetStatus() }
            && status.is_err()
        {
            return Err(Error::Windows(format!("encoder error {:#x}", status.0)));
        }
        Ok(false)
    }

    /// Feeds one sample and collects what is ready.
    fn feed(&mut self, sample: &IMFSample, out: &mut Vec<Packet>) -> Result<()> {
        match self.events.clone() {
            None => {
                loop {
                    match unsafe { self.mft.ProcessInput(0, sample, 0) } {
                        Ok(()) => break,
                        Err(e) if e.code() == MF_E_NOTACCEPTING => {
                            if !self.pull(out)? {
                                return Err(Error::Windows(
                                    "the encoder takes no input and gives no output".into(),
                                ));
                            }
                        }
                        Err(e) => return Err(win(e)),
                    }
                }
                while self.pull(out)? {}
            }
            Some(ev) => {
                while self.need_input == 0 {
                    let e = unsafe {
                        ev.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                            .map_err(win)?
                    };
                    self.event(&e, out)?;
                }
                unsafe { self.mft.ProcessInput(0, sample, 0).map_err(win)? };
                self.need_input -= 1;
                while let Ok(e) = unsafe { ev.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                    self.event(&e, out)?;
                }
            }
        }
        Ok(())
    }

    /// Ends the stream and returns everything still inside.
    fn drain(&mut self, out: &mut Vec<Packet>) -> Result<()> {
        unsafe {
            self.mft
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)
                .map_err(win)?;
            self.mft
                .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                .map_err(win)?;
        }
        match self.events.clone() {
            None => while self.pull(out)? {},
            Some(ev) => loop {
                let e = unsafe {
                    ev.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
                        .map_err(win)?
                };
                if self.event(&e, out)? {
                    break;
                }
            },
        }
        Ok(())
    }
}

impl Drop for Mft {
    fn drop(&mut self) {
        unsafe {
            if self.streaming {
                let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            }
            let _ = self.activate.ShutdownObject();
        }
    }
}

/// A video encoder of the system, fed 4:2:0 frames (8 bits as NV12, 10
/// bits as P010).
pub struct VideoEncoder {
    mft: Mft,
    settings: VideoEncoderSettings,
    name: String,
    /// The graphics card's device, for encoders that need frames as its
    /// textures.
    device: Option<crate::d3d::Device>,
}

// SAFETY: the MFT is created in the MTA and used by one thread at a time
// (the render's).
unsafe impl Send for VideoEncoder {}

impl std::fmt::Debug for VideoEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoEncoder")
            .field("name", &self.name)
            .field("settings", &self.settings)
            .finish()
    }
}

/// Media Foundation's interlace mode of a field order.
fn interlace_mode(order: bv_core::FieldOrder) -> u32 {
    match order {
        bv_core::FieldOrder::Progressive => MFVideoInterlace_Progressive.0 as u32,
        bv_core::FieldOrder::UpperFirst => MFVideoInterlace_FieldInterleavedUpperFirst.0 as u32,
        bv_core::FieldOrder::LowerFirst => MFVideoInterlace_FieldInterleavedLowerFirst.0 as u32,
    }
}

impl VideoEncoder {
    pub fn new(s: VideoEncoderSettings) -> Result<VideoEncoder> {
        match VideoEncoder::open(s, false) {
            // Some cards' encoders (AMD's) only take frames on a Direct3D
            // device they are given.
            Err(_) if s.vendor != Vendor::Software => VideoEncoder::open(s, true),
            r => r,
        }
    }

    fn open(s: VideoEncoderSettings, d3d: bool) -> Result<VideoEncoder> {
        init()?;
        let subtype = video_subtype(s.codec)?;
        let hardware = s.vendor != Vendor::Software;
        let (activate, name) = activates(s.codec, hardware)
            .into_iter()
            .find_map(|a| {
                let name = string_attr(&a, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_default();
                let id =
                    string_attr(&a, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute).unwrap_or_default();
                let vendor = if hardware {
                    vendor_of(&id, &name)
                } else {
                    Vendor::Software
                };
                (vendor == s.vendor || (hardware && s.vendor == Vendor::OtherHardware))
                    .then_some((a, name))
            })
            .ok_or_else(|| Error::Windows(format!("no {:?} {:?} encoder", s.vendor, s.codec)))?;
        let mut mft = Mft::create(activate)?;
        let device = if d3d {
            let dev = crate::d3d::Device::new(s.vendor)?;
            unsafe {
                mft.mft
                    .ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, dev.manager.as_raw() as usize)
                    .map_err(win)?;
            }
            Some(dev)
        } else {
            None
        };
        unsafe {
            if let Ok(api) = mft.mft.cast::<ICodecAPI>() {
                let mode = match s.rate_control {
                    RateControl::Cbr => eAVEncCommonRateControlMode_CBR,
                    RateControl::Vbr => eAVEncCommonRateControlMode_PeakConstrainedVBR,
                    RateControl::Quality(_) => eAVEncCommonRateControlMode_Quality,
                };
                set_api(&api, &CODECAPI_AVEncCommonRateControlMode, mode.0 as u32);
                if let RateControl::Quality(q) = s.rate_control {
                    set_api(&api, &CODECAPI_AVEncCommonQuality, q.min(100));
                } else {
                    set_api(&api, &CODECAPI_AVEncCommonMeanBitRate, s.bitrate);
                    set_api(
                        &api,
                        &CODECAPI_AVEncCommonMaxBitRate,
                        s.max_bitrate.max(s.bitrate),
                    );
                }
                set_api(&api, &CODECAPI_AVEncMPVGOPSize, s.gop.max(1));
                // No B-frames: frames come out in display order.
                set_api(&api, &CODECAPI_AVEncMPVDefaultBPictureCount, 0);
                set_api(&api, &CODECAPI_AVEncCommonQualityVsSpeed, 70);
                set_api(&api, &CODECAPI_AVLowLatencyMode, 0);
            }

            let size = pack(s.width, s.height);
            let rate = pack(s.frame_rate.num, s.frame_rate.den);
            let out = MFCreateMediaType().map_err(win)?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(win)?;
            out.SetGUID(&MF_MT_SUBTYPE, &subtype).map_err(win)?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, s.bitrate.max(1))
                .map_err(win)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(win)?;
            out.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(win)?;
            out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))
                .map_err(win)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, interlace_mode(s.field_order))
                .map_err(win)?;
            let profile = match (s.codec, s.ten_bit) {
                (Codec::H264, _) => Some(eAVEncH264VProfile_High.0 as u32),
                (Codec::Hevc, false) => Some(eAVEncH265VProfile_Main_420_8.0 as u32),
                (Codec::Hevc, true) => Some(eAVEncH265VProfile_Main_420_10.0 as u32),
                (Codec::Av1, false) => Some(eAVEncAV1VProfile_Main_420_8.0 as u32),
                (Codec::Av1, true) => Some(eAVEncAV1VProfile_Main_420_10.0 as u32),
                _ => None,
            };
            if let Some(p) = profile {
                out.SetUINT32(&MF_MT_MPEG2_PROFILE, p).map_err(win)?;
            }
            out.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)
                .map_err(win)?;
            out.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
                .map_err(win)?;
            out.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)
                .map_err(win)?;
            out.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
                .map_err(win)?;
            // An encoder that codes no fields takes the frames as they are.
            if mft.mft.SetOutputType(0, &out, 0).is_err() {
                out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                    .map_err(win)?;
                mft.mft.SetOutputType(0, &out, 0).map_err(win)?;
            }
            let mode = out
                .GetUINT32(&MF_MT_INTERLACE_MODE)
                .unwrap_or(MFVideoInterlace_Progressive.0 as u32);

            let inp = MFCreateMediaType().map_err(win)?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(win)?;
            let input = if s.ten_bit {
                MFVideoFormat_P010
            } else {
                MFVideoFormat_NV12
            };
            inp.SetGUID(&MF_MT_SUBTYPE, &input).map_err(win)?;
            inp.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(win)?;
            inp.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(win)?;
            inp.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))
                .map_err(win)?;
            inp.SetUINT32(&MF_MT_INTERLACE_MODE, mode).map_err(win)?;
            inp.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)
                .map_err(win)?;
            inp.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
                .map_err(win)?;
            mft.mft.SetInputType(0, &inp, 0).map_err(win)?;
        }
        mft.start()?;
        Ok(VideoEncoder {
            mft,
            settings: s,
            name,
            device,
        })
    }

    /// The encoder's name, as Windows lists it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The parameter sets the encoder announces in its output type (Annex
    /// B), when it does; hardware encoders put them in the first keyframe.
    pub fn sequence_header(&self) -> Option<Vec<u8>> {
        let t = unsafe { self.mft.mft.GetOutputCurrentType(0).ok()? };
        blob(&t, &MF_MT_MPEG_SEQUENCE_HEADER).filter(|b| !b.is_empty())
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
        // The thread may not be the one that made the encoder.
        init()?;
        let (w, h) = (self.settings.width as usize, self.settings.height as usize);
        let ten = self.settings.ten_bit;
        let bytes = (w * h + 2 * (w / 2) * (h / 2)) * if ten { 2 } else { 1 };
        if data.len() < bytes {
            return Err(Error::Windows(format!(
                "a frame of {} bytes for a {w}×{h} encoder",
                data.len()
            )));
        }
        let data = &data[..bytes];
        let s = match &mut self.device {
            Some(dev) => {
                let buffer = dev.upload(data, w as u32, h as u32, ten)?;
                unsafe {
                    let s = MFCreateSample().map_err(win)?;
                    s.AddBuffer(&buffer).map_err(win)?;
                    s
                }
            }
            None => new_sample(data, 0)?,
        };
        unsafe {
            s.SetSampleTime(time).map_err(win)?;
            s.SetSampleDuration(duration).map_err(win)?;
        }
        let mut out = Vec::new();
        self.mft.feed(&s, &mut out)?;
        Ok(out)
    }

    /// Ends the stream: the packets still inside the encoder.
    pub fn finish(&mut self) -> Result<Vec<Packet>> {
        init()?;
        let mut out = Vec::new();
        self.mft.drain(&mut out)?;
        Ok(out)
    }
}

/// An audio encoder of the system, fed interleaved float samples
/// (converted to 16 or 24-bit PCM).
pub struct AudioEncoder {
    mft: Mft,
    settings: AudioEncoderSettings,
    name: String,
    config: Vec<u8>,
    sent: i64,
    bits: u16,
}

// SAFETY: as for [`VideoEncoder`].
unsafe impl Send for AudioEncoder {}

impl std::fmt::Debug for AudioEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioEncoder")
            .field("name", &self.name)
            .field("settings", &self.settings)
            .finish()
    }
}

fn pcm_type(rate: u32, channels: u16, bits: u16) -> Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType().map_err(win)?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(win)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(win)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)
            .map_err(win)?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels as u32)
            .map_err(win)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, bits as u32)
            .map_err(win)?;
        let block = channels as u32 * (bits as u32 / 8);
        t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block)
            .map_err(win)?;
        t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, block * rate)
            .map_err(win)?;
        t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)
            .map_err(win)?;
        Ok(t)
    }
}

impl AudioEncoder {
    pub fn new(s: AudioEncoderSettings) -> Result<AudioEncoder> {
        init()?;
        let subtype = audio_subtype(s.codec)?;
        let activate = activates(s.codec, false)
            .into_iter()
            .next()
            .ok_or_else(|| Error::Windows(format!("no {:?} encoder", s.codec)))?;
        let name = string_attr(&activate, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_default();
        let mft = Mft::create(activate)?;
        let bits = if s.bits >= 24 { 24 } else { 16 };
        unsafe {
            let inp = pcm_type(s.sample_rate, s.channels, bits)?;
            // Most encoders take the input first and then offer outputs;
            // the MP3 one wants its output first.
            let input_first = mft.mft.SetInputType(0, &inp, 0).is_ok();
            let out = Self::output_type(&mft.mft, &s, subtype, bits)?;
            mft.mft.SetOutputType(0, &out, 0).map_err(win)?;
            if !input_first {
                mft.mft.SetInputType(0, &inp, 0).map_err(win)?;
            }
        }
        let mut enc = AudioEncoder {
            mft,
            settings: s,
            name,
            config: Vec::new(),
            sent: 0,
            bits,
        };
        enc.mft.start()?;
        enc.config = enc.read_config();
        Ok(enc)
    }

    /// The output type closest to the asked bit rate among those the
    /// encoder offers at the asked rate and channels (built by hand for an
    /// encoder that offers none: AAC at 96–192 kbit/s).
    fn output_type(
        mft: &IMFTransform,
        s: &AudioEncoderSettings,
        subtype: GUID,
        bits: u16,
    ) -> Result<IMFMediaType> {
        unsafe {
            let mut best: Option<(u32, IMFMediaType)> = None;
            let want = s.bitrate / 8;
            for i in 0.. {
                let Ok(t) = mft.GetOutputAvailableType(0, i) else {
                    break;
                };
                if t.GetGUID(&MF_MT_SUBTYPE).ok() != Some(subtype) {
                    continue;
                }
                let rate = t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0);
                let ch = t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0);
                if rate != s.sample_rate || ch != s.channels as u32 {
                    continue;
                }
                let bytes = t.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND).unwrap_or(0);
                let d = bytes.abs_diff(want);
                if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                    best = Some((d, t));
                }
            }
            if let Some((_, t)) = best {
                return Ok(t);
            }
            let t = MFCreateMediaType().map_err(win)?;
            t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .map_err(win)?;
            t.SetGUID(&MF_MT_SUBTYPE, &subtype).map_err(win)?;
            t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, s.sample_rate)
                .map_err(win)?;
            t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, s.channels as u32)
                .map_err(win)?;
            t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, bits as u32)
                .map_err(win)?;
            let bytes = match s.bitrate {
                0..=112_000 => 12_000,
                112_001..=144_000 => 16_000,
                144_001..=176_000 => 20_000,
                _ => 24_000,
            };
            t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes)
                .map_err(win)?;
            if s.codec == Codec::Aac {
                t.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0).map_err(win)?;
            }
            Ok(t)
        }
    }

    fn read_config(&self) -> Vec<u8> {
        let Some(t) = (unsafe { self.mft.mft.GetOutputCurrentType(0).ok() }) else {
            return Vec::new();
        };
        let data = blob(&t, &MF_MT_USER_DATA).unwrap_or_default();
        match self.settings.codec {
            // HEAACWAVEINFO's extra fields (12 bytes), then the
            // AudioSpecificConfig.
            Codec::Aac => data.get(12..).map(<[u8]>::to_vec).unwrap_or_default(),
            _ => data,
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

    /// Bit rate of the chosen output type.
    pub fn bitrate(&self) -> u32 {
        unsafe {
            self.mft
                .mft
                .GetOutputCurrentType(0)
                .ok()
                .and_then(|t| t.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND).ok())
                .map_or(0, |b| b * 8)
        }
    }

    /// Encodes interleaved samples; returns the packets that are ready.
    pub fn encode(&mut self, interleaved: &[f32]) -> Result<Vec<Packet>> {
        if interleaved.is_empty() {
            return Ok(Vec::new());
        }
        let mut pcm = Vec::with_capacity(interleaved.len() * self.bits as usize / 8);
        for &v in interleaved {
            let v = v.clamp(-1.0, 1.0);
            if self.bits == 24 {
                let i = (v * 8_388_607.0).round() as i32;
                pcm.extend_from_slice(&i.to_le_bytes()[..3]);
            } else {
                pcm.extend_from_slice(&((v * 32_767.0).round() as i16).to_le_bytes());
            }
        }
        let frames = (interleaved.len() / self.settings.channels.max(1) as usize) as i64;
        let rate = self.settings.sample_rate as i64;
        let s = new_sample(&pcm, 0)?;
        unsafe {
            s.SetSampleTime(self.sent * HNS / rate).map_err(win)?;
            s.SetSampleDuration(frames * HNS / rate).map_err(win)?;
        }
        self.sent += frames;
        let mut out = Vec::new();
        self.mft.feed(&s, &mut out)?;
        Ok(out)
    }

    /// Ends the stream: the packets still inside the encoder.
    pub fn finish(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        self.mft.drain(&mut out)?;
        if self.config.is_empty() {
            self.config = self.read_config();
        }
        Ok(out)
    }
}
