//! Video files: H.264, HEVC or AV1 from the encoders of this computer
//! (processor or graphics card) with AAC audio, in BananaV's MP4 writer.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use bv_core::{ColorInfo, FrameRate, ImageRgbaF32, PixelFormat};
use bv_mf::{AudioEncoder, AudioEncoderSettings, Codec, Packet, RateControl, VideoEncoder};
use bv_mp4::{AudioFormat, AudioTrack, Container, VideoFormat, VideoTrack, Writer};
use bv_timeline::Project;

use super::{FileFormat, Output, RateMode, RenderSettings};

const HNS: i64 = 10_000_000;

/// Counts what goes through a file.
struct Counted<W> {
    inner: W,
    written: u64,
}

impl<W: std::io::Write> std::io::Write for Counted<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: std::io::Seek> std::io::Seek for Counted<W> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl<W> std::fmt::Debug for Counted<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Counted")
            .field("written", &self.written)
            .finish()
    }
}

type Mp4File = Writer<Counted<BufWriter<File>>>;

pub(super) struct Mp4Out {
    writer: Option<Mp4File>,
    video: VideoEncoder,
    audio: Option<AudioEncoder>,
    rate: FrameRate,
    format: PixelFormat,
    channels: usize,
    audio_samples: u64,
    path: PathBuf,
    interleaved: Vec<f32>,
    /// The project's video levels formats: its studio RGB kept.
    video_levels: bool,
}

/// AAC takes these rates; others are mixed at 48 kHz.
pub(super) fn aac_rate(rate: u32) -> u32 {
    if rate == 44_100 { 44_100 } else { 48_000 }
}

fn markers(project: &Project, s: &RenderSettings) -> Vec<bv_mp4::Chapter> {
    if !s.options.save_markers {
        return Vec::new();
    }
    project
        .markers
        .iter()
        .filter(|m| s.range.contains(m.time))
        .map(|m| bv_mp4::Chapter {
            start_ms: ((m.time - s.range.start).as_seconds_f64() * 1000.0).round() as u64,
            title: m.name.clone(),
        })
        .collect()
}

pub(super) fn metadata(project: &Project, r: &RenderSettings) -> bv_mp4::Metadata {
    let s = &project.summary;
    bv_mp4::Metadata {
        title: s.title.clone(),
        artist: s.artist.clone(),
        comment: s.comments.clone(),
        copyright: s.copyright.clone(),
        encoder: "BananaV".into(),
        project: r.reference().unwrap_or_default(),
    }
}

