//! Audio files: uncompressed PCM (Wave, Wave64, AIFF), FLAC, and the
//! system's encoders for MP3, AAC (.m4a) and Windows Media Audio.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use bv_core::Tick;
use bv_flac::FlacWriter;
use bv_mf::{AsfAudio, AsfSettings, AsfWriter, AudioEncoder, AudioEncoderSettings, Codec};
use bv_mp4::{AudioFormat, AudioTrack, Container, Writer};
use bv_timeline::Project;
use bv_wav::{AiffWriter, Cue, SampleFormat, W64Writer, WavWriter};

use super::video::{aac_rate, metadata};
use super::{FileFormat, Output, RenderSettings};

type Buffered = BufWriter<File>;

enum Sink {
    Wav(WavWriter<Buffered>),
    W64(W64Writer<Buffered>),
    Aiff(AiffWriter<Buffered>),
    Flac(FlacWriter<Buffered>),
    /// MP3 frames straight into the file, after an ID3 tag.
    Mp3 {
        out: Buffered,
        encoder: AudioEncoder,
        bytes: u64,
    },
    /// AAC in an MPEG-4 file.
    M4a {
        out: Box<Writer<Buffered>>,
        encoder: AudioEncoder,
        samples: u64,
    },
    Wma {
        out: AsfWriter,
        written: u64,
        rate: u32,
    },
}

pub(super) struct AudioOut {
    sink: Option<Sink>,
    path: PathBuf,
    channels: usize,
    interleaved: Vec<f32>,
}

/// The project's markers and regions in the render, as cue points at
/// `rate`.
fn cues(project: &Project, s: &RenderSettings, rate: u32) -> Vec<Cue> {
    if !s.options.save_markers {
        return Vec::new();
    }
    let frames = |t: bv_core::Tick| (t.as_seconds_f64() * rate as f64).round() as u32;
    let markers = project
        .markers
        .iter()
        .filter(|m| s.range.contains(m.time))
        .map(|m| Cue {
            frame: frames(m.time - s.range.start),
            length: 0,
            name: m.name.clone(),
        });
    let regions = project
        .regions
        .iter()
        .filter(|r| s.range.contains(r.range.start))
        .map(|r| Cue {
            frame: frames(r.range.start - s.range.start),
            length: frames(r.range.end().min(s.range.end()) - r.range.start),
            name: r.name.clone(),
        });
    let mut all: Vec<Cue> = markers.chain(regions).collect();
    all.sort_by_key(|c| c.frame);
    all
}

/// An ID3v2.3 tag with the project's summary (UTF-16 text frames).
fn id3(project: &Project) -> Vec<u8> {
    let s = &project.summary;
    let text = |t: &str| -> Vec<u8> {
        let mut v = vec![1, 0xFF, 0xFE];
        for u in t.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    };
    let mut frames = Vec::new();
    let mut frame = |id: &[u8; 4], body: Vec<u8>| {
        frames.extend_from_slice(id);
        frames.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frames.extend_from_slice(&[0, 0]);
        frames.extend_from_slice(&body);
    };
    for (id, value) in [
        (b"TIT2", &s.title),
        (b"TPE1", &s.artist),
        (b"TCOP", &s.copyright),
        (b"TENC", &s.engineer),
    ] {
        if !value.is_empty() {
            frame(id, text(value));
        }
    }
    if !s.comments.is_empty() {
        // Encoding, language, empty description, text.
        let mut body = vec![1];
        body.extend_from_slice(b"eng");
        body.extend_from_slice(&[0xFF, 0xFE, 0, 0]);
        body.extend_from_slice(&text(&s.comments)[1..]);
        frame(b"COMM", body);
    }
    frame(b"TSSE", text("BananaV"));
    let n = frames.len() as u32;
    // Sizes in the tag header are "synchsafe": 7 bits per byte.
    let size = [
        ((n >> 21) & 0x7F) as u8,
        ((n >> 14) & 0x7F) as u8,
        ((n >> 7) & 0x7F) as u8,
        (n & 0x7F) as u8,
    ];
    let mut out = b"ID3\x03\x00\x00".to_vec();
    out.extend_from_slice(&size);
    out.extend_from_slice(&frames);
    out
}

