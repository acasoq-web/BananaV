//! MPEG-2 files for DVD and Blu-ray authoring (File › Render As's MPEG-2):
//! BananaV's own MPEG-2 video encoder (`bv_mpeg2`) and the system's MPEG
//! audio (Layer II) encoder — linear PCM where there is none — in a
//! program stream of DVD-sized packs written by BananaV (`bv_mp4::ps`).
//! MPEG-1 files for Video CDs alike: MPEG-1 video and MPEG audio (which
//! they need) in a system stream of Video CD's packs.
//! Frames at the project's markers are I-frames when the markers are
//! saved (VEGAS's "Render I-frames at markers", so authoring programs
//! make chapters of them); the picture's shape goes in the sequence
//! headers.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use bv_mf::{AudioEncoder, AudioEncoderSettings, Codec};
use bv_mp4::ps::PsWriter;
use bv_timeline::Project;

use super::{Output, RenderSettings};

/// How the sound goes in.
enum Sound {
    Mpeg(AudioEncoder),
    /// Linear PCM (16-bit, big-endian), when the system has no MPEG audio
    /// encoder.
    Pcm,
    None,
}

pub(super) struct Mpeg2Out {
    /// MPEG-1's (for the encoder's name).
    mpeg1: bool,
    writer: Option<PsWriter<BufWriter<File>>>,
    video: bv_mpeg2::Encoder,
    sound: Sound,
    /// 90 kHz ticks a frame (as a fraction of the rate).
    rate: bv_core::FrameRate,
    sample_rate: u32,
    /// Samples written (PCM's times).
    samples: u64,
    /// Frames made I-frames (the markers').
    keys: Vec<u64>,
    /// Pictures written: the next one's decoding time.
    coded: u64,
    path: PathBuf,
}

/// MPEG-2's aspect ratio code for a picture shown `dw` : `dh` (its display
/// shape): 4:3, 16:9 or 2.21:1, the nearest.
pub(crate) fn aspect_code(dw: f64, dh: f64) -> u8 {
    let r = dw / dh.max(1e-9);
    [(4.0 / 3.0, 2u8), (16.0 / 9.0, 3), (2.21, 4)]
        .into_iter()
        .min_by(|a, b| (a.0 - r).abs().total_cmp(&(b.0 - r).abs()))
        .map_or(2, |(_, c)| c)
}

/// The project's picture's display shape code (an anamorphic frame is
/// filled: see `normalized`).
pub(crate) fn project_aspect(project: &Project) -> u8 {
    let (pw, ph) = project.settings.output_size();
    let par = project.settings.pixel_aspect;
    aspect_code(
        pw as f64 * par.num as f64 / par.den.max(1) as f64,
        ph as f64,
    )
}

/// BananaV's MPEG-2 (or MPEG-1) encoder for the template's video; `dvd`:
/// DVD-Video's, an interlaced sequence with the disc's aspect code.
pub(crate) fn video_encoder(
    project: &Project,
    v: &super::VideoParams,
    gop: u32,
    dvd: Option<u8>,
    mpeg1: bool,
) -> bv_mpeg2::Encoder {
    bv_mpeg2::Encoder::new(bv_mpeg2::Settings {
        width: v.width,
        height: v.height,
        rate: (v.frame_rate.num, v.frame_rate.den),
        bitrate: v.bitrate.max(500_000),
        max_bitrate: v.max_bitrate.max(v.bitrate),
        gop,
        aspect: dvd.unwrap_or_else(|| project_aspect(project)),
        top_field_first: match v.field_order {
            bv_core::FieldOrder::Progressive => None,
            bv_core::FieldOrder::UpperFirst => Some(true),
            bv_core::FieldOrder::LowerFirst => Some(false),
        },
        interlaced_sequence: dvd.is_some(),
        mpeg1,
        // IBBP, as DVD's and Video CD's.
        b_frames: 2,
    })
}