pub(super) fn mp4(project: &Project, s: &RenderSettings) -> Result<Box<dyn Output>, String> {
    let v = s.video.ok_or("no video settings")?;
    let (codec, format) = match s.format {
        FileFormat::HevcMp4 => (Codec::Hevc, VideoFormat::Hevc),
        FileFormat::Av1Mp4 => (Codec::Av1, VideoFormat::Av1),
        _ => (Codec::H264, VideoFormat::H264),
    };
    let ten_bit = v.bit_depth > 8 && s.format.ten_bit(v.encoder);
    // 4:2:0 needs even sizes.
    let (w, h) = (v.width & !1, v.height & !1);
    let gop = if v.keyframe_interval > 0 {
        v.keyframe_interval
    } else {
        v.frame_rate.as_f64().round().max(1.0) as u32
    };
    let video = VideoEncoder::new(bv_mf::VideoEncoderSettings {
        codec,
        vendor: v.encoder.vendor(),
        width: w,
        height: h,
        frame_rate: v.frame_rate,
        bitrate: v.bitrate,
        max_bitrate: v.max_bitrate.max(v.bitrate),
        rate_control: match v.rate_mode {
            RateMode::Cbr => RateControl::Cbr,
            RateMode::Vbr => RateControl::Vbr,
            RateMode::Quality => RateControl::Quality(v.quality as u32),
        },
        gop,
        ten_bit,
        field_order: v.field_order,
    })
    .map_err(|e| {
        format!(
            "{} ({}): {e}",
            s.format.codec_name(),
            v.encoder.short_name()
        )
    })?;
    let audio = match &s.audio {
        Some(a) => Some(
            AudioEncoder::new(AudioEncoderSettings {
                codec: Codec::Aac,
                sample_rate: aac_rate(a.sample_rate),
                channels: a.channels,
                bitrate: a.bitrate,
                bits: 16,
            })
            .map_err(|e| format!("AAC: {e}"))?,
        ),
        None => None,
    };
    let rate = v.frame_rate;
    let frames = (s.range.duration.as_seconds_f64() * rate.as_f64()).ceil() as u64 + 1;
    let packets = s.audio.map_or(0, |a| {
        (s.range.duration.as_seconds_f64() * a.sample_rate as f64 / 1024.0).ceil() as u64 + 4
    });
    let file = File::create(&s.path).map_err(|e| format!("{}: {e}", s.path.display()))?;
    let mut writer = Writer::new(
        Counted {
            inner: BufWriter::with_capacity(1 << 20, file),
            written: 0,
        },
        Container::Mp4,
        Some(VideoTrack {
            format,
            width: w,
            height: h,
            timescale: rate.num,
            bit_depth: if ten_bit { 10 } else { 8 },
            color: (1, 1, 1, false),
            alpha: false,
            pixel_aspect: pixel_aspect(project),
            spherical: project.settings.output_360,
        }),
        audio.as_ref().zip(s.audio).map(|(enc, a)| AudioTrack {
            format: AudioFormat::Aac {
                config: if enc.config().is_empty() {
                    bv_mp4::aac_config(aac_rate(a.sample_rate), a.channels)
                } else {
                    enc.config().to_vec()
                },
                bitrate: enc.bitrate(),
            },
            sample_rate: aac_rate(a.sample_rate),
            channels: a.channels,
            priming: bv_mf::AAC_PRIMING,
        }),
        Mp4File::reserve_for(frames, packets),
    )
    .map_err(|e| e.to_string())?;
    if let Some(h) = video.sequence_header() {
        writer.add_parameter_sets(&h);
    }
    writer.set_metadata(metadata(project, s));
    writer.set_chapters(markers(project, s));
    Ok(Box::new(Mp4Out {
        writer: Some(writer),
        video,
        audio,
        rate,
        format: if ten_bit {
            PixelFormat::Yuv420P10
        } else {
            PixelFormat::Yuv420P8
        },
        channels: s.audio.map_or(2, |a| a.channels as usize),
        video_levels: project.settings.pixel_format.is_video_levels(),
        audio_samples: 0,
        path: s.path.clone(),
        interleaved: Vec::new(),
    }))
}