pub(super) fn open(project: &Project, s: &RenderSettings) -> Result<Box<dyn Output>, String> {
    let a = s.audio.ok_or("no audio settings")?;
    let create = || -> Result<Buffered, String> {
        let file = File::create(&s.path).map_err(|e| format!("{}: {e}", s.path.display()))?;
        Ok(BufWriter::with_capacity(1 << 20, file))
    };
    let format = if a.float {
        SampleFormat::Float(32)
    } else {
        SampleFormat::Int(a.bit_depth as u16)
    };
    let err = |e: bv_wav::Error| e.to_string();
    let sum = &project.summary;
    let sink = match s.format {
        FileFormat::Wave64 => {
            Sink::W64(W64Writer::new(create()?, a.sample_rate, a.channels, format).map_err(err)?)
        }
        FileFormat::Aiff => Sink::Aiff(
            AiffWriter::new(create()?, a.sample_rate, a.channels, a.bit_depth as u16)
                .map_err(err)?,
        ),
        FileFormat::Flac => {
            let bits = if a.bit_depth > 16 { 24 } else { 16 };
            let tags = [
                ("TITLE", sum.title.as_str()),
                ("ARTIST", sum.artist.as_str()),
                ("COPYRIGHT", sum.copyright.as_str()),
                ("COMMENT", sum.comments.as_str()),
                ("ENCODED-BY", sum.engineer.as_str()),
            ];
            Sink::Flac(
                FlacWriter::new(create()?, a.sample_rate, a.channels, bits, &tags)
                    .map_err(|e| e.to_string())?,
            )
        }
        FileFormat::Mp3 => {
            let encoder = AudioEncoder::new(AudioEncoderSettings {
                codec: Codec::Mp3,
                sample_rate: a.sample_rate,
                channels: a.channels,
                bitrate: a.bitrate,
                bits: 16,
            })
            .map_err(|e| format!("MP3: {e}"))?;
            let mut out = create()?;
            let tag = id3(project);
            out.write_all(&tag).map_err(|e| e.to_string())?;
            Sink::Mp3 {
                out,
                encoder,
                bytes: tag.len() as u64,
            }
        }
        FileFormat::Aac => {
            let rate = aac_rate(a.sample_rate);
            let encoder = AudioEncoder::new(AudioEncoderSettings {
                codec: Codec::Aac,
                sample_rate: rate,
                channels: a.channels,
                bitrate: a.bitrate,
                bits: 16,
            })
            .map_err(|e| format!("AAC: {e}"))?;
            let packets =
                (s.range.duration.as_seconds_f64() * rate as f64 / 1024.0).ceil() as u64 + 4;
            let mut out = Writer::new(
                create()?,
                Container::Mp4,
                None,
                Some(AudioTrack {
                    format: AudioFormat::Aac {
                        config: if encoder.config().is_empty() {
                            bv_mp4::aac_config(rate, a.channels)
                        } else {
                            encoder.config().to_vec()
                        },
                        bitrate: encoder.bitrate(),
                    },
                    sample_rate: rate,
                    channels: a.channels,
                    priming: bv_mf::AAC_PRIMING,
                }),
                Writer::<Buffered>::reserve_for(0, packets),
            )
            .map_err(|e| e.to_string())?;
            out.set_metadata(metadata(project, s));
            Sink::M4a {
                out: Box::new(out),
                encoder,
                samples: 0,
            }
        }
        FileFormat::Wma => Sink::Wma {
            out: AsfWriter::create(
                &s.path.to_string_lossy(),
                AsfSettings {
                    video: None,
                    audio: Some(AsfAudio {
                        sample_rate: a.sample_rate,
                        channels: a.channels,
                        bitrate: a.bitrate,
                        lossless: false,
                    }),
                },
            )
            .map_err(|e| format!("WMA: {e}"))?,
            written: 0,
            rate: a.sample_rate,
        },
        _ => {
            // File › Properties › Summary goes into the file's INFO chunk.
            let info = [
                (*b"INAM", sum.title.as_str()),
                (*b"IART", sum.artist.as_str()),
                (*b"IENG", sum.engineer.as_str()),
                (*b"ICOP", sum.copyright.as_str()),
                (*b"ICMT", sum.comments.as_str()),
                (*b"ISFT", "BananaV"),
            ];
            let mut w = WavWriter::with_info(create()?, a.sample_rate, a.channels, format, &info)
                .map_err(err)?;
            w.set_cues(cues(project, s, a.sample_rate));
            if let Some(p) = s.reference() {
                w.set_project(&p);
            }
            Sink::Wav(w)
        }
    };
    Ok(Box::new(AudioOut {
        sink: Some(sink),
        path: s.path.clone(),
        channels: a.channels as usize,
        interleaved: Vec::new(),
    }))
}