pub(super) fn open(project: &Project, s: &RenderSettings) -> Result<Box<dyn Output>, String> {
    let v = s.video.ok_or("no video settings")?;
    let gop = if v.keyframe_interval > 0 {
        v.keyframe_interval
    } else {
        // About half a second, as DVD's GOPs.
        (v.frame_rate.as_f64() / 2.0).round().max(1.0) as u32
    };
    let mpeg1 = s.format == super::FileFormat::Mpeg1;
    let video = video_encoder(project, &v, gop, None, mpeg1);
    let sound = match &s.audio {
        Some(a) => match AudioEncoder::new(AudioEncoderSettings {
            codec: Codec::Mp2,
            sample_rate: a.sample_rate,
            channels: a.channels,
            bitrate: a.bitrate,
            bits: 16,
        }) {
            Ok(enc) => Sound::Mpeg(enc),
            // MPEG-1's system streams carry MPEG audio only.
            Err(e) if mpeg1 => return Err(format!("no MPEG audio encoder: {e}")),
            Err(_) => Sound::Pcm,
        },
        None => Sound::None,
    };
    let keys = if s.options.save_markers {
        project
            .markers
            .iter()
            .filter(|m| m.time >= s.range.start && m.time < s.range.end())
            .map(|m| v.frame_rate.frame_at(m.time - s.range.start).max(0) as u64)
            .collect()
    } else {
        Vec::new()
    };
    let file = File::create(&s.path).map_err(|e| format!("{}: {e}", s.path.display()))?;
    let audio_bits = match &sound {
        Sound::Mpeg(_) => s.audio.map_or(0, |a| a.bitrate),
        Sound::Pcm => s
            .audio
            .map_or(0, |a| a.sample_rate * a.channels as u32 * 16),
        Sound::None => 0,
    };
    let bitrate = v.max_bitrate.max(v.bitrate) + audio_bits;
    let out = BufWriter::with_capacity(1 << 20, file);
    // Video CD's packs fit its sectors' 2324 bytes.
    let vcd = matches!((v.width, v.height), (352, 240 | 288));
    let pack = if vcd { 2324 } else { bv_mp4::ps::PACK };
    let writer = match &sound {
        _ if mpeg1 => PsWriter::mpeg1(out, bitrate, !matches!(sound, Sound::None), pack),
        Sound::Pcm => PsWriter::with_pcm(out, bitrate, s.audio.map_or(48_000, |a| a.sample_rate)),
        Sound::Mpeg(_) => PsWriter::new(out, bitrate, true),
        Sound::None => PsWriter::new(out, bitrate, false),
    };
    Ok(Box::new(Mpeg2Out {
        mpeg1,
        writer: Some(writer),
        video,
        sound,
        rate: v.frame_rate,
        sample_rate: s.audio.map_or(48_000, |a| a.sample_rate),
        samples: 0,
        keys,
        coded: 0,
        path: s.path.clone(),
    }))
}

impl Mpeg2Out {
    /// Frame `f`'s time (90 kHz).
    fn at(&self, f: u64) -> u64 {
        f * self.rate.den as u64 * 90_000 / self.rate.num.max(1) as u64
    }

    /// Coded pictures into the stream: each decoded a frame after the one
    /// before and shown a frame after its place, so B-pictures, which come
    /// after the anchor they need, show in time (the sound waits as long).
    fn write_pictures(&mut self, pictures: Vec<bv_mpeg2::Picture>) -> Result<(), String> {
        for p in pictures {
            let (pts, dts) = (self.at(p.display + 1), self.at(self.coded));
            self.coded += 1;
            let w = self.writer.as_mut().ok_or("closed")?;
            w.write_video(&p.data, pts, dts)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn write_audio_packets(&mut self, packets: Vec<bv_mf::Packet>) -> Result<(), String> {
        let delay = self.at(1);
        let w = self.writer.as_mut().ok_or("closed")?;
        for p in packets {
            let pts = (p.time.max(0) * 9 / 1000) as u64 + delay;
            w.write_audio(&p.data, pts).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Output for Mpeg2Out {
    fn packed_420(&self) -> Option<bool> {
        Some(false)
    }

    fn video_packed(&mut self, data: &[u8], n: u64) -> Result<(), String> {
        let pictures = self.video.encode(data, self.keys.contains(&n));
        self.write_pictures(pictures)
    }

    fn audio(&mut self, planar: &[Vec<f32>]) -> Result<(), String> {
        let delay = self.at(1);
        let n = planar.first().map_or(0, Vec::len);
        let mut interleaved = Vec::with_capacity(n * planar.len());
        for i in 0..n {
            for ch in planar {
                interleaved.push(ch[i]);
            }
        }
        match &mut self.sound {
            Sound::Mpeg(enc) => {
                let packets = enc.encode(&interleaved).map_err(|e| e.to_string())?;
                self.write_audio_packets(packets)
            }
            Sound::Pcm => {
                let pts = self.samples * 90_000 / self.sample_rate.max(1) as u64 + delay;
                let mut pcm = Vec::with_capacity(2 * interleaved.len());
                for v in &interleaved {
                    let s = (v.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
                    pcm.extend_from_slice(&s.to_be_bytes());
                }
                self.samples += n as u64;
                let w = self.writer.as_mut().ok_or("closed")?;
                w.write_pcm(&pcm, pts).map_err(|e| e.to_string())
            }
            Sound::None => Ok(()),
        }
    }

    fn encoder_name(&self) -> String {
        if self.mpeg1 {
            "BananaV MPEG-1".into()
        } else {
            "BananaV MPEG-2".into()
        }
    }

    fn bytes(&self) -> u64 {
        self.writer.as_ref().map_or(0, |w| w.bytes_written())
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        let rest = self.video.finish();
        self.write_pictures(rest)?;
        if let Sound::Mpeg(a) = &mut self.sound {
            let rest = a.finish().map_err(|e| e.to_string())?;
            self.write_audio_packets(rest)?;
        }
        self.writer
            .take()
            .ok_or("closed")?
            .finish()
            .map_err(|e| e.to_string())?;
        Ok(vec![self.path.clone()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_get_their_aspect_codes() {
        assert_eq!(aspect_code(1920.0, 1080.0), 3);
        assert_eq!(aspect_code(640.0, 480.0), 2);
        assert_eq!(aspect_code(2.35, 1.0), 4);
    }
}