impl Mp4Out {
    fn put_video(&mut self, packets: Vec<Packet>) -> Result<(), String> {
        let w = self.writer.as_mut().ok_or("closed")?;
        let (num, den) = (self.rate.num as i64, self.rate.den as i64);
        for p in packets {
            // 100 ns → frame → track ticks (timescale = rate.num).
            let frame = (p.time * num + den * HNS / 2) / (den * HNS);
            let dts = p
                .decode_time
                .map(|d| (d * num + den * HNS / 2) / (den * HNS) * den);
            w.write_video(&p.data, frame * den, dts, den as u32, p.keyframe)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn put_audio(&mut self, packets: Vec<Packet>) -> Result<(), String> {
        let w = self.writer.as_mut().ok_or("closed")?;
        for p in packets {
            w.write_audio(&p.data, 1024).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Output for Mp4Out {
    fn video(&mut self, img: &ImageRgbaF32, n: u64) -> Result<(), String> {
        let (w, h) = (img.width & !1, img.height & !1);
        let ten_bit = self.format == PixelFormat::Yuv420P10;
        let data = if (w, h) == (img.width, img.height) {
            bv_color::linear_to_420_levels(img, ten_bit, self.video_levels)
        } else {
            let mut even = ImageRgbaF32::transparent(w, h);
            for y in 0..h {
                for x in 0..w {
                    *even.pixel_mut(x, y) = img.pixel(x, y);
                }
            }
            bv_color::linear_to_420_levels(&even, ten_bit, self.video_levels)
        };
        self.video_packed(&data, n)
    }

    fn packed_420(&self) -> Option<bool> {
        Some(self.format == PixelFormat::Yuv420P10)
    }

    fn video_packed(&mut self, data: &[u8], n: u64) -> Result<(), String> {
        let (num, den) = (self.rate.num as i64, self.rate.den as i64);
        let t = n as i64 * den * HNS / num;
        let d = (n as i64 + 1) * den * HNS / num - t;
        let packets = self
            .video
            .encode_packed(data, t, d)
            .map_err(|e| e.to_string())?;
        self.put_video(packets)
    }

    fn audio(&mut self, planar: &[Vec<f32>]) -> Result<(), String> {
        let Some(enc) = &mut self.audio else {
            return Ok(());
        };
        let n = planar.first().map_or(0, Vec::len);
        self.interleaved.clear();
        for i in 0..n {
            for c in 0..self.channels {
                self.interleaved.push(planar.get(c).map_or(0.0, |ch| ch[i]));
            }
        }
        self.audio_samples += n as u64;
        let packets = enc.encode(&self.interleaved).map_err(|e| e.to_string())?;
        self.put_audio(packets)
    }

    fn encoder_name(&self) -> String {
        self.video.name().to_string()
    }

    fn bytes(&self) -> u64 {
        self.writer.as_ref().map_or(0, |w| w.bytes_written())
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        let rest = self.video.finish().map_err(|e| e.to_string())?;
        self.put_video(rest)?;
        if let Some(enc) = &mut self.audio {
            let rest = enc.finish().map_err(|e| e.to_string())?;
            self.put_audio(rest)?;
        }
        let mut w = self.writer.take().ok_or("closed")?;
        if self.audio.is_some() {
            w.set_audio_length(self.audio_samples);
        }
        w.finish().map_err(|e| e.to_string())?;
        Ok(vec![self.path.clone()])
    }
}

/// Windows Media Video 9 with WMA audio, through Windows' ASF writer.
pub(super) struct WmvOut {
    out: Option<bv_mf::AsfWriter>,
    rate: FrameRate,
    sample_rate: u32,
    channels: usize,
    written: u64,
    frames: u64,
    bitrate: u64,
    path: PathBuf,
    interleaved: Vec<f32>,
    /// The project's video levels formats: its studio RGB kept.
    video_levels: bool,
}

pub(super) fn wmv(s: &RenderSettings, video_levels: bool) -> Result<Box<dyn Output>, String> {
    let v = s.video.ok_or("no video settings")?;
    let out = bv_mf::AsfWriter::create(
        &s.path.to_string_lossy(),
        bv_mf::AsfSettings {
            video: Some(bv_mf::AsfVideo {
                width: v.width & !1,
                height: v.height & !1,
                frame_rate: v.frame_rate,
                bitrate: v.bitrate,
            }),
            audio: s.audio.map(|a| bv_mf::AsfAudio {
                sample_rate: a.sample_rate,
                channels: a.channels,
                bitrate: a.bitrate,
                lossless: false,
            }),
        },
    )
    .map_err(|e| format!("WMV: {e}"))?;
    Ok(Box::new(WmvOut {
        out: Some(out),
        rate: v.frame_rate,
        sample_rate: s.audio.map_or(48_000, |a| a.sample_rate),
        channels: s.audio.map_or(2, |a| a.channels as usize),
        written: 0,
        frames: 0,
        bitrate: v.bitrate as u64 + s.audio.map_or(0, |a| a.bitrate as u64),
        video_levels,
        path: s.path.clone(),
        interleaved: Vec::new(),
    }))
}

impl Output for WmvOut {
    fn video(&mut self, img: &ImageRgbaF32, n: u64) -> Result<(), String> {
        let (w, h) = (img.width & !1, img.height & !1);
        let mut even = ImageRgbaF32::transparent(w, h);
        for y in 0..h {
            for x in 0..w {
                *even.pixel_mut(x, y) = img.pixel(x, y);
            }
        }
        let frame = bv_color::linear_to_frame_levels(
            &even,
            PixelFormat::Yuv420P8,
            ColorInfo::default(),
            self.video_levels,
        );
        let t = self.rate.frame_start(n as i64);
        let d = self.rate.frame_start(n as i64 + 1) - t;
        self.frames = n + 1;
        self.out
            .as_mut()
            .ok_or("closed")?
            .write_video(&frame, t, d)
            .map_err(|e| e.to_string())
    }

    fn audio(&mut self, planar: &[Vec<f32>]) -> Result<(), String> {
        let n = planar.first().map_or(0, Vec::len);
        self.interleaved.clear();
        for i in 0..n {
            for c in 0..self.channels {
                self.interleaved.push(planar.get(c).map_or(0.0, |ch| ch[i]));
            }
        }
        let rate = self.sample_rate as f64;
        let t = bv_core::Tick::from_seconds_f64(self.written as f64 / rate);
        let d = bv_core::Tick::from_seconds_f64((self.written + n as u64) as f64 / rate) - t;
        self.written += n as u64;
        self.out
            .as_mut()
            .ok_or("closed")?
            .write_audio(&self.interleaved, t, d)
            .map_err(|e| e.to_string())
    }

    fn encoder_name(&self) -> String {
        "Windows Media Video 9".into()
    }

    fn bytes(&self) -> u64 {
        // The writer keeps the file to itself: the size at the bit rate.
        (self.frames as f64 / self.rate.as_f64() * self.bitrate as f64 / 8.0) as u64
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        self.out
            .take()
            .ok_or("closed")?
            .finish()
            .map_err(|e| e.to_string())?;
        Ok(vec![self.path.clone()])
    }
}

/// Apple ProRes (BananaV's encoder) with PCM audio, in QuickTime MOV.
pub(super) struct ProResOut {
    writer: Option<Writer<Counted<BufWriter<File>>>>,
    encoder: bv_prores::Encoder,
    profile: super::ProResProfile,
    alpha: bool,
    bits: u16,
    /// Frame duration in the track's timescale (the rate's numerator).
    den: u32,
    path: PathBuf,
    /// The project's video levels formats: its studio RGB kept.
    video_levels: bool,
    /// The compressed frames it takes as they are.
    pass: super::smart::Passable,
}

pub(super) fn prores(project: &Project, s: &RenderSettings) -> Result<Box<dyn Output>, String> {
    let v = s.video.ok_or("no video settings")?;
    let profile = v.prores;
    let alpha = v.alpha && profile.is_444();
    let encoder = bv_prores::Encoder::new(
        match profile {
            super::ProResProfile::Proxy => bv_prores::Profile::Proxy,
            super::ProResProfile::Lt => bv_prores::Profile::Lt,
            super::ProResProfile::Standard => bv_prores::Profile::Standard,
            super::ProResProfile::Hq => bv_prores::Profile::Hq,
            super::ProResProfile::P4444 => bv_prores::Profile::P4444,
            super::ProResProfile::P4444Xq => bv_prores::Profile::P4444Xq,
        },
        v.width,
        v.height,
        v.frame_rate.as_f64(),
        alpha,
    )
    .interlaced(match v.field_order {
        bv_core::FieldOrder::Progressive => None,
        bv_core::FieldOrder::UpperFirst => Some(true),
        bv_core::FieldOrder::LowerFirst => Some(false),
    });
    let bits = s
        .audio
        .map_or(16, |a| if a.bit_depth > 16 { 24 } else { 16 });
    let file = File::create(&s.path).map_err(|e| format!("{}: {e}", s.path.display()))?;
    let frames = (s.range.duration.as_seconds_f64() * v.frame_rate.as_f64()).ceil() as u64 + 1;
    let mut writer = Writer::new(
        Counted {
            inner: BufWriter::with_capacity(4 << 20, file),
            written: 0,
        },
        Container::Mov,
        Some(VideoTrack {
            format: VideoFormat::ProRes {
                fourcc: profile.fourcc(),
            },
            width: v.width,
            height: v.height,
            timescale: v.frame_rate.num,
            bit_depth: 10,
            color: (1, 1, 1, false),
            alpha,
            pixel_aspect: pixel_aspect(project),
            spherical: project.settings.output_360,
        }),
        s.audio.map(|a| AudioTrack {
            format: AudioFormat::Pcm { bits, float: false },
            sample_rate: a.sample_rate,
            channels: a.channels,
            priming: 0,
        }),
        Writer::<File>::reserve_for(frames, frames * 2),
    )
    .map_err(|e| e.to_string())?;
    writer.set_metadata(metadata(project, s));
    writer.set_chapters(markers(project, s));
    Ok(Box::new(ProResOut {
        writer: Some(writer),
        encoder,
        profile,
        alpha,
        bits,
        den: v.frame_rate.den,
        path: s.path.clone(),
        video_levels: project.settings.pixel_format.is_video_levels(),
        pass: super::smart::Passable {
            fourcc: profile.fourcc(),
            width: v.width,
            height: v.height,
            rate: v.frame_rate,
            field_order: v.field_order,
            alpha,
        },
    }))
}

impl Output for ProResOut {
    fn video(&mut self, img: &ImageRgbaF32, n: u64) -> Result<(), String> {
        let format = if self.profile.is_444() {
            PixelFormat::Yuv444P10
        } else {
            PixelFormat::Yuv422P10
        };
        // Straight colour for files with alpha.
        let straight;
        let source = if self.alpha {
            let mut s = img.clone();
            for px in &mut s.data {
                if px[3] > 0.0 {
                    for c in 0..3 {
                        px[c] /= px[3];
                    }
                }
            }
            straight = s;
            &straight
        } else {
            img
        };
        let frame = bv_color::linear_to_frame_levels(
            source,
            format,
            ColorInfo::default(),
            self.video_levels,
        );
        let plane = |i: usize| -> Vec<u16> {
            let p = &frame.planes[i];
            let mut out = Vec::with_capacity(p.width * p.height);
            for y in 0..p.height {
                for x in 0..p.width {
                    out.push(p.sample(x, y));
                }
            }
            out
        };
        let alpha: Vec<u16> = if self.alpha {
            img.data
                .iter()
                .map(|px| (px[3].clamp(0.0, 1.0) * 65535.0).round() as u16)
                .collect()
        } else {
            Vec::new()
        };
        let (y, cb, cr) = (plane(0), plane(1), plane(2));
        let data = self.encoder.encode(&bv_prores::Frame {
            y: &y,
            cb: &cb,
            cr: &cr,
            alpha: self.alpha.then_some(&alpha[..]),
        });
        let den = self.den;
        let w = self.writer.as_mut().ok_or("closed")?;
        w.write_video(&data, n as i64 * den as i64, None, den, true)
            .map_err(|e| e.to_string())
    }

    fn audio(&mut self, planar: &[Vec<f32>]) -> Result<(), String> {
        let n = planar.first().map_or(0, Vec::len);
        let bytes = self.bits as usize / 8;
        let mut pcm = Vec::with_capacity(n * planar.len() * bytes);
        for i in 0..n {
            for ch in planar {
                let s = ch[i].clamp(-1.0, 1.0);
                if self.bits == 24 {
                    let v = (s * 8_388_607.0).round() as i32;
                    pcm.extend_from_slice(&v.to_le_bytes()[..3]);
                } else {
                    pcm.extend_from_slice(&((s * 32_767.0).round() as i16).to_le_bytes());
                }
            }
        }
        let w = self.writer.as_mut().ok_or("closed")?;
        w.write_audio(&pcm, n as u32).map_err(|e| e.to_string())
    }

    fn encoder_name(&self) -> String {
        format!("BananaV ProRes {}", self.profile.name())
    }

    fn passable(&self) -> Option<super::smart::Passable> {
        Some(self.pass)
    }

    fn compressed(&mut self, data: &[u8], n: u64) -> Result<(), String> {
        let den = self.den;
        let w = self.writer.as_mut().ok_or("closed")?;
        w.write_video(data, n as i64 * den as i64, None, den, true)
            .map_err(|e| e.to_string())
    }

    fn bytes(&self) -> u64 {
        self.writer.as_ref().map_or(0, |w| w.bytes_written())
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        self.writer
            .take()
            .ok_or("closed")?
            .finish()
            .map_err(|e| e.to_string())?;
        Ok(vec![self.path.clone()])
    }
}

/// The shape of the rendered frames' pixels: the project's.
fn pixel_aspect(project: &bv_timeline::Project) -> (u32, u32) {
    let p = project.settings.pixel_aspect;
    (p.num, p.den)
}