impl AudioOut {
    fn interleave(&mut self, planar: &[Vec<f32>]) {
        let n = planar.first().map_or(0, Vec::len);
        self.interleaved.clear();
        for i in 0..n {
            for c in 0..self.channels {
                self.interleaved.push(planar.get(c).map_or(0.0, |ch| ch[i]));
            }
        }
    }
}

impl Output for AudioOut {
    fn audio(&mut self, planar: &[Vec<f32>]) -> Result<(), String> {
        let n = planar.first().map_or(0, Vec::len);
        if matches!(
            self.sink,
            Some(Sink::Mp3 { .. } | Sink::M4a { .. } | Sink::Wma { .. })
        ) {
            self.interleave(planar);
        }
        let interleaved = &self.interleaved;
        match self.sink.as_mut().ok_or("closed")? {
            Sink::Wav(w) => w.write_planar(planar).map_err(|e| e.to_string()),
            Sink::W64(w) => w.write_planar(planar).map_err(|e| e.to_string()),
            Sink::Aiff(w) => w.write_planar(planar).map_err(|e| e.to_string()),
            Sink::Flac(w) => w.write_planar(planar).map_err(|e| e.to_string()),
            Sink::Mp3 {
                out,
                encoder,
                bytes,
            } => {
                for p in encoder.encode(interleaved).map_err(|e| e.to_string())? {
                    out.write_all(&p.data).map_err(|e| e.to_string())?;
                    *bytes += p.data.len() as u64;
                }
                Ok(())
            }
            Sink::M4a {
                out,
                encoder,
                samples,
            } => {
                *samples += n as u64;
                for p in encoder.encode(interleaved).map_err(|e| e.to_string())? {
                    out.write_audio(&p.data, 1024).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            Sink::Wma { out, written, rate } => {
                let t = Tick::from_seconds_f64(*written as f64 / *rate as f64);
                let d = Tick::from_seconds_f64((*written + n as u64) as f64 / *rate as f64) - t;
                *written += n as u64;
                out.write_audio(interleaved, t, d)
                    .map_err(|e| e.to_string())
            }
        }
    }

    fn bytes(&self) -> u64 {
        match &self.sink {
            Some(Sink::Wav(w)) => w.bytes_written(),
            Some(Sink::W64(w)) => w.bytes_written(),
            Some(Sink::Aiff(w)) => w.bytes_written(),
            Some(Sink::Flac(w)) => w.bytes_written(),
            Some(Sink::Mp3 { bytes, .. }) => *bytes,
            Some(Sink::M4a { out, .. }) => out.bytes_written(),
            Some(Sink::Wma { written, rate, .. }) => {
                // Unknown until the end: the expected size at the bit rate.
                *written * 24_000 / (*rate).max(1) as u64
            }
            None => 0,
        }
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        let e = |e: &dyn std::fmt::Display| e.to_string();
        match self.sink.take().ok_or("closed")? {
            Sink::Wav(w) => drop(w.finish().map_err(|x| e(&x))?),
            Sink::W64(w) => drop(w.finish().map_err(|x| e(&x))?),
            Sink::Aiff(w) => drop(w.finish().map_err(|x| e(&x))?),
            Sink::Flac(w) => drop(w.finish().map_err(|x| e(&x))?),
            Sink::Mp3 {
                mut out,
                mut encoder,
                ..
            } => {
                for p in encoder.finish().map_err(|x| e(&x))? {
                    out.write_all(&p.data).map_err(|x| e(&x))?;
                }
                out.flush().map_err(|x| e(&x))?;
            }
            Sink::M4a {
                mut out,
                mut encoder,
                samples,
            } => {
                for p in encoder.finish().map_err(|x| e(&x))? {
                    out.write_audio(&p.data, 1024).map_err(|x| e(&x))?;
                }
                out.set_audio_length(samples);
                out.finish().map_err(|x| e(&x))?;
            }
            Sink::Wma { out, .. } => out.finish().map_err(|x| e(&x))?,
        }
        Ok(vec![self.path.clone()])
    }
}
